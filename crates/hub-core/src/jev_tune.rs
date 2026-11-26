//! A brief for tuning the decision model (Jev): how it works, how it is set, and its latest
//! decisions with the whole of what each one read and what happened in the thread afterwards.
//! The Jev card's Tune hands it, cut to a budget, to one short engine answer in the background
//! (`run_tune`), which comes back as suggestions kept in `TUNE`. Only a threshold suggestion you
//! use changes a setting; the others are for a thread to build.

use crate::locked::ResultExt;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::apis::{ApiDecision, FEATURES, Seen};
use crate::hub::Hub;
use crate::model::{EventKind, now_ms};

/// Decisions written out in full; the rest of the kept ones are one line each.
const FULL: usize = 60;
/// Briefs kept in the data folder.
const KEEP: usize = 5;
/// How long after a decision a message in its thread counts as what happened next.
const AFTER_MS: i64 = 30 * 60 * 1000;
/// The latest tune, in the store.
pub(crate) const TUNE: &str = "apis.tune";
/// Characters of brief a background tune sends (about 20k tokens): decisions in full until this
/// is reached, the rest one line each.
const BUDGET: usize = 80_000;
/// Suggestions kept from one tune.
const MAX_SUGGESTIONS: usize = 8;
/// One tune at a time.
static RUNNING: AtomicBool = AtomicBool::new(false);

/// `POST /api/apis/tune`: where the brief was written and what to ask a thread.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TuneBrief {
    pub path: String,
    pub prompt: String,
    /// Decisions in it.
    pub decisions: u32,
}

/// `GET /api/apis/tune/state`: the latest background tune and what it suggested.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct TuneState {
    /// A tune is reading the log now.
    pub running: bool,
    /// When the latest tune finished (ms), 0 before the first.
    #[ts(type = "number")]
    pub at: i64,
    /// Decisions it read in full, and in all.
    pub read: u32,
    pub decisions: u32,
    pub suggestions: Vec<TuneSuggestion>,
    /// Why the latest tune gave nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}

/// One change the tune proposes, with the decisions that back it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct TuneSuggestion {
    pub id: String,
    /// "context" (more for Jev to read), "wording" (the question or its options) or "threshold".
    pub kind: String,
    /// One of `apis::FEATURES`.
    pub feature: String,
    pub text: String,
    /// For a threshold: the new one (0.5 to 0.99), which Use sets.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sure: Option<f64>,
    /// The backing decisions, by when they were made (ms), as in `apis.decisions`.
    #[ts(type = "number[]")]
    pub backing: Vec<i64>,
    /// "used" or "skipped" once you answered it.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub answer: Option<String>,
}

/// The thresholds `reading.rs` and `judge.rs` use when you have not changed them.
fn default_sure(feature: &str) -> f64 {
    match feature {
        "proof" | "topic" => 0.75,
        _ => 0.6,
    }
}

use crate::text::clip;

/// A moment as `YYYY-MM-DD HH:MM UTC`.
fn when(ms: i64) -> String {
    let minute = ms.div_euclid(60_000).rem_euclid(24 * 60);
    format!("{} {:02}:{:02} UTC", crate::activity::date_of(ms, 0), minute / 60, minute % 60)
}

impl Hub {
    /// Writes the brief to `<data>/jev-tune/` and says what to ask a thread to do with it.
    pub async fn jev_tune(&self) -> Result<TuneBrief> {
        let (text, decisions, _) = self.jev_tune_text(usize::MAX).await?;
        let dir = self.config.data_dir.join("jev-tune");
        std::fs::create_dir_all(&dir)?;
        let path: PathBuf = dir.join(format!("brief-{}.md", now_ms()));
        std::fs::write(&path, text)?;
        let mut old: Vec<PathBuf> = std::fs::read_dir(&dir)?.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "md")).collect();
        old.sort();
        while old.len() > KEEP {
            crate::locked::clean_file(old.remove(0));
        }
        let path = path.to_string_lossy().into_owned();
        let prompt = format!(
            "Read the brief at {path}. It describes Jev, Hub's decision model, how it is set, and its latest {decisions} decisions with everything each one read and what happened in the thread afterwards.\n\n\
             Help me make Jev better. For each feature, judge from the evidence whether Jev decided right, and when it was unsure or wrong, whether it simply had too little to go on. Then propose changes one at a time, each with the decisions that back it:\n\
             - context Hub should add to what Jev reads (this feeds the coming handoff package for Jev),\n\
             - the wording of the question or its options,\n\
             - thresholds, remembering that a low confidence that was right matters as much as a high one.\n\n\
             Don't change code or settings yet; I'll pick which to do."
        );
        Ok(TuneBrief { path, prompt, decisions: decisions as u32 })
    }

    /// One decision in full: when, what Jev read, what it said and what the person wrote next.
    /// Also the thread's title, for the one-line list.
    async fn full_entry(&self, i: usize, d: &ApiDecision, seen: &BTreeMap<(i64, String), Seen>) -> (String, Option<(String, String)>) {
        let mut out = String::new();
        let mut thread_title = None;
        let thread = match &d.thread {
            Some(id) => match self.store.thread(id).await.ok().flatten() {
                Some(t) => {
                    thread_title = Some((id.clone(), t.title.clone()));
                    Some(t)
                }
                None => None,
            },
            None => None,
        };
        out.push_str(&format!(
            "### #{} · {} · {} · said \"{}\" at {:.0}% · {}\n\n",
            i + 1,
            when(d.at),
            d.feature,
            d.said,
            d.confidence.unwrap_or(0.0) * 100.0,
            if d.acted { "Hub went by it" } else { "not sure enough, Hub used its own rules" }
        ));
        if let Some(t) = &thread {
            out.push_str(&format!("Thread: \"{}\" ({}), {:?} now.\n\n", t.title, t.cwd, t.status));
        }
        match seen.get(&(d.at, d.feature.clone())) {
            Some(s) => {
                out.push_str(&format!("Asked: {} (goes by it from {:.0}%)\n\nOptions:\n", s.instructions, s.sure * 100.0));
                for (name, meaning) in &s.options {
                    out.push_str(&format!("- {name}: {meaning}\n"));
                }
                out.push_str(&format!("\nWhat Jev read:\n\n````text\n{}\n````\n\n", s.state));
            }
            None => out.push_str(&format!("What Jev read was not kept (an older decision). It began: {}\n\n", d.text)),
        }
        if let Some(t) = &thread {
            let events = self.store.events_of(&t.id, 0, &[EventKind::UserMessage]).await.unwrap_or_default();
            let next = events
                .iter()
                .filter(|e| e.kind == EventKind::UserMessage && e.ts > d.at && e.ts - d.at < AFTER_MS)
                .find_map(|e| e.said());
            match next {
                Some(text) => out.push_str(&format!("What the person wrote next in the thread: \"{}\"\n\n", clip(text.trim(), 500))),
                None => out.push_str("The person wrote nothing in the thread in the half hour after.\n\n"),
            }
        }
        (out, thread_title)
    }

    /// The brief, and how many decisions it holds and how many of them in full. With a `budget`
    /// (characters), decisions go in full until it is reached, unsure and disagreeing ones first.
    pub(crate) async fn jev_tune_text(&self, budget: usize) -> Result<(String, usize, usize)> {
        let prefs = self.api_prefs().await?;
        let decisions: Vec<ApiDecision> = self.api_decisions().await?;
        let seen: BTreeMap<(i64, String), Seen> = self.api_seen().await?.into_iter().map(|s| ((s.at, s.feature.clone()), s)).collect();
        let mut out = String::new();
        out.push_str("# Jev tune brief\n\n");
        out.push_str(&format!("Written {}.\n\n", when(now_ms())));
        out.push_str(
            "## How Jev works\n\n\
             Jev is TypeSafe's decision model, reached with a key Christoffer added by hand. Hub asks it one choice question \
             at a time (`Hub::choose` in apis.rs): instructions, a few named options, and a state text Hub writes for that \
             feature (clipped to 6,000 characters). Jev answers with one option and how sure it is. Hub goes by the answer \
             only when the confidence reaches the feature's threshold; otherwise it does what it did before Jev (below). \
             Input costs $0.042 per million tokens and output is free, so more context is cheap.\n\n",
        );
        out.push_str(&features_section(&prefs));
        // Summary per feature.
        out.push_str(&summary_section(&decisions));
        // Which go in full: within a budget, the ones Hub learns most from (unsure, or not what Hub
        // would have done) first, then the latest.
        let mut order: Vec<usize> = (0..decisions.len()).collect();
        if budget != usize::MAX {
            order.sort_by_key(|&i| (decisions[i].acted && decisions[i].agree, i));
        }
        let mut full: Vec<usize> = Vec::new();
        let mut entries: BTreeMap<usize, String> = BTreeMap::new();
        let mut titles: BTreeMap<String, String> = BTreeMap::new();
        let mut used = out.len();
        for i in order.into_iter().take(FULL) {
            let d = &decisions[i];
            let (out, thread_title) = self.full_entry(i, d, &seen).await;
            if let Some((id, title)) = thread_title {
                titles.insert(id, title);
            }
            if used + out.len() > budget && !full.is_empty() {
                break;
            }
            used += out.len();
            full.push(i);
            entries.insert(i, out);
        }
        out.push_str(&format!("\n## {} decisions in full, newest first\n\n", full.len()));
        for text in entries.values() {
            out.push_str(text);
        }
        if decisions.len() > full.len() {
            out.push_str("## The other decisions, one line each\n\n");
            for (i, d) in decisions.iter().enumerate().filter(|(i, _)| !entries.contains_key(i)) {
                out.push_str(&one_line(i, d, &titles));
            }
        }
        Ok((out, decisions.len(), full.len()))
    }
}

/// What the background tune is asked, around the brief.
const ASK: &str = "You are tuning Jev, the decision model Hub asks small choice questions. The brief below \
says how Jev works, how it is set, and its decisions, numbered #1 and up, with what each one read and \
what the person wrote next. For each feature, judge from the evidence whether Jev decided right, and \
when it was unsure or wrong, whether it simply had too little to go on. Then propose at most 6 changes, \
the strongest first, each backed by the decisions that show it:\n\
- \"context\": something Hub should add to what Jev reads for that feature,\n\
- \"wording\": a better question or better options,\n\
- \"threshold\": a new threshold for a feature in the Features table, between 0.5 and 0.99. A low \
confidence that was right matters as much as a high one.\n\
Propose nothing the evidence does not back; an empty list is a fine answer. Write each text as one or \
two plain sentences to Christoffer, in English. Answer with JSON only, in this shape:\n\
{\"suggestions\": [{\"kind\": \"context\", \"feature\": \"turns\", \"text\": \"...\", \"sure\": null, \"decisions\": [3, 7]}]}";

/// The suggestions in the model's reply, wherever it put its JSON. Decision numbers become the
/// decisions' times; a threshold without a usable value is dropped.
fn parse_suggestions(reply: &str, decisions: &[ApiDecision]) -> Option<Vec<TuneSuggestion>> {
    let start = reply.find('{')?;
    let end = reply.rfind('}')?;
    let v: Value = serde_json::from_str(reply.get(start..=end)?).ok()?;
    let list = v["suggestions"].as_array()?;
    let mut out = Vec::new();
    for (n, s) in list.iter().enumerate() {
        let kind = s["kind"].as_str().unwrap_or_default().trim().to_lowercase();
        let feature = s["feature"].as_str().unwrap_or_default().trim().to_owned();
        let text = clip(s["text"].as_str().unwrap_or_default().trim(), 400);
        if !["context", "wording", "threshold"].contains(&kind.as_str()) || text.is_empty() || !FEATURES.iter().any(|(f, _)| *f == feature) {
            continue;
        }
        let sure = s["sure"].as_f64().filter(|v| v.is_finite()).map(|v| v.clamp(0.5, 0.99));
        if kind == "threshold" && sure.is_none() {
            continue;
        }
        let backing = s["decisions"]
            .as_array()
            .map(|a| a.iter().filter_map(|n| n.as_u64()).filter_map(|n| decisions.get((n as usize).checked_sub(1)?)).map(|d| d.at).collect())
            .unwrap_or_default();
        out.push(TuneSuggestion { id: format!("{}", n + 1), kind: kind.clone(), feature, text, sure: sure.filter(|_| kind == "threshold"), backing, answer: None });
        if out.len() == MAX_SUGGESTIONS {
            break;
        }
    }
    Some(out)
}

impl Hub {
    /// The latest tune, and whether one is reading now.
    pub async fn tune_state(&self) -> Result<TuneState> {
        let mut state: TuneState = match self.store.get_value(TUNE).await? {
            Some(json) => serde_json::from_str(&json).unwrap_or_default(),
            None => TuneState::default(),
        };
        state.running = RUNNING.load(Ordering::SeqCst);
        Ok(state)
    }

    /// Starts a tune in the background unless one is reading already. It asks the tidy engine
    /// once, with the brief cut to `BUDGET`, and keeps what comes back in `TUNE`.
    pub fn start_tune(self: &Arc<Self>) -> bool {
        if RUNNING.swap(true, Ordering::SeqCst) {
            return false;
        }
        let hub = self.clone();
        tokio::spawn(async move {
            let done = hub.run_tune().await;
            let mut state = hub.tune_state().await.unwrap_or_default();
            match done {
                Ok(next) => state = next,
                Err(err) => {
                    tracing::warn!("tuning Jev: {err:#}");
                    state.at = now_ms();
                    state.error = Some(format!("{err:#}"));
                }
            }
            state.running = false;
            if let Ok(json) = serde_json::to_string(&state) {
                hub.store.set_value(TUNE, &json).await.warn_on_err("save TUNE");
            }
            RUNNING.store(false, Ordering::SeqCst);
        });
        true
    }

    async fn run_tune(&self) -> Result<TuneState> {
        let decisions = self.api_decisions().await?;
        if decisions.is_empty() {
            bail!("Jev has made no decisions to learn from yet");
        }
        let (brief, all, full) = self.jev_tune_text(BUDGET).await?;
        let name = self.config.tidy.engine.as_ref().unwrap_or(&self.config.default_engine);
        let engine = self.config.engines.get(name).with_context(|| format!("unknown engine {name:?}"))?;
        let cwd = std::env::temp_dir().display().to_string();
        let prompt = format!("{ASK}\n\n<brief>\n{brief}\n</brief>");
        let answer = crate::engine::complete("jev_tune", engine, &cwd, Some("sonnet"), prompt);
        let reply = tokio::time::timeout(Duration::from_secs(300), answer).await.context("the tune took too long")??;
        let suggestions = parse_suggestions(&reply, &decisions).context("the model's answer was not the JSON asked for")?;
        Ok(TuneState { running: false, at: now_ms(), read: full as u32, decisions: all as u32, suggestions, error: None })
    }

    /// Answers a suggestion: Use sets a threshold suggestion's threshold; for the others it only
    /// marks it (the card starts the thread that builds it). Skip marks it.
    pub async fn answer_tune(&self, id: &str, used: bool) -> Result<TuneState> {
        let mut state = self.tune_state().await?;
        let s = state.suggestions.iter_mut().find(|s| s.id == id).context("no such suggestion")?;
        if used && s.kind == "threshold" {
            let sure = s.sure.context("the suggestion has no threshold")?;
            let mut prefs = self.api_prefs().await?;
            prefs.sure.insert(s.feature.clone(), sure);
            self.set_api_prefs(prefs).await?;
        }
        s.answer = Some(if used { "used" } else { "skipped" }.into());
        let mut kept = state.clone();
        kept.running = false;
        self.store.set_value(TUNE, &serde_json::to_string(&kept)?).await?;
        Ok(state)
    }
}

/// The table of features with their thresholds, and the connection Jev is reached over.
fn features_section(prefs: &crate::apis::ApiPrefs) -> String {
    let mut out = String::new();
        out.push_str("## Features\n\n| Feature | What it decides | Threshold | On | Without Jev |\n| --- | --- | --- | --- | --- |\n");
        let without = |f: &str| match f {
            "turns" => "the thread just ends; only a permission ask counts as needs you",
            "judge" => "the person's queue or steer setting decides",
            "topic" => "no offer of a thread of its own",
            "focus" => "the small subscription model decides",
            "proof" => "the exit code alone decides",
            _ => "",
        };
        for &(name, what) in FEATURES.iter() {
            let sure = prefs.sure.get(name).copied().unwrap_or(default_sure(name));
            let extra = if name == "judge" { " (stop 80%, new thread 75%)" } else { "" };
            let on = if prefs.off.iter().any(|f| f == name) { "off" } else { "on" };
            out.push_str(&format!("| {name} | {what} | {:.0}%{extra} | {on} | {} |\n", sure * 100.0, without(name)));
        }
        if let Some(c) = prefs.connections.iter().find(|c| prefs.helper.as_ref().is_none_or(|h| h == &c.name)) {
            out.push_str(&format!(
                "\nConnection: {} ({}), model {}, daily cap {}.\n",
                c.label,
                c.kind,
                c.model.as_deref().unwrap_or("default"),
                if c.daily_tokens > 0 { format!("{} tokens", c.daily_tokens) } else { "none".into() }
            ));
        }
    out
}

/// How many decisions each feature made, how many Hub acted on, and the average confidence.
fn summary_section(decisions: &[ApiDecision]) -> String {
    let mut out = String::new();
        out.push_str("\n## Summary\n\n| Feature | Decisions | Acted on | Average confidence |\n| --- | --- | --- | --- |\n");
        let mut by: BTreeMap<&str, (u32, u32, f64)> = BTreeMap::new();
        for d in decisions {
            let e = by.entry(d.feature.as_str()).or_default();
            e.0 += 1;
            e.1 += d.acted as u32;
            e.2 += d.confidence.unwrap_or(0.0);
        }
        for (f, (n, acted, sum)) in &by {
            out.push_str(&format!("| {f} | {n} | {acted} | {:.0}% |\n", sum / f64::from(*n) * 100.0));
        }
    out
}

/// A decision that does not go in full: one line.
fn one_line(i: usize, d: &ApiDecision, titles: &BTreeMap<String, String>) -> String {
    let title = d.thread.as_ref().and_then(|id| titles.get(id)).cloned().unwrap_or_default();
    format!(
        "- #{} {} · {} · \"{}\" {:.0}%{} · {} · {}\n",
        i + 1,
        when(d.at),
        d.feature,
        d.said,
        d.confidence.unwrap_or(0.0) * 100.0,
        if d.acted { "" } else { " (unsure)" },
        title,
        clip(&d.text, 100)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_brief_holds_what_each_decision_read() -> Result<()> {
        let data = tempfile::tempdir()?;
        let config = crate::Config { data_dir: data.path().to_owned(), ..Default::default() };
        let hub = Hub::new(crate::Store::in_memory()?, config);
        let at = now_ms();
        let d = ApiDecision {
            at,
            feature: "turns".into(),
            connection: "jev".into(),
            thread: None,
            text: "Request: fix it".into(),
            hub: String::new(),
            said: "done".into(),
            confidence: Some(0.55),
            agree: false,
            ms: 10,
            acted: false,
            quote: None,
            tokens: Some(100),
        };
        hub.store.set_value(crate::apis::DECISIONS, &serde_json::to_string(&vec![d])?).await?;
        let seen = vec![Seen {
            at,
            feature: "turns".into(),
            thread: None,
            instructions: "Where did the turn leave the work".into(),
            options: vec![("done".into(), "Finished".into())],
            state: "Request:\nfix the login test\n\nReply:\nFixed.".into(),
            sure: 0.6,
        }];
        hub.store.set_value(crate::apis::SEEN, &serde_json::to_string(&seen)?).await?;
        let brief = hub.jev_tune().await?;
        let text = std::fs::read_to_string(&brief.path)?;
        assert_eq!(brief.decisions, 1);
        assert!(text.contains("fix the login test"));
        assert!(text.contains("- done: Finished"));
        assert!(text.contains("| turns | 1 | 0 | 55% |"));
        assert!(brief.prompt.contains(&brief.path));
        for _ in 0..6 {
            std::thread::sleep(std::time::Duration::from_millis(2));
            hub.jev_tune().await?;
        }
        assert_eq!(std::fs::read_dir(data.path().join("jev-tune"))?.count(), KEEP);
        Ok(())
    }

    #[tokio::test]
    async fn a_suggestion_names_its_decisions_and_use_sets_a_threshold() -> Result<()> {
        let hub = Hub::new(crate::Store::in_memory()?, crate::Config::default());
        let d = |at| ApiDecision {
            at,
            feature: "judge".into(),
            connection: "jev".into(),
            thread: None,
            text: String::new(),
            hub: String::new(),
            said: "ask".into(),
            confidence: Some(0.7),
            agree: true,
            ms: 1,
            acted: true,
            quote: None,
            tokens: None,
        };
        let decisions = vec![d(30), d(20), d(10)];
        let reply = r#"Here: {"suggestions": [
            {"kind": "threshold", "feature": "judge", "text": "Lower it.", "sure": 0.4, "decisions": [1, 3, 9]},
            {"kind": "context", "feature": "turns", "text": "Give it the test output.", "sure": 0.7, "decisions": [2]},
            {"kind": "threshold", "feature": "turns", "text": "No number."},
            {"kind": "context", "feature": "nope", "text": "Unknown feature."}
        ]}"#;
        let list = parse_suggestions(reply, &decisions).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].sure, Some(0.5));
        assert_eq!(list[0].backing, vec![30, 10]);
        assert_eq!(list[1].sure, None);
        let state = TuneState { at: 1, suggestions: list, ..Default::default() };
        hub.store.set_value(TUNE, &serde_json::to_string(&state)?).await?;
        let after = hub.answer_tune("1", true).await?;
        assert_eq!(after.suggestions[0].answer.as_deref(), Some("used"));
        assert_eq!(hub.api_prefs().await?.sure.get("judge"), Some(&0.5));
        hub.answer_tune("2", false).await?;
        assert_eq!(hub.tune_state().await?.suggestions[1].answer.as_deref(), Some("skipped"));
        assert!(parse_suggestions("no json", &decisions).is_none());
        Ok(())
    }
}

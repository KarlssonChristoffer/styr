//! What one bench run cost in time, turns, tools and tokens, read from the thread's event log
//! only, and whether the run is valid at all (separate from whether its checks passed). Times
//! come from `timings::turns`, which already cuts each turn; this file only adds them up.
//!
//! Per turn or per session? Settled by reading the adapters' sources (no recording needed):
//! - `TurnEnd.usage` (ACP's `PromptResponse.usage`) is PER TURN for the Claude Code adapter
//!   (claude-agent-acp 0.99.1, the version the recordings in `tests/fixtures` came from): it
//!   resets its tally when a turn becomes active and adds each model result of that turn to it,
//!   so the tokens of a run are the sum over its turns, which is what `measure` does.
//! - `usage_update.cost.amount` is CUMULATIVE for the agent's session: the adapter passes on the
//!   SDK's `total_cost_usd`, documented as the running total of the `query()` call (a resumed
//!   session continues from the total it saved; `/clear` resets it). So the run's cost is the
//!   last amount, and when the amount drops (a new session, a reset) the peak before the drop is
//!   kept and the counting goes on from the new value. That reading is also right for an engine
//!   whose sessions restart in the middle of a run.
//!
//! unverified: the Codex adapter (codex-acp 2.5.0) ships as a binary, so its `usage` could not be
//! read; its one recorded turn fits a per-turn reading, and it sends no `cost`. The runner's
//! first two-turn run on it should check that the second turn's `input` is not the first turn's
//! plus more (cumulative); if it is, sum only the last turn there.

use serde::{Deserialize, Serialize};

use crate::model::{Event, EventKind, PermissionAutoData, QuotaSwitchData, TurnUsage, UserMessageData};

/// What a run took. Times in milliseconds; `None` when the log does not say.
/// Fields missing from a stored row read as their defaults (`bench/results.rs`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Measured {
    /// From the first sent prompt to the end of the last turn (Hub's own follow-ups included).
    pub wall_ms: Option<i64>,
    /// From the first sent prompt to the first words of the answer.
    pub first_word_ms: Option<i64>,
    /// Of the first turn: the engine starting (`TurnTiming.engine_ms`).
    pub engine_ms: Option<i64>,
    /// Of the first turn: until the first model call answered (`TurnTiming.model_ms`).
    pub model_ms: Option<i64>,
    /// Every turn in the log, Hub's follow-ups (proof, docs, leaks) included.
    pub turns: u32,
    pub tool_calls: u32,
    pub tool_failed: u32,
    /// Messages Hub sent asking for proof (`user_message` marked `proof`; a proof and docs request
    /// in one message counts here).
    pub proof_rounds: u32,
    /// Messages Hub sent asking only about the docs (marked `docs`).
    pub docs_rounds: u32,
    /// Permission questions Hub answered itself (`permission_auto`), allowed and refused. Nobody
    /// answers by hand in a bench run; a question left open fails `settled`.
    pub perm_allowed: u32,
    pub perm_refused: u32,
    /// Summed over the turns that carried usage, the side jobs' turns included; `None` when none did.
    pub tokens: Option<TurnUsage>,
    /// What the engines said the run cost (see the module notes), its side jobs included, so the
    /// suite's cost cap counts them; `None` when none ever said.
    pub cost_usd: Option<f64>,
    /// How many side jobs the run started, whose tokens and cost are in the two above.
    pub side_jobs: u32,
    /// The model that answered last, as the engine named it.
    pub model: Option<String>,
    /// Why the run cannot be compared with others, when it cannot: the quota guard moved it to
    /// another model, the plan limit was reached ([`PLAN_LIMIT`]), or another model answered than
    /// was asked for.
    pub invalid: Option<String>,
    /// What Hub refused the agent, in its words (`permission_auto.why`), so a refusal by the
    /// bench's stricter guard is visible in the report.
    pub refused: Vec<String>,
}

/// `Measured.invalid` when the engine's plan limit stopped a turn: the bench measures Hub, and a
/// spent plan is not Hub failing a case. The runner stops the run on it.
pub const PLAN_LIMIT: &str = "the plan limit was reached";

impl Measured {
    /// All tokens of the run, cache included: what the summary compares.
    pub fn total_tokens(&self) -> Option<i64> {
        let t = self.tokens.as_ref()?;
        Some([t.input, t.output, t.cache_read, t.cache_write].iter().flatten().sum())
    }

    /// The run with what its side jobs spent added to its tokens and cost (each job's [`spent`]).
    /// Times, turns, tools and validity stay the run's own: a side job may run on another model
    /// by the person's choice, which says nothing about the case.
    pub fn with_side_jobs(mut self, jobs: &[Measured]) -> Measured {
        for job in jobs {
            if let Some(u) = &job.tokens {
                self.tokens = Some(add(self.tokens.take(), u));
            }
            if let Some(c) = job.cost_usd {
                self.cost_usd = Some(self.cost_usd.unwrap_or(0.0) + c);
            }
            self.side_jobs += 1;
        }
        self
    }
}

/// What a side job's thread spent, read from its log the way [`measure`] reads the run's own.
pub fn spent(events: &[Event]) -> Measured {
    measure(events, &[], None, &[])
}

/// Measures a finished run. `sent` is the seq of each prompt the runner sent, in order;
/// `asked_model` the model the runner asked for (resolved, not "fast"), `None` to skip that test;
/// `limit_phrases` the engine's words for a spent plan (`EngineTraits.limit_phrases`), which
/// `resume::limit_hit` reads an error by, as Hub's own wait for a reset does.
pub fn measure(events: &[Event], sent: &[i64], asked_model: Option<&str>, limit_phrases: &[String]) -> Measured {
    let turns = crate::timings::turns(events);
    let first_sent = sent.first().copied();
    let first = turns.iter().find(|t| Some(t.prompt) == first_sent).or(turns.first());
    let started = first_sent.and_then(|seq| events.iter().find(|e| e.seq == seq)).map(|e| e.ts);
    let ended = events.iter().rev().find(|e| e.kind == EventKind::TurnEnd).map(|e| e.ts);
    let marked = |mark: &str| events.iter().filter_map(Event::data_as::<UserMessageData>).filter(|m| m.marks.contains_key(mark)).count() as u32;
    let auto: Vec<bool> = events.iter().filter_map(Event::data_as::<PermissionAutoData>).map(|p| p.allowed).collect();
    let tools = turns.iter().flat_map(|t| &t.tools);
    let models: Vec<&str> = turns.iter().filter_map(|t| t.model.as_deref()).collect();

    // A turn that stopped on the plan limit, by the engine's words or by Hub waiting for the reset.
    let limit = events.iter().any(|e| e.kind == EventKind::QuotaWait)
        || events.iter().filter(|e| e.kind == EventKind::Error).any(|e| crate::resume::limit_hit(e.failure().unwrap_or_default(), limit_phrases).is_some());
    let invalid = match (events.iter().find_map(Event::data_as::<QuotaSwitchData>), asked_model) {
        (Some(q), _) => Some(format!("the quota guard moved the thread to {} ({} at {})", q.to, q.limit, q.percent)),
        (None, _) if limit => Some(PLAN_LIMIT.to_owned()),
        (None, Some(asked)) => models.iter().find(|m| !same_model(asked, m)).map(|m| format!("{m} answered, not {asked}")),
        (None, None) => None,
    };
    let refused = events.iter().filter_map(Event::data_as::<PermissionAutoData>).filter(|p| !p.allowed).map(|p| p.why).collect();

    Measured {
        wall_ms: started.zip(ended).map(|(a, b)| (b - a).max(0)),
        first_word_ms: first.and_then(|t| Some(t.begin_ms? + t.text_ms?)),
        engine_ms: first.and_then(|t| t.engine_ms),
        model_ms: first.and_then(|t| t.model_ms),
        turns: turns.len() as u32,
        tool_calls: tools.clone().count() as u32,
        tool_failed: tools.filter(|t| t.failed).count() as u32,
        proof_rounds: marked("proof"),
        docs_rounds: marked("docs"),
        perm_allowed: auto.iter().filter(|a| **a).count() as u32,
        perm_refused: auto.iter().filter(|a| !**a).count() as u32,
        tokens: turns.iter().filter_map(|t| t.usage.as_ref()).fold(None, |sum, u| Some(add(sum, u))),
        cost_usd: cost(events),
        side_jobs: 0,
        model: models.last().map(|m| (*m).to_owned()),
        invalid,
        refused,
    }
}

fn add(sum: Option<TurnUsage>, u: &TurnUsage) -> TurnUsage {
    let s = sum.unwrap_or_default();
    let plus = |a: Option<i64>, b: Option<i64>| if a.is_none() && b.is_none() { None } else { Some(a.unwrap_or(0) + b.unwrap_or(0)) };
    TurnUsage { input: plus(s.input, u.input), output: plus(s.output, u.output), cache_read: plus(s.cache_read, u.cache_read), cache_write: plus(s.cache_write, u.cache_write) }
}

/// The run's cost from the engine's running totals: the last amount, plus the peak before each
/// time the total went down (a new session started counting from zero again).
fn cost(events: &[Event]) -> Option<f64> {
    let amounts = events
        .iter()
        .filter(|e| e.kind == EventKind::Update && crate::model::update_kind(&e.data) == Some("usage_update"))
        .filter_map(|e| e.data["cost"]["amount"].as_f64());
    let mut seen = false;
    let (mut banked, mut last) = (0.0_f64, 0.0_f64);
    for amount in amounts {
        if amount < last {
            banked += last;
        }
        last = amount;
        seen = true;
    }
    seen.then_some(banked + last)
}

/// The model asked for answered: the same name, or one with a dated or variant suffix on it.
fn same_model(asked: &str, answered: &str) -> bool {
    let (a, b) = (asked.to_lowercase(), answered.to_lowercase());
    a == b || b.starts_with(&format!("{a}-")) || b.starts_with(&format!("{a}[")) || a.starts_with(&format!("{b}-"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dated_name_is_the_same_model() {
        assert!(same_model("model-small-4-5", "model-small-4-5-20251001"));
        assert!(same_model("model-big-5", "model-big-5[1m]"));
        assert!(!same_model("model-small-4-5", "model-big-5"));
    }

    fn typed<T: crate::model::EventData>(seq: i64, data: &T) -> Event {
        Event { thread_id: "t".into(), seq, ts: seq, kind: T::KIND, data: data.to_json() }
    }

    #[test]
    fn a_spent_plan_makes_a_repeat_invalid_not_failed() {
        use crate::model::{ErrorData, QuotaWaitData, UserMessageData};
        let phrases = vec!["you've hit your".to_owned()];
        let asked = typed(1, &UserMessageData { text: "go".into(), ..Default::default() });
        // The engine's own words for a spent plan, read the way Hub's wait for a reset reads them.
        let hit = ErrorData { prompt: Some(1), ..ErrorData::new("Internal error: You've hit your limit · resets 3am (Europe/Stockholm)") };
        let events = [asked.clone(), typed(2, &hit)];
        assert_eq!(measure(&events, &[1], None, &phrases).invalid.as_deref(), Some(PLAN_LIMIT));
        // Another error is the engine failing the case: not invalid.
        let other = ErrorData { prompt: Some(1), ..ErrorData::new("the engine fell over") };
        assert_eq!(measure(&[asked.clone(), typed(2, &other)], &[1], None, &phrases).invalid, None);
        // Hub waiting for the reset says the same, whatever the words were.
        let wait = QuotaWaitData { state: "waiting".into(), ..Default::default() };
        assert_eq!(measure(&[asked, typed(2, &other), typed(3, &wait)], &[1], None, &[]).invalid.as_deref(), Some(PLAN_LIMIT));
    }

    #[test]
    fn a_side_jobs_spend_is_the_runs() {
        use crate::model::{TurnEndData, TurnStartData, UpdateData, UserMessageData};
        let usage = |input, output| TurnUsage { input: Some(input), output: Some(output), cache_read: None, cache_write: None };
        let cost = |seq, amount: f64| {
            let update = serde_json::json!({ "sessionUpdate": "usage_update", "cost": { "amount": amount, "currency": "USD" } });
            typed(seq, &UpdateData(update.as_object().cloned().unwrap_or_default()))
        };
        // One turn with its usage and the engine's running cost: the parent's and a side job's.
        let log = |input, amount| {
            vec![
                typed(1, &UserMessageData { text: "go".into(), ..Default::default() }),
                typed(2, &TurnStartData { prompt: Some(1), ..Default::default() }),
                cost(3, amount),
                typed(4, &TurnEndData { stop_reason: "end_turn".into(), prompt: Some(1), usage: Some(usage(input, 10)), ..Default::default() }),
            ]
        };
        let parent = measure(&log(100, 0.5), &[1], None, &[]);
        let job = spent(&log(40, 0.25));
        assert_eq!((job.tokens.clone(), job.cost_usd), (Some(usage(40, 10)), Some(0.25)));
        let whole = parent.clone().with_side_jobs(&[job]);
        assert_eq!((whole.tokens, whole.cost_usd, whole.side_jobs), (Some(usage(140, 20)), Some(0.75), 1));
        assert_eq!((whole.turns, whole.wall_ms), (parent.turns, parent.wall_ms), "times and turns stay the run's own");
        // A side job that said nothing adds nothing but its count.
        let quiet = parent.clone().with_side_jobs(&[Measured::default()]);
        assert_eq!((quiet.tokens, quiet.cost_usd, quiet.side_jobs), (parent.tokens, parent.cost_usd, 1));
    }

    #[test]
    fn what_hub_refused_is_kept_in_its_words() {
        use crate::model::PermissionAutoData;
        let refused = PermissionAutoData { allowed: false, why: "a bench case stays inside its own folder: it never pushes".into(), ..Default::default() };
        let allowed = PermissionAutoData { allowed: true, why: "allowed".into(), ..Default::default() };
        let m = measure(&[typed(1, &refused), typed(2, &allowed)], &[], None, &[]);
        assert_eq!((m.perm_refused, m.perm_allowed), (1, 1));
        assert_eq!(m.refused, ["a bench case stays inside its own folder: it never pushes"]);
    }
}

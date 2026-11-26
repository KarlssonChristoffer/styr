//! A bench suite read from TOML: the run's settings and its cases, each with the prompts sent in
//! order, the files its folder starts from and the hard checks the run must pass. Parsing is
//! strict: an unknown key or check word is an error that names the case and the key, so a typo
//! never becomes a check that always passes. The suites ship inside hubd (`BUILTIN`), and the
//! hash of a suite's text is its version. `lay_down` makes a case's folder; `attachments` are the
//! files a case sends in its first message instead.
//! Format and vocabulary: docs/architecture/bench.md.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use regex::Regex;
use sha2::{Digest, Sha256};
use toml::Value;

/// The suites baked into the binary, by name, so a hubd without a checkout has them.
/// The first is the default the app starts.
const BUILTIN: &[(&str, &str)] = &[("smoke", include_str!("../../bench/smoke.toml")), ("features", include_str!("../../bench/features.toml"))];

/// `model = "fast"`: the engine's cheapest model (`EngineModels.fast`). Any other value is a
/// model name, picked the way a new thread picks one.
pub const FAST: &str = "fast";

/// One suite. `hash` is its version: a changed prompt, fixture or check starts a new result series.
#[derive(Debug, Clone)]
pub struct Suite {
    pub name: String,
    /// An engine's name from the settings. Data: the code names no engine.
    pub engine: String,
    /// [`FAST`] or a model name.
    pub model: String,
    /// How many times each case runs.
    pub repeats: u32,
    /// Per prompt, until the thread has settled.
    pub timeout_s: u64,
    /// For the whole run.
    pub cost_cap_usd: f64,
    pub cases: Vec<Case>,
    pub hash: String,
}

/// One task: its prompts, the files it starts from and what must hold when it is done.
#[derive(Debug, Clone)]
pub struct Case {
    pub id: String,
    /// Sent in order, each once the thread has settled after the one before.
    pub prompts: Vec<String>,
    /// Words for the engine's mode (empty: the engine's default).
    pub mode: String,
    /// Path (relative, `/`-separated) to content, laid down by [`lay_down`].
    pub files: BTreeMap<String, String>,
    /// File name to text, sent with the first prompt as the person's attachments (never laid
    /// down in the folder): the agent can only answer from them as they came in the message.
    pub attach: BTreeMap<String, String>,
    /// What this machine must have for the case to say anything about Hub (words of [`NEEDS`]);
    /// without it every repeat is invalid, not failed, and nothing is sent (`run.rs`).
    pub needs: Vec<String>,
    pub checks: Vec<Check>,
}

/// What a proof check wants of the last `proof` event (`proof.rs`, `hub/after_turn.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProofWant {
    /// The last proof event says "shown".
    Shown,
    /// Hub asked for proof and none showed the code working ("failed", "missing" or "asked").
    Failed,
    /// No proof was asked: no proof event, or only a "skipped" one.
    NotAsked,
}

/// One hard check, read by `bench::check`. What each reads is in docs/architecture/bench.md.
#[derive(Debug, Clone)]
pub enum Check {
    /// `ended = "end_turn"`: the turn of each sent prompt ended with this stop reason.
    Ended(String),
    /// `no_error = true`: no `error` event.
    NoError,
    /// `settled = true`: no permission question or `ask_user` question left without an answer.
    Settled,
    /// `refused_max = N`: at most N permission questions Hub refused itself.
    RefusedMax(u64),
    /// `file = "x"` with optional `matches = "regex"` and `lines = N` (non-blank lines).
    File { path: String, matches: Option<Regex>, lines: Option<u64> },
    /// `unchanged = [..]`: each file still equals the case's fixture.
    Unchanged(Vec<String>),
    /// `reply = "regex"`: the last reply matches.
    Reply(Regex),
    /// `names = [..]`: the last reply names each file, and each is somewhere in the folder.
    Names(Vec<String>),
    /// `hub_tool = "name"` with `min = N` (default 1): Hub's tool called at least N times.
    HubTool { name: String, min: u64 },
    /// `proof = "shown" | "failed" | "none"`.
    Proof(ProofWant),
    /// `page = true`: the agent showed a page.
    Page,
    /// `steps = N`: the last steps list has N entries, all completed.
    Steps(u64),
    /// `commits = N`: exactly N commits after the fixture's.
    Commits(u64),
    /// `event = "kind"` with optional `matches = "regex"`: an event of that kind is in the log;
    /// with `matches`, one whose payload (as JSON text) matches.
    Logged { kind: crate::model::EventKind, name: String, matches: Option<Regex> },
}

/// The check words and the keys each may carry beside it.
const WORDS: &[(&str, &[&str])] = &[
    ("ended", &[]),
    ("no_error", &[]),
    ("settled", &[]),
    ("refused_max", &[]),
    ("file", &["matches", "lines"]),
    ("unchanged", &[]),
    ("reply", &[]),
    ("names", &[]),
    ("hub_tool", &["min"]),
    ("proof", &[]),
    ("page", &[]),
    ("steps", &[]),
    ("commits", &[]),
    ("event", &["matches"]),
];

const SUITE_KEYS: &[&str] = &["suite", "engine", "model", "repeats", "timeout_s", "cost_cap_usd", "case"];
const CASE_KEYS: &[&str] = &["id", "prompts", "mode", "files", "attach", "needs", "check"];

/// The words of a case's `needs`. `handoff_tool`: one of the person's own tools takes handoffs
/// (`own_tools::takes_handoffs`).
pub const NEEDS: &[&str] = &["handoff_tool"];

impl Check {
    /// The check as the suite wrote it, the name a result goes by: `file = "sum.js"`.
    pub fn label(&self) -> String {
        let list = |l: &[String]| l.iter().map(|s| format!("{s:?}")).collect::<Vec<_>>().join(", ");
        match self {
            Check::Ended(stop) => format!("ended = {stop:?}"),
            Check::NoError => "no_error".into(),
            Check::Settled => "settled".into(),
            Check::RefusedMax(n) => format!("refused_max = {n}"),
            Check::File { path, matches, lines } => {
                let mut s = format!("file = {path:?}");
                if let Some(re) = matches {
                    s.push_str(&format!(", matches = {:?}", re.as_str()));
                }
                if let Some(n) = lines {
                    s.push_str(&format!(", lines = {n}"));
                }
                s
            }
            Check::Unchanged(paths) => format!("unchanged = [{}]", list(paths)),
            Check::Reply(re) => format!("reply = {:?}", re.as_str()),
            Check::Names(names) => format!("names = [{}]", list(names)),
            Check::HubTool { name, min } => format!("hub_tool = {name:?}, min = {min}"),
            Check::Proof(want) => format!("proof = {:?}", match want {
                ProofWant::Shown => "shown",
                ProofWant::Failed => "failed",
                ProofWant::NotAsked => "none",
            }),
            Check::Page => "page".into(),
            Check::Steps(n) => format!("steps = {n}"),
            Check::Commits(n) => format!("commits = {n}"),
            Check::Logged { name, matches, .. } => match matches {
                Some(re) => format!("event = {name:?}, matches = {:?}", re.as_str()),
                None => format!("event = {name:?}"),
            },
        }
    }
}

/// The names of the suites baked in.
pub fn builtin_names() -> impl Iterator<Item = &'static str> {
    BUILTIN.iter().map(|(name, _)| *name)
}

/// The text of a suite baked into the binary.
pub fn builtin_text(name: &str) -> Option<&'static str> {
    BUILTIN.iter().find(|(n, _)| *n == name).map(|(_, text)| *text)
}

/// A suite baked into the binary, parsed.
pub fn builtin(name: &str) -> Result<Suite> {
    let (_, text) = BUILTIN.iter().find(|(n, _)| *n == name).ok_or_else(|| anyhow!("there is no suite called {name:?}"))?;
    parse(text).with_context(|| format!("the built-in suite {name:?}"))
}

/// The suite's text with Windows line ends made plain, so a checkout that turned them into CRLF
/// reads, and hashes, as the same suite.
fn plain(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// The version of a suite: 12 hex characters of the SHA-256 of its text (line ends made plain).
pub fn suite_hash(text: &str) -> String {
    let mut hex = format!("{:x}", Sha256::digest(plain(text).as_bytes()));
    hex.truncate(12);
    hex
}

/// Reads a suite. Every error says where: the suite key, or the case and its key.
pub fn parse(text: &str) -> Result<Suite> {
    let text = plain(text);
    let table: toml::Table = text.parse().context("the suite is not valid TOML")?;
    unknown_keys(&table, SUITE_KEYS, "the suite")?;
    let name = string(&table, "suite", "the suite")?;
    let engine = string(&table, "engine", "the suite")?;
    let model = string(&table, "model", "the suite")?;
    let repeats = count(&table, "repeats", "the suite")?;
    let timeout_s = count(&table, "timeout_s", "the suite")?;
    let cost_cap_usd = match table.get("cost_cap_usd") {
        Some(Value::Float(f)) if *f > 0.0 => *f,
        Some(Value::Integer(i)) if *i > 0 => *i as f64,
        Some(_) => bail!("the suite: `cost_cap_usd` must be a number above 0"),
        None => bail!("the suite: `cost_cap_usd` is missing"),
    };
    if repeats == 0 || timeout_s == 0 {
        bail!("the suite: `repeats` and `timeout_s` must be above 0");
    }
    let raw = match table.get("case") {
        Some(Value::Array(cases)) if !cases.is_empty() => cases,
        _ => bail!("the suite has no `[[case]]`"),
    };
    let mut cases: Vec<Case> = Vec::new();
    for (n, raw) in raw.iter().enumerate() {
        let Value::Table(raw) = raw else { bail!("case {}: not a table", n + 1) };
        let case = parse_case(raw, n)?;
        if cases.iter().any(|c| c.id == case.id) {
            bail!("case {:?}: the id is used twice", case.id);
        }
        cases.push(case);
    }
    Ok(Suite { name, engine, model, repeats: u32::try_from(repeats).unwrap_or(u32::MAX), timeout_s, cost_cap_usd, cases, hash: suite_hash(&text) })
}

fn parse_case(raw: &toml::Table, n: usize) -> Result<Case> {
    let id = match raw.get("id") {
        Some(Value::String(id)) if !id.trim().is_empty() => id.clone(),
        _ => bail!("case {}: `id` is missing", n + 1),
    };
    let at = format!("case {id:?}");
    unknown_keys(raw, CASE_KEYS, &at)?;
    let prompts = strings(raw, "prompts", &at)?;
    let mode = match raw.get("mode") {
        None => String::new(),
        Some(Value::String(m)) => m.clone(),
        Some(_) => bail!("{at}: `mode` must be a string"),
    };
    let files = file_table(raw, "files", &at)?;
    let attach = file_table(raw, "attach", &at)?;
    let needs = match raw.get("needs") {
        None => Vec::new(),
        Some(_) => strings(raw, "needs", &at)?,
    };
    if let Some(word) = needs.iter().find(|n| !NEEDS.contains(&n.as_str())) {
        bail!("{at}: needs: unknown word {word:?} (the words: {})", NEEDS.join(", "));
    }
    let raw_checks = match raw.get("check") {
        Some(Value::Array(c)) if !c.is_empty() => c,
        _ => bail!("{at}: no `[[case.check]]`"),
    };
    let mut checks = Vec::new();
    for (k, c) in raw_checks.iter().enumerate() {
        let Value::Table(c) = c else { bail!("{at}: check {} is not a table", k + 1) };
        let check = parse_check(c).with_context(|| format!("{at}: check {}", k + 1))?;
        if let Check::Unchanged(paths) = &check
            && let Some(p) = paths.iter().find(|p| !files.contains_key(*p))
        {
            bail!("{at}: unchanged: {p:?} is not one of the case's files");
        }
        checks.push(check);
    }
    Ok(Case { id, prompts, mode, files, attach, needs, checks })
}

/// A case's `files` or `attach`: relative path to text.
fn file_table(raw: &toml::Table, key: &str, at: &str) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    match raw.get(key) {
        None => {}
        Some(Value::Table(t)) => {
            for (path, content) in t {
                safe_path(path).with_context(|| format!("{at}: {key}"))?;
                let Value::String(content) = content else { bail!("{at}: {key}: {path:?} must be a string") };
                out.insert(path.clone(), content.clone());
            }
        }
        Some(_) => bail!("{at}: `{key}` must be a table"),
    }
    Ok(out)
}

fn parse_check(c: &toml::Table) -> Result<Check> {
    let words: Vec<&str> = c.keys().map(String::as_str).filter(|k| WORDS.iter().any(|(w, _)| w == k)).collect();
    let word = match words.as_slice() {
        [one] => *one,
        [] => bail!("no check word among {:?} (the words: {})", c.keys().collect::<Vec<_>>(), word_list()),
        many => bail!("one check word per `[[case.check]]`, got {many:?}"),
    };
    let allowed = WORDS.iter().find(|(w, _)| *w == word).map(|(_, extra)| *extra).unwrap_or_default();
    if let Some(k) = c.keys().find(|k| k.as_str() != word && !allowed.contains(&k.as_str())) {
        bail!("unknown key `{k}` beside `{word}` (it takes: {})", if allowed.is_empty() { "nothing more".into() } else { allowed.join(", ") });
    }
    let at = "the check";
    Ok(match word {
        "ended" => Check::Ended(string(c, word, at)?),
        "no_error" => yes(c, word).map(|_| Check::NoError)?,
        "settled" => yes(c, word).map(|_| Check::Settled)?,
        "refused_max" => Check::RefusedMax(count(c, word, at)?),
        "file" => {
            let path = string(c, word, at)?;
            safe_path(&path)?;
            let matches = c.get("matches").map(|_| regex(c, "matches")).transpose()?;
            let lines = c.get("lines").map(|_| count(c, "lines", at)).transpose()?;
            Check::File { path, matches, lines }
        }
        "unchanged" => Check::Unchanged(strings(c, word, at)?),
        "reply" => Check::Reply(regex(c, word)?),
        "names" => Check::Names(strings(c, word, at)?),
        "hub_tool" => Check::HubTool { name: string(c, word, at)?.to_lowercase(), min: c.get("min").map(|_| count(c, "min", at)).transpose()?.unwrap_or(1) },
        "proof" => Check::Proof(match string(c, word, at)?.as_str() {
            "shown" => ProofWant::Shown,
            "failed" => ProofWant::Failed,
            "none" => ProofWant::NotAsked,
            other => bail!("proof = {other:?}: say \"shown\", \"failed\" or \"none\""),
        }),
        "page" => yes(c, word).map(|_| Check::Page)?,
        "steps" => Check::Steps(count(c, word, at)?),
        "commits" => Check::Commits(count(c, word, at)?),
        "event" => {
            let name = string(c, word, at)?;
            let kind = serde_json::from_value::<crate::model::EventKind>(serde_json::Value::String(name.clone()))
                .ok()
                .filter(|k| *k != crate::model::EventKind::Unknown)
                .ok_or_else(|| anyhow!("event = {name:?} is not a kind of event"))?;
            let matches = c.get("matches").map(|_| regex(c, "matches")).transpose()?;
            Check::Logged { kind, name, matches }
        }
        other => bail!("the check word `{other}` has no reader"),
    })
}

fn word_list() -> String {
    WORDS.iter().map(|(w, _)| *w).collect::<Vec<_>>().join(", ")
}

fn unknown_keys(t: &toml::Table, known: &[&str], at: &str) -> Result<()> {
    match t.keys().find(|k| !known.contains(&k.as_str())) {
        Some(k) => bail!("{at}: unknown key `{k}` (known: {})", known.join(", ")),
        None => Ok(()),
    }
}

fn string(t: &toml::Table, key: &str, at: &str) -> Result<String> {
    match t.get(key) {
        Some(Value::String(s)) if !s.trim().is_empty() => Ok(s.clone()),
        Some(_) => bail!("{at}: `{key}` must be a string that says something"),
        None => bail!("{at}: `{key}` is missing"),
    }
}

fn strings(t: &toml::Table, key: &str, at: &str) -> Result<Vec<String>> {
    let Some(Value::Array(items)) = t.get(key) else { bail!("{at}: `{key}` must be a list of strings") };
    let out: Vec<String> = items.iter().filter_map(|v| v.as_str().filter(|s| !s.trim().is_empty()).map(str::to_owned)).collect();
    if out.is_empty() || out.len() != items.len() {
        bail!("{at}: `{key}` must be a list of strings that say something");
    }
    Ok(out)
}

fn count(t: &toml::Table, key: &str, at: &str) -> Result<u64> {
    match t.get(key) {
        Some(Value::Integer(i)) if *i >= 0 => Ok(*i as u64),
        Some(_) => bail!("{at}: `{key}` must be a whole number, 0 or more"),
        None => bail!("{at}: `{key}` is missing"),
    }
}

/// A word that only means something switched on: `no_error = true`.
fn yes(t: &toml::Table, key: &str) -> Result<()> {
    match t.get(key) {
        Some(Value::Boolean(true)) => Ok(()),
        _ => bail!("`{key}` only takes `true` (leave the check out instead)"),
    }
}

fn regex(t: &toml::Table, key: &str) -> Result<Regex> {
    let pattern = string(t, key, "the check")?;
    Regex::new(&pattern).with_context(|| format!("`{key}` is not a regular expression"))
}

/// A fixture or checked path as a relative path that stays inside the folder: no root, no drive,
/// no `..`, no `.`.
pub fn safe_path(path: &str) -> Result<PathBuf> {
    let p = Path::new(path);
    if path.trim().is_empty() || !p.components().all(|c| matches!(c, Component::Normal(_))) {
        bail!("{path:?} must be a relative path inside the folder (no root, drive or `..`)");
    }
    Ok(p.to_path_buf())
}

/// Lays a case's files down in `dir` (new or empty) and makes it a git repository with them as
/// its one commit, by Hub. Returns that commit. The checks count later commits from it.
pub fn lay_down(case: &Case, dir: &Path) -> Result<String> {
    if std::fs::read_dir(dir).is_ok_and(|mut d| d.next().is_some()) {
        bail!("{} is not empty; a case starts in a fresh folder", dir.display());
    }
    std::fs::create_dir_all(dir).with_context(|| format!("making {}", dir.display()))?;
    for (path, content) in &case.files {
        let to = dir.join(safe_path(path)?);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).with_context(|| format!("making {}", parent.display()))?;
        }
        std::fs::write(&to, content).with_context(|| format!("writing {}", to.display()))?;
    }
    crate::git::checked(dir, &["init", "-q"])?;
    crate::git::checked(dir, &["add", "-A"])?;
    // Hub's own author, as checkpoints; never the user's signing setup on a scratch folder.
    let message = format!("Bench fixture: {}", case.id);
    crate::checkpoint::run(dir, &["-c", "commit.gpgsign=false", "commit", "-q", "--allow-empty", "-m", &message], None, None)?;
    crate::git::checked(dir, &["rev-parse", "HEAD"])
}

/// A case's `attach` as the attachments of its first message: text files, as the app sends a text
/// file the person drops in.
pub fn attachments(case: &Case) -> Vec<crate::model::Attachment> {
    case.attach
        .iter()
        .map(|(name, text)| crate::model::Attachment { name: name.clone(), mime: "text/plain".into(), data: None, text: Some(text.clone()), path: None, url: None })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMALL: &str = r#"
suite = "t"
engine = "e"
model = "fast"
repeats = 1
timeout_s = 60
cost_cap_usd = 1

[[case]]
id = "one"
prompts = ["hi"]
[case.files]
"a.txt" = "a\n"
[[case.check]]
ended = "end_turn"
"#;

    fn with_check(check: &str) -> String {
        format!("{SMALL}[[case.check]]\n{check}\n")
    }

    #[test]
    fn a_small_suite_reads() -> Result<()> {
        let s = parse(SMALL)?;
        assert_eq!((s.name.as_str(), s.repeats, s.cases.len()), ("t", 1, 1));
        assert!(matches!(&s.cases[0].checks[..], [Check::Ended(stop)] if stop == "end_turn"));
        Ok(())
    }

    #[test]
    fn every_word_reads_and_labels_itself() -> Result<()> {
        for (check, label) in [
            ("no_error = true", "no_error"),
            ("settled = true", "settled"),
            ("refused_max = 0", "refused_max = 0"),
            ("file = \"x/y.txt\"\nmatches = 'a+'\nlines = 2", "file = \"x/y.txt\", matches = \"a+\", lines = 2"),
            ("unchanged = [\"a.txt\"]", "unchanged = [\"a.txt\"]"),
            ("reply = '7'", "reply = \"7\""),
            ("names = [\"a.txt\"]", "names = [\"a.txt\"]"),
            ("hub_tool = \"page_check\"", "hub_tool = \"page_check\", min = 1"),
            ("proof = \"none\"", "proof = \"none\""),
            ("page = true", "page"),
            ("steps = 4", "steps = 4"),
            ("commits = 1", "commits = 1"),
            ("event = \"child\"", "event = \"child\""),
            ("event = \"handoff_offer\"\nmatches = 'x'", "event = \"handoff_offer\", matches = \"x\""),
        ] {
            let s = parse(&with_check(check))?;
            assert_eq!(s.cases[0].checks[1].label(), label);
        }
        Ok(())
    }

    #[test]
    fn mistakes_are_refused_and_say_where() {
        for (check, says) in [
            ("endd = \"end_turn\"", "case \"one\""),
            ("file = \"x\"\nmatch = 'a'", "unknown key `match`"),
            ("no_error = false", "only takes `true`"),
            ("reply = '('", "not a regular expression"),
            ("proof = \"maybe\"", "shown"),
            ("unchanged = [\"b.txt\"]", "not one of the case's files"),
            ("file = \"../x\"", "relative path"),
            ("ended = \"end_turn\"\nreply = 'x'", "one check word"),
            ("event = \"no_such_kind\"", "not a kind of event"),
            ("event = \"unknown\"", "not a kind of event"),
            ("event = \"child\"\nmatches = '('", "not a regular expression"),
        ] {
            let err = format!("{:#}", parse(&with_check(check)).expect_err(check));
            assert!(err.contains(says), "{check}: {err}");
        }
        let err = format!("{:#}", parse(&SMALL.replace("prompts", "prompt")).expect_err("a typo in a case key"));
        assert!(err.contains("case \"one\"") && err.contains("`prompt`"), "{err}");
        let err = format!("{:#}", parse(&SMALL.replace("repeats", "repeat")).expect_err("a typo in a suite key"));
        assert!(err.contains("`repeat`"), "{err}");
    }

    #[test]
    fn attached_files_are_sent_not_laid_down() -> Result<()> {
        let s = parse(&SMALL.replace("[case.files]", "[case.attach]\n\"receipt.txt\" = \"Taxi 412.50\\n\"\n[case.files]"))?;
        let case = &s.cases[0];
        assert!(!case.files.contains_key("receipt.txt"));
        let sent = attachments(case);
        assert_eq!(sent.len(), 1);
        assert_eq!((sent[0].name.as_str(), sent[0].mime.as_str(), sent[0].text.as_deref()), ("receipt.txt", "text/plain", Some("Taxi 412.50\n")));
        let err = format!("{:#}", parse(&SMALL.replace("[case.files]", "[case.attach]\n\"../x.txt\" = \"x\"\n[case.files]")).expect_err("an attachment named outside"));
        assert!(err.contains("attach") && err.contains("relative path"), "{err}");
        Ok(())
    }

    #[test]
    fn needs_takes_known_words_only() -> Result<()> {
        let with = |needs: &str| parse(&SMALL.replace("[case.files]", &format!("needs = {needs}\n[case.files]")));
        assert_eq!(with("[\"handoff_tool\"]")?.cases[0].needs, ["handoff_tool"]);
        assert!(parse(SMALL)?.cases[0].needs.is_empty());
        let err = format!("{:#}", with("[\"handoff_tools\"]").expect_err("a typo in needs"));
        assert!(err.contains("case \"one\"") && err.contains("handoff_tools") && err.contains("handoff_tool"), "{err}");
        assert!(with("\"handoff_tool\"").is_err(), "needs is a list");
        Ok(())
    }

    #[test]
    fn paths_stay_inside() {
        for bad in ["../x", "a/../../x", "/etc/passwd", "", "./a", "C:/x"] {
            if cfg!(not(windows)) && bad == "C:/x" {
                continue;
            }
            assert!(safe_path(bad).is_err(), "{bad}");
        }
        assert!(safe_path("notes/inventory.md").is_ok());
    }

    #[test]
    fn the_hash_ignores_line_ends_only() {
        assert_eq!(suite_hash("a\r\nb\n"), suite_hash("a\nb\n"));
        assert_ne!(suite_hash("a\nb\n"), suite_hash("a\nc\n"));
        assert_eq!(suite_hash("x").len(), 12);
    }
}

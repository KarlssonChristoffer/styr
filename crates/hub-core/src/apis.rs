//! API connections: outside services Hub reaches with a key you add by hand, such as Jev, the
//! decision model from TypeSafe. They are the one place Hub uses API keys: engines stay on your
//! subscriptions, so a connection is never an engine and never carries a model provider's own
//! key (`ENGINE_KEYS`).
//!
//! Adding one is the same for every service: pick it from the catalog (or describe a custom one),
//! paste the key once (it goes to the vault, `secrets.rs`), and test it. Then it is used in two
//! ways. Hub's own features ask it questions (`Hub::ask_api`), and every call is counted per day.
//! And a project can turn it on, so the threads in that folder get the key and the address as
//! environment variables for the code they write and run, and `api_connections` tells the agent
//! what it has.
//!
//! Connections, the projects they are on and the counts are kept in the store; keys only in the
//! vault.

use crate::locked::{LockExt, ResultExt};
use std::collections::BTreeMap;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ts_rs::TS;

use crate::activity::date_of;
use crate::folders::same_folder;
use crate::hub::Hub;
use crate::model::now_ms;

const PREFS: &str = "apis.prefs";
const USAGE: &str = "apis.usage";
pub(crate) const DECISIONS: &str = "apis.decisions";
const CHECKS: &str = "apis.checks";
/// Calls and tokens per connection since Hub started counting, kept for good (`ApiStatus.total`).
const TOTALS: &str = "apis.totals";
/// The whole of what the connection was asked and shown for each kept decision (`Seen`), apart
/// from the decisions so the status bar's list stays small. Read by `jev_tune`.
pub(crate) const SEEN: &str = "apis.seen";
/// How many of Hub's own decisions are kept to look back at.
const KEEP_DECISIONS: usize = 300;
/// Days of counts kept.
const KEEP_DAYS: usize = 62;
/// How a test says the key was refused, which is when Hub tries the other ways to send it.
const REFUSED: &str = "The key was refused";
/// Threads whose agents may not ask the decision model (`ask_jev`), as a list of ids.
pub(crate) const AGENT_OFF: &str = "apis.agent_off";
/// What an agent's `ask_jev` may take in a day, in tokens, across threads (about $0.008), and how
/// many calls one thread may make. Hub's own features have the connection's `daily_tokens` instead.
const AGENT_DAY_TOKENS: u64 = 200_000;
const AGENT_THREAD_CALLS: u32 = 50;
/// How long an agent's question waits.
const AGENT_WAIT: Duration = Duration::from_secs(4);
/// The most items one `ask_jev` call sorts.
const AGENT_ITEMS: usize = 20;
/// What agents asked today (date, tokens) and how many times each thread asked. Kept in memory: a
/// restart gives a fresh allowance, and the connection's own daily cap still applies.
static AGENT_USE: LazyLock<std::sync::Mutex<(String, u64, BTreeMap<String, u32>)>> = LazyLock::new(Default::default);

/// What a test or a question you try by hand waits.
const HAND_WAIT: Duration = Duration::from_secs(20);

/// Keys that would move an engine off your subscription (`config::ENGINE_KEYS`). A connection
/// never holds one, so turning it on for a project can't do that.
pub use crate::config::ENGINE_KEYS;

pub(crate) use crate::http::CLIENT;
/// Read-change-write of the counts and the decisions, one at a time.
static WRITE: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(Default::default);
/// The last error per connection, since the core started.
static ERRORS: LazyLock<std::sync::Mutex<BTreeMap<String, (String, i64)>>> = LazyLock::new(Default::default);

fn valid(c: &ApiConnection) -> Result<()> {
    if c.name.is_empty() || !c.name.chars().all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-') {
        bail!("a connection's name is lowercase letters, digits and -, like jev");
    }
    if c.secret.is_empty() || !c.secret.chars().all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_') {
        bail!("{}: the key's name is like an environment variable, e.g. TYPESAFE_API_KEY", c.name);
    }
    if ENGINE_KEYS.contains(&c.secret.as_str()) {
        bail!(
            "{} is how an engine would pay per token instead of using your subscription; Hub keeps engines on subscriptions",
            c.secret
        );
    }
    if !(c.url.starts_with("https://") || c.url.starts_with("http://")) {
        bail!("{}: the address starts with https://", c.name);
    }
    Ok(())
}

/// The variable that carries a connection's address, next to its key: TYPESAFE_API_KEY →
/// TYPESAFE_BASE_URL.
pub fn url_var(secret: &str) -> String {
    let base = secret.strip_suffix("_API_KEY").or_else(|| secret.strip_suffix("_KEY")).or_else(|| secret.strip_suffix("_TOKEN")).unwrap_or(secret);
    format!("{base}_BASE_URL")
}

use crate::text::clip_line as clip;

impl Hub {
    pub async fn api_prefs(&self) -> Result<ApiPrefs> {
        Ok(match self.store.get_value(PREFS).await? {
            Some(json) => serde_json::from_str(&json).unwrap_or_default(),
            None => ApiPrefs::default(),
        })
    }

    pub async fn set_api_prefs(&self, mut prefs: ApiPrefs) -> Result<ApiPrefs> {
        let mut seen = std::collections::BTreeSet::new();
        for c in &mut prefs.connections {
            c.url = c.url.trim().trim_end_matches('/').to_owned();
            c.name = c.name.trim().to_owned();
            valid(c)?;
            if !seen.insert(c.name.clone()) {
                bail!("two connections are called {}", c.name);
            }
            c.projects.dedup_by(|a, b| same_folder(a, b));
        }
        if let Some(name) = &prefs.helper
            && !prefs.connections.iter().any(|c| &c.name == name && self.asks(c))
        {
            bail!("{name} can't answer Hub's questions");
        }
        prefs.off.retain(|f| FEATURES.iter().any(|(name, _)| name == f));
        prefs.sure.retain(|f, v| FEATURES.iter().any(|(name, _)| name == f) && v.is_finite());
        for v in prefs.sure.values_mut() {
            *v = v.clamp(0.5, 0.99);
        }
        prefs.off.sort();
        prefs.off.dedup();
        let before = self.api_prefs().await?;
        let usage = self.api_usage().await?;
        let totals = self.api_totals().await?;
        for c in &mut prefs.connections {
            let Some(credit) = &mut c.credit else { continue };
            if !credit.usd.is_finite() || credit.usd < 0.0 {
                bail!("{}'s credit has to be zero or more dollars", c.label);
            }
            let kept = before.connections.iter().find(|b| b.name == c.name).and_then(|b| b.credit.clone());
            match kept {
                Some(old) if old.usd == credit.usd => *credit = old,
                _ => {
                    credit.tokens_at = total_of(&usage, &totals, &c.name).input_tokens;
                    credit.at = now_ms();
                }
            }
        }
        self.store.set_value(PREFS, &serde_json::to_string(&prefs)?).await?;
        Ok(prefs)
    }

    fn asks(&self, c: &ApiConnection) -> bool {
        catalog().iter().any(|k| k.kind == c.kind && k.asks)
    }

    pub async fn api_state(&self) -> Result<ApiState> {
        let prefs = self.api_prefs().await?;
        let keys: BTreeMap<String, String> = self.secrets.list().await?.into_iter().map(|s| (s.name, s.hint)).collect();
        let usage = self.api_usage().await?;
        let today = date_of(now_ms(), 0);
        let errors = ERRORS.locked().clone();
        let checks = self.api_checks().await?;
        let totals = self.api_totals().await?;
        let status = prefs
            .connections
            .iter()
            .map(|c| {
                let days = usage.get(&c.name);
                let mut month = ApiUsage::default();
                for (day, u) in days.into_iter().flatten() {
                    if day[..7] == today[..7] {
                        month.calls += u.calls;
                        month.input_tokens += u.input_tokens;
                        month.output_tokens += u.output_tokens;
                        month.errors += u.errors;
                    }
                }
                let error = errors.get(&c.name);
                ApiStatus {
                    name: c.name.clone(),
                    key: keys.get(&c.secret).cloned(),
                    today: days.and_then(|d| d.get(&today)).cloned().unwrap_or_default(),
                    month,
                    total: Some(total_of(&usage, &totals, &c.name)),
                    last_error: error.map(|e| e.0.clone()),
                    last_error_at: error.map(|e| e.1),
                    check: checks.get(&c.name).cloned(),
                }
            })
            .collect();
        let features = FEATURES.iter().map(|(name, what)| ApiFeature { name: name.to_string(), what: what.to_string() }).collect();
        Ok(ApiState { prefs, catalog: catalog(), features, status, redirect: self.redirect_uri() })
    }

    async fn api_checks(&self) -> Result<BTreeMap<String, ApiCheck>> {
        Ok(match self.store.get_value(CHECKS).await? {
            Some(json) => serde_json::from_str(&json).unwrap_or_default(),
            None => BTreeMap::new(),
        })
    }

    pub(crate) async fn note_check(&self, name: &str, ok: bool, message: &str) {
        let _one = WRITE.lock().await;
        let Ok(mut checks) = self.api_checks().await else { return };
        checks.insert(name.to_owned(), ApiCheck { ok, message: clip(message, 300), at: now_ms() });
        if let Ok(json) = serde_json::to_string(&checks) {
            self.store.set_value(CHECKS, &json).await.warn_on_err("save CHECKS");
        }
    }

    async fn api_usage(&self) -> Result<BTreeMap<String, BTreeMap<String, ApiUsage>>> {
        Ok(match self.store.get_value(USAGE).await? {
            Some(json) => serde_json::from_str(&json).unwrap_or_default(),
            None => BTreeMap::new(),
        })
    }

    async fn api_totals(&self) -> Result<BTreeMap<String, ApiUsage>> {
        Ok(match self.store.get_value(TOTALS).await? {
            Some(json) => serde_json::from_str(&json).unwrap_or_default(),
            None => BTreeMap::new(),
        })
    }

    pub(crate) async fn count(&self, name: &str, answer: Option<&ApiAnswer>, error: Option<&str>) {
        if let Some(error) = error {
            ERRORS.locked().insert(name.to_owned(), (clip(error, 300), now_ms()));
        }
        let _one = WRITE.lock().await;
        let Ok(mut usage) = self.api_usage().await else { return };
        let Ok(mut totals) = self.api_totals().await else { return };
        let mut total = total_of(&usage, &totals, name);
        total.calls += 1;
        if let Some(a) = answer {
            total.input_tokens += a.input_tokens;
            total.output_tokens += a.output_tokens;
        }
        if error.is_some() {
            total.errors += 1;
        }
        totals.insert(name.to_owned(), total);
        if let Ok(json) = serde_json::to_string(&totals) {
            self.store.set_value(TOTALS, &json).await.warn_on_err("save TOTALS");
        }
        let days = usage.entry(name.to_owned()).or_default();
        let day = days.entry(date_of(now_ms(), 0)).or_default();
        day.calls += 1;
        if let Some(a) = answer {
            day.input_tokens += a.input_tokens;
            day.output_tokens += a.output_tokens;
        }
        if error.is_some() {
            day.errors += 1;
        }
        while days.len() > KEEP_DAYS {
            days.pop_first();
        }
        if let Ok(json) = serde_json::to_string(&usage) {
            self.store.set_value(USAGE, &json).await.warn_on_err("save USAGE");
        }
    }

    pub(crate) async fn connection(&self, name: &str) -> Result<(ApiConnection, String)> {
        let prefs = self.api_prefs().await?;
        let c = prefs.connections.into_iter().find(|c| c.name == name).with_context(|| format!("no connection called {name}"))?;
        let key = self.secrets.env(std::slice::from_ref(&c.secret)).await?.pop().map(|(_, v)| v);
        let key = key.with_context(|| match c.auth {
            ApiAuth::Login { .. } => format!("{} isn't logged in yet. Press Allow on its page in Settings > API connections.", c.label),
            _ => format!("{name} has no key yet; add {} first", c.secret),
        })?;
        Ok((c, key))
    }

    /// Checks a connection's key with its cheap GET. A connection you added yourself whose key
    /// is refused as a Bearer token is tried the other usual ways, and kept the way that works
    /// (`gateway::probe`), so you never pick how a key is sent.
    pub async fn test_api(&self, name: &str) -> Result<ApiTest> {
        let (c, key) = self.connection(name).await?;
        let started = Instant::now();
        let mut test = match self.signing(&c, &key).await {
            Ok(signing) => self.try_key(&c, &signing).await,
            Err(err) => ApiTest { ok: false, message: format!("{err:#}"), ms: 0, models: Vec::new() },
        };
        if !test.ok
            && test.message.starts_with(REFUSED)
            && c.kind == "custom"
            && c.auth == ApiAuth::Bearer
            && let Some((auth, found)) = self.probe(&c, &key).await
        {
            let mut prefs = self.api_prefs().await?;
            if let Some(x) = prefs.connections.iter_mut().find(|x| x.name == c.name) {
                x.auth = auth.clone();
            }
            self.set_api_prefs(prefs).await?;
            test = ApiTest { message: format!("{} Hub found that {} wants it {}.", found.message, c.label, auth.says()), ..found };
        }
        test.ms = started.elapsed().as_millis() as u32;
        if !test.ok {
            ERRORS.locked().insert(c.name.clone(), (test.message.clone(), now_ms()));
        } else {
            ERRORS.locked().remove(&c.name);
        }
        self.note_check(&c.name, test.ok, &test.message).await;
        Ok(test)
    }

    /// One GET of the test path, signed one way.
    pub(crate) async fn try_key(&self, c: &ApiConnection, signing: &Signing) -> ApiTest {
        let path = if c.test_path.is_empty() { "/" } else { &c.test_path };
        let started = Instant::now();
        let sent = signing.apply(CLIENT.get(format!("{}{path}", c.url))).timeout(HAND_WAIT).send().await;
        let ms = started.elapsed().as_millis() as u32;
        match sent {
            Err(err) => ApiTest { ok: false, message: format!("No answer from {}: {}", c.url, err.without_url()), ms, models: Vec::new() },
            Ok(r) if r.status() == 401 || r.status() == 403 => {
                ApiTest { ok: false, message: format!("{REFUSED} ({}).", r.status()), ms, models: Vec::new() }
            }
            Ok(r) if !r.status().is_success() => {
                ApiTest { ok: false, message: format!("{} answered {}.", c.url, r.status()), ms, models: Vec::new() }
            }
            Ok(r) => {
                let body: Value = r.json().await.unwrap_or_default();
                let models: Vec<String> = body["models"]
                    .as_array()
                    .or(body["data"].as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|m| m["name"].as_str().or(m["id"].as_str()).map(str::to_owned))
                    .collect();
                // An account's own page says whose it is.
                let who = ["display_name", "login", "username", "email"].iter().find_map(|k| body[*k].as_str().filter(|s| !s.is_empty()));
                let message = match who {
                    Some(who) => format!("Works, as {who}."),
                    None => "The key works.".into(),
                };
                ApiTest { ok: true, message, ms, models }
            }
        }
    }

    /// Asks a connection typed questions about `state` (TypeSafe's `systemone`), counting the
    /// call. Refused once the day's tokens are spent.
    pub async fn ask_api(&self, name: &str, state: &str, questions: Value, wait: Duration) -> Result<ApiAnswer> {
        let (c, key) = self.connection(name).await?;
        if !self.asks(&c) {
            bail!("Hub can't ask {} questions", c.label);
        }
        if c.daily_tokens > 0 {
            let used = self.api_usage().await?.get(name).and_then(|d| d.get(&date_of(now_ms(), 0)).cloned()).unwrap_or_default();
            if used.input_tokens + used.output_tokens >= c.daily_tokens {
                bail!("{} has used today's {} tokens", c.label, c.daily_tokens);
            }
        }
        let signing = self.signing(&c, &key).await?;
        let body = json!({ "state": state, "model": c.model.as_deref().unwrap_or("jev-latest"), "questions": questions });
        let started = Instant::now();
        let mut tries = 0;
        let answered = loop {
            tries += 1;
            let sent = signing.apply(CLIENT.post(format!("{}/v1/systemone", c.url))).json(&body).timeout(wait).send().await;
            match sent {
                // Busy or rate limited: once more after a moment, if there is time.
                Ok(r) if (r.status() == 429 || r.status().as_u16() == 529) && tries < 2 && started.elapsed() < wait / 2 => {
                    tokio::time::sleep(Duration::from_millis(400)).await;
                }
                Ok(r) if r.status().is_success() => break r.json::<Value>().await.map_err(anyhow::Error::from),
                Ok(r) => {
                    let status = r.status();
                    let text = r.text().await.unwrap_or_default();
                    break Err(anyhow::anyhow!("{} answered {status}: {}", c.label, clip(&text, 200)));
                }
                Err(err) => break Err(anyhow::anyhow!("no answer from {}: {}", c.label, err.without_url())),
            }
        };
        let ms = started.elapsed().as_millis() as u32;
        match answered {
            Ok(v) => {
                let answer = ApiAnswer {
                    model: v["model"].as_str().unwrap_or_default().to_owned(),
                    answers: v["answers"].clone(),
                    ms,
                    input_tokens: v["usage"]["input_tokens"].as_u64().unwrap_or(0),
                    output_tokens: v["usage"]["output_tokens"].as_u64().unwrap_or(0),
                };
                self.count(name, Some(&answer), None).await;
                self.note_check(name, true, "Answered a question.").await;
                Ok(answer)
            }
            Err(err) => {
                self.count(name, None, Some(&format!("{err:#}"))).await;
                self.note_check(name, false, &format!("{err:#}")).await;
                Err(err)
            }
        }
    }

    /// One yes/no question you try by hand from Settings.
    pub async fn try_api(&self, name: &str, ask: &ApiTry) -> Result<ApiAnswer> {
        if ask.state.trim().is_empty() || ask.statement.trim().is_empty() {
            bail!("a text and a statement about it are needed");
        }
        let questions = json!({ "check": { "type": "noul", "instructions": ask.statement.trim() } });
        self.ask_api(name, ask.state.trim(), questions, HAND_WAIT).await
    }

    pub async fn api_decisions(&self) -> Result<Vec<ApiDecision>> {
        Ok(match self.store.get_value(DECISIONS).await? {
            Some(json) => serde_json::from_str(&json).unwrap_or_default(),
            None => Vec::new(),
        })
    }

    async fn note_decision(&self, decision: ApiDecision) -> Result<()> {
        let _one = WRITE.lock().await;
        let mut all = self.api_decisions().await?;
        all.insert(0, decision);
        all.truncate(KEEP_DECISIONS);
        self.store.set_value(DECISIONS, &serde_json::to_string(&all)?).await
    }

    pub(crate) async fn api_seen(&self) -> Result<Vec<Seen>> {
        Ok(match self.store.get_value(SEEN).await? {
            Some(json) => serde_json::from_str(&json).unwrap_or_default(),
            None => Vec::new(),
        })
    }

    async fn note_seen(&self, seen: Seen) -> Result<()> {
        let _one = WRITE.lock().await;
        let mut all = self.api_seen().await?;
        all.insert(0, seen);
        all.truncate(KEEP_DECISIONS);
        self.store.set_value(SEEN, &serde_json::to_string(&all)?).await
    }

    /// Whether agents of `thread` may ask the decision model: the `agent` feature is on and the
    /// thread is not in the off list.
    pub async fn agent_jev_on(&self, thread: &str) -> bool {
        self.helper("agent").await.is_some() && !self.agent_off().await.iter().any(|t| t == thread)
    }

    async fn agent_off(&self) -> Vec<String> {
        match self.store.get_value(AGENT_OFF).await {
            Ok(Some(json)) => serde_json::from_str(&json).unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    /// Turns the decision model on or off for one thread's agents.
    pub async fn set_agent_jev(&self, thread: &str, on: bool) -> Result<()> {
        let _one = WRITE.lock().await;
        let mut off = self.agent_off().await;
        off.retain(|t| t != thread);
        if !on {
            off.push(thread.to_owned());
        }
        self.store.set_value(AGENT_OFF, &serde_json::to_string(&off)?).await
    }

    /// `ask_jev`: a thread's agent asks one choice question about `text`, or a list of up to 20
    /// items the same question. Never an error the agent can't carry on from: when Jev is off,
    /// slow, unsure or the allowance is spent, the answer says so and the agent decides itself.
    pub async fn ask_for_agent(&self, thread: &str, question: &str, text: &str, options: Vec<(String, String)>, items: Vec<String>) -> Value {
        let off = |why: &str| json!({ "answered": false, "why": why, "note": "Decide yourself." });
        if self.helper("agent").await.is_none() {
            return off("Jev is off or not connected");
        }
        if self.agent_off().await.iter().any(|t| t == thread) {
            return off("Jev is off in this chat");
        }
        let yes_no = options.is_empty();
        let options = if yes_no {
            vec![("yes".to_owned(), "Yes".to_owned()), ("no".to_owned(), "No".to_owned())]
        } else {
            options
        };
        let bulk = !items.is_empty();
        let texts: Vec<String> = if items.is_empty() { vec![text.to_owned()] } else { items.into_iter().take(AGENT_ITEMS).collect() };
        // The allowance: a day's tokens across threads, a number of calls per thread.
        let today = date_of(now_ms(), 0);
        {
            let mut u = AGENT_USE.locked();
            if u.0 != today {
                *u = (today.clone(), 0, BTreeMap::new());
            }
            let calls = u.2.get(thread).copied().unwrap_or(0);
            if u.1 >= AGENT_DAY_TOKENS {
                return off("the day's allowance for agents is spent");
            }
            if calls + texts.len() as u32 > AGENT_THREAD_CALLS {
                return off("this thread has used its Jev calls");
            }
            *u.2.entry(thread.to_owned()).or_default() += texts.len() as u32;
        }
        let instructions = question.trim().to_owned();
        let asked = texts.iter().map(|t| {
            let state = if t.is_empty() { text.to_owned() } else if bulk { format!("{text}\n\n{t}") } else { t.clone() };
            let (instructions, options) = (&instructions, &options);
            async move { self.choose("agent", Some(thread), instructions, &state, &state, options, "", 0.6, AGENT_WAIT).await }
        });
        let answers: Vec<Option<crate::apis::Choice>> = futures::future::join_all(asked).await;
        // Chars read stand in for tokens when a call took none we can see; the decision log has the real count.
        let spent: u64 = texts.iter().map(|t| (t.chars().count() as u64 + text.chars().count() as u64) / 4 + 200).sum();
        AGENT_USE.locked().1 += spent;
        let one = |c: &Option<Choice>| match c {
            None => json!({ "answered": false, "why": "Jev did not answer", "note": "Decide yourself." }),
            Some(c) if yes_no => json!({
                "answered": true, "answer": c.pick == "yes", "confidence": c.confidence, "sure": c.acted,
            }),
            Some(c) => json!({ "answered": true, "pick": c.pick, "confidence": c.confidence, "sure": c.acted }),
        };
        let left = AGENT_DAY_TOKENS.saturating_sub(AGENT_USE.locked().1);
        if !bulk {
            let mut v = one(&answers[0]);
            v["tokens_left_today"] = json!(left);
            return v;
        }
        json!({ "answers": answers.iter().map(one).collect::<Vec<_>>(), "tokens_left_today": left })
    }

    /// The connection that answers `feature` (one of `FEATURES`), if it is on and has its key.
    pub(crate) async fn helper(&self, feature: &str) -> Option<String> {
        let prefs = self.api_prefs().await.ok()?;
        if prefs.off.iter().any(|f| f == feature) {
            return None;
        }
        let keys: Vec<String> = self.secrets.list().await.ok()?.into_iter().map(|s| s.name).collect();
        prefs
            .connections
            .iter()
            .filter(|c| prefs.helper.as_ref().is_none_or(|h| h == &c.name))
            .find(|c| self.asks(c) && keys.contains(&c.secret))
            .map(|c| c.name.clone())
    }

    /// Asks the connection for `feature` one choice question about `text`: which of `options`
    /// (name, meaning) fits. The pick is to be gone by when its confidence reaches `sure`, or the
    /// threshold you set for the feature (`ApiPrefs::sure`). Keeps the answer, with `quote` (the
    /// words read, in short), next to what Hub would have done without it (`hub_said`). None when
    /// there is no connection for it or no answer in `wait`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn choose(
        &self,
        feature: &str,
        thread: Option<&str>,
        instructions: &str,
        text: &str,
        quote: &str,
        options: &[(String, String)],
        hub_said: &str,
        sure: f64,
        wait: Duration,
    ) -> Option<Choice> {
        let name = self.helper(feature).await?;
        let sure = self.api_prefs().await.ok().and_then(|p| p.sure.get(feature).copied()).unwrap_or(sure);
        let questions = json!({
            "pick": {
                "type": "choice",
                "instructions": instructions,
                "criteria": options.iter().map(|(k, v)| (k.clone(), json!(v))).collect::<serde_json::Map<_, _>>(),
            }
        });
        let state = clip(text, 6_000);
        match self.ask_api(&name, &state, questions, wait).await {
            Ok(a) => {
                let said = a.answers["pick"]["choice"].as_str().unwrap_or_default().to_owned();
                let confidence = a.answers["pick"]["confidence"].as_f64().unwrap_or(0.0);
                let acted = !said.is_empty() && confidence >= sure;
                let decision = ApiDecision {
                    at: now_ms(),
                    feature: feature.into(),
                    connection: name.clone(),
                    thread: thread.map(str::to_owned),
                    text: clip(text, 140),
                    hub: hub_said.into(),
                    agree: said == hub_said,
                    said: said.clone(),
                    confidence: Some(confidence),
                    ms: a.ms,
                    acted,
                    quote: Some(clip(quote.split_whitespace().collect::<Vec<_>>().join(" ").trim(), 160)).filter(|q| !q.is_empty()),
                    tokens: Some(a.input_tokens + a.output_tokens),
                };
                let seen = Seen {
                    at: decision.at,
                    feature: feature.into(),
                    thread: thread.map(str::to_owned),
                    instructions: instructions.into(),
                    options: options.to_vec(),
                    state: state.clone(),
                    sure,
                };
                if let Err(err) = self.note_decision(decision).await {
                    tracing::warn!("keeping a {feature} decision: {err:#}");
                }
                if let Err(err) = self.note_seen(seen).await {
                    tracing::warn!("keeping what a {feature} decision read: {err:#}");
                }
                Some(Choice { connection: name, pick: said, confidence, acted, ms: a.ms })
            }
            Err(err) => {
                tracing::info!("{name} did not answer for {feature}: {err:#}");
                None
            }
        }
    }

    /// The keys and addresses of the connections turned on for a project, for its threads'
    /// environment.
    pub async fn api_env(&self, folder: &str) -> Vec<(String, String)> {
        let Ok(prefs) = self.api_prefs().await else { return Vec::new() };
        let on: Vec<ApiConnection> =
            prefs.connections.into_iter().filter(|c| c.projects.iter().any(|p| same_folder(p, folder))).collect();
        // A key sent any other way than Bearer stays with Hub: the thread gets the gateway's
        // address, which signs each call.
        let names: Vec<String> = on.iter().filter(|c| !c.auth.gateway()).map(|c| c.secret.clone()).collect();
        let mut env = self.secrets.env(&names).await.unwrap_or_default();
        env.retain(|(n, _)| !ENGINE_KEYS.contains(&n.as_str()));
        for c in &on {
            if c.auth.gateway() {
                match self.gateway_url(&c.name, folder).await {
                    Ok(url) => env.push((gateway_url_var(&c.name), url)),
                    Err(err) => tracing::warn!("no gateway address for {}: {err:#}", c.name),
                }
            } else if env.iter().any(|(n, _)| n == &c.secret) {
                env.push((url_var(&c.secret), c.url.clone()));
            }
        }
        env
    }

    /// What a project's agent is told about the connections it has (`api_connections`).
    pub async fn api_brief(&self, folder: &str) -> Result<Value> {
        let prefs = self.api_prefs().await?;
        let keys: Vec<String> = self.secrets.list().await?.into_iter().map(|s| s.name).collect();
        let on: Vec<Value> = prefs
            .connections
            .iter()
            .filter(|c| c.projects.iter().any(|p| same_folder(p, folder)))
            .map(|c| {
                let what = catalog().into_iter().find(|k| k.kind == c.kind).map(|k| k.what);
                if c.auth.gateway() {
                    return json!({
                        "name": c.label,
                        "what": what,
                        "url_env": gateway_url_var(&c.name),
                        "calls": c.url,
                        "docs": c.docs,
                        "auth": "none: call the address in url_env with the API's own paths; Hub signs each call and \
                                 keeps the login fresh, so send no key or Authorization header",
                        "key_set": keys.contains(&c.secret),
                    });
                }
                json!({
                    "name": c.label,
                    "what": what,
                    "key_env": c.secret,
                    "url_env": url_var(&c.secret),
                    "url": c.url,
                    "model": c.model,
                    "docs": c.docs,
                    "auth": "Authorization: Bearer <key>",
                    "key_set": keys.contains(&c.secret),
                })
            })
            .collect();
        let others: Vec<&str> =
            prefs.connections.iter().filter(|c| !c.projects.iter().any(|p| same_folder(p, folder))).map(|c| c.label.as_str()).collect();
        Ok(json!({
            "connections": on,
            "not_on_here": others,
            "note": "Your environment has each key and address under these names. Read them from the environment; \
                     never write a key into a file, a commit or a reply. Restart the thread to pick up a connection \
                     turned on after it started. Ask the user to turn on one listed under not_on_here in the project page.",
        }))
    }
}

mod types;
pub use types::*;

mod vault;

#[cfg(test)]
mod tests;

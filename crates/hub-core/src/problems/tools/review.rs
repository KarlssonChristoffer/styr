//! Vakten's review of the tools that fail most: which tools pass the failure threshold on this and
//! every online machine (with the earlier days for a trend), the brief Vakten is handed, and its
//! verdicts per tool, kept as the latest `ToolReview` for the Tools tab.

use super::*;

/// A tool fails often enough to be worth Vakten's look: this many failures in the review's days,
pub(super) const REVIEW_FAILED: i64 = 5;
/// or this many and at least this share of its calls.
pub(super) const REVIEW_SOME: i64 = 3;
pub(super) const REVIEW_RATE: f64 = 0.10;
/// The days a review looks at.
pub const REVIEW_DAYS: u32 = 7;
/// Tools handed to one review at most, most failures first.
pub(super) const REVIEW_MAX: usize = 12;
/// Where the latest review is kept.
pub(super) const REVIEW: &str = "problems.tools.review";

/// A tool that failed often enough for the review, on one machine.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub machine: Option<String>,
    pub rate: ToolRate,
    /// Failures in the days before the review's, for the trend.
    pub before: i64,
}

/// Whether a tool's failures pass the review's threshold.
pub fn worth_reviewing(failed: i64, calls: i64) -> bool {
    failed >= REVIEW_FAILED || (failed >= REVIEW_SOME && calls > 0 && failed as f64 / calls as f64 >= REVIEW_RATE)
}

/// The tools worth a review on one machine: `now` over the review's days, `then` over twice that.
pub fn candidates(machine: Option<&str>, now: &ToolStats, then: &ToolStats) -> Vec<Candidate> {
    now.tools
        .iter()
        .filter(|t| worth_reviewing(t.failed, t.calls))
        .map(|t| {
            let all = then.tools.iter().find(|o| o.engine == t.engine && o.tool == t.tool).map(|o| o.failed).unwrap_or(t.failed);
            Candidate { machine: machine.map(str::to_owned), rate: t.clone(), before: (all - t.failed).max(0) }
        })
        .collect()
}

/// Every machine's tools worth a review: this one's and each online machine's, most failures first.
pub async fn review_candidates(hub: &Hub) -> Vec<Candidate> {
    let mut out = Vec::new();
    if let (Ok(now), Ok(then)) = (stats(hub, REVIEW_DAYS).await, stats(hub, REVIEW_DAYS * 2).await) {
        out.extend(candidates(None, &now, &then));
    }
    for host in hub.remotes.online() {
        let now = hub.remotes.get_from::<ToolStats>(&host, &format!("/api/problems/tools?days={REVIEW_DAYS}")).await;
        let then = hub.remotes.get_from::<ToolStats>(&host, &format!("/api/problems/tools?days={}", REVIEW_DAYS * 2)).await;
        if let (Ok(now), Ok(then)) = (now, then) {
            out.extend(candidates(Some(&host), &now, &then));
        }
    }
    out.sort_by_key(|x| std::cmp::Reverse(x.rate.failed));
    out.truncate(REVIEW_MAX);
    out
}

/// What Vakten is handed for its review of the tools that fail most.
pub fn review_brief(found: &[Candidate]) -> String {
    let mut text = format!(
        "\nTool-call review: these agent tools failed often in the last {REVIEW_DAYS} days (at least {REVIEW_FAILED} failures, or \
{REVIEW_SOME} and {}% of their calls). Their problems (source tool) are archived until they get worse, so nobody has looked yet.\n",
        (REVIEW_RATE * 100.0) as i64
    );
    for c in found {
        let r = &c.rate;
        let on = c.machine.as_deref().map(|m| format!(" on {m}")).unwrap_or_default();
        let pct = if r.calls > 0 { r.failed * 100 / r.calls } else { 0 };
        let trend = match (c.before, r.failed) {
            (0, _) => "new this week".to_owned(),
            (b, n) if n > b * 2 => format!("up from {b} the week before"),
            (b, n) if n * 2 < b => format!("down from {b} the week before"),
            (b, _) => format!("{b} the week before"),
        };
        text.push_str(&format!("- {} · {}{on}: {} of {} calls failed ({pct}%), {trend}", r.engine, r.tool, r.failed, r.calls));
        for p in &r.problems {
            text.push_str(&format!("\n    #{} {}× {}", p.id, p.count, p.title));
        }
        text.push('\n');
    }
    text.push_str(
        "Read the problems and their hits (input, breadcrumbs) and decide for each kind of failure what it is worth:\n\
- agent: the agent uses the tool wrong (a wrong flag, a path that never exists, a command for another OS). Fix it with a rule in \
the project's CLAUDE.md or AGENTS.md: a problem_thread whose contract adds the rule.\n\
- environment: something is missing or broken on the machine (a program, a sign-in, a setting). Say what to install or change; \
fix it with a problem_thread only when it is safe, else the ask lane.\n\
- hub: Hub itself causes it (its tools, its wrapper, how it starts engines). A problem_thread with a contract, as for any bug.\n\
- external: a service outside (rate limits, the network). Nothing to fix; archive until worse and say so.\n\
- noise: expected failures that carry no signal (a grep that found nothing, a test run that failed on purpose). Archive forever \
and suggest a rule so they stop counting.\n\
Skip what is not worth the effort and say why. Stamp the problems you act on as usual. Then save the review with tool_review: a \
short text for the user (what fails most, why, what you did, what waits for them) and one finding per tool.\n",
    );
    text
}

/// Vakten's verdict on one tool in a review.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolFinding {
    pub engine: String,
    pub tool: String,
    #[serde(default)]
    #[ts(optional)]
    pub machine: Option<String>,
    /// agent, environment, hub, external or noise.
    pub cause: String,
    /// What Vakten did or suggests, in a line.
    pub action: String,
    /// "worth" (it is being fixed or should be), "skip" (not worth the effort), "waits" (for the user).
    #[serde(default)]
    #[ts(optional)]
    pub verdict: Option<String>,
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub problem: Option<i64>,
}

/// Vakten's latest look at the tools that fail most.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolReview {
    #[ts(type = "number")]
    pub ts: i64,
    pub text: String,
    pub findings: Vec<ToolFinding>,
}

pub async fn review(hub: &Hub) -> Result<Option<ToolReview>> {
    Ok(hub.store.get_value(REVIEW).await?.and_then(|v| serde_json::from_str(&v).ok()))
}

pub async fn save_review(hub: &Hub, text: &str, findings: Vec<ToolFinding>) -> Result<ToolReview> {
    let text = text.trim();
    if text.is_empty() {
        anyhow::bail!("text is missing");
    }
    let review = ToolReview { ts: now_ms(), text: clip(text, 4000), findings };
    hub.store.set_value(REVIEW, &serde_json::to_string(&review)?).await?;
    Ok(review)
}

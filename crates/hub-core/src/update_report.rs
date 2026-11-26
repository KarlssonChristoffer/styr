//! Tells GitHub where each version of the branch stands on this machine, so the threads that
//! opened its pull requests can read it instead of guessing ("waiting for Christoffer to release
//! it" was the guess). Each change of the update's status becomes a commit status on the version's
//! commit, with the context `hub/update · <machine>`: pending while it waits for a quiet branch or
//! builds (with the step), success once it is ready and once it is installed, failure when it did
//! not build. A version holds every commit merged since the running one, so the status goes on each
//! of them (first parent, at most `MAX_COMMITS`), and every pull request's merge commit carries it;
//! which commits those were is kept in `update-commits` for the "installed" the new version posts
//! after the restart. Posted one at a time in order, through `gh`, which the updater's clone is
//! logged in with; a failure to post is only logged.

use crate::locked::ResultExt;
use crate::locked::Ignore;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use crate::config::Config;
use crate::model::{UpdateState, UpdateStatus};

/// GitHub cuts a status's description at 140 characters.
const MAX: usize = 140;
/// The most commits of one version that get the status.
const MAX_COMMITS: usize = 30;

/// A commit status to set: the version (short commit), the version it follows (whose commits are
/// not part of it), the state and what it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Report {
    pub version: String,
    pub since: Option<String>,
    pub state: &'static str,
    pub text: String,
}

/// What `status` says about the versions it names, on `host`.
pub(crate) fn reports(status: &UpdateStatus, host: &str, quiet_mins: u64) -> Vec<Report> {
    let report = |version: &str, state: &'static str, text: String| {
        let text = if text.chars().count() > MAX { format!("{}…", text.chars().take(MAX - 1).collect::<String>()) } else { text };
        let since = (status.version != "dev" && status.version != version).then(|| status.version.clone());
        Report { version: version.to_owned(), since, state, text }
    };
    let building = |version: &str| match (&status.waiting_until, &status.step) {
        (Some(_), _) => report(version, "pending", format!("Rust changed: builds on {host} once the branch has been quiet {quiet_mins} min")),
        (None, Some(step)) => report(version, "pending", format!("Building on {host}: {step}")),
        (None, None) => report(version, "pending", format!("Building on {host}")),
    };
    let mut out = Vec::new();
    match status.state {
        UpdateState::Building => out.extend(status.target.as_deref().map(building)),
        UpdateState::Ready => {
            out.extend(status.target.as_deref().map(|v| report(v, "success", format!("Built on {host}: ready to install"))));
            out.extend(status.building.as_deref().map(building));
        }
        UpdateState::Restarting => out.extend(status.target.as_deref().map(|v| report(v, "success", format!("Installing on {host}")))),
        UpdateState::Failed => {
            let why = status.message.as_deref().and_then(|m| m.lines().next()).unwrap_or("see Settings > Updates");
            out.extend(status.target.as_deref().map(|v| report(v, "failure", format!("Did not build on {host}: {why}"))));
        }
        UpdateState::Idle if status.version != "dev" => out.push(report(&status.version, "success", format!("Installed on {host}"))),
        _ => {}
    }
    out
}

struct Sender {
    tx: tokio::sync::mpsc::UnboundedSender<Report>,
    /// What was last sent for each version, so a status that says the same again is not posted.
    sent: Mutex<HashMap<String, (&'static str, String)>>,
}

static SENDER: OnceLock<Sender> = OnceLock::new();

/// Posts what changed in `status` to GitHub, when `update.report_status` is on and this machine
/// builds its own versions.
pub(crate) fn tell(config: &Config, status: &UpdateStatus) {
    let cfg = &config.update;
    if !cfg.report_status || cfg.pushed || !cfg.is_enabled(&hub_install::running_version()) {
        return;
    }
    let Ok(runtime) = tokio::runtime::Handle::try_current() else { return };
    let src = cfg.source_in(&config.data_dir);
    let data = config.data_dir.clone();
    let sender = SENDER.get_or_init(|| {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        runtime.spawn(post_all(src, data, rx));
        Sender { tx, sent: Mutex::default() }
    });
    let host = sysinfo::System::host_name().unwrap_or_else(|| "this computer".into());
    for report in reports(status, &host, cfg.rust_quiet_mins) {
        {
            let mut sent = sender.sent.lock().unwrap_or_else(|e| e.into_inner());
            let now = (report.state, report.text.clone());
            if sent.get(&report.version) == Some(&now) {
                continue;
            }
            sent.insert(report.version.clone(), now);
        }
        sender.tx.send(report).ignore("the poster ends with the runtime it started on (a test's may be gone)");
    }
}

async fn post_all(src: PathBuf, data: PathBuf, mut rx: tokio::sync::mpsc::UnboundedReceiver<Report>) {
    let host = sysinfo::System::host_name().unwrap_or_else(|| "this computer".into());
    let context = format!("hub/update · {host}");
    while let Some(report) = rx.recv().await {
        let (dir, data2, version, since) = (src.clone(), data.clone(), report.version.clone(), report.since.clone());
        let shas = tokio::task::spawn_blocking(move || commits(&dir, &data2, &version, since.as_deref())).await.unwrap_or_default();
        for sha in shas {
            let path = format!("repos/{{owner}}/{{repo}}/statuses/{sha}");
            let (state, description, ctx) = (format!("state={}", report.state), format!("description={}", report.text), format!("context={context}"));
            let args = ["api", "--silent", "-X", "POST", &path, "-f", &state, "-f", &ctx, "-f", &description];
            if let Err(err) = crate::proc::gh_within(&src, &args, Duration::from_secs(30)).await {
                let why = match err {
                    crate::proc::GhError::Missing => "gh is not installed".to_owned(),
                    crate::proc::GhError::Failed(why) => why,
                };
                tracing::debug!("could not tell GitHub that {} is {}: {why}", report.version, report.state);
                break;
            }
        }
    }
}

/// The commits `version` brings: those since `since` on the first-parent line, else the ones
/// remembered for it, else only its own.
fn commits(src: &Path, data: &Path, version: &str, since: Option<&str>) -> Vec<String> {
    let kept = data.join("update-commits");
    if let Some(since) = since {
        let range = format!("{since}..{version}");
        let max = format!("--max-count={MAX_COMMITS}");
        if let Some(out) = crate::git::run(src, &["rev-list", "--first-parent", &max, &range]) {
            let shas: Vec<String> = out.split_whitespace().map(str::to_owned).collect();
            if !shas.is_empty() {
                std::fs::write(&kept, format!("{version}\n{}\n", shas.join("\n"))).warn_on_err("write file");
                return shas;
            }
        }
    }
    if let Some(shas) = std::fs::read_to_string(&kept).ok().and_then(|t| {
        let mut lines = t.lines();
        (lines.next() == Some(version)).then(|| lines.map(str::to_owned).filter(|l| !l.is_empty()).collect::<Vec<_>>())
    })
        && !shas.is_empty() {
            return shas;
        }
    crate::git::run(src, &["rev-parse", version]).map(|s| vec![s.trim().to_owned()]).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(state: UpdateState) -> UpdateStatus {
        UpdateStatus {
            version: "aaaaaaaaaaaa".into(),
            state,
            target: Some("bbbbbbbbbbbb".into()),
            message: None,
            building: None,
            step: None,
            checked_at: None,
            waiting_until: None,
            waiting: None,
            rolled_back: None,
            probation: None,
        }
    }

    #[test]
    fn each_state_says_where_the_version_is() {
        let mut s = status(UpdateState::Building);
        s.waiting_until = Some(1);
        let r = reports(&s, "PC", 10);
        assert_eq!((r[0].version.as_str(), r[0].since.as_deref(), r[0].state), ("bbbbbbbbbbbb", Some("aaaaaaaaaaaa"), "pending"));
        assert!(r[0].text.contains("quiet 10 min"), "{}", r[0].text);

        s.waiting_until = None;
        s.step = Some("Step 3 of 5: cargo test".into());
        assert_eq!(reports(&s, "PC", 10)[0].text, "Building on PC: Step 3 of 5: cargo test");

        // Ready, with a newer one building behind it: both are told.
        let mut s = status(UpdateState::Ready);
        s.building = Some("cccccccccccc".into());
        let r = reports(&s, "PC", 10);
        assert_eq!(r.iter().map(|r| (r.version.as_str(), r.state)).collect::<Vec<_>>(), [("bbbbbbbbbbbb", "success"), ("cccccccccccc", "pending")]);

        let mut s = status(UpdateState::Failed);
        s.message = Some("cargo test failed\nmore".into());
        assert_eq!(reports(&s, "PC", 10)[0].text, "Did not build on PC: cargo test failed");

        assert_eq!(reports(&status(UpdateState::Idle), "PC", 10)[0], Report { version: "aaaaaaaaaaaa".into(), since: None, state: "success", text: "Installed on PC".into() });
        let mut dev = status(UpdateState::Idle);
        dev.version = "dev".into();
        assert!(reports(&dev, "PC", 10).is_empty());
    }

    #[test]
    fn every_commit_a_version_brings_gets_the_status() {
        let src = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| crate::update::git(src.path(), args).unwrap().trim().to_owned();
        git(&["init", "--quiet"]);
        let commit = |n: &str| {
            git(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "--quiet", "--allow-empty", "-m", n]);
            git(&["rev-parse", "HEAD"])
        };
        let first = commit("running");
        let (a, b) = (commit("pr 1"), commit("pr 2"));
        let (src, data) = (src.path(), data.path());
        assert_eq!(commits(src, data, &b, Some(&first)), [b.clone(), a.clone()]);
        // After the restart the new version names no earlier one: the kept list answers.
        assert_eq!(commits(src, data, &b, None), [b.clone(), a.clone()]);
        // Another version without a range is only its own commit.
        assert_eq!(commits(src, data, &a, None), [a]);
    }

    #[test]
    fn a_long_message_fits_in_what_github_keeps() {
        let mut s = status(UpdateState::Failed);
        s.message = Some("x".repeat(500));
        assert_eq!(reports(&s, "PC", 10)[0].text.chars().count(), MAX);
    }
}

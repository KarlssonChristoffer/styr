//! CI on pull requests, on the machine that runs the update's tests (`update.tests_on`, see
//! `checks`). While the PC is idle it looks at the open pull requests, sends each new head commit
//! to that machine, waits for the tests there and puts the verdict on the commit as the status
//! `hub/ci · <machine>` (pending, success or failure). Report only: nothing waits for it, merging
//! stays with the threads. One pull request at a time, and never while the update itself is
//! building, since the machine runs one test run at a time and the update's comes first. A run
//! that could not finish (the machine went away, the update took over, the machine stopped it to
//! install an update or for memory) is posted as waiting, not failed, and tried again after a while. When the tests passed it also posts `hub/pins · <machine>` from
//! `scripts/pins.py` (tests, big files and weeds against main): red asks the pull request to say why.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde_json::Value;

use crate::update::checks::{self, Outcome};
use crate::hub::Hub;
use crate::locked::ResultExt;
use crate::proc::gh_within;
use crate::update::{self, blocking, git};

/// How often it looks for a new head: a thread merges within minutes, so the verdict must start early.
const EVERY: Duration = Duration::from_secs(20);
/// How long a run that was lost waits before it is tried again.
const RETRY: Duration = Duration::from_secs(10 * 60);
/// The first tries after a lost run are quick (the update's tests took the machine, or hubd there
/// restarted), and say "waiting"; only the `QUICK_TRIES`th says "error".
const QUICK: Duration = Duration::from_secs(60);
const QUICK_TRIES: u32 = 3;
/// How long a run is waited for; the testing machine holds its own install for a run as long.
const LIMIT: Duration = checks::RUN_LIMIT;
const MAX: usize = 140;
/// How long the pins script may take before it is stopped.
const PINS_LIMIT: Duration = Duration::from_secs(5 * 60);

/// A pull request that is open, not a draft and from this repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub number: u64,
    pub head: String,
}

/// The pull requests of `gh pr list --json number,headRefOid,isDraft,isCrossRepository,statusCheckRollup`
/// that have no passed or failed verdict on their head yet under `context`.
pub(crate) fn candidates(list: &str, context: &str) -> Vec<Candidate> {
    let Ok(rows) = serde_json::from_str::<Vec<Value>>(list) else { return Vec::new() };
    let mut out: Vec<Candidate> = rows
        .iter()
        .filter(|r| !r["isDraft"].as_bool().unwrap_or(false) && !r["isCrossRepository"].as_bool().unwrap_or(false))
        .filter(|r| {
            let seen = r["statusCheckRollup"].as_array().is_some_and(|c| c.iter().any(|c| c["context"].as_str() == Some(context) && matches!(c["state"].as_str(), Some("SUCCESS" | "FAILURE"))));
            !seen
        })
        .filter_map(|r| Some(Candidate { number: r["number"].as_u64()?, head: r["headRefOid"].as_str()?.to_owned() }))
        .collect();
    // The newest first: what was just pushed is what its thread waits for.
    out.sort_by_key(|x| std::cmp::Reverse(x.number));
    out
}

fn why(err: crate::proc::GhError) -> String {
    match err {
        crate::proc::GhError::Missing => "gh is not installed".into(),
        crate::proc::GhError::Failed(why) => why,
    }
}

/// What a run that could not finish says on the commit, and how long until the next try: the first
/// tries are `pending` ("waiting", not "not tested") and quick, then `error` and slow.
fn lost_verdict(tries: u32, why: &str) -> (&'static str, String, Duration) {
    if tries < QUICK_TRIES {
        ("pending", describe(&format!("Waiting, tries again: {why}")), QUICK)
    } else {
        ("error", describe(&format!("Not tested: {why}")), RETRY)
    }
}

/// What failed, from the failing step's output: the names of the tests that fell (the machine puts
/// them first, `update::failed_tests`), as many as fit in `room` characters and "+N more", else the
/// last line that says something went wrong.
fn reason(tail: &str, room: usize) -> String {
    let names = update::failed_tests(tail);
    let Some((first, rest)) = names.split_first() else {
        return tail.lines().rev().find(|l| l.contains("FAILED") || l.contains("failed") || l.contains("error")).unwrap_or(tail).trim().to_owned();
    };
    let mut text = format!("{} {first}", if rest.is_empty() { "test" } else { "tests" });
    let mut shown = 1;
    for name in rest {
        let left = names.len() - shown - 1;
        let more = if left > 0 { format!(" +{left} more").len() } else { 0 };
        if text.chars().count() + 2 + name.chars().count() + more > room {
            break;
        }
        text = format!("{text}, {name}");
        shown += 1;
    }
    if shown < names.len() {
        text = format!("{text} +{} more", names.len() - shown);
    }
    text
}

/// What an outcome posts on the commit (state and text) and, for a run that could not finish, how
/// long until it is tried again. `tries` is how often this head was lost before.
fn posted(outcome: &Outcome, host: &str, tries: u32) -> (&'static str, String, Option<Duration>) {
    match outcome {
        Outcome::Passed => ("success", format!("Tests passed on {host}"), None),
        Outcome::Failed(tail) => {
            let head = format!("Failed on {host}: ");
            ("failure", describe(&format!("{head}{}", reason(tail, MAX.saturating_sub(head.chars().count())))), None)
        }
        Outcome::Lost(why) => {
            let (state, text, wait) = lost_verdict(tries, why);
            (state, text, Some(wait))
        }
    }
}

/// What the pins of a passed pull request say, as a commit status state and text. `None` when the
/// script could not run (no Python, no script in this checkout yet): then nothing is posted.
fn pins_verdict(code: Option<i32>, out: &str) -> Option<(&'static str, String)> {
    let line = out.lines().rev().find(|l| l.starts_with("ok:") || l.starts_with("red:"))?;
    match code? {
        0 => Some(("success", describe(line))),
        1 => Some(("failure", describe(line))),
        _ => None,
    }
}

/// Posts `hub/pins · <machine>`: tests, big files and weeds against main (`scripts/pins.py`).
/// Informational: a red pin asks the pull request to say why; it does not stop a merge by itself.
async fn pins(hub: &Arc<Hub>, src: &std::path::Path, sha: &str, host: &str) {
    let cfg = hub.config.update.clone();
    let (dir, sha2, base) = (src.to_owned(), sha.to_owned(), format!("{}/{}", cfg.remote, cfg.branch));
    let ran = blocking(move || {
        let script = dir.join("scripts").join("pins.py");
        if !script.exists() {
            return Ok(None);
        }
        // `python3` can be the Windows Store stub that starts and fails: try the next one then.
        for python in ["python3", "python"] {
            let child = crate::proc::command(python)
                .arg("-I")
                .arg(&script)
                .arg(&dir)
                .args(["--head", &sha2, "--base", &base, "--line"])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn();
            let Ok(mut child) = child else { continue };
            let started = Instant::now();
            let code = loop {
                match child.try_wait() {
                    Ok(Some(status)) => break status.code(),
                    Ok(None) if started.elapsed() < PINS_LIMIT => std::thread::sleep(Duration::from_millis(250)),
                    _ => {
                        // Stuck: it must not hold up the checks of the next pull request.
                        child.kill().warn_on_err("stop the pins script");
                        child.wait().warn_on_err("reap the pins script");
                        break None;
                    }
                }
            };
            let mut text = String::new();
            if let Some(mut out) = child.stdout.take() {
                std::io::Read::read_to_string(&mut out, &mut text).warn_on_err("read the pins line");
            }
            if let Some(verdict) = pins_verdict(code, &text) {
                return Ok(Some(verdict));
            }
        }
        Ok(None)
    })
    .await;
    if let Ok(Some((state, text))) = ran {
        post(src, sha, &format!("hub/pins · {host}"), state, &text).await;
    }
}

fn describe(text: &str) -> String {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("see the tests on the machine");
    if line.chars().count() > MAX { format!("{}…", line.chars().take(MAX - 1).collect::<String>()) } else { line.to_owned() }
}

async fn post(src: &std::path::Path, sha: &str, context: &str, state: &str, text: &str) {
    let path = format!("repos/{{owner}}/{{repo}}/statuses/{sha}");
    let (state, description, ctx) = (format!("state={state}"), format!("description={text}"), format!("context={context}"));
    let args = ["api", "--silent", "-X", "POST", &path, "-f", &state, "-f", &ctx, "-f", &description];
    if let Err(err) = gh_within(src, &args, Duration::from_secs(30)).await {
        tracing::debug!("could not post the status of {sha}: {}", why(err));
    }
}

/// Starts the loop, once per process. Does nothing unless `update.tests_on` is set and this machine
/// follows the branch itself.
pub fn start(hub: &Arc<Hub>) {
    static STARTED: AtomicBool = AtomicBool::new(false);
    let cfg = hub.config.update.clone();
    let Some(host) = cfg.tests_on.clone().filter(|_| !cfg.pushed) else { return };
    if !cfg.is_enabled(&hub_install::running_version()) || STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let (hub, src) = (hub.clone(), cfg.source_in(&hub.config.data_dir));
    tokio::spawn(async move {
        let mut lost: HashMap<String, Lost> = HashMap::new();
        loop {
            tokio::time::sleep(EVERY).await;
            if update::building() || checks::watching() {
                continue;
            }
            if let Err(err) = round(&hub, &host, &src, &mut lost).await {
                tracing::debug!("pull request checks: {err:#}");
            }
        }
    });
}

/// A head whose run could not finish and when it is tried again.
struct Lost {
    at: Instant,
    wait: Duration,
    tries: u32,
}

async fn round(hub: &Arc<Hub>, host: &str, src: &std::path::Path, lost: &mut HashMap<String, Lost>) -> Result<()> {
    let context = format!("hub/ci · {host}");
    let fields = "number,headRefOid,isDraft,isCrossRepository,statusCheckRollup";
    let list = gh_within(src, &["pr", "list", "--state", "open", "--base", "main", "--limit", "30", "--json", fields], Duration::from_secs(30)).await.map_err(|e| anyhow::anyhow!(why(e)))?;
    lost.retain(|_, l| l.at.elapsed() < RETRY * 3);
    let Some(pr) = candidates(&list, &context).into_iter().find(|c| lost.get(&c.head).is_none_or(|l| l.at.elapsed() >= l.wait)) else { return Ok(()) };
    let short: String = pr.head.chars().take(12).collect();
    // The pull request's head lands in the updater's clone without touching what it has checked out.
    let (dir, number) = (src.to_owned(), pr.number);
    let sha = blocking(move || {
        git(&dir, &["fetch", "--quiet", "origin", &format!("pull/{number}/head")])?;
        git(&dir, &["rev-parse", "FETCH_HEAD"]).map(|s| s.trim().to_owned())
    })
    .await?;
    if sha != pr.head {
        // It moved between the list and the fetch; the next round takes the new head.
        return Ok(());
    }
    post(src, &sha, &context, "pending", &format!("Testing on {host}")).await;
    let (hub2, host2, dir2, sha2) = (hub.clone(), host.to_owned(), src.to_owned(), sha.clone());
    let outcome = blocking(move || {
        let session = checks::start(&hub2, &host2, &sha2, &dir2, &|_| {})?;
        checks::wait(&hub2, &session, LIMIT, &|_| {}, &AtomicBool::new(false))
    })
    .await
    .unwrap_or_else(|err| Outcome::Lost(format!("{err:#}")));
    let tries = lost.get(&pr.head).map_or(0, |l| l.tries);
    let (state, text, again) = posted(&outcome, host, tries);
    // A run that could not finish is not left saying "testing", and is not the commit's fault.
    if let Some(wait) = again {
        tracing::debug!("pull request {} ({short}): {text}", pr.number);
        lost.insert(pr.head.clone(), Lost { at: Instant::now(), wait, tries: tries + 1 });
    }
    post(src, &sha, &context, state, &text).await;
    if outcome == Outcome::Passed {
        pins(hub, src, &sha, host).await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_new_heads_of_open_pull_requests_of_this_repository_are_tested() {
        let ctx = "hub/ci · tincan";
        let list = serde_json::json!([
            {"number": 1, "headRefOid": "a1", "isDraft": false, "isCrossRepository": false, "statusCheckRollup": []},
            {"number": 2, "headRefOid": "b2", "isDraft": true, "isCrossRepository": false, "statusCheckRollup": []},
            {"number": 3, "headRefOid": "c3", "isDraft": false, "isCrossRepository": true, "statusCheckRollup": []},
            {"number": 4, "headRefOid": "d4", "isDraft": false, "isCrossRepository": false, "statusCheckRollup": [{"context": ctx, "state": "SUCCESS"}]},
            {"number": 5, "headRefOid": "e5", "isDraft": false, "isCrossRepository": false, "statusCheckRollup": [{"context": ctx, "state": "PENDING"}]},
            {"number": 7, "headRefOid": "g7", "isDraft": false, "isCrossRepository": false, "statusCheckRollup": [{"context": ctx, "state": "ERROR"}]},
            {"number": 6, "headRefOid": "f6", "isDraft": false, "isCrossRepository": false, "statusCheckRollup": [{"context": "hub/update · PC", "state": "SUCCESS"}]}
        ])
        .to_string();
        let got: Vec<u64> = candidates(&list, ctx).iter().map(|c| c.number).collect();
        assert_eq!(got, [7, 6, 5, 1]);
        assert!(candidates("not json", ctx).is_empty());
    }

    #[test]
    fn a_lost_run_waits_and_tries_again_before_it_says_not_tested() {
        let (state, text, wait) = lost_verdict(0, "tincan no longer has the run");
        assert_eq!((state, wait), ("pending", QUICK));
        assert!(text.contains("tries again") && text.contains("no longer has the run"), "{text}");
        assert_eq!(lost_verdict(QUICK_TRIES - 1, "x").0, "pending");
        let (state, text, wait) = lost_verdict(QUICK_TRIES, "x");
        assert_eq!((state, wait), ("error", RETRY));
        assert!(text.starts_with("Not tested"), "{text}");
    }

    #[test]
    fn pins_say_ok_red_or_nothing() {
        assert_eq!(pins_verdict(Some(0), "ok: tests_rust 5\n"), Some(("success", "ok: tests_rust 5".into())));
        assert_eq!(pins_verdict(Some(1), "red: dead 0→2\n"), Some(("failure", "red: dead 0→2".into())));
        assert_eq!(pins_verdict(Some(2), "ok: x\n"), None);
        assert_eq!(pins_verdict(Some(0), "Traceback\n"), None);
        assert_eq!(pins_verdict(None, "ok: x"), None);
    }

    #[test]
    fn a_failure_names_the_test_that_fell() {
        let out = "test a::fine ... ok\ntest rules::only_fewer ... FAILED\n\nerror: test failed, to rerun pass `-p hub-core --test it`";
        assert_eq!(reason(out, MAX), "test rules::only_fewer");
        assert_eq!(reason("test a::x ... FAILED\ntest b::y ... FAILED\ntest a::x ... FAILED", MAX), "tests a::x, b::y");
        assert_eq!(reason("compiling\nerror: could not compile `hub-core`", MAX), "error: could not compile `hub-core`");
    }

    /// libtest's end of a run where two tests fell: their output, then the list of names.
    fn cargo_end(names: &[&str]) -> String {
        let mut out = String::from("failures:\n\n");
        for name in names {
            out += &format!("---- {name} stdout ----\n\nthread '{name}' panicked at crates/hub-core/tests/it/rules.rs:88:5:\nassertion failed\n    left: 1\n   right: 2\n\n");
        }
        out += "\nfailures:\n";
        for name in names {
            out += &format!("    {name}\n");
        }
        out += &format!("\ntest result: FAILED. 1203 passed; {} failed; 4 ignored; 0 measured; 0 filtered out; finished in 92.31s\n\nerror: test failed, to rerun pass `-p hub-core --test it`\n", names.len());
        out
    }

    #[test]
    fn a_failure_far_from_the_end_of_a_long_output_is_named() {
        // The `test x ... FAILED` line scrolled away thousands of lines up; the names under the
        // closing `failures:` are what is left.
        let mut out: String = (0..3000).map(|n| format!("test mod{n}::fine ... ok\n")).collect();
        out += "test rules::only_fewer ... FAILED\n";
        out += &(0..3000).map(|n| format!("test more{n}::fine ... ok\n")).collect::<String>();
        out += &cargo_end(&["rules::only_fewer"]);
        assert_eq!(reason(&out, MAX), "test rules::only_fewer");
        // What the machine sends: its step error puts the names first, before the last lines
        // (`update::run_logged`), and the PC reads them from there.
        let sent = "`cargo test --workspace --exclude hub-app` failed (full log in /home/hub/.hub/checks.log):\ntest rules::only_fewer ... FAILED\ntest result: FAILED. 1203 passed; 1 failed\nerror: test failed, to rerun pass `-p hub-core --test it`";
        assert_eq!(reason(sent, MAX), "test rules::only_fewer");
        // #1083 (head 71b89f3f): the last lines held no name, and the line that said so was posted.
        let without = "`cargo test --workspace --exclude hub-app` failed (full log in /home/hub/.hub/checks.log):\nerror: test failed, to rerun pass `-p hub-core --test it`";
        assert_eq!(reason(without, MAX), "error: test failed, to rerun pass `-p hub-core --test it`");
        // Many that fell: the first names that fit, then how many more.
        let many: Vec<String> = (0..12).map(|n| format!("engine::a_long_test_name_number_{n}")).collect();
        let text = posted(&Outcome::Failed(cargo_end(&many.iter().map(String::as_str).collect::<Vec<_>>())), "tincan", 0).1;
        assert!(text.starts_with("Failed on tincan: tests engine::a_long_test_name_number_0, engine::"), "{text}");
        assert!(text.ends_with(" more") && text.contains(" +") && text.chars().count() <= MAX, "{text}");
    }

    #[test]
    fn a_run_stopped_for_an_install_waits_and_tries_again_like_a_lost_one() {
        let job = |state, tail: &str| checks::JobView { version: "abc".into(), state, step: None, started_at: 1, finished_at: None, tail: Some(tail.into()) };
        let stopped = checks::outcome_of("tincan", &job(checks::JobState::Stopped, checks::STOPPED_FOR_INSTALL)).unwrap();
        let (state, text, again) = posted(&stopped, "tincan", 0);
        assert_eq!((state, text.as_str(), again), ("pending", "Waiting, tries again: stopped to install the update", Some(QUICK)));
        // The same counts as a lost run and a memory stop: quick and pending first, then error and slow.
        assert_eq!(posted(&stopped, "tincan", QUICK_TRIES).2, Some(RETRY));
        assert_eq!(posted(&stopped, "tincan", QUICK_TRIES).0, "error");
        let memory = checks::outcome_of("tincan", &job(checks::JobState::Stopped, "`cargo test` was stopped because the computer was running out of memory")).unwrap();
        assert_eq!(posted(&memory, "tincan", 0).0, "pending");
        // An older machine's stop that read as a failure (#1083, head 2e6491aa) is not red either.
        let old = checks::outcome_of("tincan", &job(checks::JobState::Failed, "stopped to install the version that was ready")).unwrap();
        assert_eq!(posted(&old, "tincan", 0).0, "pending");
        // A failed test is red, and a pass is green with no retry.
        assert_eq!(posted(&Outcome::Failed("test a::b ... FAILED".into()), "tincan", 0), ("failure", "Failed on tincan: test a::b".into(), None));
        assert_eq!(posted(&Outcome::Passed, "tincan", 0), ("success", "Tests passed on tincan".into(), None));
    }

    #[test]
    fn a_failure_is_told_in_one_short_line() {
        assert_eq!(describe("\n  \nfirst\nsecond"), "first");
        assert!(describe(&"x".repeat(500)).chars().count() <= MAX);
    }
}

//! Whether this machine is ready for work, as its own page shows it: each configured engine found
//! and signed in, git knowing who you are, room left on the disk, a PowerShell that runs scripts. Every check that is not fine
//! carries the fix, so a new machine is made ready from its page instead of by remembering what
//! it lacks. Engines are read from the config and the logins `prefs` already knows; no engine is
//! named here.

use crate::config::Config;
use crate::model::{GitIdentity, MachineReady, ReadyCheck, ReadyFix, ReadyState, now_ms};
use crate::proc::find_program;
use crate::signin::{Kept, Lost, SignIns};

/// Below this much free space a thread's build can run out of room.
const LOW_DISK: u64 = 5 * 1024 * 1024 * 1024;

pub async fn check(cfg: &Config, sign_ins: SignIns) -> MachineReady {
    let cfg = cfg.clone();
    tokio::task::spawn_blocking(move || check_now(&cfg, &sign_ins, now_ms())).await.unwrap_or_else(|e| MachineReady {
        checks: vec![ReadyCheck {
            key: "check".into(),
            label: "Check".into(),
            state: ReadyState::Warn,
            detail: format!("the check itself failed: {e}"),
            fix: None,
        }],
        git: None,
    })
}

fn check_now(cfg: &Config, sign_ins: &SignIns, now: i64) -> MachineReady {
    check_with(cfg, &crate::prefs::accounts(cfg), sign_ins, now)
}

/// How one engine stands on this machine: installed, and signed in.
fn engine_check(cfg: &Config, name: &str, engine: &crate::config::EngineConfig, accounts: &crate::model::Accounts, sign_ins: &SignIns, now: i64) -> ReadyCheck {
    let account = accounts.engines.iter().find(|a| a.engine == name);
    let label = account.map(|a| a.label.clone()).unwrap_or_else(|| name.to_owned());
    let words = crate::project::split_command(&engine.command);
    let program = words.first().map(String::as_str).unwrap_or_default();
    let cli = engine.terminal.as_deref().and_then(crate::machine::engine_cli);
    let traits = cfg.traits(name);
    // Hub's own lasting sign-in where the engine can make one, else its CLI's login.
    let sign_in_fix = || match (&traits.sign_in, cli.filter(|c| find_program(c).is_some())) {
        (Some(_), _) if traits.sign_in_token.is_none() => ReadyFix::SignInHere { engine: name.to_owned() },
        (Some(_), _) => ReadyFix::KeepSignedIn { engine: name.to_owned() },
        (None, Some(_)) => ReadyFix::SignIn { engine: name.to_owned() },
        (None, None) => ReadyFix::Shell { hint: format!("{} is not installed there to sign in with.", cli.unwrap_or(name)) },
    };
    let kept = sign_ins.kept.get(name).filter(|k| k.until > now);
    // A failure after the last sign-in, and after the engine's login file last changed.
    let saved = account.and_then(|a| a.source.as_deref()).and_then(modified_ms);
    let lost = sign_ins
        .lost
        .get(name)
        .filter(|l| kept.is_none_or(|k| l.at > k.at) && (kept.is_some() || saved.is_none_or(|s| l.at > s)));
    let (state, detail, fix) = if find_program(program).is_none() {
        let hint = format!("{program} is not installed there. Install it (Install hubd again does Node), then check again.");
        // Signing in installs the CLI first, so where Hub knows how, the fix is the sign-in.
        let installable = traits.sign_in.as_deref().is_some_and(|c| crate::engines::install_command(cfg, name, c).is_some());
        let fix = if installable { sign_in_fix() } else { ReadyFix::Shell { hint } };
        (ReadyState::Missing, format!("{program} not found"), Some(fix))
    } else if let Some(lost) = lost {
        (ReadyState::Missing, lost_detail(lost, kept.is_some(), now), Some(sign_in_fix()))
    } else if let Some(kept) = kept {
        kept_check(kept, now, sign_in_fix)
    } else {
        match account.map(|a| (a, a.logged_in)) {
            // On a Mac the CLI's own login is in the Keychain, which hubd, started by launchd
            // before anyone logs in, cannot open: only a sign-in Hub keeps works there.
            Some((_, false)) if cfg!(target_os = "macos") && traits.sign_in_token.is_some() => {
                (ReadyState::Missing, "not signed in for Hub (a Mac keeps its own login in the Keychain, which Hub's service cannot read)".to_owned(), Some(sign_in_fix()))
            }
            Some((_, false)) => (ReadyState::Missing, "not signed in".to_owned(), Some(sign_in_fix())),
            Some((a, true)) => {
                let detail = signed_in(a.email.as_deref(), a.plan.as_deref());
                let api_key = a.method.as_deref() == Some("api_key");
                let state = if api_key { ReadyState::Warn } else { ReadyState::Ok };
                match (api_key, &traits.sign_in_token) {
                    (true, _) => (state, format!("{detail}, with an API key instead of the subscription"), None),
                    // The CLI's own login lives only while it is used and refreshed here.
                    (false, Some(_)) => (state, format!("{detail}, lasts while used here"), Some(sign_in_fix())),
                    (false, None) => (state, detail, None),
                }
            }
            // An engine whose login Hub cannot read: found is all that can be said.
            None => (ReadyState::Ok, "installed".to_owned(), None),
        }
    };
    ReadyCheck { key: format!("engine:{name}"), label, state, detail, fix }
}

/// Whether git is there and knows who commits.
fn git_check(git: &Option<GitIdentity>) -> ReadyCheck {
    if find_program("git").is_none() {
        ReadyCheck {
            key: "git".into(),
            label: "Git".into(),
            state: ReadyState::Missing,
            detail: "git not found".into(),
            fix: Some(ReadyFix::Shell { hint: "Install git there, then check again.".into() }),
        }
    } else {
        match git {
            Some(id) => ReadyCheck {
                key: "git".into(),
                label: "Git".into(),
                state: ReadyState::Ok,
                detail: format!("commits as {} <{}>", id.name, id.email),
                fix: None,
            },
            None => ReadyCheck {
                key: "git".into(),
                label: "Git".into(),
                state: ReadyState::Missing,
                detail: "no name and email to commit with".into(),
                fix: Some(ReadyFix::GitIdentity),
            },
        }
    }
}

/// The check with the engines' login state given, so a test does not read this machine's.
fn check_with(cfg: &Config, accounts: &crate::model::Accounts, sign_ins: &SignIns, now: i64) -> MachineReady {
    let mut checks = Vec::new();
    // The simulated engine is Hub itself, there wherever hubd is.
    for (name, engine) in cfg.engines.iter().filter(|(name, _)| name.as_str() != crate::sim::ENGINE) {
        checks.push(engine_check(cfg, name, engine, accounts, sign_ins, now));
    }

    let git = crate::git_identity::identity();
    checks.push(git_check(&git));

    let home = crate::folders::home();
    if let Some(disk) = crate::host::disk_of(&home) {
        let low = disk.free < LOW_DISK;
        checks.push(ReadyCheck {
            key: "disk".into(),
            label: "Disk".into(),
            state: if low { ReadyState::Warn } else { ReadyState::Ok },
            detail: format!("{} free", gib(disk.free)),
            fix: low.then(|| ReadyFix::Shell { hint: "Make room there: old build folders (target, node_modules) are the usual culprits.".into() }),
        });
    }
    // Agents that run their commands in PowerShell start npm, npx and the like as scripts.
    if cfg!(windows)
        && let Some(policy) = powershell_policy()
        && let Some(check) = scripts_check(&policy)
    {
        checks.push(check);
    }
    MachineReady { checks, git }
}

/// The execution policy in effect for Windows PowerShell (the one Hub's own terminal opens;
/// PowerShell 7 keeps its own), None when it cannot be asked.
fn powershell_policy() -> Option<String> {
    crate::host::quiet("powershell", &["-NoProfile", "-NonInteractive", "-Command", "Get-ExecutionPolicy"]).map(|out| out.trim().to_owned())
}

/// The row for a PowerShell that refuses scripts: an agent's `npm` then fails with "running
/// scripts is disabled on this system" (twice in a week of real threads, 2026-11-10). None when
/// scripts run.
fn scripts_check(policy: &str) -> Option<ReadyCheck> {
    matches!(policy.to_lowercase().as_str(), "restricted" | "allsigned").then(|| ReadyCheck {
        key: "powershell".into(),
        label: "PowerShell".into(),
        state: ReadyState::Warn,
        detail: format!("scripts are not allowed ({policy}), so npm and npx fail for an agent that works in PowerShell"),
        fix: Some(ReadyFix::Shell { hint: "In PowerShell there: Set-ExecutionPolicy -Scope CurrentUser RemoteSigned, then check again. If it stays, a group policy sets it (Get-ExecutionPolicy -List) and only an administrator can change it.".into() }),
    })
}

fn lost_detail(lost: &Lost, kept: bool, now: i64) -> String {
    let what = if kept { "the sign-in Hub kept stopped working" } else { "the sign-in ran out" };
    let ago = crate::text::ago(now - lost.at);
    let first_line = lost.message.lines().next().unwrap_or_default();
    if first_line.is_empty() { format!("{what} {ago}") } else { format!("{what} {ago}: {first_line}") }
}

fn kept_check(kept: &Kept, now: i64, fix: impl Fn() -> ReadyFix) -> (ReadyState, String, Option<ReadyFix>) {
    let until = crate::activity::date_of(kept.until, 0);
    let days = (kept.until - now) / 86_400_000;
    if kept.until - now < crate::signin::SOON_MS {
        let left = if days < 1 { "less than a day".to_owned() } else { format!("{days} days") };
        (ReadyState::Warn, format!("signed in by Hub until {until}, {left} left"), Some(fix()))
    } else {
        (ReadyState::Ok, format!("signed in by Hub until {until} ({})", kept.hint), None)
    }
}

/// When a file last changed, in ms.
fn modified_ms(path: &str) -> Option<i64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(modified.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as i64)
}

fn gib(bytes: u64) -> String {
    let g = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    if g >= 10.0 { format!("{g:.0} GB") } else { format!("{g:.1} GB") }
}

/// "signed in as you@x.se · Max plan": the account when it can be read, and the plan as a plan,
/// so a plan name never reads as the account.
fn signed_in(email: Option<&str>, plan: Option<&str>) -> String {
    let plan = plan.filter(|p| !p.trim().is_empty()).map(|p| {
        let mut chars = p.trim().chars();
        let first = chars.next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default();
        format!("{first}{} plan", chars.as_str())
    });
    match (email.filter(|e| !e.trim().is_empty()), plan) {
        (Some(email), Some(plan)) => format!("signed in as {email} · {plan}"),
        (Some(email), None) => format!("signed in as {email}"),
        (None, Some(plan)) => format!("signed in, {plan}"),
        (None, None) => "signed in".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Claude row as on a machine where the agent is installed (`git` stands in for it, being
    /// everywhere the tests run) and Claude Code is not logged in. Nothing is read from this
    /// machine: on the PC a real login made the row Ok and failed the updater's test run.
    fn claude_row(sign_ins: &SignIns, now: i64) -> Option<ReadyCheck> {
        let mut cfg = Config::default();
        cfg.engines.get_mut("claude").unwrap().command = "git acp".into();
        let accounts = crate::model::Accounts {
            engines: vec![crate::model::EngineAccount {
                engine: "claude".into(),
                label: "Claude".into(),
                command: Some("git acp".into()),
                logged_in: false,
                method: None,
                plan: None,
                email: None,
                organization: None,
                source: None,
            }],
            keys: Vec::new(),
        };
        check_with(&cfg, &accounts, sign_ins, now).checks.into_iter().find(|c| c.key == "engine:claude")
    }

    #[test]
    fn a_lost_sign_in_shows_until_signed_in_again() {
        let now = now_ms();
        let mut sign_ins = SignIns::default();
        sign_ins.lost.insert("claude".into(), Lost { at: now - 5 * 60_000, message: "OAuth session expired".into() });
        let row = claude_row(&sign_ins, now).unwrap();
        assert_eq!(row.state, ReadyState::Missing, "{}", row.detail);
        assert!(row.detail.contains("ran out 5 min ago: OAuth session expired"), "{}", row.detail);
        assert!(matches!(row.fix, Some(ReadyFix::KeepSignedIn { .. })));

        // Kept after it failed: fine, and says until when.
        sign_ins.kept.insert("claude".into(), Kept { at: now, until: now + 365 * 86_400_000, hint: "…abcd".into(), from: None });
        let row = claude_row(&sign_ins, now).unwrap();
        assert_eq!(row.state, ReadyState::Ok, "{}", row.detail);
        assert!(row.detail.starts_with("signed in by Hub until"), "{}", row.detail);

        // A month before the end it warns, with the way to renew.
        let late = now + 340 * 86_400_000;
        let row = claude_row(&sign_ins, late).unwrap();
        assert_eq!(row.state, ReadyState::Warn, "{}", row.detail);
        assert!(row.fix.is_some());
    }

    #[test]
    fn a_powershell_that_refuses_scripts_is_a_row_with_the_fix() {
        let row = scripts_check("Restricted").unwrap();
        assert_eq!((row.key.as_str(), row.state), ("powershell", ReadyState::Warn));
        assert!(matches!(row.fix, Some(ReadyFix::Shell { ref hint }) if hint.contains("Set-ExecutionPolicy")));
        assert!(scripts_check("allsigned").is_some());
        assert!(scripts_check("RemoteSigned").is_none() && scripts_check("Unrestricted").is_none() && scripts_check("Bypass").is_none());
    }

    #[test]
    fn programs_are_found_on_the_path_or_not_at_all() {
        let sh = if cfg!(windows) { "cmd" } else { "sh" };
        assert!(find_program(sh).is_some());
        assert!(find_program("surely-no-program-is-called-this").is_none());
        assert!(find_program("").is_none());
    }

    #[test]
    fn a_plan_never_reads_as_the_account() {
        assert_eq!(signed_in(None, Some("max")), "signed in, Max plan");
        assert_eq!(signed_in(Some("c@x.se"), Some("plus")), "signed in as c@x.se · Plus plan");
        assert_eq!(signed_in(Some("c@x.se"), None), "signed in as c@x.se");
        assert_eq!(signed_in(None, None), "signed in");
    }

    #[test]
    fn every_engine_and_git_are_checked() {
        let cfg = Config::default();
        let ready = check_now(&cfg, &SignIns::default(), now_ms());
        for name in cfg.engines.keys() {
            assert!(ready.checks.iter().any(|c| c.key == format!("engine:{name}")), "{name} is checked");
        }
        let git = ready.checks.iter().find(|c| c.key == "git").unwrap();
        assert!(git.state == ReadyState::Ok || git.fix.is_some(), "a git that is not fine says how to fix it");
    }
}

//! Guards self-update: when a build may start (quiet branch, build modes, the pages-only lane and
//! its `covers:` check), how it stops or fails (memory, hung steps, fallen tests named), the
//! handover and its probation, and pruning that keeps the running and last good versions.

#[test]
fn npm_install_is_skipped_only_while_package_files_match_the_stamp() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path();
    std::fs::create_dir_all(src.join("app/node_modules")).unwrap();
    std::fs::write(src.join("app/package.json"), "{}").unwrap();
    std::fs::write(src.join("app/package-lock.json"), "{}").unwrap();
    let step = crate::config::BuildStep { dir: "app".into(), command: vec!["npm".into(), "install".into()] };
    let (d, stamp) = npm_install_stamp(src, &step).unwrap();
    assert_eq!(node_modules_stamp(&d), None);
    std::fs::write(d.join("node_modules/.hub-lock-stamp"), &stamp).unwrap();
    assert_eq!(node_modules_stamp(&d).as_deref(), Some(stamp.as_str()));
    std::fs::write(src.join("app/package-lock.json"), "{\"a\":1}").unwrap();
    assert_ne!(npm_install_stamp(src, &step).unwrap().1, stamp);
    let other = crate::config::BuildStep { dir: "app".into(), command: vec!["npm".into(), "run".into(), "check".into()] };
    assert!(npm_install_stamp(src, &other).is_none());
}

#[test]
fn a_missing_sign_in_is_told_from_a_missing_network() {
    use super::is_sign_in_failure;
    assert!(is_sign_in_failure("git fetch failed: remote: Invalid username or token.\nfatal: Authentication failed for 'https://github.com/x/y/'"));
    assert!(is_sign_in_failure("fatal: User cancelled dialog."));
    assert!(is_sign_in_failure("fatal: could not read Username for 'https://github.com': terminal prompts disabled"));
    assert!(!is_sign_in_failure("fatal: unable to access 'https://github.com/x/y/': Could not resolve host: github.com"));
}
use super::*;

#[test]
fn a_stopped_build_names_the_programs_that_held_the_memory() {
    let line = biggest_processes();
    assert!(line.contains(" MB") && line.contains(" x"), "{line}");
}

#[test]
fn a_dip_in_memory_does_not_stop_the_build_but_a_lasting_one_does() {
    let t0 = std::time::Instant::now();
    let mut since = None;
    let low = Some(LOW_MEMORY - 1);
    let fine = Some(LOW_MEMORY + (1 << 30));
    // Below the line for a second or two: nothing yet.
    assert!(!memory_short(&mut since, low, t0));
    assert!(!memory_short(&mut since, low, t0 + Duration::from_secs(2)));
    // Memory comes back: the dip is forgotten, so the next one starts its clock again.
    assert!(!memory_short(&mut since, fine, t0 + Duration::from_secs(3)));
    assert!(!memory_short(&mut since, low, t0 + Duration::from_secs(4)));
    assert!(!memory_short(&mut since, low, t0 + Duration::from_secs(4) + LOW_MEMORY_GRACE - Duration::from_secs(1)));
    // Short for the whole grace period: the build goes.
    assert!(memory_short(&mut since, low, t0 + Duration::from_secs(4) + LOW_MEMORY_GRACE));
    // A machine that cannot say how much is left (macOS) never stops a build.
    let mut none = None;
    assert!(!memory_short(&mut none, None, t0 + Duration::from_secs(999)));
}

#[test]
fn the_build_folder_is_held_by_one_build_at_a_time() {
    let stop = AtomicBool::new(false);
    let held = hold_build_folder(&|| {}, &stop).unwrap();
    let waited = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|s| {
        let other = s.spawn(|| {
            let _second = hold_build_folder(&|| waited.store(true, Ordering::Relaxed), &AtomicBool::new(false)).unwrap();
        });
        while !waited.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(10));
        }
        drop(held);
        other.join().unwrap();
    });
    // A stop ends the wait.
    let _held = hold_build_folder(&|| {}, &stop).unwrap();
    stop.store(true, Ordering::Relaxed);
    assert!(std::thread::scope(|s| s.spawn(|| hold_build_folder(&|| {}, &stop).is_err()).join().unwrap()));
}

fn exiting(code: u8) -> std::process::Child {
    let mut cmd = if cfg!(windows) { Command::new("cmd") } else { Command::new("sh") };
    let arg = if cfg!(windows) { "/C" } else { "-c" };
    cmd.args([arg, &format!("exit {code}")]).spawn().unwrap()
}

#[tokio::test]
async fn a_new_version_that_exits_does_not_take_over() {
    let data = tempfile::tempdir().unwrap();
    let err = await_takeover(data.path(), "abc", &mut exiting(3), Duration::from_secs(10)).await.unwrap_err();
    assert!(err.to_string().contains("exited at start"), "{err:#}");
    assert_eq!(hub_install::current_version(data.path()), None);
}

#[tokio::test]
async fn a_new_version_that_points_current_at_itself_took_over() {
    let data = tempfile::tempdir().unwrap();
    hub_install::set_current(data.path(), "abc").unwrap();
    await_takeover(data.path(), "abc", &mut exiting(0), Duration::from_secs(10)).await.unwrap();
}

#[test]
fn a_step_that_hangs_is_stopped_and_one_that_fails_shows_its_output() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("update.log");
    let shell = |script: &str| -> Vec<String> {
        if cfg!(windows) { vec!["cmd".into(), "/C".into(), script.into()] } else { vec!["sh".into(), "-c".into(), script.into()] }
    };
    let hang = if cfg!(windows) { "ping -n 30 127.0.0.1 >NUL" } else { "sleep 30" };
    let start = std::time::Instant::now();
    let err = run_logged(dir.path(), &shell(hang), &[], &log, Duration::from_secs(1), &AtomicBool::new(false)).unwrap_err();
    assert!(err.to_string().contains("still running after"), "{err:#}");
    assert!(start.elapsed() < Duration::from_secs(15));

    let err = run_logged(dir.path(), &shell("echo the reason && exit 3"), &[], &log, Duration::from_secs(30), &AtomicBool::new(false)).unwrap_err();
    assert!(err.to_string().contains("the reason"), "{err:#}");

    let err = run_logged(dir.path(), &shell(hang), &[], &log, Duration::from_secs(30), &AtomicBool::new(true)).unwrap_err();
    assert!(err.to_string().contains("stopped to install"), "{err:#}");

    // rustc's backtrace pushes the cause out of the last lines; it is still seen.
    let lines = (0..40).map(|n| format!("echo {n}: 0x7ffaae7143e2")).collect::<Vec<_>>().join(" && ");
    let crash = format!("echo memory allocation of 1081360 bytes failed && {lines} && exit 3");
    let err = run_logged(dir.path(), &shell(&crash), &[], &log, Duration::from_secs(30), &AtomicBool::new(false)).unwrap_err();
    assert!(out_of_memory(&format!("{err:#}")), "{err:#}");
    assert!(err.to_string().contains("because it ran out of memory"), "{err:#}");
}

#[test]
fn a_failing_test_step_names_the_tests_that_fell_before_its_last_lines() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("update.log");
    // A failing test whose output pushes everything but the error out of the last lines.
    let noise = (0..30).map(|n| format!("echo noise {n}")).collect::<Vec<_>>().join(" && ");
    let script = format!("echo test a::fine ... ok && echo test rules::only_fewer ... FAILED && {noise} && echo error: test failed && exit 101");
    let shell = if cfg!(windows) { vec!["cmd".to_owned(), "/C".to_owned(), script] } else { vec!["sh".to_owned(), "-c".to_owned(), script] };
    let err = format!("{:#}", run_logged(dir.path(), &shell, &[], &log, Duration::from_secs(30), &AtomicBool::new(false)).unwrap_err());
    let first = err.lines().nth(1).unwrap_or_default();
    assert_eq!(first.trim(), "test rules::only_fewer ... FAILED", "{err}");
    assert!(err.chars().take(4000).collect::<String>().contains("only_fewer"));
}

#[test]
fn the_names_of_fallen_tests_come_from_both_kinds_of_line() {
    let out = "test a::x ... FAILED\nfailures:\n\n---- a::x stdout ----\n    left: 1\n\nfailures:\n    a::x\n    b::y\n\ntest result: FAILED. 3 passed; 2 failed\n    not::a_name\n";
    assert_eq!(failed_tests(out), ["a::x", "b::y"]);
    assert!(failed_tests("test a ... ok\nerror: could not compile").is_empty());
}

#[test]
fn a_long_update_log_is_cut_to_its_tail() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("update.log");
    let line = "x".repeat(99) + "\n";
    std::fs::write(&log, line.repeat(90_000)).unwrap(); // 9 MB
    cap_log(&log);
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(text.len() <= (2 << 20) + 64, "{}", text.len());
    assert!(text.starts_with("== earlier output cut at 8 MB\n") && text.ends_with(&line));
    assert!(text.lines().skip(1).all(|l| l.len() == 99), "whole lines only");
    cap_log(&log);
    assert_eq!(std::fs::read_to_string(&log).unwrap(), text, "a short log is left alone");
}

#[test]
fn pruning_keeps_the_new_and_the_running_version() {
    let data = tempfile::tempdir().unwrap();
    for v in ["old", "new", "dev"] {
        std::fs::create_dir_all(hub_install::bin_dir(data.path()).join(v)).unwrap();
    }
    // The shell wrapper lives beside the builds and is no build.
    std::fs::write(hub_install::bin_dir(data.path()).join("hub-shell.exe"), "x").unwrap();
    prune(data.path(), "new");
    assert!(hub_install::bin_dir(data.path()).join("hub-shell.exe").exists(), "engines still running use it");
    std::fs::remove_file(hub_install::bin_dir(data.path()).join("hub-shell.exe")).unwrap();
    let mut left: Vec<_> = std::fs::read_dir(hub_install::bin_dir(data.path())).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    left.sort();
    // "dev" is this test binary's own version (no VERSION file beside it).
    assert_eq!(left, vec!["dev", "new"]);
}

#[test]
fn pruning_keeps_the_last_good_version_and_what_a_probation_replaced() {
    use hub_install::probation;
    let data = tempfile::tempdir().unwrap();
    for v in ["ancient", "good", "old", "new", "newest"] {
        std::fs::create_dir_all(hub_install::bin_dir(data.path()).join(v)).unwrap();
    }
    std::fs::write(probation::last_good_file(data.path()), "good").unwrap();
    hub_install::set_current(data.path(), "new").unwrap();
    probation::begin(data.path(), "new", Some("old"), None, 0).unwrap();
    prune(data.path(), "newest");
    let mut left: Vec<_> = std::fs::read_dir(hub_install::bin_dir(data.path())).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    left.sort();
    assert_eq!(left, vec!["good", "new", "newest", "old"]);
}

#[test]
fn taking_over_puts_the_version_on_probation() {
    let data = tempfile::tempdir().unwrap();
    hub_install::set_current(data.path(), "old").unwrap();
    std::fs::create_dir_all(crate::backup::dir(data.path())).unwrap();
    std::fs::write(crate::backup::before_update(data.path(), "abc"), "").unwrap();
    begin_probation(data.path(), "abc", hub_install::current_version(data.path()).as_deref());
    hub_install::set_current(data.path(), "abc").unwrap();
    let p = hub_install::probation::read(data.path()).unwrap();
    assert_eq!((p.version.as_str(), p.previous.as_deref(), p.db_backup.as_deref()), ("abc", Some("old"), Some("pre-abc.db")));
    assert_eq!(hub_install::current_version(data.path()).as_deref(), Some("abc"));
}

#[cfg(unix)]
#[test]
fn stopping_a_step_stops_what_it_started() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("update.log");
    let marker = format!("hub-update-test-{}", std::process::id());
    // sh starts sleep as its child; the step is stopped after a second, and the sleep with it.
    let step = vec!["sh".to_owned(), "-c".to_owned(), format!("sleep 30; echo {marker}")];
    let err = run_logged(dir.path(), &step, &[], &log, Duration::from_secs(1), &AtomicBool::new(false)).unwrap_err();
    assert!(err.to_string().contains("still running after"), "{err:#}");
    std::thread::sleep(Duration::from_millis(200));
    let left = Command::new("pgrep").args(["-f", &format!("sleep 30; echo {marker}")]).output().unwrap();
    assert!(left.stdout.is_empty(), "the shell's child is gone too: {}", String::from_utf8_lossy(&left.stdout));
}

#[test]
fn running_out_of_memory_is_told_apart_from_a_broken_build() {
    assert!(out_of_memory("failed to mmap file 'x.rlib': Sidväxlingsfilen är för liten (os error 1455)"));
    assert!(out_of_memory("The paging file is too small for this operation to complete."));
    assert!(out_of_memory("Otillräcklig minneskvot för att kunna avsluta den begärda tjänsten. (os error 1453)"));
    assert!(!out_of_memory("error[E0425]: cannot find value `x` in this scope"));
    assert!(build_jobs() >= 1);
}

#[test]
fn a_broken_build_folder_is_told_apart_from_a_broken_build() {
    assert!(broken_build_folder("error: crate `windows` required to be available in rlib format, but was not found in this form"));
    assert!(broken_build_folder("error[E0463]: can't find crate for `serde`"));
    assert!(!broken_build_folder("error[E0282]: type annotations needed"));
}

#[test]
fn leftovers_in_the_updaters_own_clone_are_discarded() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path();
    let file = |name: &str, text: &str| {
        let path = src.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    git(src, &["init", "--quiet"]).unwrap();
    // Git for Windows checks files out with CRLF by default; this test compares contents.
    git(src, &["config", "core.autocrlf", "false"]).unwrap();
    file(".gitignore", "target/\n");
    file("Cargo.lock", "old\n");
    git(src, &["add", "."]).unwrap();
    git(src, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "--quiet", "-m", "first"]).unwrap();
    file("Cargo.lock", "rewritten by a build\n");
    file("app/new.ts", "generated\n");
    file("target/debug/keep", "cache\n");

    let own = UpdateConfig { source_dir: Some(src.to_owned()), ..UpdateConfig::default() };
    assert!(clean(&own, src).unwrap_err().downcast_ref::<DirtySource>().is_some());

    clean(&UpdateConfig::default(), src).unwrap();
    assert_eq!(git(src, &["status", "--porcelain"]).unwrap(), "");
    assert_eq!(std::fs::read_to_string(src.join("Cargo.lock")).unwrap(), "old\n");
    assert!(src.join("target/debug/keep").exists());
}

fn put(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn commit(dir: &Path, message: &str) {
    git(dir, &["add", "."]).unwrap();
    git(dir, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "--quiet", "-m", message]).unwrap();
}

/// A checkout with what `shell::stamp` and `native_stamp` read, as one commit.
fn checkout() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path();
    git(src, &["init", "--quiet"]).unwrap();
    git(src, &["config", "core.autocrlf", "false"]).unwrap();
    put(src, "app/src-tauri/src/main.rs", "fn main() {}\n");
    put(src, "crates/hub-install/src/lib.rs", "\n");
    put(src, "crates/hub-core/src/lib.rs", "\n");
    put(src, "Cargo.toml", "[workspace]\n");
    put(src, "Cargo.lock", "version = 4\n[[package]]\nname = \"hub-app\"\nversion = \"0.1.0\"\n");
    put(src, "app/package-lock.json", "{}");
    put(src, "app/src/App.svelte", "<p>hi</p>\n");
    commit(src, "first");
    dir
}

#[test]
fn only_a_change_to_the_programs_changes_what_they_are_built_from() {
    let dir = checkout();
    let src = dir.path();
    let first = native_stamp(src).unwrap();
    // Pages, docs and design files are not the programs.
    put(src, "app/src/App.svelte", "<p>changed</p>\n");
    put(src, "docs/FOCUS.md", "more\n");
    commit(src, "pages");
    assert_eq!(native_stamp(src).unwrap(), first);
    // The Rust code, the lock file and the shell are.
    for (file, text) in [("crates/hub-core/src/lib.rs", "pub fn x() {}\n"), ("Cargo.lock", "version = 4\n"), ("app/src-tauri/src/main.rs", "fn main() { 1; }\n")] {
        let before = native_stamp(src).unwrap();
        put(src, file, text);
        commit(src, file);
        assert_ne!(native_stamp(src).unwrap(), before, "{file}");
    }
}

#[test]
fn a_version_of_only_new_pages_keeps_the_programs_of_the_one_before() {
    let dir = checkout();
    let src = dir.path();
    let data = tempfile::tempdir().unwrap();
    let cfg = UpdateConfig::default();

    // A full build was made of this commit: programs, pages and their stamps.
    let before = hub_install::bin_dir(data.path()).join("aaaa");
    for name in ["hubd", "hub-app", "hub-launcher"] {
        put(&before, &hub_install::exe(name), &format!("{name} v1"));
    }
    put(&before, "VERSION", "aaaa");
    put(&before, hub_install::NATIVE, &native_stamp(src).unwrap());
    let shell = crate::shell::stamp(src).unwrap();
    put(&before, hub_install::SHELL, &shell);
    // No stamp yet, or other Rust code: nothing to keep.
    assert_eq!(pages_only_donor(&cfg, data.path(), src, "bbbb"), Some(before.clone()));
    assert!(pages_only_donor(&UpdateConfig { fast_pages: false, ..UpdateConfig::default() }, data.path(), src, "bbbb").is_none());

    put(src, "app/src/App.svelte", "<p>new</p>\n");
    commit(src, "pages");
    let donor = pages_only_donor(&cfg, data.path(), src, "bbbb").unwrap();
    put(src, "app/dist/index.html", "<p>new</p>");
    let partial = hub_install::bin_dir(data.path()).join("bbbb.partial");
    assemble_pages(&donor, src, &partial, "bbbb").unwrap();
    place(data.path(), &partial, "bbbb").unwrap();
    let made = hub_install::bin_dir(data.path()).join("bbbb");
    assert_eq!(std::fs::read_to_string(made.join(hub_install::exe("hubd"))).unwrap(), "hubd v1");
    assert_eq!(std::fs::read_to_string(made.join("ui/index.html")).unwrap(), "<p>new</p>");
    assert_eq!(std::fs::read_to_string(made.join("VERSION")).unwrap(), "bbbb");
    assert_eq!(hub_install::shell_stamp(&made), Some(shell));

    // Other Rust code: a full build is due.
    put(src, "crates/hub-core/src/lib.rs", "pub fn y() {}\n");
    commit(src, "rust");
    assert!(pages_only_donor(&cfg, data.path(), src, "cccc").is_none());
}

#[test]
fn a_rust_change_waits_for_a_quiet_branch_and_pages_do_not() {
    let dir = checkout();
    let src = dir.path();
    let data = tempfile::tempdir().unwrap();
    let at: i64 = git(src, &["log", "-1", "--format=%ct", "HEAD"]).unwrap().trim().parse().unwrap();
    let (just_after, later) = (at * 1000 + 60_000, at * 1000 + 11 * 60_000);

    // Nothing installed has this Rust: it waits until ten minutes after the commit.
    assert_eq!(quiet_until(data.path(), src, "HEAD", "HEAD", 10, 0, just_after), Some(at * 1000 + 10 * 60_000));
    assert_eq!(quiet_until(data.path(), src, "HEAD", "HEAD", 10, 0, later), None);
    assert_eq!(quiet_until(data.path(), src, "HEAD", "HEAD", 0, 0, just_after), None);
    // Merged while the last build ran: that was its wait, so it is next in line at once.
    assert_eq!(quiet_until(data.path(), src, "HEAD", "HEAD", 10, at * 1000 + 30_000, just_after), None);
    // Merged after that build ended: the quiet time counts from the commit.
    assert_eq!(quiet_until(data.path(), src, "HEAD", "HEAD", 10, at * 1000 - 30_000, just_after), Some(at * 1000 + 10 * 60_000));

    // An installed version has the same Rust, so only pages changed: no wait.
    put(&hub_install::bin_dir(data.path()).join("aaaa"), hub_install::NATIVE, &native_stamp(src).unwrap());
    put(src, "app/src/App.svelte", "<p>new</p>\n");
    commit(src, "pages");
    assert_eq!(quiet_until(data.path(), src, "HEAD", "HEAD", 10, 0, just_after), None);

    // Other Rust code waits again.
    put(src, "crates/hub-core/src/lib.rs", "pub fn y() {}\n");
    commit(src, "rust");
    assert!(quiet_until(data.path(), src, "HEAD", "HEAD", 10, 0, now_ms() - 1_000).is_some());
}

#[test]
fn the_build_mode_decides_when_rust_changes_build_and_pages_never_wait() {
    let dir = checkout();
    let src = dir.path();
    let data = tempfile::tempdir().unwrap();
    let running = git(src, &["rev-parse", "HEAD"]).unwrap();
    for n in 0..3 {
        put(src, "crates/hub-core/src/lib.rs", &format!("pub fn x{n}() {{}}\n"));
        commit(src, &format!("rust {n}"));
    }
    put(src, "app/src/App.svelte", "<p>new</p>\n");
    commit(src, "pages");
    let now = now_ms();
    let cfg = |mode, merges| UpdateConfig { build_mode: mode, build_merges: merges, build_max_wait_mins: 60, ..UpdateConfig::default() };

    // Three of five Rust changes wait; the page change is counted but does not build anything.
    let held = hold(data.path(), src, "HEAD", &running, &cfg(BuildMode::Merges, 5), 0, now).unwrap();
    assert_eq!((held.wait.rust, held.wait.total, held.wait.goal), (3, 4, Some(5)));
    assert!(held.until.is_some());
    // The goal reached: it builds.
    assert!(hold(data.path(), src, "HEAD", &running, &cfg(BuildMode::Merges, 3), 0, now).is_none());
    // The oldest change has waited past the cap: it builds, however few wait.
    assert!(hold(data.path(), src, "HEAD", &running, &cfg(BuildMode::Merges, 5), 0, now + 61 * 60_000).is_none());
    // Only on a click: no time, and it is still held long after.
    let manual = hold(data.path(), src, "HEAD", &running, &cfg(BuildMode::Manual, 8), 0, now + 10 * 3_600_000).unwrap();
    assert_eq!((manual.until, manual.wait.mode.as_str()), (None, "manual"));
    // A set time is the next one on that clock; without any, nothing says when, so it builds.
    let times = UpdateConfig { build_times: vec!["23:59".into()], ..cfg(BuildMode::Times, 8) };
    assert!(hold(data.path(), src, "HEAD", &running, &times, 0, now).unwrap().until.is_some());
    assert!(hold(data.path(), src, "HEAD", &running, &cfg(BuildMode::Times, 8), 0, now).is_none());

    // An installed version has this Rust: only pages changed, so no mode holds it.
    put(&hub_install::bin_dir(data.path()).join("aaaa"), hub_install::NATIVE, &native_stamp(src).unwrap());
    assert!(hold(data.path(), src, "HEAD", &running, &cfg(BuildMode::Manual, 8), 0, now).is_none());

    // The list tells Rust from pages, newest first.
    let list = changes_since(src, &running, "HEAD");
    assert_eq!(list.iter().map(|c| c.rust).collect::<Vec<_>>(), vec![false, true, true, true]);
    assert_eq!(list[0].title, "pages");
}

#[test]
fn set_times_are_read_on_the_clock_they_are_given_on() {
    let hour = 3_600_000_i64;
    let t = |s: &str| vec![s.to_owned()];
    let midnight = 20_000 * 24 * hour;
    // 16:30 on a clock two hours ahead of UTC is 14:30 UTC the same day, or the next day's.
    assert_eq!(next_slot(&t("16:30"), 120, midnight), Some(midnight + 14 * hour + 30 * 60_000));
    assert_eq!(next_slot(&t("16:30"), 120, midnight + 15 * hour), Some(midnight + 38 * hour + 30 * 60_000));
    // The first of several, and nonsense is skipped.
    assert_eq!(next_slot(&["25:00".into(), "x".into(), "03:00".into(), "22:00".into()], 0, midnight + 4 * hour), Some(midnight + 22 * hour));
    assert_eq!(next_slot(&[], 0, midnight), None);
}

#[test]
fn merges_that_keep_coming_cannot_hold_a_rust_build_off_for_ever() {
    let hour = 3600;
    let t0 = 1_700_000_000;
    // Quiet for ten minutes after the newest commit, as before.
    assert_eq!(wait_until(t0 + hour, None, 10), (t0 + hour) * 1000 + 600_000);
    assert_eq!(wait_until(t0 + 120, Some(t0), 10), (t0 + 120) * 1000 + 600_000);
    // A branch that has not been quiet for ten minutes in an hour builds thirty minutes after
    // its first unbuilt Rust change.
    assert_eq!(wait_until(t0 + hour, Some(t0), 10), t0 * 1000 + 1_800_000);
}

#[test]
fn docs_that_point_at_a_removed_file_stop_a_pages_only_version() {
    let dir = checkout();
    let src = dir.path();
    covers_hold(src).unwrap();
    put(src, "docs/ARCHITECTURE.md", "# A\n\n## Part\n<!-- covers: app/src/Gone.svelte -->\ntext\n");
    assert!(covers_hold(src).unwrap_err().to_string().contains("Gone.svelte"));
}

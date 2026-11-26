//! Guards the probation verdict and the roll back: when a new version waits, counts as healthy
//! or rolls back (crashes, a window that never said it is alive, missing programs, core-only
//! machines), which version it goes back to, that `last-good` and prune keep what going back
//! needs, and that the probation files read back as written.

use super::*;

fn data() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn install(data: &Path, version: &str) {
    let dir = bin_dir(data).join(version);
    std::fs::create_dir_all(&dir).unwrap();
    for p in PROGRAMS {
        std::fs::write(dir.join(exe(p)), "x").unwrap();
    }
}

/// `new` has taken over from `old` at time 0.
fn on_probation() -> tempfile::TempDir {
    let d = data();
    install(d.path(), "old");
    install(d.path(), "new");
    crate::set_current(d.path(), "new").unwrap();
    begin(d.path(), "new", Some("old"), Some("pre-new.db"), 0).unwrap();
    d
}

const L: Limits = Limits { crashes: 2, alive_secs: 90 };

#[test]
fn the_file_is_read_back_as_it_was_written() {
    let d = on_probation();
    note_window_loaded(d.path(), "new", 5000);
    let p = read(d.path()).unwrap();
    assert_eq!(p.version, "new");
    assert_eq!(p.previous.as_deref(), Some("old"));
    assert_eq!(p.db_backup.as_deref(), Some("pre-new.db"));
    assert_eq!((p.starts, p.clean_exits), (1, 0));
    assert!(p.window_needed && !p.window_ok && !p.core_ok);
    assert_eq!(p.window_loaded_ms, Some(5000));
    let text = std::fs::read_to_string(probation_file(d.path())).unwrap();
    assert!(text.contains("\"version\": \"new\""), "{text}");
}

#[test]
fn names_with_quotes_and_unknown_keys_survive() {
    let d = data();
    let r = RolledBack { from: "a\"b\\c".into(), to: "old".into(), reason: "line\none".into(), at_ms: 7, db_set_aside: None };
    write_rolled_back(d.path(), &r).unwrap();
    assert_eq!(read_rolled_back(d.path()), Some(r));
    std::fs::write(probation_file(d.path()), r#"{"version":"v","started_ms":1,"later":{"x":1},"core_ok":true}"#).unwrap();
    // A key this version does not know (here an object) makes it unreadable rather than wrong.
    assert!(read(d.path()).is_none());
    std::fs::write(probation_file(d.path()), r#"{"version":"v","started_ms":1,"later":"x","n":null,"core_ok":true}"#).unwrap();
    assert!(read(d.path()).unwrap().core_ok);
}

#[test]
fn a_version_not_on_probation_is_healthy() {
    assert_eq!(verdict(data().path(), 0, L), Verdict::Healthy);
}

#[test]
fn a_fresh_version_waits() {
    let d = on_probation();
    assert_eq!(verdict(d.path(), 10_000, L), Verdict::Wait);
}

#[test]
fn a_version_with_a_healthy_core_and_no_window_is_healthy() {
    let d = on_probation();
    note_core_ok(d.path(), "new");
    assert_eq!(verdict(d.path(), 0, L), Verdict::Healthy);
}

#[test]
fn a_window_that_is_needed_must_have_said_it_is_alive() {
    let d = on_probation();
    note_core_ok(d.path(), "new");
    note_window_loaded(d.path(), "new", 1000);
    assert_eq!(verdict(d.path(), 50_000, L), Verdict::Wait);
    // 90 s after the pages loaded, still silent: but the core is healthy, so only the window fails.
    assert_eq!(verdict(d.path(), 1000 + 91_000, L), Verdict::RollBack(Reason::NoWindow));
    note_alive(d.path(), "new");
    assert_eq!(verdict(d.path(), 1000 + 91_000, L), Verdict::Healthy);
}

#[test]
fn two_crashes_roll_back_and_a_clean_exit_is_not_one() {
    let d = on_probation();
    note_clean_exit(d.path(), "new");
    note_start(d.path(), "new");
    assert_eq!(read(d.path()).unwrap().crashes(), 0);
    assert_eq!(verdict(d.path(), 0, L), Verdict::Wait);
    note_start(d.path(), "new");
    assert_eq!(verdict(d.path(), 0, L), Verdict::Wait);
    note_start(d.path(), "new");
    assert_eq!(verdict(d.path(), 0, L), Verdict::RollBack(Reason::Crashed(2)));
}

#[test]
fn many_asked_for_restarts_never_roll_back() {
    let d = on_probation();
    for _ in 0..10 {
        note_clean_exit(d.path(), "new");
        note_start(d.path(), "new");
    }
    assert_eq!(read(d.path()).unwrap().crashes(), 0);
    assert_eq!(verdict(d.path(), 0, L), Verdict::Wait);
}

#[test]
fn a_version_that_lost_its_programs_rolls_back_at_once() {
    let d = on_probation();
    std::fs::remove_file(bin_dir(d.path()).join("new").join(exe("hubd"))).unwrap();
    assert_eq!(verdict(d.path(), 0, L), Verdict::RollBack(Reason::Missing));
}

/// A machine that only runs the core: `hubd` and the list that says so.
fn core_only(data: &Path, version: &str) {
    let dir = bin_dir(data).join(version);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(exe("hubd")), "x").unwrap();
    std::fs::write(dir.join(PROGRAMS_FILE), "hubd").unwrap();
}

#[test]
fn a_core_only_version_is_complete_and_not_rolled_back() {
    let d = data();
    install(d.path(), "old");
    core_only(d.path(), "new");
    crate::set_current(d.path(), "new").unwrap();
    begin(d.path(), "new", Some("old"), None, 0).unwrap();
    assert!(complete(d.path(), "new"));
    assert_eq!(verdict(d.path(), 0, L), Verdict::Wait);
}

#[test]
fn a_core_only_version_without_its_core_is_still_missing() {
    let d = data();
    install(d.path(), "old");
    core_only(d.path(), "new");
    std::fs::remove_file(bin_dir(d.path()).join("new").join(exe("hubd"))).unwrap();
    crate::set_current(d.path(), "new").unwrap();
    begin(d.path(), "new", Some("old"), None, 0).unwrap();
    assert_eq!(verdict(d.path(), 0, L), Verdict::RollBack(Reason::Missing));
}

#[test]
fn a_folder_without_a_list_needs_every_program() {
    let d = data();
    install(d.path(), "v");
    std::fs::remove_file(bin_dir(d.path()).join("v").join(exe("hub-launcher"))).unwrap();
    assert!(!complete(d.path(), "v"));
}

#[test]
fn a_core_pushed_before_the_list_existed_is_core_only() {
    let d = data();
    install(d.path(), "old");
    core_only(d.path(), "new");
    std::fs::remove_file(bin_dir(d.path()).join("new").join(PROGRAMS_FILE)).unwrap();
    crate::set_current(d.path(), "new").unwrap();
    begin(d.path(), "new", Some("old"), None, 0).unwrap();
    assert!(complete(d.path(), "new"));
    assert_eq!(verdict(d.path(), 0, L), Verdict::Wait);
}

#[test]
fn a_program_the_list_names_must_be_there() {
    let d = data();
    install(d.path(), "v");
    std::fs::write(bin_dir(d.path()).join("v").join(PROGRAMS_FILE), "hubd\nhub-app").unwrap();
    std::fs::remove_file(bin_dir(d.path()).join("v").join(exe("hub-app"))).unwrap();
    assert!(!complete(d.path(), "v"));
}

#[test]
fn a_probation_current_no_longer_points_at_is_left_alone() {
    let d = on_probation();
    crate::set_current(d.path(), "newer").unwrap();
    std::fs::remove_file(bin_dir(d.path()).join("new").join(exe("hubd"))).unwrap();
    assert_eq!(verdict(d.path(), 0, L), Verdict::Healthy);
}

#[test]
fn a_version_left_behind_is_not_recorded_as_good() {
    let d = on_probation();
    crate::set_current(d.path(), "newer").unwrap();
    confirm_healthy(d.path(), "new").unwrap();
    assert_eq!(last_good(d.path()), None);
}

#[test]
fn a_version_taking_over_from_an_unproven_one_goes_back_past_it() {
    let d = on_probation();
    install(d.path(), "good");
    std::fs::write(last_good_file(d.path()), "good").unwrap();
    crate::set_current(d.path(), "newer").unwrap();
    begin(d.path(), "newer", Some("new"), None, 5).unwrap();
    assert_eq!(read(d.path()).unwrap().previous.as_deref(), Some("good"));
}

#[test]
fn a_shell_that_never_loads_the_pages_still_runs_out_of_time() {
    let d = on_probation();
    note_core_ok(d.path(), "new");
    note_window_needed(d.path(), "new", 1000);
    assert_eq!(verdict(d.path(), 50_000, L), Verdict::Wait);
    assert_eq!(verdict(d.path(), 92_000, L), Verdict::RollBack(Reason::NoWindow));
}

#[test]
fn dev_builds_are_never_on_probation() {
    let d = data();
    begin(d.path(), "dev", Some("old"), None, 0).unwrap();
    assert!(read(d.path()).is_none());
}

#[test]
fn the_target_is_what_it_replaced_then_the_last_good_then_the_newest_other() {
    let d = on_probation();
    assert_eq!(roll_back_target(d.path(), "new").as_deref(), Some("old"));
    // `previous` gone: last-good.
    std::fs::remove_dir_all(bin_dir(d.path()).join("old")).unwrap();
    install(d.path(), "good");
    install(d.path(), "other");
    crate::set_current(d.path(), "good").unwrap();
    confirm_healthy(d.path(), "good").unwrap();
    crate::set_current(d.path(), "new").unwrap();
    // confirm_healthy of another version leaves the probation of `new` alone.
    assert!(read(d.path()).is_some());
    assert_eq!(roll_back_target(d.path(), "new").as_deref(), Some("good"));
    // No last-good either: whichever other complete version is newest.
    std::fs::remove_file(last_good_file(d.path())).unwrap();
    assert_eq!(roll_back_target(d.path(), "new").as_deref(), Some("other"));
    // Half a folder is not a target.
    std::fs::remove_file(bin_dir(d.path()).join("other").join(exe("hub-app"))).unwrap();
    std::fs::remove_dir_all(bin_dir(d.path()).join("good")).unwrap();
    assert_eq!(roll_back_target(d.path(), "new"), None);
}

#[test]
fn act_moves_current_back_and_tells_what_happened() {
    let d = on_probation();
    assert_eq!(act_at(d.path(), 0, L), Acted::Nothing);
    std::fs::remove_file(bin_dir(d.path()).join("new").join(exe("hubd"))).unwrap();
    let Acted::RolledBack(r) = act_at(d.path(), 0, L) else { panic!("not rolled back") };
    assert_eq!((r.from.as_str(), r.to.as_str()), ("new", "old"));
    assert_eq!(current_version(d.path()).as_deref(), Some("old"));
    assert_eq!(std::fs::read_to_string(d.path().join("update-failed")).unwrap(), "new");
    assert_eq!(read_rolled_back(d.path()), Some(r));
    assert!(read(d.path()).is_none());
    // Whoever comes second finds nothing left to do.
    assert_eq!(act_at(d.path(), 0, L), Acted::Nothing);
}

#[test]
fn act_leaves_current_when_there_is_nothing_to_go_back_to() {
    let d = data();
    install(d.path(), "only");
    crate::set_current(d.path(), "only").unwrap();
    begin(d.path(), "only", None, None, 0).unwrap();
    std::fs::remove_file(bin_dir(d.path()).join("only").join(exe("hubd"))).unwrap();
    assert_eq!(act_at(d.path(), 0, L), Acted::NoTarget(Reason::Missing));
    assert_eq!(current_version(d.path()).as_deref(), Some("only"));
}

#[test]
fn a_roll_back_without_a_probation_still_works_for_a_version_that_cannot_start() {
    let d = data();
    install(d.path(), "good");
    install(d.path(), "broken");
    std::fs::write(last_good_file(d.path()), "good").unwrap();
    crate::set_current(d.path(), "broken").unwrap();
    let r = roll_back(d.path(), "broken", "x", 1).unwrap();
    assert_eq!(r.to, "good");
    assert_eq!(current_version(d.path()).as_deref(), Some("good"));
}

#[test]
fn a_healthy_version_becomes_the_last_good_and_ends_its_probation() {
    let d = on_probation();
    confirm_healthy(d.path(), "new").unwrap();
    assert_eq!(last_good(d.path()).as_deref(), Some("new"));
    assert!(read(d.path()).is_none());
}

#[test]
fn a_second_start_while_on_probation_keeps_the_count_and_the_previous_version() {
    let d = on_probation();
    begin(d.path(), "new", Some("new"), None, 99).unwrap();
    let p = read(d.path()).unwrap();
    assert_eq!((p.starts, p.started_ms, p.previous.as_deref()), (2, 0, Some("old")));
}

#[test]
fn a_prune_keeps_what_going_back_needs() {
    let d = on_probation();
    crate::set_current(d.path(), "old").unwrap();
    confirm_healthy(d.path(), "old").unwrap();
    crate::set_current(d.path(), "new").unwrap();
    let mut keep = keep(d.path());
    keep.sort();
    keep.dedup();
    assert_eq!(keep, vec!["new", "old"]);
}

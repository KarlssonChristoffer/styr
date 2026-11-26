//! Starts the installed version that `current` names and exits right away, so it is never
//! running when the updater replaces it. Before it starts anything it asks `hub_install::probation`
//! whether that version is failing its probation and, if so, goes back to the one before; the
//! decision is made there and is the same one the app's shell and the core take.
#![cfg_attr(windows, windows_subsystem = "windows")]

use hub_install::probation::{self, Acted, Reason};

fn main() {
    let data = hub_install::data_dir();
    match probation::act(&data) {
        Acted::RolledBack(r) => eprintln!("went back from {} to {}: {}", r.from, r.to, r.reason),
        Acted::NoTarget(reason) => eprintln!("the new version is failing ({reason}) but there is nothing to go back to"),
        Acted::Nothing => {}
    }
    // A version that cannot be started at all (a folder gone, a program broken) is rolled back
    // the same way; three tries at most, so a broken install never loops.
    for _ in 0..3 {
        let Some(version) = hub_install::current_version(&data) else {
            eprintln!("Hub is not installed yet; run scripts/install.ps1");
            std::process::exit(1);
        };
        let app = hub_install::bin_dir(&data).join(&version).join(hub_install::exe("hub-app"));
        // From the data folder, not the version's own: a folder a running program is in cannot be
        // removed on Windows, and the updater removes old versions.
        let err = match hub_install::proc::command(&app).current_dir(&data).spawn() {
            Ok(_) => return,
            Err(err) => err,
        };
        eprintln!("could not start {}: {err}", app.display());
        // Only a version that is really not there: a lock or a virus scanner is no reason to
        // blacklist a commit, and a version that has every program it was installed with (a core-only
        // one has no app) is not missing anything.
        if probation::complete(&data, &version) {
            std::process::exit(1);
        }
        if probation::roll_back(&data, &version, &Reason::Missing.to_string(), probation::now_ms()).is_none() {
            std::process::exit(1);
        }
    }
    std::process::exit(1);
}

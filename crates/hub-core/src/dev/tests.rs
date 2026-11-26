//! Tests of dev mode: the clone, its packages and build cache, shipping, and the preview's git
//! steps, look for pushes, merged list and served-page check.

#[test]
fn the_preview_looks_often_only_while_watched() {
    let now = 10_000_000;
    assert_eq!(preview_wait_after(now - 30_000, now), PREVIEW_EVERY);
    assert_eq!(preview_wait_after(now - WATCHING_MS - 1, now), PREVIEW_IDLE_EVERY);
    assert_eq!(preview_wait_after(0, now), PREVIEW_IDLE_EVERY);
}

use super::*;
use super::build::{PACKAGES_STAMP, packages_stale, packages_stamp};
use super::ship::push_branch;
use super::preview::{
    LIVE, PREVIEW_BRANCH, PREVIEW_EVERY, PREVIEW_IDLE_EVERY, WATCHING_MS, abort_merge, build_preview, drop_ref, leave_preview, news, not_installed, park, phrases,
    preview_only, preview_wait_after, serves_old,
};
use crate::dev_state::{MergeCommit, MergedPull};
use crate::model::DevPreviewPr;

fn sh(dir: &Path, args: &[&str]) {
    git(dir, args).unwrap();
}

#[test]
fn packages_are_stale_until_installed_and_again_when_the_lock_changes() {
    let tmp = tempfile::tempdir().unwrap();
    let app = tmp.path();
    std::fs::write(app.join("package.json"), "{\"dependencies\":{}}").unwrap();
    std::fs::write(app.join("package-lock.json"), "{}").unwrap();
    assert!(packages_stale(app), "no node_modules yet");
    std::fs::create_dir_all(app.join("node_modules")).unwrap();
    assert!(packages_stale(app), "node_modules without a stamp: unknown, so install");
    std::fs::write(app.join(PACKAGES_STAMP), packages_stamp(app)).unwrap();
    assert!(!packages_stale(app), "installed from these files");
    std::fs::write(app.join("package.json"), "{\"dependencies\":{\"katex\":\"^0.16\"}}").unwrap();
    assert!(packages_stale(app), "a package was added after the install");
}

#[test]
fn the_clone_comes_from_the_updaters_clone_and_follows_its_remote() {
    let tmp = tempfile::tempdir().unwrap();
    let (remote, source, dev) = (tmp.path().join("remote.git"), tmp.path().join("src"), tmp.path().join("dev/hub"));
    std::fs::create_dir_all(&remote).unwrap();
    sh(&remote, &["init", "--bare", "-b", "main"]);
    sh(tmp.path(), &["clone", &remote.display().to_string(), "src"]);
    sh(&source, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "--allow-empty", "-m", "first"]);
    sh(&source, &["push", "origin", "main"]);

    clone(&source, &dev, "origin", "main", &tmp.path().join("dev.log")).unwrap();
    assert_eq!(git(&dev, &["remote", "get-url", "origin"]).unwrap(), git(&source, &["remote", "get-url", "origin"]).unwrap());
    assert_eq!(git_state(&dev, "origin/main"), (Some("main".into()), 0, 0));

    std::fs::write(dev.join("new.txt"), "x").unwrap();
    sh(&dev, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "--allow-empty", "-m", "second"]);
    assert_eq!(git_state(&dev, "origin/main"), (Some("main".into()), 1, 1));
    // Running it again leaves the clone alone.
    clone(&source, &dev, "origin", "main", &tmp.path().join("dev.log")).unwrap();
}

#[test]
fn shipping_pushes_a_branch_of_its_own_and_leaves_main_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let (remote, source, dev) = (tmp.path().join("remote.git"), tmp.path().join("src"), tmp.path().join("dev/hub"));
    std::fs::create_dir_all(&remote).unwrap();
    sh(&remote, &["init", "--bare", "-b", "main"]);
    sh(tmp.path(), &["clone", &remote.display().to_string(), "src"]);
    sh(&source, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "--allow-empty", "-m", "first"]);
    sh(&source, &["push", "origin", "main"]);
    clone(&source, &dev, "origin", "main", &tmp.path().join("dev.log")).unwrap();
    sh(&dev, &["config", "user.name", "t"]);
    sh(&dev, &["config", "user.email", "t@t"]);
    let main_before = git(&remote, &["rev-parse", "main"]).unwrap();

    // A commit a thread made on main, and a file changed since.
    sh(&dev, &["commit", "--allow-empty", "-m", "made on main"]);
    std::fs::write(dev.join("page.svelte"), "green").unwrap();
    sh(&dev, &["add", "-A"]);
    let name = branch_name("Make the page green", 1_700_000_000_000);
    assert!(name.starts_with("hub-dev/make-the-page-green-"), "{name}");
    let shipped = push_branch(&dev, "origin", "main", &name, false, Some("Make the page green")).unwrap();
    assert!(shipped.ends_with(" Make the page green"));

    assert_eq!(git(&remote, &["rev-parse", "main"]).unwrap(), main_before, "main is the merge queue's");
    assert_eq!(git(&remote, &["rev-list", "--count", &format!("main..{name}")]).unwrap(), "2");
    assert_eq!(git(&dev, &["rev-parse", "main"]).unwrap(), main_before, "the clone's main goes back to the remote's");
    assert_eq!(git(&dev, &["branch", "--show-current"]).unwrap(), name);

    // More work goes to the same branch while its pull request is open.
    std::fs::write(dev.join("page.svelte"), "greener").unwrap();
    sh(&dev, &["add", "-A"]);
    push_branch(&dev, "origin", "main", &name, true, Some("Greener")).unwrap();
    assert_eq!(git(&remote, &["rev-list", "--count", &format!("main..{name}")]).unwrap(), "3");

    // Nothing new on a fresh branch from main is nothing to ship.
    sh(&dev, &["switch", "main"]);
    let err = push_branch(&dev, "origin", "main", "hub-dev/empty", false, None).unwrap_err();
    assert!(err.to_string().contains("nothing to ship"), "{err}");
}

#[test]
fn a_title_without_letters_still_names_a_branch() {
    assert!(branch_name("!!!", 0).starts_with("hub-dev/"));
    assert!(!branch_name("Fix: the — thing", 0).contains("--"));
}

#[test]
fn a_folder_with_other_things_in_it_is_not_cloned_into() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("notes.txt"), "mine").unwrap();
    let err = clone(tmp.path(), tmp.path(), "origin", "main", &tmp.path().join("dev.log")).unwrap_err();
    assert!(err.to_string().contains("not a clone of Hub"));
}

#[test]
fn the_preview_merges_open_pull_requests_and_keeps_what_was_changed() {
    let tmp = tempfile::tempdir().unwrap();
    let (remote, source, dev) = (tmp.path().join("remote.git"), tmp.path().join("src"), tmp.path().join("dev/hub"));
    std::fs::create_dir_all(&remote).unwrap();
    sh(&remote, &["init", "--bare", "-b", "main"]);
    sh(tmp.path(), &["clone", &remote.display().to_string(), "src"]);
    sh(&source, &["config", "user.name", "t"]);
    sh(&source, &["config", "user.email", "t@t"]);
    std::fs::write(source.join("page.svelte"), "grey").unwrap();
    sh(&source, &["add", "-A"]);
    sh(&source, &["commit", "-m", "first"]);
    sh(&source, &["push", "origin", "main"]);
    // Three pull requests as GitHub keeps them: one new file, one that changes the core,
    // and one that clashes with the first.
    let pull = |n: u32, file: &str, text: &str| {
        sh(&source, &["switch", "-q", "-c", &format!("pr{n}"), "main"]);
        if let Some(parent) = Path::new(file).parent() {
            std::fs::create_dir_all(source.join(parent)).unwrap();
        }
        std::fs::write(source.join(file), text).unwrap();
        sh(&source, &["add", "-A"]);
        sh(&source, &["commit", "-m", &format!("pr {n}")]);
        sh(&source, &["push", "origin", &format!("HEAD:refs/pull/{n}/head")]);
    };
    pull(1, "page.svelte", "green");
    pull(2, "crates/core.rs", "fn main() {}");
    pull(3, "page.svelte", "red");
    pull(4, "app/x.svelte", "new");
    clone(&source, &dev, "origin", "main", &tmp.path().join("dev.log")).unwrap();
    std::fs::write(dev.join("page.svelte"), "half done").unwrap();

    let pr = |number| DevPreviewPr { number, title: format!("PR {number}"), url: String::new(), why: None, files: None };
    let built = build_preview(&dev, "origin", "main", &[pr(1), pr(2), pr(3)], false, 1_700_000_000_000).unwrap();
    assert_eq!(built.included.iter().map(|p| p.number).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(built.skipped.len(), 1);
    assert!(built.skipped[0].why.as_deref().unwrap().contains("the ones before it"));
    assert!(built.core_changed);
    assert!(built.app_changed, "the changed file was kept aside, so what the server showed is gone");
    assert_eq!(std::fs::read_to_string(dev.join("page.svelte")).unwrap(), "green");
    assert!(dev.join("crates/core.rs").exists());
    assert_eq!(git(&dev, &["branch", "--show-current"]).unwrap(), PREVIEW_BRANCH);
    assert_eq!(git(&dev, &["status", "--porcelain"]).unwrap(), "", "a clashing merge leaves nothing behind");

    // What was changed in the clone is on a branch of its own.
    let parked = built.parked.expect("the changed file was parked");
    assert_eq!(git(&dev, &["show", &format!("{parked}:page.svelte")]).unwrap(), "half done");

    // One pull request only, and back again.
    let built = build_preview(&dev, "origin", "main", &[pr(3)], false, 1_700_000_001_000).unwrap();
    assert!(built.parked.is_none() && !built.core_changed);
    assert_eq!(std::fs::read_to_string(dev.join("page.svelte")).unwrap(), "red");
    assert_eq!(leave_preview(&dev, "origin", "main", 1_700_000_002_000).unwrap(), None);
    assert_eq!(git(&dev, &["branch", "--show-current"]).unwrap(), "main");
    assert_eq!(std::fs::read_to_string(dev.join("page.svelte")).unwrap(), "grey");
    assert_eq!(git(&remote, &["for-each-ref", "--format=%(refname)", "refs/heads"]).unwrap(), "refs/heads/main", "nothing was pushed");

    // Live mode: only what changes the pages, so it runs against the installed core.
    let built = build_preview(&dev, "origin", "main", &[pr(1), pr(2), pr(3)], true, 1_700_000_003_000).unwrap();
    assert_eq!(built.included.iter().map(|p| p.number).collect::<Vec<_>>(), [1]);
    assert_eq!(built.skipped.iter().map(|p| p.number).collect::<Vec<_>>(), [2, 3]);
    assert!(built.skipped[0].why.as_deref().unwrap().contains("Rust"));
    assert!(!built.core_changed);
    assert!(!dev.join("crates/core.rs").exists());
    assert_eq!(std::fs::read_to_string(dev.join("page.svelte")).unwrap(), "green");
    assert_eq!(built.included[0].files.as_deref(), Some(&["page.svelte".to_owned()][..]), "what it changes, to blame it by");

    // A pull request GitHub lists as open but has no head for leaves the others alone.
    let built = build_preview(&dev, "origin", "main", &[pr(1), pr(9)], true, 1_700_000_004_000).unwrap();
    assert_eq!(built.included.iter().map(|p| p.number).collect::<Vec<_>>(), [1]);
    assert_eq!(built.skipped.iter().map(|p| p.number).collect::<Vec<_>>(), [9]);
    assert!(built.skipped[0].why.as_deref().unwrap().contains("no head"));

    // Vite is to start again when a page changed, and only then.
    let built = build_preview(&dev, "origin", "main", &[pr(1), pr(4)], true, 1_700_000_005_000).unwrap();
    assert!(built.app_changed);
    let built = build_preview(&dev, "origin", "main", &[pr(1), pr(4)], true, 1_700_000_006_000).unwrap();
    assert!(!built.app_changed, "the same pages again need no restart");
    let built = build_preview(&dev, "origin", "main", &[pr(1)], true, 1_700_000_007_000).unwrap();
    assert!(built.app_changed, "a pull request that went away takes its pages with it");
}

#[test]
fn a_merge_is_aborted_only_when_one_is_in_progress_and_a_gone_ref_is_no_failure() {
    let tmp = tempfile::tempdir().unwrap();
    let (repo, wt) = (tmp.path().join("repo"), tmp.path().join("wt"));
    std::fs::create_dir_all(&repo).unwrap();
    sh(&repo, &["init", "-q", "-b", "main"]);
    sh(&repo, &["config", "user.name", "t"]);
    sh(&repo, &["config", "user.email", "t@t"]);
    std::fs::write(repo.join("page.svelte"), "grey").unwrap();
    sh(&repo, &["add", "-A"]);
    sh(&repo, &["commit", "-q", "-m", "first"]);
    sh(&repo, &["switch", "-q", "-c", "other"]);
    std::fs::write(repo.join("page.svelte"), "red").unwrap();
    sh(&repo, &["commit", "-q", "-am", "red"]);
    sh(&repo, &["switch", "-q", "main"]);
    std::fs::write(repo.join("page.svelte"), "green").unwrap();
    sh(&repo, &["commit", "-q", "-am", "green"]);
    let merging = |dir: &Path| git(dir, &["rev-parse", "-q", "--verify", "MERGE_HEAD"]).is_ok();

    // No merge in progress: nothing to abort, and that is no failure (so nothing is logged).
    abort_merge(&repo).unwrap();
    assert_eq!(park(&repo, 1_700_000_000_000).unwrap(), None);

    // A clashing merge left half done, as a refresh that was cut off leaves it.
    assert!(git(&repo, &["merge", "other"]).is_err(), "the merge clashes");
    assert!(merging(&repo));
    assert_eq!(park(&repo, 1_700_000_001_000).unwrap(), None, "the aborted merge leaves nothing to park");
    assert!(!merging(&repo));
    assert_eq!(std::fs::read_to_string(repo.join("page.svelte")).unwrap(), "green");

    // The same in a linked worktree, whose MERGE_HEAD is in its own git dir.
    sh(&repo, &["worktree", "add", "-q", "-b", "wt", &wt.display().to_string()]);
    abort_merge(&wt).unwrap();
    assert!(git(&wt, &["merge", "other"]).is_err());
    assert!(merging(&wt) && !merging(&repo));
    abort_merge(&repo).unwrap();
    assert!(merging(&wt), "the main checkout's abort leaves the worktree's merge alone");
    abort_merge(&wt).unwrap();
    assert!(!merging(&wt));
    assert_eq!(git(&wt, &["status", "--porcelain"]).unwrap(), "");

    // A preview head that is gone already is no failure; one that is there goes.
    drop_ref(&repo, "refs/hub-preview/9").unwrap();
    sh(&repo, &["update-ref", "refs/hub-preview/9", "HEAD"]);
    drop_ref(&repo, "refs/hub-preview/9").unwrap();
    assert!(git(&repo, &["rev-parse", "-q", "--verify", "refs/hub-preview/9"]).is_err());
}

#[test]
fn a_look_for_pushes_says_what_moved_since_the_build() {
    // Nothing to compare with before the first build.
    assert_eq!(news(None, "aaa live 1:x"), (vec![], false));
    assert_eq!(news(Some("aaa live 1:x 2:y"), "aaa live 1:x 2:y"), (vec![], false));
    // A new head, a new pull request, and a pull request that went away (merged) are told apart.
    assert_eq!(news(Some("aaa live 1:x 2:y"), "aaa live 1:z 2:y 3:w"), (vec![1, 3], false));
    assert_eq!(news(Some("aaa live 1:x 2:y"), "bbb live 1:x"), (vec![], true));
}

#[test]
fn a_merged_pull_request_is_listed_until_the_installed_version_has_it() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    sh(dir, &["init", "-q", "-b", "main"]);
    sh(dir, &["config", "user.name", "t"]);
    sh(dir, &["config", "user.email", "t@t"]);
    let commit = |what: &str| {
        std::fs::write(dir.join("f"), what).unwrap();
        sh(dir, &["add", "-A"]);
        sh(dir, &["commit", "-q", "-m", what]);
        git(dir, &["rev-parse", "HEAD"]).unwrap()
    };
    let installed = commit("first");
    let first = commit("second");
    let second = commit("third");
    let pull = |number: u32, oid: &str| MergedPull {
        number,
        title: format!("PR {number}"),
        url: String::new(),
        merge_commit: Some(MergeCommit { oid: oid.to_owned() }),
    };
    let merged = [pull(7, &second), pull(5, &first), pull(3, &installed)];
    let list = not_installed(dir, &installed[..12], &merged, "merged, being built");
    assert_eq!(list.iter().map(|p| p.number).collect::<Vec<_>>(), [5, 7], "the one the installed version has is not listed");
    assert_eq!(list[0].why.as_deref(), Some("merged, being built"));
    // A version this clone has not got, or a development build, says nothing.
    assert!(not_installed(dir, "0123456789ab", &merged, "x").is_empty());
    assert!(not_installed(dir, "dev", &merged, "x").is_empty());
}

#[test]
fn a_page_served_old_is_told_from_one_served_as_it_is() {
    let added = "<span>Share of all</span>\n// a comment about something else\n{#if x} <b>Inside Hub</b> {/if}\nconst a = 1;";
    let needles = phrases(added);
    assert_eq!(needles, ["Share of all", "Inside Hub"]);
    // Compiled output keeps the text, with whatever whitespace the compiler left.
    assert!(!serves_old("t(\"Share  of\n all\")", &needles));
    assert!(serves_old("t(\"Outside Hub\")", &needles), "the old page has none of the new words");
    assert!(!serves_old("anything", &[]), "a change with no words in it proves nothing");
}

#[test]
fn the_preview_setting_reads_back() {
    assert_eq!(preview_only("all"), None);
    assert_eq!(preview_only("pr:129"), Some(129));
    assert_eq!(preview_only(LIVE), None);
}

#[test]
fn the_cache_is_trimmed_past_its_cap() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("target");
    std::fs::create_dir_all(target.join("debug/incremental")).unwrap();
    std::fs::write(target.join("debug/incremental/a"), vec![0u8; 3000]).unwrap();
    std::fs::write(target.join("debug/hubd"), vec![0u8; 1000]).unwrap();
    assert_eq!(trim_cache(&target, 10_000), 4000);
    assert_eq!(trim_cache(&target, 2000), 1000);
    assert!(target.join("debug/hubd").exists());
    assert_eq!(trim_cache(&target, 500), 0);
}

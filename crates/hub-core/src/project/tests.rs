//! Guards the project page's reading and writing: every path stays inside the project (raw
//! reads, downloads that never overwrite, images under `docs/images`, reveal), the plan's
//! checkboxes read and edited, the stack found, GitHub links from remotes, repo names and the
//! setup prompt.

#[test]
fn raw_reads_known_kinds_inside_the_project() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.PNG"), [137, 80, 78, 71]).unwrap();
    std::fs::write(dir.path().join("b.txt"), "x").unwrap();
    let (mime, bytes) = super::raw(dir.path(), "a.PNG").unwrap();
    assert_eq!((mime, bytes.len()), ("image/png", 4));
    assert!(super::raw(dir.path(), "b.txt").is_err(), "text is read as text");
    assert!(super::raw(dir.path(), "../a.png").is_err(), "stays inside the project");
    assert_eq!(super::raw_mime("doc.pdf"), Some("application/pdf"));
}

use super::*;

#[test]
fn downloads_keep_to_their_folder_and_never_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let first = save_in(dir.path(), "picture.png", b"one").unwrap();
    let second = save_in(dir.path(), "picture.png", b"two").unwrap();
    assert_eq!(first, dir.path().join("picture.png"));
    assert_eq!(second, dir.path().join("picture (1).png"));
    assert_eq!(std::fs::read(&first).unwrap(), b"one");
    let escaped = save_in(dir.path(), "..\\..\\evil?.png", b"x").unwrap();
    assert_eq!(escaped, dir.path().join("evil_.png"));
    assert_eq!(save_in(dir.path(), "..", b"x").unwrap(), dir.path().join("download"));
}

#[test]
fn project_images_go_to_docs_images_of_a_real_folder() {
    let dir = tempfile::tempdir().unwrap();
    let saved = add_image(dir.path(), "shot.png", b"png").unwrap();
    assert_eq!(saved, dir.path().join("docs").join("images").join("shot.png"));
    assert!(add_image(&dir.path().join("missing"), "shot.png", b"png").is_err());
}

#[test]
fn reveal_refuses_paths_that_do_not_exist() {
    let missing = std::env::temp_dir().join("hub-reveal-missing-7f3a").join("nope.txt");
    assert!(reveal(&missing).is_err());
}

#[test]
fn reads_and_edits_the_plan() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let added = edit_plan(root, &PlanEdit { path: String::new(), line: None, done: None, text: Some("First  step".into()) }).unwrap();
    assert_eq!(added.len(), 1);
    assert_eq!(added[0].text, "First step");
    edit_plan(root, &PlanEdit { path: String::new(), line: None, done: None, text: Some("Second".into()) }).unwrap();
    let ticked = edit_plan(root, &PlanEdit { path: String::new(), line: Some(added[0].line), done: Some(true), text: None }).unwrap();
    assert!(ticked[0].done && !ticked[1].done);
    let text = std::fs::read_to_string(root.join(PLAN)).unwrap();
    assert!(text.starts_with("# Plan\n") && text.contains("- [x] First step\n- [ ] Second\n"));
    // A line that is not a step is refused rather than rewritten.
    assert!(edit_plan(root, &PlanEdit { path: String::new(), line: Some(0), done: Some(true), text: None }).is_err());
}

#[test]
fn repo_names_and_setup_prompts() {
    assert_eq!(repo_name("Untitled game"), "Untitled-game");
    assert_eq!(repo_name("  Spel: Åsa & co!  "), "Spel-sa-co");
    assert_eq!(repo_name("???"), "project");
    let prompt = setup_prompt("Spel", Some("Ett litet plattformsspel"), Some("https://github.com/a/Spel"), None);
    assert!(prompt.contains("Ett litet plattformsspel") && prompt.contains("push to the GitHub repository (https://github.com/a/Spel)"));
    assert!(setup_prompt("X", None, None, None).contains("4. Commit the result."));
    let from_template = setup_prompt("X", Some("a tool"), None, Some("Rust command line tool (clap)"));
    assert!(from_template.contains("already wrote a starter") && from_template.contains("Do not scaffold again"));
    assert!(from_template.contains("4. Commit the result."));
}

#[test]
fn parses_checkboxes_only() {
    let items = parse_plan("# Plan\n\n- [x] Done\n  * [X] Nested done\n- [ ] Next\n- plain\n- [ ]\n");
    let got: Vec<_> = items.iter().map(|i| (i.text.as_str(), i.done, i.line)).collect();
    assert_eq!(got, vec![("Done", true, 2), ("Nested done", true, 3), ("Next", false, 4)]);
}

#[test]
fn finds_the_stack() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("Cargo.toml"), "[workspace.dependencies]\ntokio = \"1\"\naxum.workspace = true\ntokio-util = \"0.7\"\n").unwrap();
    std::fs::create_dir_all(root.join("app")).unwrap();
    std::fs::write(root.join("app/package.json"), r#"{"dependencies":{"svelte":"5"},"devDependencies":{"typescript":"5","vite":"6"}}"#).unwrap();
    std::fs::create_dir_all(root.join("node_modules/x")).unwrap();
    std::fs::write(root.join("node_modules/x/package.json"), r#"{"dependencies":{"react":"18"}}"#).unwrap();
    assert_eq!(stack(root), vec!["Rust", "axum", "Tokio", "TypeScript", "Svelte", "Vite"]);
}

#[test]
fn github_links_from_remotes() {
    for remote in [
        "git@github.com:KarlssonChristoffer/hub.git",
        "https://github.com/KarlssonChristoffer/hub",
        "https://x-access-token:abc@github.com/KarlssonChristoffer/hub.git",
    ] {
        assert_eq!(github_web_url(remote).as_deref(), Some("https://github.com/KarlssonChristoffer/hub"), "{remote}");
    }
    assert_eq!(github_web_url("https://gitlab.com/a/b.git"), None);
}

#[test]
fn stays_inside_the_project() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("docs")).unwrap();
    std::fs::write(dir.path().join("docs/a.md"), "# Arkitektur\nText").unwrap();
    assert!(read(dir.path(), "../secret").is_err());
    assert_eq!(read(dir.path(), "docs/a.md").unwrap().text, "# Arkitektur\nText");
    let docs = docs(dir.path());
    assert_eq!(docs[0].title, "Arkitektur");
    let top = tree(dir.path(), "").unwrap();
    assert_eq!(top.iter().map(|e| (e.rel.as_str(), e.dir)).collect::<Vec<_>>(), vec![("docs", true)]);
}

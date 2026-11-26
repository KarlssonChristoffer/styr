//! Guards the notes index: front matter, tags, links and headings read; links resolved both ways
//! with mentions; titles searched before text and filtered by tag; only changed files reread; and
//! previews that skip headings and badges.

use super::*;

fn write(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[test]
fn reads_front_matter_tags_links_and_headings() {
    let d = parse(
        "docs/engines.md",
        "---\ntitle: Engines\nsummary: How Hub starts agents\ntags: [engine, core]\naliases:\n  - motor\n---\n# Ignored\nSee [[Core and hubd]] and [[Workers#Process trees|trees]] and `[[not a link]]`.\n#ui stuff, not #123\n```\n[[in code]] #nope\n```\n## Starting **an** engine\n[old](../README.md#setup)\n",
        0,
        0,
        false,
    );
    assert_eq!(d.title, "Engines");
    assert_eq!(d.summary, "How Hub starts agents");
    assert_eq!(d.tags, vec!["ui", "engine", "core"]);
    assert_eq!(d.props.iter().find(|p| p.key == "aliases").unwrap().value, "motor");
    let targets: Vec<_> = d.links.iter().map(|(l, _)| (l.target.as_str(), l.heading.as_deref())).collect();
    assert_eq!(targets, vec![("Core and hubd", None), ("Workers", Some("Process trees")), ("../README.md", Some("setup"))]);
    assert_eq!(d.headings.iter().map(|h| h.slug.as_str()).collect::<Vec<_>>(), vec!["ignored", "starting-an-engine"]);
    assert!(d.body.starts_with("# Ignored"));
}

#[test]
fn resolves_links_and_finds_backlinks_and_mentions() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "README.md", "# hub\nRead [the engines](docs/arch/engines.md).\n");
    write(root, "docs/arch/engines.md", "# Engines\nStarted by [[core]]. Missing: [[Health checks]].\n");
    write(root, "docs/arch/core.md", "# Core\nRuns [[Engines#Starting]].\n");
    write(root, "docs/notes/core.md", "# Other core\nUnrelated, mentions engines in passing.\n");
    write(root, "node_modules/x/readme.md", "# vendored\n");

    let page = doc(root, "Engines").unwrap();
    assert_eq!(page.rel, "docs/arch/engines.md");
    // The nearer of two docs named core wins.
    assert_eq!(page.links[0].to.as_deref(), Some("docs/arch/core.md"));
    assert_eq!(page.links[1].to, None);
    let from: Vec<_> = page.backlinks.iter().map(|b| b.from.as_str()).collect();
    assert_eq!(from, vec!["README.md", "docs/arch/core.md"]);
    assert_eq!(page.mentions.iter().map(|m| m.from.as_str()).collect::<Vec<_>>(), vec!["docs/notes/core.md"]);

    let vault = map(root);
    assert_eq!(vault.docs.len(), 4);
    let engines = vault.docs.iter().find(|d| d.rel == "docs/arch/engines.md").unwrap();
    assert_eq!((engines.links_in, engines.links_out), (2, 1));
    assert!(outline(root).contains("Linked but not written yet: Health checks"));
}

#[test]
fn searches_titles_before_text_and_filters_by_tag() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "docs/quota.md", "# Quota resume\nThreads carry on when the limit resets.\n#engine\n");
    write(root, "docs/engines.md", "# Engines\n## Limits\nWhen a quota is hit the turn parks and resumes later.\n");
    write(root, "docs/ui.md", "# UI\nNothing about it.\n");

    let hits = search(root, "quota res", 10);
    assert_eq!(hits.iter().map(|h| h.rel.as_str()).collect::<Vec<_>>(), vec!["docs/quota.md", "docs/engines.md"]);
    assert_eq!(hits[1].heading.as_ref().unwrap().text, "Limits");
    assert!(hits[1].snippet.contains("quota is hit"));
    let tagged = search(root, "#engine", 10);
    assert_eq!(tagged.iter().map(|h| h.rel.as_str()).collect::<Vec<_>>(), vec!["docs/quota.md"]);
    assert!(search(root, "nothing-like-this", 10).is_empty());
}

#[test]
fn rereads_only_changed_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "a.md", "# A\nold\n");
    assert_eq!(search(root, "old", 5).len(), 1);
    write(root, "a.md", "# A\nnew text that is longer\n");
    assert_eq!(search(root, "old", 5).len(), 0);
    assert_eq!(search(root, "new", 5).len(), 1);
}

#[test]
fn previews_skip_headings_and_badges() {
    let text = "# Hub\n\n[![CI](x)](y)\n\nOne place to work with **all** your AI threads,\nengines and `machines`.\n\nMore.";
    assert_eq!(preview(text, 200), "One place to work with all your AI threads, engines and machines.");
    assert_eq!(preview("# T\n\nalpha beta gamma delta", 12), "alpha beta…");
    assert_eq!(preview("# Only a heading\n", 50), "");
}

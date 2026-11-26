//! Guards the Plugins page's reading of GitHub: repository names and kinds, the week of stars
//! (counted or projected) and its curve, how many stargazer pages to read, a README without
//! pictures, what runs on install, and the first-look warnings.


#[test]
fn star_pages_to_read_follow_the_rate() {
    let h = 3_600_000;
    // 100 stars over the last 10 hours: two weeks back is ~34 pages, capped by what is left.
    let read: Vec<i64> = (0..100).map(|i| 1_000 * h - i * h / 10).collect();
    let since = 1_000 * h - 14 * 24 * h;
    assert_eq!(pages_to_reach(&read, since, 4), 4);
    // 100 stars over 10 days: one more page reaches two weeks, plus one to be safe.
    let slow: Vec<i64> = (0..100).map(|i| 1_000 * h - i * h * 24 / 10).collect();
    assert_eq!(pages_to_reach(&slow, since, 14), 2);
    assert_eq!(pages_to_reach(&[], since, 3), 3);
    assert_eq!(pages_to_reach(&slow, since, 0), 0);
}

use super::*;

#[test]
fn kinds_come_from_topics_name_and_description() {
    let t = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(kind_of("github-mcp-server", "", &[]), "mcp");
    assert_eq!(kind_of("knegarn", "", &t(&["hub-worker", "claude-code"])), "worker");
    assert_eq!(kind_of("browser", "", &t(&["mcp-server"])), "mcp");
    assert_eq!(kind_of("thing", "An MCP server for PDFs", &[]), "mcp");
    assert_eq!(kind_of("frontend-skills", "", &[]), "skill");
    assert_eq!(kind_of("x", "", &t(&["agent-skills"])), "skill");
    assert_eq!(kind_of("repomix", "Pack a repo into one file", &[]), "tool");
    // "mcp" as a word in the name, not any word containing the letters.
    assert_eq!(kind_of("mcpherson-notes", "", &[]), "tool");
}

#[test]
fn repositories_come_out_of_links() {
    assert_eq!(repo_from_link("someone/tool").as_deref(), Some("someone/tool"));
    assert_eq!(repo_from_link("https://github.com/someone/tool.git").as_deref(), Some("someone/tool"));
    assert_eq!(repo_from_link("https://github.com/someone/tool/tree/main/src?x=1").as_deref(), Some("someone/tool"));
    assert_eq!(repo_from_link("github.com/a/b#readme").as_deref(), Some("a/b"));
    assert_eq!(repo_from_link("https://github.com/someone"), None);
    assert_eq!(repo_from_link("not a repo"), None);
}

#[test]
fn names_must_be_owner_and_repo() {
    assert_eq!(check_name("a/b").unwrap(), "a/b");
    assert_eq!(check_name(" /owner/re.po-1/ ").unwrap(), "owner/re.po-1");
    assert!(check_name("a").is_err());
    assert!(check_name("a/b/c").is_err());
    assert!(check_name("../x").is_err());
    assert!(check_name("a/..").is_err());
    assert!(check_name("a/b?x=1").is_err());
}

#[test]
fn a_week_is_counted_when_the_sample_reaches_back_and_projected_when_not() {
    let now = 100 * DAY;
    // 10 stars over the last 10 days, one a day: 7 in the last week.
    let times: Vec<i64> = (0..10).map(|d| now - d * DAY - 1).collect();
    assert_eq!(week_of(&times, now, false), Some((7, false)));
    // 100 stars in the last day only, with more before: projected to 700.
    let times: Vec<i64> = (0..100).map(|i| now - i * (DAY / 100) - 1).collect();
    let (w, est) = week_of(&times, now, false).unwrap();
    assert!(est && (690..=710).contains(&w), "{w}");
    // Every star there is, all within the day: counted, not projected.
    assert_eq!(week_of(&times, now, true), Some((100, false)));
    assert_eq!(week_of(&[], now, false), None);
}

#[test]
fn the_curve_runs_back_as_far_as_the_sample_reaches() {
    let now = 100 * DAY;
    let times = vec![now - 1, now - 2, now - DAY - 5, now - 3 * DAY];
    // Not every star: the line spans back to the oldest one read, three days.
    let (line, span) = curve(&times, now, 30 * DAY, 3, false);
    assert_eq!(span, 3 * DAY);
    assert_eq!(line, vec![1, 2, 4]);
    // Every star there is: the whole window, quiet at first.
    let (line, span) = curve(&times, now, 6 * DAY, 3, true);
    assert_eq!(span, 6 * DAY);
    assert_eq!(line, vec![0, 1, 4]);
    assert_eq!(curve(&[], now, DAY, 5, false), (Vec::new(), 0));
}

#[test]
fn pictures_leave_the_readme() {
    let md = "# Hi\n![logo](img/logo.png)\nText [![badge](b.svg)](https://ci) and <img src=\"x.png\" width=10> end ![r][ref]";
    let out = without_pictures(md);
    assert!(!out.contains("logo.png") && !out.contains("x.png") && !out.contains("b.svg"), "{out}");
    assert!(out.contains("# Hi") && out.contains("Text") && out.contains("end"));
    assert!(out.contains("(https://ci)"));
    assert_eq!(without_pictures("no pictures"), "no pictures");
    assert_eq!(without_pictures("broken ![alt"), "broken ![alt");
}

#[test]
fn what_is_inside_and_what_runs_on_install() {
    let files: Vec<String> = ["package.json", "SKILL.md", "Dockerfile"].iter().map(|s| s.to_string()).collect();
    let pkg = r#"{"bin":{"x":"cli.js"},"dependencies":{"@modelcontextprotocol/sdk":"1"},"scripts":{"postinstall":"node fetch.js","test":"t"}}"#;
    let inside = inside_of(&files, Some(pkg), None);
    assert!(inside.iter().any(|s| s.contains("MCP server in Node")));
    assert!(inside.iter().any(|s| s.contains("command line")));
    assert!(inside.iter().any(|s| s.contains("SKILL.md")));
    assert_eq!(install_scripts(pkg), vec!["postinstall"]);
    assert!(install_scripts("not json").is_empty());
}

#[test]
fn what_marks_a_repository_as_made_for_an_engine_stays_the_same() {
    // One search per topic, in this order: ties in stars keep it.
    assert_eq!(
        TOPICS.iter().copied().collect::<Vec<_>>(),
        vec!["mcp-server", "model-context-protocol", "claude-code", "claude-skills", "agent-skills", "codex"]
    );
    let inside = inside_of(&[".claude-plugin".to_string()], None, None);
    assert_eq!(inside, vec!["A Claude Code plugin (.claude-plugin)".to_string()]);
    assert_eq!(kind_of("x", "A Claude skill for PDFs", &[]), "skill");
}

#[test]
fn checks_warn_about_young_unlicensed_and_idle_repos() {
    let now = 1000 * DAY;
    let repo = PluginRepo { created_at: now - 5 * DAY, pushed_at: now - 200 * DAY, ..Default::default() };
    let c = checks_of(&repo, Some(now - 30 * DAY), &["postinstall".into()], now);
    assert!(c.iter().all(|c| c.level == "warn"), "{c:?}");
    let repo = PluginRepo { created_at: now - 400 * DAY, pushed_at: now - DAY, license: Some("MIT".into()), ..Default::default() };
    let c = checks_of(&repo, Some(now - 900 * DAY), &[], now);
    assert!(c.iter().all(|c| c.level == "ok"), "{c:?}");
}

//! Guards your own tools: `hub.toml` parsing refuses what cannot work, inputs and env expand into
//! arguments as documented, a script tool is read and its actions run as the Workers view does,
//! only project and approved folders run, the panic button finds a pause action, and handoff
//! drafts and follow answers read right.

use super::*;
use serde_json::json;

#[test]
fn render_reads_pointers_and_templates() {
    let doc = json!({ "headline": "Working on R1", "n": 3, "e": { "reason": null }, "word": "ok" });
    assert_eq!(render("/headline", &doc).as_deref(), Some("Working on R1"));
    assert_eq!(render("/n", &doc).as_deref(), Some("3"));
    assert_eq!(render("/missing", &doc), None);
    assert_eq!(render("Capture is {/word}", &doc).as_deref(), Some("Capture is ok"));
    assert_eq!(render("{/word} · {/e/reason}", &doc).as_deref(), Some("ok"));
    assert_eq!(render("{/e/reason}", &doc), None);
}

#[test]
fn inputs_fill_whole_and_part_arguments_and_must_be_given() {
    let inputs = HashMap::from([("id".to_owned(), "q-1".to_owned()), ("text".to_owned(), "a b; rm -rf /".to_owned())]);
    assert_eq!(fill("{id}", &inputs).unwrap(), "q-1");
    assert_eq!(fill("--reason={text}", &inputs).unwrap(), "--reason=a b; rm -rf /");
    assert_eq!(fill("{not an input}", &inputs).unwrap(), "{not an input}");
    assert!(fill("{reason}", &inputs).is_err());
}

#[test]
fn expand_uses_the_file_env_then_the_core_env_then_the_default() {
    let env = BTreeMap::from([("A".to_owned(), "from-file".to_owned())]);
    assert_eq!(expand("${A}/x", &env), "from-file/x");
    assert_eq!(expand("${HUB_SURELY_UNSET_VAR:-fallback}", &env), "fallback");
    assert_eq!(expand("${HUB_SURELY_UNSET_VAR}", &env), "");
    assert!(!expand("~/x", &env).starts_with('~'));
}

#[test]
fn parse_refuses_what_cannot_work() {
    assert!(parse("name = \"X\"\n[[list]]\ntitle = \"L\"\nitems = \"/a\"\ntext = \"/t\"\n").is_err());
    assert!(parse("name = \"X\"\n[[action]]\nid = \"a\"\nlabel = \"A\"\nrun = [\"x\"]\non = \"Nope\"\n").is_err());
    assert!(parse("name = \"X\"\nnope = 1\n").is_err());
    assert!(parse("name = \"X\"\n[[action]]\nid = \"a\"\nlabel = \"A\"\nrun = [\"x\"]\npanic = \"stop\"\n").is_err());
    assert!(parse("name = \"X\"\n[[action]]\nid = \"a\"\nlabel = \"A\"\nrun = [\"x\"]\npanic = \"pause\"\nask = [\"why\"]\n").is_err());
}

#[test]
fn the_panic_button_finds_a_tools_pause_action() {
    let dir = tempfile::tempdir().unwrap();
    let file = "name = \"Forman\"\n[[action]]\nid = \"hold\"\nlabel = \"Hold\"\nrun = [\"x\"]\npanic = \"pause\"\n";
    std::fs::write(dir.path().join(FILE), file).unwrap();
    assert_eq!(panic_action(dir.path(), "pause"), Some(("Forman".into(), "hold".into())));
    assert_eq!(panic_action(dir.path(), "resume"), None);
}

#[test]
fn a_draft_is_a_title_then_the_brief() {
    assert_eq!(split_draft("Title: Add retries\n\n## Goal\nRetry.", "x"), ("Add retries".into(), "## Goal\nRetry.".into()));
    assert_eq!(split_draft("## Goal only", "Thread"), ("Thread".into(), "## Goal only".into()));
    assert!(parse("name = \"X\"\n[handoff]\nlabel = \"Hand\"\nguide = \"\"\nrun = [\"x\"]\n").is_err());
    let knegarn = parse(include_str!("../../../../docs/examples/knegarn-hub.toml")).unwrap();
    assert_eq!(knegarn.handoff.unwrap().label, "Hand to Knegarn");
    parse(include_str!("../../../../docs/examples/forman-hub.toml")).unwrap();
    parse(include_str!("../../../../docs/examples/aftermath-hub.toml")).unwrap();
}

/// A tool that is a script printing JSON, read the way the Workers view reads it.
#[cfg(unix)]
#[tokio::test]
async fn reads_a_tool_and_runs_its_actions() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("tool.sh"),
        r#"#!/bin/sh
if [ "$1" = now ]; then
  echo '{"headline":"Two things","word":"working","questions":[{"id":"q1","q":"Which image?","status":"open"},{"id":"q2","q":"Old","status":"answered"}],"events":[{"t":"first"},{"t":"second"}]}'
elif [ "$1" = answer ]; then
  echo "answered $2 with $3"
fi
"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join(FILE),
        r#"name = "Fake"
[status]
run = ["sh", "tool.sh", "now"]
headline = "/headline"
state = "/word"
[[list]]
title = "Questions"
items = "/questions"
where = { status = "open" }
id = "/id"
text = "/q"
needs_you = true
[[list]]
title = "Events"
items = "/events"
text = "/t"
reverse = true
[[action]]
id = "answer"
label = "Answer"
run = ["sh", "tool.sh", "answer", "{id}", "{text}"]
on = "Questions"
ask = ["text"]
[[action]]
id = "halt"
label = "Halt"
run = ["sh", "tool.sh", "halt"]
danger = true
"#,
    )
    .unwrap();
    let found = candidates(&[], &[dir.path().parent().unwrap().to_path_buf()]);
    assert!(found.iter().any(|f| f == dir.path()));

    let tool = read(dir.path()).await;
    assert_eq!(tool.error, None);
    assert_eq!(tool.name, "Fake");
    assert_eq!(tool.headline.as_deref(), Some("Two things"));
    assert_eq!(tool.state.as_deref(), Some("working"));
    assert_eq!(tool.needs_you, 1);
    assert_eq!(tool.lists[0].items[0].id.as_deref(), Some("q1"));
    assert_eq!(tool.lists[1].items[0].text, "second");

    let ask = RunOwnToolAction {
        folder: dir.path().display().to_string(),
        action: "answer".into(),
        item: Some("q1".into()),
        inputs: BTreeMap::from([("text".into(), "the slim one; really".into())]),
    };
    let ran = act(dir.path(), &ask, false).await.unwrap();
    assert!(ran.ok);
    assert_eq!(ran.output, "answered q1 with the slim one; really");

    let halt = RunOwnToolAction { action: "halt".into(), item: None, inputs: BTreeMap::new(), ..ask };
    assert!(act(dir.path(), &halt, false).await.is_err());
    assert!(act(dir.path(), &halt, true).await.unwrap().ok);
}

#[test]
fn a_follow_answer_settles_only_when_the_work_ended() {
    assert!(settled("Knegarn", "Fix", &serde_json::json!({ "state": "running" })).is_none());
    assert!(settled("Knegarn", "Fix", &serde_json::json!({ "said": "no state" })).is_none());
    let done = settled("Knegarn", "Fix", &serde_json::json!({
        "state": "done", "proven": true, "model": "claude-sonnet-5", "cost": 1.5, "link": "https://x/pr/1"
    }))
    .unwrap();
    assert_eq!((done.proven, done.cost, done.title.as_str()), (Some(Some(true)), Some(Some(1.5)), "Fix"));
    let stopped = settled("Knegarn", "Fix", &serde_json::json!({ "state": "stopped", "said": "contract_drift" })).unwrap();
    assert_eq!((stopped.proven, stopped.cost), (Some(None), Some(None)));
}

#[tokio::test]
async fn only_projects_and_approved_folders_run() {
    let home = tempfile::tempdir().unwrap();
    let (old, cloned) = (home.path().join("old"), home.path().join("cloned"));
    for d in [&old, &cloned] {
        std::fs::create_dir(d).unwrap();
        std::fs::write(d.join(FILE), "name = \"T\"\n").unwrap();
    }
    let config = crate::Config { data_dir: crate::janitor::test_dir(), ..Default::default() };
    let store = crate::Store::in_memory().unwrap();
    let ctx = crate::ctx::Ctx::new(&store, &config);
    // First run: what is there is approved, so the setup that worked keeps working.
    let (_, found) = scanned_in(home.path()).await;
    migrate(&ctx, &found).await;
    let fresh = home.path().join("fresh");
    std::fs::create_dir(&fresh).unwrap();
    std::fs::write(fresh.join(FILE), "name = \"Fresh\"\n").unwrap();
    let ok = allowed(&ctx).await;
    assert!(ok.iter().any(|a| same(a, &old)) && ok.iter().any(|a| same(a, &cloned)));
    assert!(!ok.iter().any(|a| same(a, &fresh)));
    // Later arrivals wait for approval, once.
    approve(&ctx, fresh.clone()).await.unwrap();
    assert!(allowed(&ctx).await.iter().any(|a| same(a, &fresh)));
    forget(&ctx, &fresh.display().to_string()).await.unwrap();
    assert!(!allowed(&ctx).await.iter().any(|a| same(a, &fresh)));
    assert!(allow(&ctx, &home.path().display().to_string()).await.is_err());
}

async fn scanned_in(root: &Path) -> (Vec<String>, Vec<PathBuf>) {
    (Vec::new(), candidates(&[], &[root.to_path_buf()]))
}

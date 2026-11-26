//! Tests of cloud threads: reading sessions, tasks and environments.

use super::*;
use super::codex_env::*;

#[test]
fn the_session_is_found_in_what_the_command_printed() {
    let out = "Creating session…\nSession started: https://claude.ai/code/session_01AbCdEf123\n";
    let (id, url) = parse_started(out).unwrap();
    assert_eq!(id, "session_01AbCdEf123");
    assert_eq!(url.as_deref(), Some("https://claude.ai/code/session_01AbCdEf123"));
    assert!(parse_started("Not signed in").is_none());
}

#[test]
fn a_follow_up_answers_in_json() {
    let (id, url) = parse_started(r#"{"ok":true,"session_id":"session_01X","url":"https://claude.ai/code/session_01X"}"#).unwrap();
    assert_eq!(id, "session_01X");
    assert!(url.unwrap().ends_with("session_01X"));
}

#[test]
fn a_repository_is_read_from_either_kind_of_remote() {
    assert_eq!(repo_of("git@github.com:KarlssonChristoffer/hub.git\n").as_deref(), Some("KarlssonChristoffer/hub"));
    assert_eq!(repo_of("https://github.com/a/b").as_deref(), Some("a/b"));
    assert_eq!(repo_of("https://gitlab.com/a/b"), None);
}

#[test]
fn a_session_is_read_for_what_it_says_and_nothing_more() {
    let s = json!({
        "session_status": "completed",
        "session_context": { "outcomes": [{ "git_info": { "branches": ["claude/fix-x"] } }] },
        "external_metadata": { "summary": "Fixed it" },
        "events": "opened https://github.com/a/b/pull/12 ok"
    });
    let r = read_session(&s);
    assert_eq!(r.state, Some("done"));
    assert_eq!(r.branch.as_deref(), Some("claude/fix-x"));
    assert_eq!(r.pr.as_ref().map(|p| p.number), Some(12));
    assert_eq!(r.last.as_deref(), Some("Fixed it"));
    assert_eq!(read_session(&json!({ "session_status": "wibble" })), Read::default());
}

#[test]
fn only_the_claude_and_codex_harnesses_start_in_the_cloud() {
    assert!(begin("claude", true).is_ok());
    assert!(begin("claude", false).is_err());
    assert_eq!(begin("codex", true).unwrap().provider, "codex");
    assert!(begin("hermes", true).is_err());
}

#[test]
fn a_codex_task_is_found_in_what_exec_printed_and_in_the_listing() {
    let (id, url) = parse_codex_started("https://chatgpt.com/codex/tasks/task_e_6821abc\n").unwrap();
    assert_eq!(id, "task_e_6821abc");
    assert_eq!(url.as_deref(), Some("https://chatgpt.com/codex/tasks/task_e_6821abc"));
    assert!(parse_codex_started("Not signed in. Please run 'codex login'").is_none());
    let list = json!({ "tasks": [{ "id": "task_e_other", "status": "pending" }, { "id": "task_e_6821abc", "status": "ready", "title": "Fix the build" }] });
    let r = read_codex_list(&list, "task_e_6821abc").unwrap();
    assert_eq!((r.state, r.last.as_deref()), (Some("done"), Some("Fix the build")));
    assert_eq!(read_codex_list(&json!([{ "id": "task_e_6821abc", "status": "pending" }]), "task_e_6821abc").unwrap().state, Some("working"));
    assert!(read_codex_list(&list, "task_e_missing").is_none());
}

#[test]
fn a_codex_environment_is_found_when_its_label_is_not_the_repository() {
    let by_repo = parse_codex_envs(&json!([{ "id": "env_1", "label": "Hub (main)" }]));
    let all = parse_codex_envs(&json!({ "environments": [{ "id": "env_1", "label": "Hub (main)" }, { "id": "env_2", "label": "other/thing" }] }));
    assert_eq!(pick_codex_envs(&by_repo, &all, "KarlssonChristoffer/hub")[0].id, "env_1");
    assert_eq!(pick_codex_envs(&[], &all[1..], "KarlssonChristoffer/hub")[0].id, "env_2");
    assert_eq!(pick_codex_envs(&[], &all, "a/HUB (main)").len(), 1);
}

#[test]
fn an_idle_session_is_not_a_finished_one_and_its_replies_are_read() {
    assert_eq!(state_of("SESSION_STATUS_IDLE"), Some("waiting"));
    assert_eq!(state_of("SESSION_STATUS_BUCKET_COMPLETED"), Some("done"));
    assert_eq!(state_of("session_status_running"), Some("working"));
    let page = json!({ "data": [
        { "user": { "uuid": "u1" } },
        { "assistant": { "uuid": "a1", "internal_anthropic_catchall": { "message": { "content": [{ "type": "tool_use" }] } } } },
        { "assistant": { "uuid": "a2", "internal_anthropic_catchall": { "message": { "content": [{ "type": "text", "text": " Hej! " }] } } } },
        { "type": "assistant", "uuid": "a3", "parent_tool_use_id": "t", "message": { "content": [{ "type": "text", "text": "sub" }] } },
        { "type": "assistant", "uuid": "a4", "message": { "content": [{ "type": "text", "text": "Klart" }] } }
    ], "last_id": "a4" });
    assert_eq!(read_events(&page), (vec!["Hej!".to_owned(), "Klart".to_owned()], Some("a4".to_owned())));
}

#[test]
fn a_codex_environment_is_found_by_the_label_earlier_tasks_ran_under() {
    let list = json!({ "tasks": [
        { "id": "t1", "environment_id": "env_other", "environment_label": "someone/else" },
        { "id": "t2", "environment_id": "env_hub", "environment_label": "KarlssonChristoffer/hub" },
        { "id": "t3", "environment_id": "env_hub", "environment_label": "karlssonchristoffer/hub" }
    ] });
    assert_eq!(codex_env_candidates(&list, "KarlssonChristoffer/hub"), ["env_hub", "KarlssonChristoffer/hub", "hub"]);
    assert_eq!(codex_env_candidates(&Value::Null, "a/b"), ["a/b", "b"]);
}

#[test]
fn the_terminal_is_told_where_its_cursor_is_and_a_failure_shows_its_last_lines() {
    assert_eq!(answer_queries(b"x\x1b[6ny\x1b[6n"), b"\x1b[1;1R\x1b[1;1R");
    assert!(answer_queries(b"plain").is_empty());
    assert_eq!(tail_of("a\n\n b \nc\nd\ne\nf\ng\n"), "b | c | d | e | f | g");
}

#[cfg(unix)]
#[tokio::test]
async fn the_start_command_runs_in_a_terminal_and_is_read_by_its_address() {
    let args = ["-c".to_owned(), "tty -s && echo '\x1b[1mhttps://claude.ai/code/session_01Abcdef\x1b[0m'; sleep 30".to_owned()];
    let (_, found) = in_terminal(Path::new("/bin/sh"), &args, "/").await.unwrap();
    assert_eq!(found.unwrap().0, "session_01Abcdef");
}

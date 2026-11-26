//! Guards how failed tool calls become problems: one name per tool across engines, the program a
//! command ran, a gist without the exit code, a silent failure naming its command or what its
//! terminal printed, refused calls not counted as failures, and a review taking what fails often.

use super::*;

#[test]
fn names_tools_the_same_on_every_engine() {
    assert_eq!(
        tool_name(
            &json!({ "title": "`ls`", "kind": "execute", "_meta": { "claudeCode": { "toolName": "Bash" } } })
        ),
        "Bash"
    );
    assert_eq!(
        tool_name(
            &json!({ "title": "x", "_meta": { "claudeCode": { "toolName": "mcp__hub__hub_status" } } })
        ),
        "hub · hub_status"
    );
    assert_eq!(
        tool_name(&json!({ "title": "Tool: github/get_issue", "kind": "other" })),
        "github · get_issue"
    );
    assert_eq!(
        tool_name(&json!({ "title": "cargo test", "kind": "execute" })),
        "Command"
    );
    assert_eq!(
        tool_name(&json!({ "title": "web_search: rust" })),
        "web_search"
    );
    assert_eq!(tool_name(&json!({})), "Other");
}

#[test]
fn finds_the_program_a_command_ran() {
    assert_eq!(
        program(&json!({ "command": "cd app && npm run check" })).as_deref(),
        Some("npm")
    );
    assert_eq!(
        program(&json!({ "command": ["bash", "-lc", "RUST_LOG=x cargo test -p hub-core"] }))
            .as_deref(),
        Some("cargo")
    );
    assert_eq!(
        program(&json!({ "command": "C:\\tools\\rg.exe foo" })).as_deref(),
        Some("rg")
    );
    assert_eq!(program(&json!({ "file_path": "a" })), None);
}

#[test]
fn the_gist_skips_the_exit_code() {
    assert_eq!(
        gist("Exit code 127\nbash: cargo: command not found"),
        "bash: cargo: command not found"
    );
    assert_eq!(
        gist("Exit code 1\n   Compiling x\nerror[E0425]: cannot find value `y`"),
        "error[E0425]: cannot find value `y`"
    );
    assert_eq!(gist(""), "It failed without saying why");
    // A fence around the output is not what it said.
    assert_eq!(gist("```\nno thread with id x\n```"), "no thread with id x");
    assert_eq!(gist("```"), "It failed without saying why");
    let call = json!({ "content": [{ "type": "content", "content": { "type": "text", "text": "<tool_use_error>File does not exist.</tool_use_error>" } }] });
    assert_eq!(said(&call), "File does not exist.");
    assert_eq!(
        said(&json!({ "rawOutput": { "stderr": "boom", "exit_code": 2 } })),
        "boom"
    );
}

#[test]
fn a_silent_failure_names_the_command_that_ran() {
    let call = json!({
        "status": "failed",
        "content": [],
        "rawInput": { "command": "git commit -q -F -\ngit status --short" },
        "_meta": { "claudeCode": { "toolName": "Bash" } },
    });
    let text = said(&call);
    assert_eq!(gist(&text), SILENT);
    let shown = failed_message(&text, &gist(&text), &call["rawInput"]);
    assert!(shown.contains("git commit -q -F -"), "{shown}");
    assert!(shown.ends_with("- no output"), "{shown}");
    // The first line only, not the rest of the script.
    assert!(!shown.contains("git status"), "{shown}");

    // Codex writes the command as a shell and its script.
    let codex = failed_message(&text, &gist(&text), &json!({ "command": ["bash", "-lc", "cargo test -p hub-core"] }));
    assert!(codex.contains("cargo test -p hub-core"), "{codex}");

    // A line that only changes folder is not the command that ran.
    let script = failed_message(&text, &gist(&text), &json!({ "command": "cd app\nnpm run check" }));
    assert_eq!(script, "`npm run check` - no output");

    // A call that said nothing beyond a fence names the command too.
    let fence = "```";
    assert_eq!(gist(fence), SILENT);
    assert_eq!(
        failed_message(fence, &gist(fence), &json!({ "command": "gh pr create" })),
        "`gh pr create` - said nothing"
    );

    // A bare exit code, and the raw output's own JSON, say nothing a reader can use.
    assert!(says_nothing("Exit code 1") && says_nothing("{\"stdout\":\"\",\"stderr\":\"\",\"exit_code\":1}"));
    assert_eq!(
        failed_message("Exit code 1", "Exit code 1", &json!({ "command": "git commit" })),
        "`git commit` - said nothing"
    );

    // A call that said something keeps today's message.
    let loud = json!({ "content": [{ "type": "content", "content": { "type": "text", "text": "error: nothing to commit" } }] });
    let text = said(&loud);
    assert_eq!(gist(&text), "error: nothing to commit");
    assert_eq!(
        failed_message(&text, &gist(&text), &json!({ "command": "git commit" })),
        "error: nothing to commit"
    );

    // A call with neither keeps the fallback.
    let empty = said(&json!({ "content": [] }));
    assert_eq!(failed_message(&empty, &gist(&empty), &json!({})), SILENT);
}

#[test]
fn a_tools_tab_error_still_finds_the_problem_it_is_kept_as() {
    let open = crate::problems::ProblemStatus::Open;
    let problems = vec![
        ToolProblem { id: 7, title: "Bash · git failed: `git commit -q -F -` - no output".into(), count: 12, status: open },
        ToolProblem { id: 8, title: "Bash · cargo failed: error[E0425]: cannot find value `y`".into(), count: 3, status: open },
    ];
    // The command a silent problem now shows is not the gist any more, so the gist finds it
    // by what the message ends with.
    assert_eq!(linked(SILENT, &problems), Some(7));
    assert_eq!(linked("Exit code 1", &problems), Some(7));
    assert_eq!(linked("error[E0425]: cannot find value `y`", &problems), Some(8));
    assert_eq!(linked("a failure nobody kept", &problems), None);
}

#[test]
fn a_refused_call_is_not_a_failure() {
    let asks = vec![
        (
            "permission_request".to_owned(),
            json!({ "id": 3, "tool_call": { "toolCallId": "a" }, "options": [{ "optionId": "no", "kind": "reject_once" }, { "optionId": "yes", "kind": "allow_once" }] }),
        ),
        (
            "permission_resolved".to_owned(),
            json!({ "id": 3, "option_id": "no" }),
        ),
        (
            "permission_request".to_owned(),
            json!({ "id": 4, "tool_call": { "toolCallId": "b" }, "options": [{ "optionId": "yes", "kind": "allow_once" }] }),
        ),
        (
            "permission_resolved".to_owned(),
            json!({ "id": 4, "option_id": "yes" }),
        ),
        (
            "permission_auto".to_owned(),
            json!({ "tool_call": { "toolCallId": "c" }, "allowed": false }),
        ),
    ];
    assert!(refused_in(&asks, "a"));
    assert!(!refused_in(&asks, "b"));
    assert!(refused_in(&asks, "c"));
    assert!(!refused_in(&asks, "d"));
    assert!(stopped(
        "The user doesn't want to proceed with this tool use."
    ));
}

#[test]
fn a_review_takes_what_fails_often_with_its_trend() {
    assert!(worth_reviewing(5, 500));
    assert!(worth_reviewing(3, 20));
    assert!(!worth_reviewing(3, 100));
    assert!(!worth_reviewing(2, 2));
    let rate = |tool: &str, calls, failed| ToolRate { engine: "claude".into(), tool: tool.into(), calls, failed, days: vec![], problems: vec![], errors: vec![] };
    let now = ToolStats { days: 7, calls: 0, failed: 0, tools: vec![rate("Bash", 100, 9), rate("Read", 300, 2)] };
    let then = ToolStats { days: 14, calls: 0, failed: 0, tools: vec![rate("Bash", 180, 12), rate("Read", 600, 4)] };
    let found = candidates(Some("tincan"), &now, &then);
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].rate.tool.as_str(), found[0].before), ("Bash", 3));
    let brief = review_brief(&found);
    assert!(brief.contains("claude · Bash on tincan: 9 of 100 calls failed (9%), up from 3 the week before"), "{brief}");
    assert!(brief.contains("tool_review"));
}

/// Claude Code and Codex stream a command's terminal and end the call with only an exit code
/// (2026-11: 139 of 1 569 Claude calls in a week read "It failed without saying why").
#[test]
fn a_failed_command_says_what_its_terminal_printed() {
    let update = |seq: i64, meta: Value| Event { thread_id: "t".into(), seq, ts: 0, kind: EventKind::Update, data: json!({ "toolCallId": "c", "_meta": meta }) };
    let updates = [
        update(1, json!({ "terminal_info": { "terminal_id": "c" } })),
        update(2, json!({ "terminal_output_delta": { "data": "Exit code 127\n/usr/bin/bash: line 1: hub-shell.exe: " } })),
        update(3, json!({ "terminal_output_delta": { "data": "No such file or directory" } })),
        update(4, json!({ "terminal_exit": { "exit_code": 127, "signal": null } })),
    ];
    let text = terminal_said(&updates);
    assert_eq!(gist(&text), "/usr/bin/bash: line 1: hub-shell.exe: No such file or directory");
    // Nothing printed: the exit code is what there is to say.
    assert_eq!(terminal_said(&updates[3..]), "Exit code 127");
    assert_eq!(terminal_said(&updates[..1]), "");
}

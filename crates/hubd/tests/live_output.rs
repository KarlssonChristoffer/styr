//! Hub's shell wrapper as Claude Code runs it: a command's output reaches the thread while it
//! runs, and the command itself (its output file, its exit code) is as without the wrapper.
#![cfg(target_os = "linux")]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use hub_core::live_output::{DIR_VAR, Live, WRAPPER};
use hub_core::model::{EventData, EventKind, NewThread};
use hub_core::{Config, Hub, Store};
use serde_json::json;

/// hubd, as cargo built it for this test program, linked under the wrapper's name.
fn wrapper() -> &'static PathBuf {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(|| {
        let dir = hub_core::janitor::test_dir();
        let link = dir.join(WRAPPER);
        let hubd = PathBuf::from(env!("CARGO_BIN_EXE_hubd"));
        // The test folder may be on another filesystem than the target (TMPDIR on a different
        // disk), and a hard link cannot cross filesystems: then copy the binary instead.
        let linked = std::fs::hard_link(&hubd, &link);
        if matches!(&linked, Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices) {
            std::fs::copy(&hubd, &link).unwrap();
            std::fs::set_permissions(&link, std::fs::Permissions::from_mode(0o755)).unwrap();
        } else {
            linked.unwrap();
        }
        link
    })
}

/// Runs `command` the way Claude Code runs a Bash command with the prefix set: its script, in
/// bash, its output to `out`. Returns the exit code.
fn run_like_claude(prefix: &Path, dir: &Path, command: &str, out: &Path) -> i32 {
    let cwd_file = out.with_extension("cwd");
    let script = format!("eval '{command}' < /dev/null && pwd -P >| {}", cwd_file.display());
    let quote = |s: &str| format!("'{}'", s.replace('\'', r"'\''"));
    let line = format!("{} {}", quote(&prefix.display().to_string()), quote(&script));
    let file = std::fs::File::create(out).unwrap();
    std::process::Command::new("bash")
        .args(["-c", &line])
        .env(DIR_VAR, dir)
        .stdout(file.try_clone().unwrap())
        .stderr(file)
        .status()
        .unwrap()
        .code()
        .unwrap()
}

#[test]
fn the_wrapper_leaves_the_command_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let notes = dir.path().join("notes");
    std::fs::create_dir_all(&notes).unwrap();
    let out = dir.path().join("cmd.output");
    let code = run_like_claude(wrapper(), &notes, "echo one; echo two >&2; exit 3", &out);
    assert_eq!(code, 3);
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "one\ntwo\n");
    // It noted where the output went.
    let note = std::fs::read_dir(&notes).unwrap().next().unwrap().unwrap().path();
    let note: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(note).unwrap()).unwrap();
    assert_eq!(PathBuf::from(note["output"].as_str().unwrap()), out);

    // A command that is not Claude's Bash script (an MCP server, a hook) runs and notes nothing.
    let other = tempfile::tempdir().unwrap();
    let status = std::process::Command::new(wrapper()).arg("echo hook").env(DIR_VAR, other.path()).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&status.stdout), "hook\n");
    assert_eq!(std::fs::read_dir(other.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn output_reaches_the_running_command_while_it_runs() {
    let config = Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() };
    let hub = Hub::new(Store::in_memory().unwrap(), config);
    let thread = hub
        .create_thread(NewThread {
            cwd: std::env::temp_dir().display().to_string(),
            title: Some("live".into()),
            engine: None,
            message: None,
            attachments: vec![],
            settings: Default::default(),
            original: None,
            client_id: None,
            attempts: vec![],
            warm: None,
            design: false,
            unattended: None,
            reach: None,
            cloud: false,
        })
        .await
        .unwrap();
    let live = Live::with_wrapper(&hub.config, &thread.id, Some(wrapper().clone()));
    let env: std::collections::HashMap<_, _> = live.env().into_iter().collect();
    assert_eq!(env["CLAUDE_CODE_SHELL_PREFIX"], wrapper().display().to_string());
    let notes = PathBuf::from(&env[DIR_VAR]);
    let (store, id) = (hub.store.clone(), thread.id.clone());
    let watching = tokio::spawn(live.clone().watch(move |data| {
        let (store, id) = (store.clone(), id.clone());
        async move { store.append_event(&id, EventKind::Output, data.to_json()).await.map(|_| ()) }
    }));

    live.saw(&json!({ "sessionUpdate": "tool_call", "toolCallId": "bash-1", "kind": "execute", "status": "pending", "rawInput": { "command": "for i in 1 2 3; do echo line $i; sleep 0.4; done" } }));
    let out = hub_core::janitor::test_dir().join("bash-1.output");
    let (prefix, out2) = (wrapper().clone(), out.clone());
    let run = std::thread::spawn(move || run_like_claude(&prefix, &notes, "for i in 1 2 3; do echo line $i; sleep 0.4; done", &out2));

    // The first line is in the log before the command ends.
    let mut seen = String::new();
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
        seen = events.iter().filter(|e| e.kind == EventKind::Output).filter_map(|e| e.data["data"].as_str()).collect();
        if !seen.is_empty() {
            break;
        }
    }
    assert!(!run.is_finished(), "the output came only after the command");
    assert!(seen.starts_with("line 1\n"), "{seen:?}");

    assert_eq!(run.join().unwrap(), 0);
    live.saw(&json!({ "sessionUpdate": "tool_call_update", "toolCallId": "bash-1", "status": "completed" }));
    tokio::time::sleep(Duration::from_millis(700)).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    let all: String = events.iter().filter(|e| e.kind == EventKind::Output).filter_map(|e| e.data["data"].as_str()).collect();
    assert_eq!(all, "line 1\nline 2\nline 3\n");
    assert!(events.iter().filter(|e| e.kind == EventKind::Output).all(|e| e.data["tool_call_id"] == "bash-1"));
    watching.abort();
}

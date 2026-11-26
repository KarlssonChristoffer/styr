//! Proof before done: a reply that changed code should show that it works. After each turn the
//! core looks through what the agent ran for a test or check run, a look at a page with Hub's
//! `page_check`, or a screenshot, and asks the agent once for one when there is none (see
//! `Hub::after_turn`). A turn that changed web pages is asked to look at them with `page_check`.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

use crate::model::{Event, EventKind};

/// What a turn showed.
#[derive(Debug, Clone, PartialEq)]
pub struct Evidence {
    /// "tests" or "screenshot".
    pub kind: &'static str,
    /// The command or tool that showed it.
    pub command: String,
    /// The run finished without failing.
    pub ok: bool,
    /// The end of what the call printed, when it printed any.
    pub output: String,
    /// Hub's `page_check` looked at a page: it says itself whether the page worked, so one that
    /// found problems counts as failed, where any other screenshot that failed counts for nothing.
    pub page: bool,
}

/// Commands that run a project's tests or checks, in the languages and tools people use.
static TESTS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)\b(
            cargo\s+(?:test|nextest|clippy|check)
          | (?:npm|pnpm|yarn|bun)\s+(?:run\s+)?(?:test|check|lint|typecheck|e2e)[\w:-]*
          | bun\s+test | deno\s+test
          | npx\s+(?:vitest|jest|playwright\s+test|svelte-check|tsc|eslint)
          | vitest | jest | svelte-check | tsc\s+--noEmit
          | pytest | python3?\s+-m\s+(?:pytest|unittest) | tox | ruff\s+check | mypy
          | go\s+(?:test|vet) | dotnet\s+test | mvn\s+(?:test|verify) | \.?/?gradlew?\s+test
          | make\s+(?:test|check) | ctest | swift\s+test | mix\s+test | rspec | phpunit
        )\b",
    )
    .expect("test pattern")
});

static SCREENSHOT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)screenshot|playwright|puppeteer").expect("screenshot pattern"));

/// How much of a call's output is kept: the end, where the summary is.
const OUTPUT_KEPT: usize = 3_500;

/// What each tool call printed: its content, or the terminal chunks an engine streams.
fn outputs(events: &[Event]) -> HashMap<String, String> {
    let mut out: HashMap<String, String> = HashMap::new();
    for e in events.iter().filter(|e| e.kind == EventKind::Update) {
        let d = &e.data;
        let Some(id) = d["toolCallId"].as_str() else { continue };
        let text: String = d["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|c| c["type"] == "content" && c["content"]["type"] == "text")
            .filter_map(|c| c["content"]["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if !text.is_empty() {
            out.insert(id.to_owned(), text);
        }
        if let Some(printed) = crate::model::terminal_output(d, out.get(id).map_or("", String::as_str)) {
            out.insert(id.to_owned(), printed);
        }
    }
    out
}

/// The end of `text`, in at most `max` characters.
fn tail(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.trim().to_owned();
    }
    let at = text.char_indices().nth(count - max).map_or(0, |(i, _)| i);
    text[at..].trim().to_owned()
}

/// Words in a command's output that say something failed, apart from the counts that say nothing
/// did ("0 failed", "no errors").
static FAILURE_WORDS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:fail(?:ed|s|ure|ures)?|error(?:s)?|panicked|assertion|✗|✘|traceback|exception|cannot find|not found)\b|\bFAIL\b|ERR!").expect("failure pattern")
});
static NO_FAILURE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:0|no|zero|without)\s+(?:test\s+)?(?:fail(?:ed|ures?)?|errors?|warnings?)\b|\b(?:fail|error)s?\s*[:=]\s*0\b|\b0\s+failing\b").expect("no-failure pattern")
});

/// Whether a run that exited cleanly printed something that may be a failure, which is worth
/// asking a decision model about (`reading.rs`). A run that printed nothing of the kind is not.
pub fn may_hide_failure(output: &str) -> bool {
    FAILURE_WORDS.is_match(&NO_FAILURE.replace_all(output, ""))
}

/// What the tool calls in `events` (one turn) showed, in the order they finished.
pub fn evidence(events: &[Event]) -> Vec<Evidence> {
    let printed = outputs(events);
    // Everything known about each call, merged, since updates only carry what changed.
    let mut calls: HashMap<String, serde_json::Value> = HashMap::new();
    let mut finished: Vec<String> = Vec::new();
    for e in events.iter().filter(|e| e.kind == EventKind::Update) {
        let d = &e.data;
        if !matches!(crate::model::update_kind(d), Some("tool_call" | "tool_call_update")) {
            continue;
        }
        let Some(id) = d["toolCallId"].as_str() else { continue };
        let call = calls.entry(id.to_owned()).or_insert_with(|| serde_json::json!({}));
        if let (Some(into), Some(from)) = (call.as_object_mut(), d.as_object()) {
            for (k, v) in from {
                if k == "content" && v.as_array().is_some_and(|a| a.is_empty()) {
                    continue;
                }
                if !v.is_null() {
                    into.insert(k.clone(), v.clone());
                }
            }
        }
        if matches!(d["status"].as_str(), Some("completed" | "failed")) {
            finished.retain(|f| f != id);
            finished.push(id.to_owned());
        }
    }
    finished
        .iter()
        .filter_map(|id| {
            let call = &calls[id];
            let ok = call["status"] == "completed";
            let command = call["rawInput"]["command"].as_str().unwrap_or_default();
            if command.is_empty() && crate::permit::hub_tool_call(call, &["page_check"]).is_some() {
                return page_looked(call, ok, printed.get(id).map_or("", String::as_str));
            }
            let title = call["title"].as_str().unwrap_or_default();
            let text = format!("{command}\n{title}");
            if let Some(m) = TESTS.find(&text) {
                let shown = if command.is_empty() { m.as_str() } else { command };
                let output = printed.get(id).map(|o| tail(o, OUTPUT_KEPT)).unwrap_or_default();
                return Some(Evidence { kind: "tests", command: one_line(shown), ok, output, page: false });
            }
            let image = call["content"].as_array().into_iter().flatten().any(|c| c["content"]["type"] == "image");
            if image || SCREENSHOT.is_match(&text) {
                let shown = if command.is_empty() { title } else { command };
                return Some(Evidence { kind: "screenshot", command: one_line(shown), ok, output: String::new(), page: false });
            }
            None
        })
        .collect()
}

/// A call to Hub's `page_check`, as proof: a screenshot that passed when the page rendered with
/// nothing wrong in it, failed when it reported problems. When the check could not look at all
/// (no browser on the machine, no such file) it shows nothing either way.
fn page_looked(call: &serde_json::Value, ok: bool, printed: &str) -> Option<Evidence> {
    let rendered = printed.contains(crate::page_check::RENDERED);
    // An engine that passes on no output leaves the call's own status to go by.
    if !(rendered || (printed.trim().is_empty() && ok)) {
        return None;
    }
    let raw = &call["rawInput"];
    let args = if raw["arguments"].is_object() { &raw["arguments"] } else { raw };
    let what = ["file", "page"].iter().find_map(|k| args[*k].as_str()).unwrap_or_default();
    let command = one_line(&format!("page_check {what}"));
    let ok = ok && (!rendered || printed.contains(crate::page_check::CLEAN));
    Some(Evidence { kind: "screenshot", command, ok, output: tail(printed, OUTPUT_KEPT), page: true })
}

/// The verdict on a turn: the last test run decides; a screenshot counts when nothing was tested,
/// and a look with `page_check` counts whichever way it went.
pub fn verdict(found: &[Evidence]) -> Option<&Evidence> {
    found.iter().rev().find(|e| e.kind == "tests").or_else(|| found.iter().rev().find(|e| e.kind == "screenshot" && (e.ok || e.page)))
}

const MISSING: &str = "Hub: your last reply changed {files} but showed no proof that it works. Before calling it \
done, run the project's tests or checks (for a visible UI change, a screenshot you look at also counts) and say \
what passed. If nothing can be run here, say why in one sentence.";

const PAGE_MISSING: &str = "Hub: your last reply changed {files} but showed no proof that it works. Before calling it \
done, look at the page with page_check (`file`: {pages}; no need to open a browser or parse the HTML yourself), fix \
what it finds and say what it showed. If page_check cannot run here, say why in one sentence.";

/// A web page, and what loads beside one.
fn is_page(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.ends_with(".html") || lower.ends_with(".htm")
}

fn beside_a_page(path: &str) -> bool {
    let lower = path.to_lowercase();
    [".css", ".js", ".mjs"].iter().any(|ext| lower.ends_with(ext))
}

/// What Hub asks after a turn that changed `paths` (the files that need proof, relative to the
/// repository `root`) and showed no proof. When they are web pages (with the styles and scripts
/// beside them), the way to show one is to look at it with `page_check`, so the question says that
/// instead of asking for tests. `page_check` finds a `file` from the thread's folder `cwd`, so each
/// page is named from there; a page outside that folder cannot be looked at so, and keeps the
/// question for tests.
pub fn ask_missing(paths: &[&str], root: &std::path::Path, cwd: &std::path::Path) -> String {
    let files = format!("{} {}", paths.len(), if paths.len() == 1 { "file" } else { "files" });
    let pages: Option<Vec<String>> = paths.iter().filter(|p| is_page(p)).map(|p| from_folder(root, cwd, p)).collect();
    let pages = pages.filter(|p| !p.is_empty() && paths.iter().all(|p| is_page(p) || beside_a_page(p)));
    let Some(pages) = pages else { return MISSING.replace("{files}", &files) };
    let list = if pages.len() == 1 { pages[0].clone() } else { format!("{}, one call each", pages.join(", ")) };
    PAGE_MISSING.replace("{files}", &files).replace("{pages}", &list)
}

/// `path`, relative to the repository `root`, as seen from the folder `cwd` inside it (with `/`),
/// or `None` when it lies outside that folder.
fn from_folder(root: &std::path::Path, cwd: &std::path::Path, path: &str) -> Option<String> {
    // Both the same way, so `D:/x` from git and `d:\x` from a thread agree.
    let canon = |p: &std::path::Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let full = canon(root).join(path);
    let rel = full.strip_prefix(canon(cwd)).ok()?;
    Some(rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"))
}

/// Docs need no proof; a reply that changed only these is done when it says so. Nor do files that
/// are the thing made for the person (a table, a drawing, a picture, a spreadsheet, a PDF).
pub fn needs_proof(path: &str) -> bool {
    let lower = path.to_lowercase();
    let made = [".csv", ".tsv", ".svg", ".png", ".jpg", ".jpeg", ".gif", ".webp", ".pdf", ".xlsx", ".xlsm", ".xls", ".ods", ".docx", ".pptx"]
        .iter()
        .any(|ext| lower.ends_with(ext));
    // A folder of things made for the person, with the script that made them.
    let output = ["output/", "outputs/"].iter().any(|dir| lower.starts_with(dir));
    let doc = made
        || output
        || [".md", ".mdx", ".rst", ".adoc"].iter().any(|ext| lower.ends_with(ext))
        || lower.starts_with("docs/")
        || lower.contains("/docs/")
        || ["license", "changelog", ".gitignore"].iter().any(|f| lower.rsplit('/').next() == Some(f));
    !doc
}

/// Lines a turn only took out: deleting code (often what the user asked for, or the agent's own
/// earlier work) leaves nothing new to show working.
pub fn only_removes(files: &[&crate::checkpoint::FileDiff]) -> bool {
    !files.is_empty() && files.iter().all(|f| f.added == 0)
}

/// The user said in their message not to test or build this time.
static WAIVED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)
          \b(?:no|without|skip|skipping|don'?t\s+(?:run|write|add|bother\s+with|need)(?:\s+any)?)\s+(?:the\s+)?(?:tests?|testing|proof|build(?:ing)?)\b
        | \bdon'?t\s+(?:test|build)\b
        | \b(?:ingen|inga|utan|skippa|hoppa\s+över)\s+(?:test|tester|testerna|testning|bevis|bygge|bygget)\b
        | \b(?:testa|bygg)\s+inte\b
        | \bbehöver\s+inte\s+(?:testa|testas|byggas|bygga)\b",
    )
    .expect("waived pattern")
});

/// Whether the person's own message asks for no tests or no build.
pub fn waived(message: &str) -> bool {
    WAIVED.is_match(message)
}

fn one_line(text: &str) -> String {
    let line = text.trim().trim_matches('`').lines().next().unwrap_or_default().trim();
    if line.chars().count() > 120 { format!("{}…", line.chars().take(119).collect::<String>()) } else { line.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn update(seq: i64, data: serde_json::Value) -> Event {
        Event { thread_id: "t".into(), seq, ts: 0, kind: EventKind::Update, data }
    }

    #[test]
    fn the_last_test_run_decides() {
        let events = [
            update(1, json!({ "sessionUpdate": "tool_call", "toolCallId": "a", "title": "`cargo test`", "kind": "execute", "status": "pending", "rawInput": { "command": "cargo test -p hub-core" } })),
            update(2, json!({ "sessionUpdate": "tool_call_update", "toolCallId": "a", "status": "failed" })),
            update(3, json!({ "sessionUpdate": "tool_call", "toolCallId": "b", "title": "Read src/lib.rs", "kind": "read", "status": "completed" })),
            update(4, json!({ "sessionUpdate": "tool_call", "toolCallId": "c", "title": "Run `npm run check`", "status": "in_progress" })),
            update(5, json!({ "sessionUpdate": "tool_call_update", "toolCallId": "c", "status": "completed" })),
        ];
        let found = evidence(&events);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0], Evidence { kind: "tests", command: "cargo test -p hub-core".into(), ok: false, output: String::new(), page: false });
        let last = verdict(&found).unwrap();
        assert_eq!((last.command.as_str(), last.ok), ("npm run check", true));
    }

    #[test]
    fn screenshots_count_when_nothing_was_tested() {
        let events = [update(1, json!({ "sessionUpdate": "tool_call", "toolCallId": "s", "title": "mcp__playwright__browser_take_screenshot", "status": "completed" }))];
        assert_eq!(verdict(&evidence(&events)).unwrap().kind, "screenshot");
        let reads = [update(1, json!({ "sessionUpdate": "tool_call", "toolCallId": "r", "title": "Read tests/attest.rs", "status": "completed" }))];
        assert!(evidence(&reads).is_empty());
    }

    /// A `page_check` call as each engine names Hub's tools, answered with `printed`.
    fn page_checks(printed: &str, status: &str) -> Vec<Vec<Event>> {
        let answer = |id: &str| json!({ "sessionUpdate": "tool_call_update", "toolCallId": id, "status": status, "content": [{ "type": "content", "content": { "type": "text", "text": printed } }] });
        [
            json!({ "title": "mcp__hub__page_check", "_meta": { "claudeCode": { "toolName": "mcp__hub__page_check" } }, "rawInput": { "file": "index.html" } }),
            json!({ "title": "Tool: hub/page_check", "rawInput": { "server": "hub", "tool": "page_check", "arguments": { "file": "index.html" } } }),
            json!({ "title": "hub.page_check", "rawInput": { "file": "index.html" } }),
        ]
        .into_iter()
        .map(|mut call| {
            call["sessionUpdate"] = json!("tool_call");
            call["toolCallId"] = json!("p");
            call["status"] = json!("in_progress");
            vec![update(1, call), update(2, answer("p"))]
        })
        .collect()
    }

    #[test]
    fn a_look_with_page_check_is_proof_and_its_problems_fail_it() {
        let clean = format!("{}720×300 px. {}", crate::page_check::RENDERED, crate::page_check::CLEAN);
        for turn in page_checks(&clean, "completed") {
            let found = evidence(&turn);
            let v = verdict(&found).unwrap();
            assert_eq!((v.kind, v.command.as_str(), v.ok, v.page), ("screenshot", "page_check index.html", true, true), "{turn:?}");
        }
        let broken = format!("{}720×300 px. 1 problem:\n- console.error: oops", crate::page_check::RENDERED);
        for turn in page_checks(&broken, "completed") {
            let found = evidence(&turn);
            assert!(!verdict(&found).unwrap().ok, "a page with an error is a failed look: {turn:?}");
        }
        // The check could not look (no browser): nothing shown either way.
        for turn in page_checks("page_check needs Chrome, Edge or Chromium on the machine", "failed") {
            assert!(verdict(&evidence(&turn)).is_none(), "{turn:?}");
        }
        // A command that only names the tool is not a look at a page.
        let grep = [update(1, json!({ "sessionUpdate": "tool_call", "toolCallId": "g", "title": "`rg hub.page_check`", "kind": "execute", "status": "completed", "rawInput": { "command": "rg hub.page_check" } }))];
        assert!(evidence(&grep).is_empty());
    }

    #[test]
    fn a_page_is_asked_to_be_looked_at_and_code_to_be_tested() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let page = ask_missing(&["index.html"], root, root);
        assert!(page.contains("page_check (`file`: index.html;") && page.contains("1 file") && !page.contains("tests"), "{page}");
        let two = ask_missing(&["index.html", "about.htm", "site.css", "app.js"], root, root);
        assert!(two.contains("`file`: index.html, about.htm, one call each;") && two.contains("4 files"), "{two}");
        for code in [&["src/main.rs"][..], &["index.html", "server.py"], &["app.js"]] {
            let asked = ask_missing(code, root, root);
            assert!(asked.contains("run the project's tests or checks") && !asked.contains("page_check"), "{asked}");
        }
    }

    /// `page_check` finds a file from the thread's folder, so a thread in a subfolder of the
    /// repository is given its pages from there; a page outside its folder is asked for as code.
    #[test]
    fn pages_are_named_from_the_threads_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("site/docs")).unwrap();
        let cwd = root.join("site");
        let asked = ask_missing(&["site/index.html", "site/docs/about.html", "site/style.css"], root, &cwd);
        assert!(asked.contains("`file`: index.html, docs/about.html, one call each;"), "{asked}");
        let outside = ask_missing(&["site/index.html", "other/index.html"], root, &cwd);
        assert!(outside.contains("run the project's tests or checks") && !outside.contains("page_check"), "{outside}");
    }

    #[test]
    fn docs_need_no_proof() {
        assert!(!needs_proof("README.md"));
        assert!(!needs_proof("docs/plan.txt"));
        assert!(needs_proof("src/main.rs"));
        assert!(needs_proof("mock.txt"));
        assert!(!needs_proof("outputs/orders.csv"));
        assert!(!needs_proof("drawing.SVG"));
        assert!(needs_proof("page.html"));
        assert!(!needs_proof("outputs/visning/skapa_visning.py"));
        assert!(!needs_proof("Output/sales.html"));
        assert!(needs_proof("src/output/render.rs"));
    }

    fn file(added: usize, removed: usize) -> crate::checkpoint::FileDiff {
        crate::checkpoint::FileDiff { path: "src/a.rs".into(), diff: String::new(), lines: added + removed, added, removed }
    }

    #[test]
    fn taking_code_out_needs_no_proof() {
        let (gone, edited) = (file(0, 40), file(3, 1));
        assert!(only_removes(&[&gone]));
        assert!(!only_removes(&[&gone, &edited]));
        assert!(!only_removes(&[]));
    }

    #[test]
    fn the_user_can_say_no_tests() {
        for yes in ["Revert it, no tests needed", "just delete it, don't run tests", "skip the tests this time", "Ta bort det, inga tester", "testa inte, bara ändra", "skippa testerna", "don't build anything"] {
            assert!(waived(yes), "{yes}");
        }
        assert!(!waived("add tests for the parser"));
        assert!(!waived("run the tests and fix what fails"));
        assert!(!waived("Lägg till tester för parsern"));
    }

    #[test]
    fn a_clean_exit_that_printed_failures_is_worth_a_second_look() {
        assert!(may_hide_failure("test a ... ok\ntest b ... FAILED\ntest result: FAILED. 1 passed; 1 failed"));
        assert!(may_hide_failure("npm ERR! code ELIFECYCLE"));
        assert!(!may_hide_failure("test result: ok. 468 passed; 0 failed; 0 ignored"));
        assert!(!may_hide_failure("All 12 checks passed. No errors, no warnings."));
        assert!(!may_hide_failure("svelte-check found 0 errors and 0 warnings"));
        assert!(!may_hide_failure(""));
    }

    #[test]
    fn output_is_gathered_from_content_and_terminal_chunks() {
        let events = [
            update(1, json!({ "sessionUpdate": "tool_call", "toolCallId": "a", "title": "`cargo test | tail`", "kind": "execute", "status": "pending", "rawInput": { "command": "cargo test | tail" } })),
            update(2, json!({ "sessionUpdate": "tool_call_update", "toolCallId": "a", "_meta": { "terminal_output_delta": { "data": "test b ... " } } })),
            update(3, json!({ "sessionUpdate": "tool_call_update", "toolCallId": "a", "_meta": { "terminal_output_delta": { "data": "FAILED\n" } } })),
            update(4, json!({ "sessionUpdate": "tool_call_update", "toolCallId": "a", "status": "completed" })),
            update(5, json!({ "sessionUpdate": "tool_call", "toolCallId": "b", "title": "Run `npm run check`", "status": "completed", "content": [{ "type": "content", "content": { "type": "text", "text": "0 errors" } }] })),
        ];
        let found = evidence(&events);
        assert_eq!(found[0].output, "test b ... FAILED");
        assert!(found[0].ok && may_hide_failure(&found[0].output));
        assert_eq!(found[1].output, "0 errors");
    }
}

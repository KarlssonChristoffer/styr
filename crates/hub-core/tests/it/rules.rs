//! The rules in CLAUDE.md that a grep can check, as ratchets. `frozen/rules.txt` holds what already
//! breaks a rule, one `rule path size` line per file so you see where the debt is, but the ratchet is
//! the TOTAL per rule across the workspace (`ratchet.rs`): it fails when a rule's total grew, and
//! names the files that rose or are new. Moving a case between files passes without a bless. Rules:
//! engine names stay in the named engine files, the browser's own controls stay out of `.svelte`, a
//! new Rust file starts with `//!`, a new Svelte file with a `<!-- -->` comment (each such file counts
//! one), and the habits in `HABITS` (a helper exists, so a new hand-rolled copy is a break).
//! The engine rule and the habits read non-test code only: whole test files (`tests.rs`,
//! `*_tests.rs`) are skipped, and in other files each item under a `#[cfg(test)]` is, wherever it
//! sits (`test_code.rs`); the code after such an item is read again.
//!
//! After you cleaned something up: `RULES_BLESS=1 cargo test -p hub-core --test it rules::` rewrites
//! the lines to what is true now; it is refused while any rule's total grew, so it never raises one.
//! A real exception is a line you add to `frozen/rules.txt` by hand.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::{ratchet, test_code};

/// Engine knowledge may live only here (relative to the repo root), see "Engines" in CLAUDE.md.
const ENGINE_FILES: &[&str] = &[
    "crates/hub-core/src/config.rs",
    "crates/hub-core/src/engines.rs",
    "crates/hub-core/src/engines/tests.rs",
    "crates/hub-core/src/activity/claude_code.rs",
    "crates/hub-core/src/connections.rs",
    "crates/hub-core/src/engine_home.rs",
    "crates/hub-core/src/spend/engines.rs",
    "crates/hub-core/src/codex_live.rs",
    "crates/hub-core/src/cloud_threads.rs",
    "crates/hub-core/src/cloud_threads/codex_env.rs",
    "crates/hub-core/src/cloud_threads/tests.rs",
    "crates/hub-core/src/plan_meter.rs",
    "crates/hub-core/src/live_output.rs",
    "crates/hub-core/src/stats.rs",
    "crates/hub-core/src/customs_known.rs",
    "crates/hub-core/src/connections/launch.rs",
    "crates/hub-core/src/connections/parse.rs",
    "crates/hub-core/src/engines/presets.rs",
    "crates/hub-core/src/engines/setup.rs",
    "crates/hub-core/src/store/usage.rs",
    // The whole file is one vendor's cloud sessions (its login files and Sessions API), as
    // `cloud_threads.rs` is.
    "crates/hub-core/src/cloud_usage.rs",
    // Each harness's own config commands and settings, as data.
    "crates/hub-core/src/harness_settings/catalog.rs",
    // Its job is to speak each engine's wire (it imitates `_meta.claudeCode.toolName`).
    "crates/mock-agent/src/lib.rs",
];
const ENGINE_WORDS: &[&str] = &["claude", "codex", "hermes", "CLAUDE_"];
/// The shared pieces that wrap a browser control (the rest use them).
const CONTROL_FILES: &[&str] = &["app/src/lib/components/Select.svelte"];

fn root() -> PathBuf {
    hub_core::source_tree::root()
}

fn files(dir: &Path, exts: &[&str], out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else { return };
    for entry in read.flatten() {
        let path = entry.path();
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if path.is_dir() {
            if !matches!(name.as_str(), "node_modules" | "target" | "bindings" | "dist" | "templates" | "gen") {
                files(&path, exts, out);
            }
        } else if path.extension().is_some_and(|e| exts.iter().any(|x| e == *x)) {
            out.push(path);
        }
    }
}

fn rel(p: &Path) -> String {
    p.strip_prefix(root().join("")).unwrap_or(p).to_string_lossy().replace('\\', "/")
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display())).replace("\r\n", "\n")
}

/// Rust source files under `crates/*/src` and `app/src-tauri/src`.
fn rust_sources() -> Vec<(String, String)> {
    let mut paths = Vec::new();
    for c in std::fs::read_dir(root().join("crates")).unwrap().flatten() {
        files(&c.path().join("src"), &["rs"], &mut paths);
    }
    files(&root().join("app/src-tauri/src"), &["rs"], &mut paths);
    let mut v: Vec<_> = paths.iter().map(|p| (rel(p), read(p))).collect();
    v.sort();
    v
}

/// Source files of the app (`.svelte` and `.ts`) plus the Rust ones.
fn all_sources() -> Vec<(String, String)> {
    let mut paths = Vec::new();
    files(&root().join("app/src"), &["svelte", "ts"], &mut paths);
    let mut v: Vec<_> = paths.iter().map(|p| (rel(p), read(p))).collect();
    v.extend(rust_sources());
    v.sort();
    v
}

/// A module of tests kept in its own file (`foo/tests.rs`, `foo/handover_tests.rs`): test code, like an item under `#[cfg(test)]`.
fn is_test_file(path: &str) -> bool {
    path.ends_with("/tests.rs") || path.ends_with("_tests.rs")
}

/// Lines of non-test Rust code that name an engine, ignoring `//` comments.
fn engine_lines(text: &str) -> usize {
    code_lines(text)
        .filter(|l| {
            let code = l.split("//").next().unwrap_or("");
            ENGINE_WORDS.iter().any(|w| code.contains(w))
        })
        .count()
}

/// Lines that open a browser control: `<select`, or an `<input>` that is not a text box.
fn control_lines(text: &str) -> usize {
    const KINDS: &[&str] = &["checkbox", "radio", "date", "time", "datetime-local", "color", "range", "file"];
    text.lines()
        .filter(|l| {
            l.contains("<select")
                || l.match_indices("<input").any(|(i, _)| {
                    let tag = &l[i..];
                    KINDS.iter().any(|k| tag.contains(&format!("type=\"{k}\"")) || tag.contains(&format!("type={k} ")))
                })
        })
        .count()
}

/// A Rust file has its `//!` line among the first lines (a `#![..]` may come first).
fn has_header(text: &str) -> bool {
    text.lines().take(30).any(|l| l.starts_with("//!"))
}

/// A Svelte component opens with a comment saying what it shows and what uses it.
fn has_svelte_header(text: &str) -> bool {
    text.trim_start().starts_with("<!--")
}

/// A habit with a shared helper: lines that match `hit` outside the files that own the helper
/// (`owners`) are counted per file, in non-test code. `exts` says which files are looked at.
struct Habit {
    rule: &'static str,
    exts: &'static [&'static str],
    owners: &'static [&'static str],
    hit: fn(&str) -> bool,
    use_instead: &'static str,
}

const HABITS: &[Habit] = &[
    Habit { rule: "lock_unwrap", exts: &["rs"], owners: &["crates/hub-core/src/locked.rs"], hit: |l| l.contains(".lock().unwrap()"), use_instead: "`.locked()` (LockExt in locked.rs) survives a poisoned lock" },
    Habit { rule: "empty_catch", exts: &["ts", "svelte"], owners: &["app/src/lib/quiet.ts"], hit: |l| l.replace(' ', "").contains("catch(()=>{})"), use_instead: "`quiet(promise, what)` from lib/quiet.ts, which says what went quiet" },
    Habit { rule: "set_interval", exts: &["ts", "svelte"], owners: &["app/src/lib/poll.ts"], hit: |l| l.contains("setInterval("), use_instead: "`poll(fn, ms)` from lib/poll.ts, the shared timer that rests when the window is hidden" },
    Habit { rule: "whole_log", exts: &["rs"], owners: &[], hit: |l| l.contains(".events(") && l.contains("1_000_000"), use_instead: "read the log in chunks or ask for the part you need; do not load the whole log" },
    Habit { rule: "own_clip", exts: &["rs", "ts", "svelte"], owners: &["crates/hub-core/src/text.rs"], hit: |l| l.contains("fn clip") || l.contains("function clip"), use_instead: "the `clip*` helpers in text.rs" },
    Habit { rule: "unwrap", exts: &["rs"], owners: &[], hit: |l| l.contains(".unwrap()"), use_instead: "`?`, `.unwrap_or*`, or `.expect(\"why it cannot fail\")`; a bare unwrap panics a daemon thread without saying why" },
    Habit { rule: "let_discard", exts: &["rs"], owners: &[], hit: let_discard, use_instead: "`.warn_on_err(what)` (ResultExt in locked.rs) logs a failure somebody should see; when the failure is expected, `.ignore(\"why\")` (Ignore in locked.rs) says why at the call site. A bare `let _ =` cannot be told from a failed write nobody noticed. Plain clean-up of a temp file uses `clean_file` / `clean_dir`" },
    Habit { rule: "raw_git", exts: &["rs"], owners: &["crates/hub-core/src/git.rs"], hit: |l| l.contains("Command::new(\"git\")") || l.contains("command(\"git\""), use_instead: "git.rs, which owns how Hub runs git" },
];

/// A bare `let _ =` opening a line or a statement after `{` or `;` (`if x { let _ = f(); }`), outside
/// a trailing `//` comment, or an `.ignore("")` that gives no reason.
fn let_discard(l: &str) -> bool {
    let code = l.split("//").next().unwrap_or("");
    code.trim_start().starts_with("let _ =") || code.contains("{ let _ =") || code.contains("; let _ =") || l.contains(".ignore(\"\")")
}

/// The lines that are not a test-only item (`test_code.rs`: a `#[cfg(test)]` hides the item under
/// it, wherever it sits, and the code after it is read again). A `.ts` or `.svelte` file has no
/// such marker, so all of it is read.
fn code_lines(text: &str) -> impl Iterator<Item = &str> {
    test_code::code_lines(text).map(|(_, l)| l)
}

fn habit_lines(h: &Habit, text: &str) -> usize {
    code_lines(text).filter(|l| !l.trim_start().starts_with("//") && (h.hit)(l)).count()
}

/// What is wrong now: `(rule, path) -> size` (the size is 0 for a rule that has none).
fn now() -> BTreeMap<(String, String), usize> {
    let mut m = BTreeMap::new();
    for (path, text) in rust_sources() {
        if !ENGINE_FILES.contains(&path.as_str()) && !is_test_file(&path) {
            let n = engine_lines(&text);
            if n > 0 {
                m.insert(("engine".to_string(), path.clone()), n);
            }
        }
        if !has_header(&text) {
            m.insert(("header".to_string(), path.clone()), 0);
        }
    }
    for (path, text) in all_sources() {
        let ext = path.rsplit('.').next().unwrap_or("");
        for h in HABITS {
            if h.exts.contains(&ext) && !h.owners.contains(&path.as_str()) && !is_test_file(&path) {
                let n = habit_lines(h, &text);
                if n > 0 {
                    m.insert((h.rule.to_string(), path.clone()), n);
                }
            }
        }
        if path.ends_with(".svelte") && !has_svelte_header(&text) {
            m.insert(("header".to_string(), path.clone()), 0);
        }
        if path.ends_with(".svelte") && !CONTROL_FILES.contains(&path.as_str()) {
            let n = control_lines(&text);
            if n > 0 {
                m.insert(("controls".to_string(), path), n);
            }
        }
    }
    m
}

fn frozen_path() -> PathBuf {
    hub_core::source_tree::hub_core().join("tests/it/frozen/rules.txt")
}

fn parse(text: &str) -> BTreeMap<(String, String), usize> {
    let mut m = BTreeMap::new();
    for line in text.replace("\r\n", "\n").lines().filter(|l| !l.trim().is_empty()) {
        let mut p = line.split_whitespace();
        let (rule, path) = (p.next().unwrap().to_string(), p.next().unwrap().to_string());
        m.insert((rule, path), p.next().map_or(0, |n| n.parse().unwrap()));
    }
    m
}

fn render(m: &BTreeMap<(String, String), usize>) -> String {
    m.iter().map(|((r, p), n)| if r == "header" { format!("{r} {p}\n") } else { format!("{r} {p} {n}\n") }).collect()
}

const HELP: &str = "\nEach rule is in CLAUDE.md. Fix the file; do not add a line to frozen/rules.txt unless it is a real exception.\nMoving a case between files needs nothing; after a clean-up: RULES_BLESS=1 cargo test -p hub-core --test it rules::";

/// A rule whose total grew, and the files that rose or are new (the likely culprits).
fn say(g: &ratchet::Grown) -> String {
    let files: Vec<String> = g
        .files
        .iter()
        .map(|(file, was, n)| match was {
            None => format!("  {file}: new ({})", detail(&g.rule, *n)),
            Some(old) => format!("  {file}: grew from {old} to {n}"),
        })
        .collect();
    format!("`{}` grew from {} to {} in total:\n{}", g.rule, g.frozen, g.now, files.join("\n"))
}

#[test]
fn rules_only_get_fewer_breaks() {
    let now = now();
    let path = frozen_path();
    let frozen = std::fs::read_to_string(&path).ok().map(|t| parse(&t));
    let grown = if std::env::var_os("RULES_BLESS").is_some() {
        // Rewrites the lines to what is true now, unless a rule's total grew (then it writes nothing).
        match ratchet::bless(frozen.as_ref(), &now) {
            Ok(out) => {
                std::fs::write(&path, render(&out)).unwrap();
                return;
            }
            Err(grown) => grown,
        }
    } else {
        ratchet::grown(&frozen.expect("frozen/rules.txt is missing: RULES_BLESS=1 writes it"), &now)
    };
    let broken: Vec<String> = grown.iter().map(say).collect();
    assert!(broken.is_empty(), "{}{HELP}\n{}", broken.join("\n"), advice());
}

fn detail(rule: &str, n: usize) -> String {
    match rule {
        "engine" => format!("{n} lines name claude, codex, hermes or CLAUDE_ outside the engine files"),
        "controls" => format!("{n} browser controls; use Hub's own (Select.svelte, the shared pieces)"),
        r if HABITS.iter().any(|h| h.rule == r) => {
            let h = HABITS.iter().find(|h| h.rule == r).unwrap();
            format!("{n} lines; use {}", h.use_instead)
        }
        _ => "no `//!` line (Rust) or `<!-- -->` comment (Svelte) on top saying what the file owns".to_string(),
    }
}

fn advice() -> String {
    format!("Engine files: {}", ENGINE_FILES.join(", "))
}

#[test]
fn the_rules_name_real_files() {
    for f in ENGINE_FILES.iter().chain(CONTROL_FILES).chain(HABITS.iter().flat_map(|h| h.owners)) {
        assert!(root().join(f).exists(), "rules.rs names {f}, which is gone");
    }
}

#[test]
fn the_readers_count_what_they_should() {
    assert_eq!(engine_lines("let a = \"claude\";\n// codex here\nlet b = 1; // hermes\nlet c = CLAUDE_X;\n#[cfg(test)]\nlet d = \"claude\";\n"), 2);
    assert_eq!(control_lines("<select bind:value={a}>\n<input type=\"checkbox\" />\n<input class=\"x\" type=\"text\" />\n<input type=\"file\" hidden>\n"), 3);
    let h = |rule: &str| HABITS.iter().find(|h| h.rule == rule).unwrap();
    assert_eq!(habit_lines(h("lock_unwrap"), "let g = m.lock().unwrap();\n// m.lock().unwrap()\n#[cfg(test)]\nm.lock().unwrap();\n"), 1);
    assert_eq!(habit_lines(h("empty_catch"), "p.catch(() => {});\np.catch( ()=>{} )\np.catch(() => log())\n"), 2);
    assert_eq!(habit_lines(h("set_interval"), "const t = setInterval(f, 5);\nwindow.setInterval(f, 5);\nclearInterval(t);\n"), 2);
    assert_eq!(habit_lines(h("whole_log"), "store.events(id, 0, 1_000_000).await?;\nstore.events(id, 0, 50).await?;\n"), 1);
    assert_eq!(habit_lines(h("own_clip"), "fn clip(a: &str) {}\nfn clip_x() {}\nfunction clipText() {}\nlet clipped = 1;\n"), 3);
    assert_eq!(habit_lines(h("unwrap"), "a.unwrap();\nb.unwrap_or(1);\n// c.unwrap()\nd.unwrap_or_default();\n"), 1);
    assert_eq!(habit_lines(h("let_discard"), "let _ = f();\n    let _ = g();\nlet _guard = h();\nlet x = 1;\ntx.send(1).ignore(\"the window closed\");\nf().ignore(\"\");\n"), 3);
    assert_eq!(habit_lines(h("let_discard"), "if a { let _ = f(); }\nx(); let _ = g();\nlet y = z(); // { let _ = old(); }\nlet _unused = 1; let _v = 2;\n"), 2);
    // A test-only item anywhere hides only itself: a `mod tests;` declared on top does not hide the code under it.
    assert_eq!(habit_lines(h("let_discard"), "#[cfg(test)]\nmod tests;\nfn f() {\n    let _ = g();\n}\n#[cfg(test)]\nfn helper() {\n    let _ = h();\n}\nfn k() { let _ = j(); }\n"), 2);
    assert_eq!(engine_lines("#[cfg(test)]\nmod tests;\nlet a = \"codex\";\n"), 1);
    assert_eq!(habit_lines(h("raw_git"), "Command::new(\"git\")\nproc::command(\"git\")\ntokio_command(\"git\")\nCommand::new(\"gh\")\n"), 3);
    assert!(has_header("#![allow(x)]\n//! what it owns\n"));
    assert!(!has_header("use std::io;\n"));
    assert!(has_svelte_header("<!-- shows x -->\n<script></script>"));
    assert!(!has_svelte_header("<script lang=\"ts\"></script>"));
}

/// hub-core has ONE integration test program (`tests/it/main.rs`): every other `tests/*.rs` is
/// another link of the whole core, which the update build pays on every version.
#[test]
fn hub_core_tests_are_one_program() {
    let dir = hub_core::source_tree::hub_core().join("tests");
    let mut loose: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !["it", "compat", "fixtures"].contains(&n.as_str()))
        .collect();
    loose.sort();
    assert!(loose.is_empty(), "tests/ holds only it/, compat/ and fixtures/; add a module to tests/it/main.rs instead of {loose:?}");
}

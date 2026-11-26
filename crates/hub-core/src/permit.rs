//! Permission answers Hub gives itself. Before a permission question from an engine reaches the
//! user, it is checked against the user's settings (Settings > Safety): protected files and
//! refuse rules are refused, allow rules and Hub's own quiet tools are allowed, and the default
//! level allows what it covers. Anything else, and anything unclear, still goes to the user.
//!
//! Only questions the engine asks are answered here. What the engine does without asking, by its
//! own permission mode, never comes past Hub.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use crate::model::{PermissionLevel, PermissionPrefs};

mod bench;
pub use bench::bench;

/// Hub's own tools that only read or show something inside Hub. They never ask.
pub const QUIET_TOOLS: &[&str] = &[
    "hub_brief",
    "problems",
    "problem",
    "problem_dossier",
    "hub_status",
    "where_am_i",
    "list_threads",
    "read_thread",
    "wait_for_thread",
    // Reads a bench run's report from Hub's data folder.
    "bench_report",
    // Runs only the read commands a tool's hub.toml names.
    "your_tools",
    // Says which API connections a project has, by variable name, never a key.
    "api_connections",
    // A cheap read-only question to the decision model; nothing leaves Hub but the text the agent gave.
    "ask_jev",
    // Declaring only puts something on Customs' list as held; asking first would defeat it.
    "customs_search",
    "customs_declare",
    // Plugins: a GitHub search, and a card in the thread the user decides on.
    "plugins_search",
    "suggest_plugin",
    // Only a card in the thread: nothing is handed over until the user approves the brief there.
    "offer_handoff",
    // A suggested recipe only waits on the Recipes page until the user keeps it.
    "list_recipes",
    "suggest_recipe",
    "docs_map",
    "docs_search",
    "docs_read",
    "docs_for",
    "show_page",
    "page_check",
    "second_opinion",
    // Fresh sessions read the design document; nothing changes.
    "review_design",
    "watch_pull_request",
    "stop_watching_pull_request",
    // Buttons under the agent's question; nothing happens until the user clicks one.
    "offer_replies",
    // A question that does not wait: the agent carries on with its own assumption.
    "ask_user",
    // The TL;DR on the thread's hover card, written by the agent; changes nothing else.
    "write_tldr",
    // The plan card the user sees while the agent works; changes nothing.
    "update_steps",
    // An emoji on the user's latest message; quiet, and the user can take it away.
    "react",
    // Marks on a thread in the user's list, each undone with one click.
    "settle_thread",
    "rename_thread",
    "pin_thread",
    "snooze_thread",
];

/// Hub's answer to a permission question.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Allow(String),
    Refuse(String),
}

/// Hub's own files that no agent reads or changes, whatever the level or the rules: the connection
/// token, the secrets file, the settings (other machines' tokens) and the database. Found by name
/// under a `hub` folder (`~/.local/share/hub/token`, `AppData\Local\hub\hub.json`) and under
/// this core's data folder, in the call's paths and its command line.
pub fn hub_files(call: &Value) -> Option<Verdict> {
    static NAMED: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)[\\/]hub[\\/](token|token\.window|secrets\.json|hub\.json|hub\.db[\w.-]*)(?:[^\w.-]|$)").expect("constant regex"));
    let data_dir = hub_install::data_dir().to_string_lossy().replace('\\', "/").to_lowercase();
    let mut texts = paths(call);
    texts.extend(command(call));
    let hit = texts.iter().any(|t| {
        let t = t.replace('\\', "/");
        NAMED.is_match(&t) || (!data_dir.is_empty() && t.to_lowercase().contains(&data_dir))
    });
    hit.then(|| Verdict::Refuse("Hub's own files (its connection token, secrets, settings and database) are never read or changed by an agent".into()))
}

/// What Hub answers by itself, or None when the user should be asked.
pub fn decide(prefs: &PermissionPrefs, call: &Value, cwd: &Path) -> Option<Verdict> {
    let kind = call["kind"].as_str().unwrap_or_default();
    // Leaving plan mode is the user approving a plan.
    if kind == "switch_mode" {
        return None;
    }
    if let Some(refused) = hub_files(call) {
        return Some(refused);
    }
    if matches!(kind, "edit" | "delete" | "move") {
        for path in paths(call) {
            if let Some(glob) = prefs.protected.iter().find(|g| path_matches(g, &path, cwd)) {
                return Some(Verdict::Refuse(format!("{path} is protected ({glob})")));
            }
        }
    }
    let command = command(call);
    let names = names(call);
    if let Some(rule) = prefs.deny.iter().find(|r| denied(r, kind, command.as_deref(), &names)) {
        return Some(Verdict::Refuse(format!("refused by your rule {rule}")));
    }
    if allowed(&prefs.allow, kind, command.as_deref(), &names) {
        return Some(Verdict::Allow("allowed by your rules".into()));
    }
    if let Some(tool) = quiet_tool(&names) {
        return Some(Verdict::Allow(format!("{tool} only reads Hub")));
    }
    let level = prefs.level;
    let reads = matches!(kind, "read" | "search" | "fetch" | "think");
    let edits = matches!(kind, "edit" | "move");
    let ok = match level {
        PermissionLevel::Ask => false,
        PermissionLevel::Read => reads,
        PermissionLevel::Edit => reads || edits,
        PermissionLevel::Everything => kind != "delete" && command.as_deref().is_none_or(|c| risky(c).is_none()),
    };
    let what = match level {
        PermissionLevel::Read => "reading",
        PermissionLevel::Edit => "editing",
        _ => "everything",
    };
    ok.then(|| Verdict::Allow(format!("your default allows {what}")))
}

/// The command line of a call, from its input or a title like "Run `cargo test`".
fn command(call: &Value) -> Option<String> {
    let input = &call["rawInput"];
    match &input["command"] {
        Value::String(c) => return Some(c.clone()),
        Value::Array(parts) => {
            let parts: Vec<&str> = parts.iter().filter_map(Value::as_str).collect();
            if !parts.is_empty() {
                return Some(parts.join(" "));
            }
        }
        _ => {}
    }
    if call["kind"] == "execute" {
        let title = call["title"].as_str().unwrap_or_default();
        return Some(ticked(title).unwrap_or(title).to_owned());
    }
    None
}

/// What a call may be called: its title, the code in it, and the tool name engines put in `_meta`.
fn names(call: &Value) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(title) = call["title"].as_str() {
        names.push(title.trim().to_owned());
        if let Some(code) = ticked(title) {
            names.push(code.to_owned());
        }
    }
    if let Some(meta) = call["_meta"].as_object() {
        for v in meta.values() {
            if let Some(name) = v["toolName"].as_str() {
                names.push(name.to_owned());
            }
        }
    }
    names
}

fn ticked(title: &str) -> Option<&str> {
    let start = title.find('`')? + 1;
    let len = title[start..].find('`')?;
    Some(&title[start..start + len])
}

/// The paths a file call touches.
fn paths(call: &Value) -> Vec<String> {
    let mut out: Vec<String> = call["locations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| l["path"].as_str().map(str::to_owned))
        .collect();
    for key in ["file_path", "path", "notebook_path", "new_path", "destination", "source"] {
        if let Some(p) = call["rawInput"][key].as_str() {
            out.push(p.to_owned());
        }
    }
    out
}

/// Hub's quiet tool a call is, as engines name MCP tools: `mcp__hub__hub_status`,
/// `hub.hub_status`, `hub/hub_status` or `hub: hub_status`.
fn quiet_tool(names: &[String]) -> Option<&'static str> {
    hub_tool(names, QUIET_TOOLS)
}

/// Which of Hub's own tools in `among` a tool call is, however its engine names MCP tools (as
/// `quiet_tool`). Proof before done reads it to know a `page_check` (`proof.rs`).
pub fn hub_tool_call(call: &Value, among: &[&'static str]) -> Option<&'static str> {
    hub_tool(&names(call), among)
}

/// Which of Hub's own tools a tool call is, any tool, however its engine names MCP tools (the same
/// matcher as `hub_tool_call`, without a list to pick from). The bench counts calls by it
/// (`bench/check.rs`); lower case, and not checked against Hub's registry.
pub fn hub_tool_name(call: &Value) -> Option<String> {
    names(call).iter().find_map(|n| hub_name(n))
}

fn hub_tool(names: &[String], among: &[&'static str]) -> Option<&'static str> {
    names.iter().find_map(|n| {
        let name = hub_name(n)?;
        among.iter().find(|t| **t == name).copied()
    })
}

/// The tool name after Hub's server name in one of a call's names (`mcp__hub__x`, `hub.x`, ...).
fn hub_name(name: &str) -> Option<String> {
    static HUB_TOOL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)(?:^|[^a-z0-9])hub(?:__|\s*[./:]\s*)([a-z_]+)").expect("constant regex"));
    Some(HUB_TOOL.captures(name)?[1].to_lowercase())
}

/// The problem tools that change something, which Vakten's own thread may use without asking.
/// Hub holds them to the lanes itself (`problems::vakten::may_fix`).
pub const VAKTEN_TOOLS: &[&str] = &["stamp_problem", "change_problem", "problem_thread", "tell_user", "tool_review", "answer_worker"];

/// Vakten's thread: like a routine (reads, looks, changes no files), plus the problem tools.
pub fn vakten(prefs: &PermissionPrefs, call: &Value, cwd: &Path) -> Verdict {
    if let Some(tool) = hub_tool(&names(call), VAKTEN_TOOLS) {
        return Verdict::Allow(format!("Vakten may use {tool}"));
    }
    match routine(prefs, call, cwd) {
        Verdict::Refuse(_) => Verdict::Refuse(
            "Vakten changes no files and runs only commands that look; start a fix thread with problem_thread instead".into(),
        ),
        allowed => allowed,
    }
}

/// Hub's answer for a thread that runs without the user (a fix thread from Problems), which
/// never waits for them: the user's settings answer first, then anything but deleting and
/// commands that destroy work is allowed, as at the "everything" level, and what is left is
/// refused with a note to work around it or say so in the pull request.
pub fn unattended(prefs: &PermissionPrefs, call: &Value, cwd: &Path) -> Verdict {
    if call["kind"] == "switch_mode" {
        return Verdict::Allow("a background thread goes on with its own plan".into());
    }
    if let Some(verdict) = decide(prefs, call, cwd) {
        return verdict;
    }
    let everything = PermissionPrefs { level: PermissionLevel::Everything, ..prefs.clone() };
    if let Some(Verdict::Allow(_)) = decide(&everything, call, cwd) {
        return Verdict::Allow("a background thread does not wait for you".into());
    }
    Verdict::Refuse(
        "this thread runs in the background and nobody answers its questions; do it another way, or say in the pull request what you wanted to run and why"
            .into(),
    )
}

/// `Unattended.by` of a thread the bench started (`bench/run.rs`).
pub const BENCH: &str = "Bench";

/// The answer for an unattended thread: [`bench`] for a bench case, [`unattended`] for the rest.
pub fn unattended_by(by: &str, prefs: &PermissionPrefs, call: &Value, cwd: &Path) -> Verdict {
    if by == BENCH { bench(prefs, call, cwd) } else { unattended(prefs, call, cwd) }
}

/// A rule split into an optional tool (`Bash(...)`) and its pattern.
fn parse_rule(rule: &str) -> (Option<&str>, &str) {
    let rule = rule.trim();
    if let Some(open) = rule.find('(')
        && rule.ends_with(')')
        && open > 0
        && rule[..open].chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        let inner = &rule[open + 1..rule.len() - 1];
        return (Some(&rule[..open]), inner);
    }
    (None, rule)
}

/// Claude Code's `npm test:*` means "npm test" and anything after it.
fn rule_glob(pattern: &str) -> String {
    match pattern.strip_suffix(":*") {
        Some(head) => format!("{head}*"),
        None => pattern.to_owned(),
    }
}

/// Whether a rule's tool part fits the call. Shell tools mean any command.
fn tool_fits(tool: &str, kind: &str, names: &[String]) -> bool {
    let shell = ["bash", "shell", "powershell", "execute", "run"].contains(&tool.to_lowercase().as_str());
    (shell && kind == "execute") || names.iter().any(|n| n.eq_ignore_ascii_case(tool))
}

fn denied(rule: &str, kind: &str, command: Option<&str>, names: &[String]) -> bool {
    let (tool, pattern) = parse_rule(rule);
    if pattern.is_empty() {
        return false;
    }
    let glob = rule_glob(pattern);
    if let Some(tool) = tool {
        if !tool_fits(tool, kind, names) {
            return false;
        }
        if glob == "*" {
            return true;
        }
    }
    // Any command in a chain is enough to refuse it.
    if let Some(c) = command
        && (text_matches(&glob, c) || parts(c).any(|p| text_matches(&glob, p)))
    {
        return true;
    }
    tool.is_none() && names.iter().any(|n| text_matches(&glob, n))
}

fn allowed(rules: &[String], kind: &str, command: Option<&str>, names: &[String]) -> bool {
    let rules: Vec<(Option<&str>, String)> = rules
        .iter()
        .map(|r| parse_rule(r))
        .filter(|(_, p)| !p.is_empty())
        .map(|(t, p)| (t, rule_glob(p)))
        .collect();
    let fits = |tool: Option<&str>| tool.is_none_or(|t| tool_fits(t, kind, names));
    if let Some(c) = command {
        // Every command in a chain must be allowed, so `npm test && rm -rf ~` is not.
        let parts: Vec<&str> = parts(c).collect();
        return !parts.is_empty()
            && parts.iter().all(|p| rules.iter().any(|(t, g)| fits(*t) && text_matches(g, p)));
    }
    rules.iter().any(|(t, g)| match t {
        Some(t) => tool_fits(t, kind, names) && (g == "*" || names.iter().any(|n| text_matches(g, n))),
        None => names.iter().any(|n| text_matches(g, n)),
    })
}

/// The commands of a command line, split at `&&`, `||`, `;` and `|`.
fn parts(line: &str) -> impl Iterator<Item = &str> {
    static SPLIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"&&|\|\||;|\|").expect("constant regex"));
    SPLIT.split(line).map(str::trim).filter(|p| !p.is_empty())
}

/// Like [`parts`], and also at `&`, a new line and the start of a `$(…)` or `` `…` ``
/// substitution, so a command hidden in any of them is read on its own.
fn every_part(line: &str) -> impl Iterator<Item = &str> {
    static SPLIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"&&|\|\||;|\||&|\r?\n|\$\(|`").expect("constant regex"));
    SPLIT.split(line).map(str::trim).filter(|p| !p.is_empty())
}

/// Whether `opt` is an option of wrapper `name` that takes the next word as its value.
fn takes_value(name: &str, opt: &str) -> bool {
    let with: &[&str] = match name {
        "sudo" | "doas" => &["-u", "-g", "-U", "-C", "-p", "-r", "-t", "-T", "-D", "-h"],
        "nice" => &["-n"],
        "ionice" => &["-c", "-n", "-p"],
        "timeout" => &["-s", "-k"],
        "env" => &["-u", "-C", "-S"],
        "xargs" => &["-I", "-n", "-P", "-L", "-d", "-a", "-E", "-s"],
        "stdbuf" => &["-i", "-o", "-e"],
        _ => &[],
    };
    with.contains(&opt)
}

/// What a part really runs, for the rules to read: without the words that only run another
/// command (`sudo -u pi rm`, `env X=1 rm`, `xargs rm`, `nohup`, `time`, `timeout 10`, `nice`,
/// `command`, `exec`, `busybox`), without the program's folder (`/bin/rm`), with one space between
/// words, and for a shell's `-c` (`sh -c "rm -rf x"`, `pwsh -Command …`, `cmd /c …`) the quoted
/// command itself.
fn bare(part: &str) -> String {
    let mut words: Vec<&str> = part.split_whitespace().collect();
    loop {
        let Some(first) = words.first().copied() else { return String::new() };
        let name = first.rsplit(['/', '\\']).next().unwrap_or(first).to_ascii_lowercase();
        let name = name.strip_suffix(".exe").unwrap_or(&name).to_owned();
        match name.as_str() {
            "sudo" | "doas" | "env" | "xargs" | "nohup" | "time" | "timeout" | "nice" | "ionice" | "command" | "exec" | "busybox"
            | "stdbuf" | "unbuffer" | "caffeinate" => {
                words.remove(0);
                // Their own options (with the value some take), `VAR=x` words, and the seconds `timeout` takes.
                while let Some(w) = words.first().copied() {
                    let option = w.starts_with('-');
                    let setting = w.contains('=') && !w.starts_with(['"', '\'']);
                    let seconds = name == "timeout" && w.chars().all(|c| c.is_ascii_digit() || "smhd.".contains(c));
                    if !(option || setting || seconds) {
                        break;
                    }
                    words.remove(0);
                    if option && takes_value(&name, w) && !words.is_empty() {
                        words.remove(0);
                    }
                }
            }
            "sh" | "bash" | "zsh" | "dash" | "ksh" | "fish" | "pwsh" | "powershell" | "cmd" => {
                // The command is the text after `-c`, `-Command` or `/c`, with one layer of quotes off.
                let lower = part.to_ascii_lowercase();
                let at = [" -c ", " -command ", " -c\"", " -c'", " /c ", " /k "].iter().filter_map(|m| lower.find(m).map(|i| i + m.len() - 1)).min();
                let Some(at) = at else { return words.join(" ") };
                let inner = part[at..].trim();
                let inner = inner.strip_prefix(['"', '\'']).map(|i| i.trim_end_matches(['"', '\''])).unwrap_or(inner);
                return bare(inner);
            }
            _ => {
                let mut out = vec![name.as_str()];
                out.extend(words.iter().skip(1).copied());
                return out.join(" ");
            }
        }
    }
}

/// A glob where `*` is anything, matched whole and ignoring case.
fn text_matches(glob: &str, text: &str) -> bool {
    let body: String = glob.trim().split('*').map(regex::escape).collect::<Vec<_>>().join(".*");
    Regex::new(&format!("(?is)^{body}$")).is_ok_and(|re| re.is_match(text.trim()))
}

/// A path glob, gitignore style: without a slash it matches a name at any depth, with one it
/// matches from the thread's folder (or from the root, when absolute). `**` crosses folders.
fn path_matches(glob: &str, path: &str, cwd: &Path) -> bool {
    let glob = glob.trim().replace('\\', "/");
    if glob.is_empty() {
        return false;
    }
    let path = path.replace('\\', "/");
    let cwd = cwd.to_string_lossy().replace('\\', "/");
    let cwd = cwd.trim_end_matches('/');
    let rel = path
        .strip_prefix(cwd)
        .filter(|r| cwd.is_empty() || r.starts_with('/'))
        .map(|r| r.trim_start_matches('/'))
        .unwrap_or(&path);
    let dir_glob = glob.ends_with('/');
    let glob = glob.trim_end_matches('/');
    let mut body = String::new();
    let mut chars = glob.trim_start_matches("./").chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' if chars.peek() == Some(&'*') => {
                chars.next();
                if chars.peek() == Some(&'/') {
                    chars.next();
                    body.push_str("(?:.*/)?");
                } else {
                    body.push_str(".*");
                }
            }
            '*' => body.push_str("[^/]*"),
            '?' => body.push_str("[^/]"),
            c => body.push_str(&regex::escape(&c.to_string())),
        }
    }
    // A folder protects what is in it.
    let tail = if dir_glob { "/.*" } else { "(?:/.*)?" };
    let anchored = glob.contains('/');
    let re = if anchored {
        format!("(?i)^/?{}{tail}$", body.trim_start_matches('/'))
    } else {
        format!("(?i)(?:^|/){body}{tail}$")
    };
    let Ok(re) = Regex::new(&re) else { return false };
    re.is_match(rel) || re.is_match(&path)
}

/// Hub's tools that start or steer other work, which a routine's run must not do.
const WORK_TOOLS: &[&str] = &["start_thread", "delegate", "send_to_thread", "stop_thread", "start_side_job", "run_recipe"];

/// What a routine's run is refused while it only reports (`routines.rs`): changing files, and
/// anything that commits, pushes, opens or merges a pull request, or starts other work.
pub fn report_only(call: &Value) -> Option<Verdict> {
    let kind = call["kind"].as_str().unwrap_or_default();
    if matches!(kind, "edit" | "delete" | "move") {
        return Some(Verdict::Refuse("a routine only reports; it does not change files".into()));
    }
    if let Some(tool) = names(call).into_iter().find(|n| WORK_TOOLS.iter().any(|t| n.ends_with(t))) {
        return Some(Verdict::Refuse(format!("a routine only reports; {tool} starts other work")));
    }
    writes(call).then(|| Verdict::Refuse("a routine only reports; it does not commit, push, open pull requests or change files".into()))
}

/// What a thread in design mode may do (`design.rs`): it writes its design document under
/// `docs/design/` without asking, and is refused every other change to files and any command
/// that commits, pushes, installs or writes. Everything else goes on as usual.
pub fn design(call: &Value, cwd: &Path) -> Option<Verdict> {
    let kind = call["kind"].as_str().unwrap_or_default();
    if matches!(kind, "edit" | "delete" | "move") {
        let paths = paths(call);
        if !paths.is_empty() && paths.iter().all(|p| crate::design::in_design_folder(p, cwd)) {
            return Some(Verdict::Allow("design mode writes its design document".into()));
        }
        return Some(Verdict::Refuse(
            "design mode is on: only the design document under docs/design/ is written, no code yet; turn design mode off to build".into(),
        ));
    }
    writes(call).then(|| {
        Verdict::Refuse("design mode is on: nothing is committed, installed or written until it is turned off".into())
    })
}

/// Whether a call runs a command that commits, pushes, opens a pull request, installs or writes
/// a file.
fn writes(call: &Value) -> bool {
    static WRITES: LazyLock<Vec<Regex>> = LazyLock::new(|| {
        [
            r"^git\s+(commit|push|merge|rebase|reset|revert|cherry-pick|am|apply|tag|stash|checkout|switch|restore|clean|rm|mv|add|branch\s+-[dDmM])\b",
            r"^gh\s+(pr|issue|release|repo|workflow|run)\s+(create|merge|edit|close|reopen|delete|comment|review|ready|run|rerun|cancel)\b",
            r"^gh\s+api\s.*(-X|--method)\s*(POST|PUT|PATCH|DELETE)",
            r"^(sudo\s+)?(rm|rmdir|mv|cp|del|erase|move|copy|remove-item|move-item|new-item|set-content)\s",
            r"^(npm|pnpm|yarn|bun)\s+(install|i|add|remove|uninstall|update|upgrade|audit\s+fix)\b",
            r"^cargo\s+(add|remove|update|install|fix)\b",
            r"^pip3?\s+(install|uninstall)\b",
            r"(^|\s)(>|>>|tee)\s",
        ]
        .into_iter()
        .map(|re| Regex::new(&format!("(?i){re}")).expect("constant regex"))
        .collect()
    });
    // A question may carry no kind, only a title like "Run `git push`".
    let Some(command) = command(call).or_else(|| call["title"].as_str().and_then(ticked).map(str::to_owned)) else {
        return false;
    };
    parts(&command).any(|p| WRITES.iter().any(|re| re.is_match(p)))
}

/// Commands a routine may run without asking: they only look at the code or check it.
/// Each part of a chain must be one of these.
const LOOKS: &[&str] = &[
    r"cd|pushd|popd|set-location|pwd|echo|printf|true|date|which|where|type|command\s+-v|get-command",
    r"ls|dir|tree|stat|file|du|df|realpath|basename|dirname|get-childitem|gci|get-item|test-path|resolve-path",
    r"cat|head|tail|less|wc|nl|sort|uniq|cut|tr|column|diff|cmp|jq|grep|egrep|fgrep|rg|ag|findstr",
    r"get-content|gc|select-string|sls|select-object|measure-object|format-table|format-list|out-string|where-object|sort-object",
    r"sed\s+-n",
    r"find(?:\s+(?:[^-\s]\S*|-(?:name|iname|path|ipath|type|maxdepth|mindepth|newer|mtime|mmin|size|not|o|and|or|print|print0|regex|empty)\b(?:\s+[^-\s]\S*)?|[()!]))*",
    r"git(?:\s+(?:-C\s+\S+|--no-pager|-c\s+\S+))*\s+(?:log|show|diff|status|branch|fetch|blame|shortlog|describe|rev-parse|rev-list|ls-files|ls-tree|ls-remote|remote|grep|config\s+--get|cat-file|merge-base|reflog|tag\s+(?:-l|--list)|tag\s*$|worktree\s+list|stash\s+list|name-rev|for-each-ref|count-objects|whatchanged|range-diff)\b",
    r"gh\s+(?:pr|issue|run|release|repo|workflow)\s+(?:list|view|diff|checks|status)\b",
    r"cargo\s+(?:check|test|clippy|build|tree|metadata|fmt\s+(?:--all\s+)?--check|fmt\s+--check|doc|nextest|audit|outdated|--version|-V)\b",
    r"(?:npm|pnpm|yarn|bun)\s+(?:run\s+)?(?:check|test|lint|typecheck|build|ls|list|outdated|audit|why|view|--version|-v)\b",
    r"npx\s+(?:--no-install\s+)?(?:tsc|svelte-check|eslint|vitest|prettier\s+--check|playwright\s+test)\b",
    r"node\s+(?:--test|--version|-v)\b",
    r"(?:python3?|py)\s+(?:-m\s+(?:pytest|mypy|ruff\s+check|pyflakes)|--version)\b",
    r"pytest|ruff\s+check|mypy|tsc|eslint|svelte-check|go\s+(?:vet|test|build|list)|rustc\s+--version",
];

/// Whether every part of `line` only looks: a known look-only command, no writing redirection,
/// no command inside a command.
pub fn only_looks(line: &str) -> bool {
    static LOOK: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(&format!("(?i)^(?:{})(?:\\s|$)", LOOKS.join("|"))).expect("constant regex"));
    // Sending errors away is fine; sending output into a file is not.
    static QUIET: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\d?>\s*(?:/dev/null|nul|\$null|&\d)").expect("constant regex"));
    static SED_IN_PLACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\bsed\b.*\s-i").expect("constant regex"));
    let parts: Vec<&str> = parts(line).collect();
    !parts.is_empty()
        && parts.iter().all(|p| {
            let p = QUIET.replace_all(p, "");
            !p.contains('>')
                && !p.contains("$(")
                && !p.contains('`')
                && !p.contains("-exec")
                && !SED_IN_PLACE.is_match(&p)
                && LOOK.is_match(p.trim())
        })
}

/// Hub's answer for a routine's run, which never waits for the user: what [`report_only`]
/// refuses is refused, reading and look-only commands are allowed, the user's own rules come
/// next, and anything left is refused with a note to put it in the report instead of asking.
pub fn routine(prefs: &PermissionPrefs, call: &Value, cwd: &Path) -> Verdict {
    if let Some(refused) = hub_files(call).or_else(|| report_only(call)) {
        return refused;
    }
    let kind = call["kind"].as_str().unwrap_or_default();
    if matches!(kind, "read" | "search" | "fetch" | "think") {
        return Verdict::Allow("a routine may read".into());
    }
    if let Some(c) = command(call)
        && only_looks(&c)
    {
        return Verdict::Allow("a routine may run commands that only look".into());
    }
    if let Some(verdict) = decide(prefs, call, cwd) {
        return verdict;
    }
    Verdict::Refuse(
        "a routine runs unattended and does not wait for an answer; say in the report what you wanted to run and why"
            .into(),
    )
}

/// Why a command line would destroy work, if it would. The same rules as the app's red
/// permission cards (`app/src/lib/risk.ts`).
pub fn risky(line: &str) -> Option<&'static str> {
    static RULES: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
        [
            (r"^(sudo\s+)?rm\s", "Deletes files"),
            (r"^rmdir\s", "Deletes folders"),
            (r"(?i)^(del|erase|rd|rmdir)\s.*/s", "Deletes files"),
            (r"(?i)^remove-item\s", "Deletes files"),
            (r"^find\s.*\s-delete\b", "Deletes every file it finds"),
            (r"^git\s+reset\s+.*--hard", "Throws away uncommitted changes"),
            (r"^git\s+clean\s+.*-\w*f", "Deletes untracked files"),
            (r"^git\s+(checkout|restore)\s+(--\s+)?\.(\s|$)", "Throws away uncommitted changes"),
            (r"^git\s+push\s+.*(--force\b|-f\b|--force-with-lease)", "Rewrites history on the remote"),
            (r"^git\s+branch\s+.*-D\b", "Deletes a branch that may not be merged"),
            (r"^git\s+stash\s+(drop|clear)", "Deletes stashed work"),
            (r"^rsync\s.*--delete", "Deletes files missing from the source"),
            (r"(?i)^(mkfs|format|diskpart|dd\s.*of=)", "Erases a disk"),
            (r"(?i)\b(drop\s+(table|database|schema)|truncate\s+table)\b", "Deletes data from a database"),
        ]
        .into_iter()
        .map(|(re, label)| (Regex::new(re).expect("constant regex"), label))
        .collect()
    });
    every_part(line).find_map(|p| {
        let p = bare(p);
        RULES.iter().find(|(re, _)| re.is_match(&p)).map(|(_, label)| *label)
    })
}

/// The option to send for a verdict: a one-time allow or refusal, never an "always" that the
/// engine would remember on its own.
pub fn option_for(verdict: &Verdict, options: &Value) -> Option<String> {
    let wanted: &[&str] = match verdict {
        Verdict::Allow(_) => &["allow_once", "allow_always"],
        Verdict::Refuse(_) => &["reject_once", "reject_always"],
    };
    let options = options.as_array()?;
    wanted.iter().find_map(|k| {
        options.iter().find(|o| o["kind"] == *k).and_then(|o| o["optionId"].as_str().map(str::to_owned))
    })
}

#[cfg(test)]
mod tests;

//! The parts of a project the phone's project page shows: Git, Docs, Backlog and Board. Git and
//! Docs are read only views of the repo. The backlog and the board are two markdown files in the
//! repo (`docs/BACKLOG.md`, `docs/BOARD.md`), so they travel with it and no outside service holds
//! them. The phone asks for them through the `phone.project*` features.
//!
//! - `BACKLOG.md`: one `- Title #tag` per idea.
//! - `BOARD.md`: `## To do`, `## Doing`, `## Done`, with `- Title` cards that may end in `#724`
//!   (a pull request) or `thread:<id>`.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::git;
use crate::model::{BacklogItem, BoardCard, BoardColumn, CommitLine, DocRow, DocsCard, GitCard, PartEdit};
use crate::proc::gh_within;

pub const BACKLOG: &str = "docs/BACKLOG.md";
pub const BOARD: &str = "docs/BOARD.md";
const COLUMNS: [&str; 3] = ["To do", "Doing", "Done"];
const RECENT: usize = 8;
const BRANCHES: usize = 8;

// ---- Git ----

/// The repository at a glance. The GitHub parts are empty, with `message` saying why, when `gh`
/// cannot answer.
pub async fn git_card(root: &Path) -> Result<GitCard> {
    let root_owned = root.to_path_buf();
    let (default, head, branch_list, current) = tokio::task::spawn_blocking(move || {
        let default = crate::github::default_branch(&root_owned);
        let head = head_of(&root_owned, &default);
        let list = crate::github::branches(&root_owned).ok();
        (default, head, list, git::current_branch(&root_owned))
    })
    .await?;
    let pulls = crate::github::pulls(root, "open").await;
    let ci = ci_of(root, &default).await;
    let mut branches = branch_list.map(|l| l.branches).unwrap_or_default();
    branches.retain(|b| b.name != default);
    branches.truncate(BRANCHES);
    Ok(GitCard {
        branch: if default.is_empty() { current.unwrap_or_default() } else { default },
        head,
        ci,
        message: pulls.message,
        pulls: pulls.pulls,
        branches,
    })
}

fn head_of(root: &Path, branch: &str) -> Option<CommitLine> {
    let out = git::run(root, &["log", "-1", "--format=%h%x09%ct%x09%s", branch])?;
    parse_head(&out)
}

fn parse_head(out: &str) -> Option<CommitLine> {
    let mut parts = out.trim_end().splitn(3, '\t');
    let (sha, at, subject) = (parts.next()?, parts.next()?, parts.next()?);
    Some(CommitLine { sha: sha.to_owned(), subject: subject.to_owned(), at: at.parse::<i64>().ok()? * 1000 })
}

/// CI of the newest run on the default branch.
async fn ci_of(root: &Path, branch: &str) -> String {
    let args = ["run", "list", "--branch", branch, "--limit", "1", "--json", "status,conclusion"];
    let Ok(out) = gh_within(root, &args, Duration::from_secs(15)).await else { return "none".into() };
    ci_state(&out)
}

fn ci_state(json: &str) -> String {
    let runs: Vec<serde_json::Value> = serde_json::from_str(json).unwrap_or_default();
    let Some(run) = runs.first() else { return "none".into() };
    if run["status"].as_str().is_some_and(|s| s != "completed") {
        return "pending".into();
    }
    match run["conclusion"].as_str() {
        Some("success") => "pass",
        Some("failure" | "timed_out" | "startup_failure") => "fail",
        _ => "none",
    }
    .into()
}

// ---- Docs ----

const PINNED: [(&str, &str); 3] = [("docs/FOCUS.md", "focus"), ("docs/ARCHITECTURE.md", "architecture"), ("design/DESIGN.md", "design book")];

/// The repo's markdown docs: the pinned ones, then the ones changed last.
pub fn docs_card(root: &Path) -> DocsCard {
    let mut all: Vec<DocRow> = Vec::new();
    for dir in ["docs", "design"] {
        let Ok(entries) = std::fs::read_dir(root.join(dir)) else { continue };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.to_ascii_lowercase().ends_with(".md") && e.path().is_file() {
                all.push(doc_row(root, &format!("{dir}/{name}")));
            }
        }
    }
    if root.join("README.md").is_file() {
        all.push(doc_row(root, "README.md"));
    }
    let count = all.len();
    let mut pinned = Vec::new();
    for (rel, what) in PINNED {
        if let Some(i) = all.iter().position(|d| d.rel == rel) {
            let mut d = all.remove(i);
            d.sub = if rel.ends_with("FOCUS.md") {
                crate::focus::summary(root).map_or_else(
                    || what.to_owned(),
                    |s| format!("Stage {} · {} to test", s.n, s.to_test),
                )
            } else {
                what.to_owned()
            };
            pinned.push(d);
        }
    }
    all.sort_by(|a, b| b.at.cmp(&a.at).then(a.rel.cmp(&b.rel)));
    all.truncate(RECENT);
    DocsCard { pinned, recent: all, count }
}

fn doc_row(root: &Path, rel: &str) -> DocRow {
    let path = root.join(rel);
    let title = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| t.lines().find_map(|l| l.strip_prefix("# ").map(|h| h.trim().to_owned())))
        .unwrap_or_else(|| Path::new(rel).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
    // When git knows the file, its last commit; otherwise the file's own time.
    let at = git::run(root, &["log", "-1", "--format=%ct", "--", rel])
        .and_then(|s| s.trim().parse::<i64>().ok())
        .map(|s| s * 1000)
        .or_else(|| {
            let m = std::fs::metadata(&path).ok()?.modified().ok()?;
            Some(m.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as i64)
        })
        .unwrap_or(0);
    let sub = Path::new(rel).parent().map(|p| p.to_string_lossy().into_owned()).filter(|p| !p.is_empty()).unwrap_or_else(|| "root".into());
    DocRow { rel: rel.to_owned(), title, sub, at }
}

// ---- Backlog ----

pub fn backlog(root: &Path) -> Vec<BacklogItem> {
    let text = std::fs::read_to_string(root.join(BACKLOG)).unwrap_or_default();
    parse_backlog(&text)
}

fn parse_backlog(text: &str) -> Vec<BacklogItem> {
    text.lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let body = l.trim_start().strip_prefix("- ")?;
            let (title, tag) = split_tag(body);
            (!title.is_empty()).then(|| BacklogItem { line: i as u32 + 1, title, tag })
        })
        .collect()
}

/// `Title #tag` into the title and the tag; a `#724` is a pull request, not a tag.
fn split_tag(body: &str) -> (String, String) {
    let body = body.trim();
    if let Some((head, last)) = body.rsplit_once(" #")
        && !last.is_empty() && !last.contains(' ') && last.parse::<u64>().is_err() {
            return (head.trim().to_owned(), last.to_owned());
        }
    (body.to_owned(), String::new())
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn edit_backlog(root: &Path, e: &PartEdit) -> Result<Vec<BacklogItem>> {
    let file = root.join(BACKLOG);
    let mut lines = read_lines(&file, "# Backlog");
    match e.op.as_str() {
        "add" => {
            let title = one_line(e.title.as_deref().unwrap_or_default());
            if title.is_empty() {
                bail!("write the idea first");
            }
            let tag = one_line(e.tag.as_deref().unwrap_or_default()).replace(' ', "-");
            lines.push(if tag.is_empty() { format!("- {title}") } else { format!("- {title} #{tag}") });
        }
        "drop" | "board" => {
            let i = find_line(&lines, e)?;
            let (title, _) = split_tag(lines[i].trim_start().trim_start_matches("- "));
            lines.remove(i);
            if e.op == "board" {
                write_lines(&file, &lines)?;
                return {
                    add_card(root, "To do", &title)?;
                    Ok(backlog(root))
                };
            }
        }
        other => bail!("unknown change {other}"),
    }
    write_lines(&file, &lines)?;
    Ok(backlog(root))
}

// ---- Board ----

pub fn board(root: &Path) -> Vec<BoardColumn> {
    parse_board(&std::fs::read_to_string(root.join(BOARD)).unwrap_or_default())
}

fn parse_board(text: &str) -> Vec<BoardColumn> {
    let mut cols: Vec<BoardColumn> = COLUMNS.iter().map(|n| BoardColumn { name: (*n).into(), cards: Vec::new() }).collect();
    let mut at: Option<usize> = None;
    for (i, l) in text.lines().enumerate() {
        if let Some(h) = l.strip_prefix("## ") {
            at = cols.iter().position(|c| c.name.eq_ignore_ascii_case(h.trim()));
        } else if let (Some(c), Some(body)) = (at, l.trim_start().strip_prefix("- ")) {
            let (mut pr, mut thread, mut words) = (None, String::new(), Vec::new());
            for w in body.split_whitespace() {
                match (w.strip_prefix('#').and_then(|n| n.parse::<u64>().ok()), w.strip_prefix("thread:")) {
                    (Some(n), _) => pr = Some(n),
                    (_, Some(t)) if !t.is_empty() => thread = t.to_owned(),
                    _ => words.push(w),
                }
            }
            if !words.is_empty() {
                cols[c].cards.push(BoardCard { line: i as u32 + 1, title: words.join(" "), thread, pr });
            }
        }
    }
    cols
}

fn column(name: &str) -> Result<&'static str> {
    COLUMNS.iter().copied().find(|c| c.eq_ignore_ascii_case(name.trim())).with_context(|| format!("no column {name}"))
}

/// Adds a card at the end of a column, making the file or the heading when missing.
fn add_card(root: &Path, to: &str, title: &str) -> Result<()> {
    let to = column(to)?;
    let file = root.join(BOARD);
    let mut lines = read_lines(&file, "# Board");
    let heading = format!("## {to}");
    let at = match lines.iter().position(|l| l.trim().eq_ignore_ascii_case(&heading)) {
        Some(i) => i,
        None => {
            // Keep the columns in order: a missing one goes before the next one that exists.
            let next = COLUMNS[COLUMNS.iter().position(|c| *c == to).unwrap_or(0) + 1..]
                .iter()
                .find_map(|n| lines.iter().position(|l| l.trim().eq_ignore_ascii_case(&format!("## {n}"))));
            let pos = next.unwrap_or(lines.len());
            lines.splice(pos..pos, [String::new(), heading.clone(), String::new()]);
            pos + 1
        }
    };
    let mut end = at + 1;
    while end < lines.len() && !lines[end].starts_with("## ") {
        end += 1;
    }
    while end > at + 1 && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    lines.insert(end, format!("- {title}"));
    write_lines(&file, &lines)
}

pub fn edit_board(root: &Path, e: &PartEdit) -> Result<Vec<BoardColumn>> {
    match e.op.as_str() {
        "add" => {
            let title = one_line(e.title.as_deref().unwrap_or_default());
            if title.is_empty() {
                bail!("write the card first");
            }
            add_card(root, e.to.as_deref().unwrap_or("To do"), &title)?;
        }
        "move" | "drop" => {
            let file = root.join(BOARD);
            let mut lines = read_lines(&file, "# Board");
            let i = find_line(&lines, e)?;
            let card = lines.remove(i);
            write_lines(&file, &lines)?;
            if e.op == "move" {
                let to = column(e.to.as_deref().context("move to which column?")?)?;
                // The card keeps its pull request and thread; only its column changes.
                add_card(root, to, card.trim_start().trim_start_matches("- "))?;
            }
        }
        other => bail!("unknown change {other}"),
    }
    Ok(board(root))
}

// ---- Files ----

/// The lines of a file, or a fresh one with `heading` when it is not there yet.
fn read_lines(file: &Path, heading: &str) -> Vec<String> {
    match std::fs::read_to_string(file) {
        Ok(t) => t.lines().map(str::to_owned).collect(),
        Err(_) => vec![heading.to_owned(), String::new()],
    }
}

fn write_lines(file: &Path, lines: &[String]) -> Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(file, lines.join("\n") + "\n").with_context(|| format!("writing {}", file.display()))
}

/// The index of the line a change names. The line number is a hint the title must confirm, so a
/// file edited meanwhile gives "changed, reload" instead of touching another item.
fn find_line(lines: &[String], e: &PartEdit) -> Result<usize> {
    let line = e.line.context("which item?")? as usize;
    let title = one_line(e.title.as_deref().unwrap_or_default());
    let has = |i: usize| lines.get(i).is_some_and(|l| title.is_empty() || l.contains(&title));
    if line >= 1 && has(line - 1) && lines[line - 1].trim_start().starts_with("- ") {
        return Ok(line - 1);
    }
    bail!("the file changed, reload and try again")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_backlog_line_gives_a_title_and_a_tag_and_a_pr_number_is_no_tag() {
        let items = parse_backlog("# Backlog\n\n- Usage forecast #usage\n- Fix thing #724\n- Plain\n");
        assert_eq!((items[0].title.as_str(), items[0].tag.as_str(), items[0].line), ("Usage forecast", "usage", 3));
        assert_eq!((items[1].title.as_str(), items[1].tag.as_str()), ("Fix thing #724", ""));
        assert_eq!(items[2].tag, "");
    }

    #[test]
    fn a_board_reads_its_columns_and_what_a_card_names() {
        let cols = parse_board("# Board\n\n## To do\n\n- A\n\n## Doing\n\n- B #724 thread:cmsg_1\n");
        assert_eq!(cols[0].cards[0].title, "A");
        let b = &cols[1].cards[0];
        assert_eq!((b.title.as_str(), b.pr, b.thread.as_str()), ("B", Some(724), "cmsg_1"));
        assert!(cols[2].cards.is_empty());
    }

    #[test]
    fn ci_is_read_from_the_newest_run() {
        assert_eq!(ci_state(r#"[{"status":"completed","conclusion":"success"}]"#), "pass");
        assert_eq!(ci_state(r#"[{"status":"in_progress","conclusion":null}]"#), "pending");
        assert_eq!(ci_state(r#"[{"status":"completed","conclusion":"failure"}]"#), "fail");
        assert_eq!(ci_state("[]"), "none");
    }

    #[test]
    fn the_head_line_is_read() {
        let h = parse_head("cee7ad3\t1700000000\tMerge #719 relay keys\n").unwrap();
        assert_eq!((h.sha.as_str(), h.at, h.subject.as_str()), ("cee7ad3", 1_700_000_000_000, "Merge #719 relay keys"));
    }
}

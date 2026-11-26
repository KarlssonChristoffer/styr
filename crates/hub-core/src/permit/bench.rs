//! Hub's answer for a bench thread (`bench/run.rs`): stricter than `unattended`. A bench case runs
//! with nobody watching, in a scratch folder Hub made for it, so anything that reaches outside that
//! folder is refused before `unattended` is asked: a file the call names outside it, a command
//! word that is a path outside it (also after an `=` or a redirection inside the word:
//! `--out=/x`, `of=../x`, `x>/y`), and any `git push`. What is left is `unattended`'s answer
//! (Hub's own files, the user's rules, no deleting, no command that destroys work).
//!
//! The command check reads the words of a command line, so it is conservative about words the
//! shell would turn into a place: any word with `$` before a letter, `_`, `{` or `(` (`$HOME`,
//! `${HOME}`, `$env:USERPROFILE`, `$(pwd)`), a backtick, a `%name%` or a `~` anywhere is refused,
//! inside the folder or not. Only the forms that cannot name a place pass (`$?`, `$$`, `$!`, `$#`,
//! `$0`-`$9`, `$@`, `$*`). The price is a false refusal now and then; it shows in the report as a
//! refused permission (`perm_refused`) and at worst fails a repeat of a case with
//! `refused_max = 0`, which is the right way round for a guard nobody watches.
//!
//! Not fenced: what a program does on its own once it runs (a script with a path written inside
//! it), and the network.

use std::path::Path;

use serde_json::Value;

use super::{Verdict, command, every_part, paths, unattended};
use crate::model::PermissionPrefs;

/// The answer for a bench thread whose run folder is `folder` (the thread's own cwd).
pub fn bench(prefs: &PermissionPrefs, call: &Value, folder: &Path) -> Verdict {
    if let Some(why) = outside(call, folder) {
        return Verdict::Refuse(format!("a bench case stays inside its own folder: {why}"));
    }
    unattended(prefs, call, folder)
}

/// What in `call` reaches outside `folder`, if anything.
fn outside(call: &Value, folder: &Path) -> Option<String> {
    let home = plain(&folder.to_string_lossy());
    if let Some(path) = paths(call).into_iter().find(|p| !inside(&home, p)) {
        return Some(format!("{path} is outside {}", folder.display()));
    }
    let line = command(call)?;
    // Before the line is split: the split takes `$(` and backticks apart.
    if let Some(w) = line.split_whitespace().find(|w| expands(w)) {
        return Some(format!("{w} is expanded by the shell and could name any place"));
    }
    for part in every_part(&line) {
        let runs = super::bare(part);
        if runs.starts_with("git ") && runs.split_whitespace().any(|w| w == "push") {
            return Some("it never pushes".into());
        }
        let words: Vec<String> = part.split_whitespace().flat_map(places).collect();
        if let Some(w) = words.iter().find(|w| looks_like_path(w) && !inside(&home, w)) {
            return Some(format!("{w} is outside {}", folder.display()));
        }
    }
    None
}

/// A word the shell turns into something else that could be a place: a variable or command
/// substitution (`$x`, `${x}`, `$(x)`, `$env:x`, backticks), a Windows `%x%`, or a `~`.
fn expands(w: &str) -> bool {
    static PERCENT: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"%[A-Za-z_][A-Za-z0-9_]*%").expect("constant regex"));
    let dollar = w.char_indices().any(|(i, c)| c == '$' && w[i + 1..].chars().next().is_some_and(|n| n.is_alphabetic() || matches!(n, '_' | '{' | '(')));
    dollar || w.contains('`') || w.contains('~') || PERCENT.is_match(w)
}

/// The places a command word can name: the word itself, and what follows an `=` or a redirection
/// inside it (`--out=/x`, `of=../x`, `x>/y`), each without its quotes.
fn places(w: &str) -> impl Iterator<Item = String> {
    std::iter::once(w).chain(w.split(['=', '>', '<'])).map(word).filter(|p| !p.is_empty())
}

/// A command word without its quotes and a leading redirection (`>out`, `2>>log`).
fn word(w: &str) -> String {
    let w = w.trim_start_matches(|c: char| c.is_ascii_digit() || c == '>' || c == '<' || c == '&');
    w.trim_matches(['"', '\'']).to_owned()
}

// The rest reads paths as text, the same on every system: `/` and `\` both separate parts, a
// drive letter makes a path absolute, and `..` is a part. So `..\x` or `C:\x` in a command is a
// place on Linux too (a bench case has no business there either); only whether such a path is
// inside the folder depends on the folder, which on Linux never starts with a drive.

/// A word that names a place on disk rather than an option, a URL or a plain name.
fn looks_like_path(w: &str) -> bool {
    if w.contains("://") || w.starts_with('-') || ["/dev/null", "nul", "NUL"].contains(&w) {
        return false;
    }
    w.starts_with(['/', '\\', '~']) || drive(w) || w.split(['/', '\\']).any(|part| part == "..")
}

/// `C:` or `c:\...` at the start.
fn drive(w: &str) -> bool {
    let b = w.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

fn is_absolute(path: &str) -> bool {
    path.starts_with(['/', '\\']) || drive(path)
}

/// Whether `path` (absolute, or relative to the folder) stays inside the folder `home` (as
/// [`plain`] writes it).
fn inside(home: &str, path: &str) -> bool {
    if path.starts_with('~') {
        return false;
    }
    let joined = if is_absolute(path) { git_bash(path) } else { format!("{home}/{path}") };
    let p = plain(&joined);
    p == home || p.starts_with(&format!("{home}/"))
}

/// Git Bash's `/c/Users/...` as `c:/Users/...`, so a Windows shell path compares with the folder.
/// Only on Windows: on Linux `/c/...` is a real folder of its own.
fn git_bash(path: &str) -> String {
    let b = path.as_bytes();
    if cfg!(windows) && b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b'/' {
        return format!("{}:{}", b[1] as char, &path[2..]);
    }
    path.to_owned()
}

/// A path written one way: `/` between parts (for `\` too), `.` and `..` worked out, no `\\?\`,
/// and in lower case on Windows only (Linux paths are case-sensitive).
fn plain(path: &str) -> String {
    let text = path.replace('\\', "/");
    let text = text.strip_prefix("//?/").unwrap_or(&text);
    let mut parts: Vec<&str> = Vec::new();
    for part in text.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    let root = if text.starts_with('/') { "/" } else { "" };
    let joined = format!("{root}{}", parts.join("/"));
    if cfg!(windows) { joined.to_lowercase() } else { joined }
}

/// The helpers on literal strings in both styles: these run the same on every system, so a green
/// run on one says the logic holds on the other; only the real folder's shape differs.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_read_the_same_on_every_system() {
        assert_eq!(plain("/srv/bench/case-1/./out/../x.txt"), "/srv/bench/case-1/x.txt");
        assert_eq!(plain("\\\\?\\D:\\Bench\\case-1"), if cfg!(windows) { "d:/bench/case-1" } else { "D:/Bench/case-1" });
        assert_eq!(plain("a\\..\\..\\b"), "b");
        for w in ["/etc/passwd", "\\\\server\\share", "~/x", "C:\\Users\\x", "c:/x", "..\\..\\x", "../x", "a/../../x", "out\\..\\..\\x"] {
            assert!(looks_like_path(w), "{w}");
        }
        for w in ["npm", "test", "out.txt", "output/orders.csv", "--test", "-m", "https://example.com/x", "/dev/null", "NUL", "", "a..b"] {
            assert!(!looks_like_path(w), "{w}");
        }
    }

    #[test]
    fn inside_is_judged_on_the_text() {
        let home = plain("/srv/bench/case-1");
        assert!(inside(&home, "out.txt") && inside(&home, "out/x.svg") && inside(&home, "/srv/bench/case-1") && inside(&home, "/srv/bench/case-1/a/../b"));
        for p in ["../x", "a/../../x", "..\\x", "a\\..\\..\\x", "/srv/bench/case-10/x", "/srv/x", "~/x", "C:\\x", "c:/srv/bench/case-1/x"] {
            assert!(!inside(&home, p), "{p}");
        }
        let win = plain("C:\\Bench\\case-1");
        assert!(inside(&win, "out.txt") && inside(&win, "C:\\Bench\\case-1\\out.txt") && inside(&win, "C:/Bench/case-1/sub/x"));
        assert!(!inside(&win, "C:\\Bench\\x") && !inside(&win, "..\\x") && !inside(&win, "/c/elsewhere"));
        // Case and Git Bash's drive form count only on Windows.
        assert_eq!(inside(&win, "c:\\bench\\CASE-1\\x"), cfg!(windows));
        assert_eq!(inside(&win, "/c/Bench/case-1/x"), cfg!(windows));
    }

    #[test]
    fn expansions_and_words() {
        for w in ["$HOME/x", "${HOME}", "$(pwd)/x", "$env:USERPROFILE\\x", "`pwd`", "%TEMP%\\x", "backup~", "a~b", "$_x"] {
            assert!(expands(w), "{w}");
        }
        for w in ["$?", "$$", "$!", "$#", "$0", "$9", "$@", "$*", "2>&1", "100%", "%", "x.txt", "$"] {
            assert!(!expands(w), "{w}");
        }
        assert_eq!(word(">out.txt"), "out.txt");
        assert_eq!(word("2>>log"), "log");
        assert_eq!(word("\"C:\\x y\""), "C:\\x y");
        assert_eq!(word("2>&1"), "");
        let of = |w: &str| places(w).collect::<Vec<_>>();
        assert_eq!(of("--out=/etc/x"), ["--out=/etc/x", "--out", "/etc/x"]);
        assert_eq!(of("of=\"../x\""), ["of=\"../x", "of", "../x"]);
        assert_eq!(of("x>/etc/y"), ["x>/etc/y", "x", "/etc/y"]);
        assert_eq!(of("2>>log"), ["log", "log"]);
        assert_eq!(of("npm"), ["npm", "npm"]);
    }
}

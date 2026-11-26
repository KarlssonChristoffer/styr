//! Guards the update manager: CLI versions read from what each prints and compared by number,
//! each CLI updated by what owns it, npm errors read from its output, release notes cut per
//! version, and machines with an older CLI following this one.

use super::*;

fn harness(id: &str) -> Harness {
    harnesses().into_iter().find(|h| h.id == id).unwrap()
}

#[test]
fn versions_are_read_from_what_the_cli_prints() {
    assert_eq!(parse_version("2.1.3 (Claude Code)").as_deref(), Some("2.1.3"));
    assert_eq!(parse_version("codex-cli 0.58.0\n").as_deref(), Some("0.58.0"));
    assert_eq!(parse_version("Hermes Agent v0.22.5").as_deref(), Some("0.22.5"));
    assert_eq!(parse_version("nothing here"), None);
}

#[test]
fn versions_compare_by_their_numbers() {
    use std::cmp::Ordering::*;
    assert_eq!(compare("0.10.2", "0.9.7"), Greater);
    assert_eq!(compare("2.1", "2.1.0"), Equal);
    assert_eq!(compare("1.2.0-beta", "1.2.0"), Less);
    assert_eq!(compare("1.0.0", "1.0.1"), Less);
}

#[test]
fn each_cli_is_updated_by_what_owns_it() {
    let npm_claude = Path::new(r"C:\Users\me\AppData\Roaming\npm\claude.cmd");
    assert_eq!(update_command(&harness("claude"), npm_claude, true).as_deref(), Some("npm install -g @anthropic-ai/claude-code@latest"));
    let native = Path::new("/home/me/.local/bin/claude");
    assert_eq!(update_command(&harness("claude"), native, false).as_deref(), Some("'/home/me/.local/bin/claude' update"));
    let hermes = Path::new(r"C:\Users\me\AppData\Local\hermes\bin\hermes.exe");
    assert_eq!(update_command(&harness("hermes"), hermes, true).as_deref(), Some(r"& 'C:\Users\me\AppData\Local\hermes\bin\hermes.exe' update"));
    let brew = Path::new("/opt/homebrew/bin/codex");
    assert_eq!(update_command(&harness("codex"), brew, false).as_deref(), Some("brew upgrade codex"));
    let anywhere = Path::new("/usr/bin/codex");
    assert_eq!(update_command(&harness("codex"), anywhere, false).as_deref(), Some("npm install -g @openai/codex@latest"));
}

fn lines(text: &str) -> Vec<String> {
    text.lines().map(str::to_owned).collect()
}

#[test]
fn npm_errors_are_read_from_its_output() {
    let global = lines(
        "npm ERR! code EACCES\nnpm ERR! syscall mkdir\nnpm ERR! path /usr/lib/node_modules/@openai\nnpm ERR! errno -13\n\
         npm ERR! Error: EACCES: permission denied, mkdir '/usr/lib/node_modules/@openai'\n\
         npm ERR! A complete log of this run can be found in:\nnpm ERR!     /home/c/.npm/_logs/x-debug-0.log",
    );
    let f = npm_failure(&global).unwrap();
    assert_eq!(f.code, "EACCES");
    assert_eq!(f.path.as_deref(), Some("/usr/lib/node_modules/@openai"));
    assert_eq!(f.summary.as_deref(), Some("EACCES: permission denied, mkdir '/usr/lib/node_modules/@openai'"));
    let cmd = "npm install -g @openai/codex@latest";
    assert!(f.explain(cmd).contains("sudo npm install -g @openai/codex@latest"));
    assert_eq!(retry_command(cmd, &f), Some(Retry::Sudo(format!("sudo -n {cmd}"))));

    let cache = lines("npm error code EACCES\nnpm error syscall open\nnpm error path /home/c/.npm/_cacache/index-v5/aa/bb\nnpm error errno -13");
    let f = npm_failure(&cache).unwrap();
    assert!(f.explain(cmd).contains("~/.npm"));
    assert!(matches!(retry_command(cmd, &f), Some(Retry::Run(c)) if c.starts_with(cmd) && c.contains("--cache")));

    let gone = lines("npm ERR! code E404\nnpm ERR! 404 Not Found - GET https://registry.npmjs.org/nope");
    let f = npm_failure(&gone).unwrap();
    assert_eq!(retry_command(cmd, &f), None);
    assert_eq!(f.explain(cmd), "npm stopped with E404");
    assert_eq!(npm_failure(&lines("bash: npm: command not found")), None);
}

#[test]
fn release_notes_are_cut_per_version() {
    let md = "# Changelog\n\n## 2.1.5\n\n- Faster resume\n- Fixed a hang\n\n## 2.1.4\n\n- Older thing\n\n## 2.1.3\n- Oldest\n";
    let all = markdown_releases(md);
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].text, "- Faster resume\n- Fixed a hang");
    let picked = between(all, Some("2.1.3"), Some("2.1.5"));
    assert_eq!(picked.iter().map(|r| r.version.as_str()).collect::<Vec<_>>(), ["2.1.5", "2.1.4"]);

    let gh = serde_json::json!([
        { "tag_name": "rust-v0.53.0-alpha.1", "prerelease": true, "body": "x" },
        { "tag_name": "rust-v0.52.0", "body": "New sandbox flag" },
        { "tag_name": "rust-v0.51.0", "draft": true, "body": "y" },
    ]);
    let all = github_releases(&gh);
    assert_eq!(all.len(), 1);
    assert_eq!((all[0].version.as_str(), all[0].text.as_str()), ("0.52.0", "New sandbox flag"));
    assert_eq!(notes_page("md:https://raw.githubusercontent.com/anthropics/claude-code/main/CHANGELOG.md"), "https://github.com/anthropics/claude-code/blob/main/CHANGELOG.md");
    assert_eq!(notes_page("github:openai/codex"), "https://github.com/openai/codex/releases");
}

fn tool(id: &str, version: &str) -> Tool {
    Tool { id: id.into(), label: id.into(), installed: true, version: Some(version.into()), update: Some("x".into()), ..Default::default() }
}

fn machine(name: &str, local: bool, online: bool, tools: Vec<Tool>) -> MachineUpdates {
    MachineUpdates { machine: name.into(), local, online, hubd: None, hubd_state: None, hubd_note: None, hubd_track: None, tools, error: None }
}

#[test]
fn machines_with_an_older_cli_follow_this_one() {
    let fleet = Fleet {
        hubd: None,
        follow: true,
        machines: vec![
            machine("pc", true, true, vec![tool("claude", "2.1.5"), tool("codex", "0.58.0")]),
            machine("tincan", false, true, vec![tool("claude", "2.1.3"), tool("codex", "0.60.0")]),
            machine("clanker", false, false, vec![tool("claude", "2.0.0")]),
        ],
        at: 0,
    };
    // tincan's newer Codex stays; clanker is offline.
    assert_eq!(behind(&fleet), vec![("tincan".to_owned(), "claude".to_owned(), "2.1.5".to_owned())]);
}

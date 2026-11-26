//! A machine's page opens things on that machine: a terminal window on this computer, logged in
//! there over SSH, running a shell, a root shell or an engine's own CLI. Nothing is kept running
//! by Hub; closing the window ends it.

use anyhow::{Result, bail};

use crate::model::{MachineOpen, TerminalOpened};

/// The engine's own CLI: the first word of its `terminal` command (`claude --resume {session}`
/// gives `claude`), so no engine is named here.
pub fn engine_cli(terminal: &str) -> Option<&str> {
    terminal.split_whitespace().next().filter(|w| !w.contains('{'))
}

/// What runs in the window. `alias` is the machine's SSH alias; `None` is this computer.
/// `cli` is the engine's CLI for `Engine` and the engine's name for `SignIn`.
///
/// No `;` anywhere: Windows Terminal reads it as the start of another tab.
///
/// An engine's CLI runs through `hubd cli <engine>` there (`signin::cli`), which hands it the
/// sign-in Hub keeps, so the CLI in a window is signed in the way the machine's threads are.
/// `engine` names it; a machine whose hubd is older than that subcommand runs the CLI bare.
pub fn command(alias: Option<&str>, what: MachineOpen, cli: Option<&str>) -> Result<Vec<String>> {
    command_for(alias, what, cli, None)
}

pub fn command_for(alias: Option<&str>, what: MachineOpen, cli: Option<&str>, engine: Option<&str>) -> Result<Vec<String>> {
    let engine = engine.filter(|e| is_word(e));
    let cli = match what {
        MachineOpen::Engine => Some(cli.filter(|c| is_word(c)).ok_or_else(|| anyhow::anyhow!("that engine has no CLI to open"))?),
        MachineOpen::SignIn => Some(cli.filter(|c| is_word(c)).ok_or_else(|| anyhow::anyhow!("which engine to sign in?"))?),
        _ => None,
    };
    let Some(alias) = alias else {
        return Ok(match (what, cli) {
            (MachineOpen::Shell, _) => Vec::new(),
            (MachineOpen::Root, _) if cfg!(windows) => bail!("Hub does not open an administrator window on Windows"),
            // Through hubd, so root gets Hub's sign-in (`signin::root`).
            (MachineOpen::Root, _) => vec![std::env::current_exe()?.display().to_string(), crate::hubd_args::ROOT_ARG.into()],
            (MachineOpen::Engine, Some(cli)) => match engine {
                Some(engine) => vec![std::env::current_exe()?.display().to_string(), crate::hubd_args::CLI_ARG.into(), engine.to_owned()],
                None => vec![cli.to_owned()],
            },
            (MachineOpen::SignIn, Some(engine)) => {
                let hubd = std::env::current_exe()?.display().to_string();
                vec![hubd, crate::hubd_args::SIGN_IN_ARG.into(), engine.to_owned()]
            }
            (MachineOpen::Engine | MachineOpen::SignIn, None) => unreachable!(),
            (MachineOpen::Attach, _) => bail!("which thread to take over?"),
        });
    };
    if alias.starts_with('-') || alias.chars().any(char::is_whitespace) {
        bail!("{alias} is not an SSH alias");
    }
    let ssh = |rest: &[&str]| std::iter::once("ssh").chain(rest.iter().copied()).map(str::to_owned).collect();
    Ok(match (what, cli) {
        (MachineOpen::Shell, _) => ssh(&[alias]),
        // Root is another user with its own home, so Claude Code's login for the login user was
        // not there ("log in again"), and its `claude` and `codex` (under the login user's home:
        // `~/.local/bin`, Hub's `tools` and `node`) were "command not found". Through the hubd the
        // service runs there (`signin::root`) root gets Hub's sign-in and the login user's folders
        // on its PATH. A machine whose hubd is older than that subcommand (the marker is in its
        // binary) gets the plain `sudo -i`, with the same PATH. `$HOME` is read by the login user's
        // shell before sudo starts, so it is theirs, not /root.
        (MachineOpen::Root, _) => ssh(&[
            "-t",
            alias,
            &format!(
                "H=${{XDG_DATA_HOME:-$HOME/.local/share}}/hub && H=$H/bin/$(cat $H/current)/hubd && grep -qa {} $H && exec $H {} || exec sudo -i env PATH=$HOME/.local/bin:${{XDG_DATA_HOME:-$HOME/.local/share}}/hub/tools/bin:${{XDG_DATA_HOME:-$HOME/.local/share}}/hub/node/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin bash",
                crate::hubd_args::ROOT_MARKER,
                crate::hubd_args::ROOT_ARG
            ),
        ]),
        // A login shell so the CLI is on the PATH, with where Hub's installer puts Node and the
        // engines' CLIs (`tools`) too. The CLI starts in the home folder.
        (MachineOpen::Engine, Some(cli)) => {
            // Through hubd when it knows `cli` (the marker is in its binary), else the CLI bare.
            let run = match engine {
                Some(engine) => format!(
                    "H=$D/bin/$(cat $D/current)/hubd && grep -qa {} $H && exec $H {} {engine} || exec {cli}",
                    crate::hubd_args::CLI_MARKER,
                    crate::hubd_args::CLI_ARG
                ),
                None => format!("exec {cli}"),
            };
            ssh(&[
                "-t",
                alias,
                &format!(
                    "bash -lc 'export PATH=$HOME/.local/bin:${{XDG_DATA_HOME:-$HOME/.local/share}}/hub/tools/bin:${{XDG_DATA_HOME:-$HOME/.local/share}}/hub/node/bin:$PATH && D=${{XDG_DATA_HOME:-$HOME/.local/share}}/hub && cd && {run}'"
                ),
            ])
        }
        // The hubd the service runs there (`install-host.sh`), with the same PATH. No double
        // quotes: Windows Terminal reads its command line again on the way to ssh.
        (MachineOpen::SignIn, Some(engine)) => ssh(&[
            "-t",
            alias,
            &format!(
                "bash -lc 'export PATH=$HOME/.local/bin:${{XDG_DATA_HOME:-$HOME/.local/share}}/hub/tools/bin:${{XDG_DATA_HOME:-$HOME/.local/share}}/hub/node/bin:$PATH && D=${{XDG_DATA_HOME:-$HOME/.local/share}}/hub && cd && exec $D/bin/$(cat $D/current)/hubd {} {engine}'",
                crate::hubd_args::SIGN_IN_ARG
            ),
        ]),
        (MachineOpen::Engine | MachineOpen::SignIn, None) => unreachable!(),
        (MachineOpen::Attach, _) => bail!("which thread to take over?"),
    })
}

/// What takes `thread` over in a terminal: `hubd attach <thread> --if-idle`, on this computer or,
/// over the SSH alias, with the hubd the service runs there (`install-host.sh`). It follows the
/// thread live, and once it is idle (or Enter is pressed) stops Hub's engine and runs the engine's
/// own CLI on the session in the thread's folder;
/// when the CLI exits the thread goes back to Hub.
pub fn attach_command(alias: Option<&str>, thread: &str) -> Result<Vec<String>> {
    if !is_word(thread) {
        bail!("{thread} is not a thread id");
    }
    let Some(alias) = alias else {
        let hubd = std::env::current_exe()?.display().to_string();
        return Ok(vec![hubd, crate::hubd_args::ATTACH_ARG.into(), thread.to_owned(), "--if-idle".into()]);
    };
    if alias.starts_with('-') || alias.chars().any(char::is_whitespace) {
        bail!("{alias} is not an SSH alias");
    }
    Ok(vec![
        "ssh".into(),
        "-t".into(),
        alias.to_owned(),
        format!(
            "bash -lc 'export PATH=$HOME/.local/bin:${{XDG_DATA_HOME:-$HOME/.local/share}}/hub/tools/bin:${{XDG_DATA_HOME:-$HOME/.local/share}}/hub/node/bin:$PATH && D=${{XDG_DATA_HOME:-$HOME/.local/share}}/hub && exec $D/bin/$(cat $D/current)/hubd {} {thread} --if-idle'",
            crate::hubd_args::ATTACH_ARG
        ),
    ])
}

/// A program name and nothing else, safe to put in a remote shell line.
fn is_word(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
}

/// Opens `argv` in a terminal window here, titled `title`, in the home folder. An empty `argv`
/// is the default shell. Where no window can be opened, says what to type instead.
pub fn open(title: &str, argv: &[String]) -> Result<TerminalOpened> {
    let title = &window_title(title);
    let line = argv.iter().map(|a| quoted(a)).collect::<Vec<_>>().join(" ");
    let typed = if line.is_empty() { "a new terminal".to_owned() } else { line };
    if !cfg!(windows) {
        return Ok(TerminalOpened { opened: false, session: None, attach: typed });
    }
    let home = crate::folders::home();
    let dir = home.display().to_string();
    let mut wt: Vec<&str> = vec!["new-tab", "--title", title, "-d", &dir];
    let mut cmd: Vec<&str> = vec!["/C", "start", title, "/D", &dir];
    if argv.is_empty() {
        cmd.push("cmd");
    }
    wt.extend(argv.iter().map(String::as_str));
    cmd.extend(argv.iter().map(String::as_str));
    let mut last = None;
    for (program, args) in [("wt", wt), ("cmd", cmd)] {
        if program == "cmd" && !cmd_safe(args.iter().copied()) {
            continue;
        }
        let mut c = crate::proc::command_with_console(program);
        c.args(&args).current_dir(&home);
        match c.spawn() {
            Ok(_) => return Ok(TerminalOpened { opened: true, session: None, attach: typed }),
            Err(e) => last = Some(e),
        }
    }
    bail!("could not open a terminal: {}", last.map(|e| e.to_string()).unwrap_or_default())
}

/// Opens `argv` in a Windows Terminal window as administrator, outside Hub: Windows asks the
/// person to allow it (UAC) and the window is elevated. An empty `argv` is the default shell.
/// PowerShell's `Start-Process -Verb RunAs` is the only way to ask for that from a program that
/// is not elevated itself. It waits for the answer, so a "No" comes back as an error to show.
pub fn open_admin(title: &str, argv: &[String]) -> Result<TerminalOpened> {
    if !cfg!(windows) {
        bail!("An administrator window is for Windows; on Linux and Mac the Root shell does it");
    }
    let title = window_title(title);
    let dir = crate::folders::home().display().to_string();
    let mut args: Vec<&str> = vec!["new-tab", "--title", &title, "-d", &dir];
    args.extend(argv.iter().map(String::as_str));
    let script = admin_script("wt", &args, false)?;
    let typed = argv.join(" ");
    let out = crate::proc::command("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|e| anyhow::anyhow!("could not ask Windows for an administrator window: {e}"))?;
    if out.status.success() {
        return Ok(TerminalOpened { opened: true, session: None, attach: typed });
    }
    let said = String::from_utf8_lossy(&out.stderr);
    if said.to_lowercase().contains("cancel") {
        bail!("Windows did not allow it (the administrator question was answered No)");
    }
    bail!("could not open an administrator window (it needs Windows Terminal): {}", said.lines().next().unwrap_or("").trim())
}

/// Starts this program as `hubd admin-host` as administrator, with no window: the elevated helper
/// that runs a terminal inside Hub (`pty::host`). Windows asks the person first and this waits for
/// the answer, so a "No" comes back as an error to show.
pub fn start_admin_host(args: &[String]) -> Result<()> {
    if !cfg!(windows) {
        bail!("An administrator terminal is for Windows; on Linux and Mac the Root shell does it");
    }
    let exe = std::env::current_exe()?.display().to_string();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let script = admin_script(&exe, &args, true)?;
    let out = crate::proc::command("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|e| anyhow::anyhow!("could not ask Windows for administrator rights: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&out.stderr);
    if said.to_lowercase().contains("cancel") {
        bail!("Windows did not allow it (the administrator question was answered No)");
    }
    bail!("could not start as administrator: {}", said.lines().next().unwrap_or("").trim())
}

/// The PowerShell line that starts `file` with `args` as administrator. Each argument is one word
/// of the command line the program gets (spaces put it in double quotes, a double quote or `;` is
/// refused: Windows Terminal reads `;` as another tab), and the whole line sits in single quotes.
fn admin_script(file: &str, args: &[&str], hidden: bool) -> Result<String> {
    let mut words = Vec::new();
    for a in args {
        if a.contains(['"', ';', '\n', '\r']) {
            bail!("that cannot go on a Windows Terminal command line");
        }
        words.push(if a.is_empty() || a.contains(' ') { format!("\"{a}\"") } else { (*a).to_owned() });
    }
    let line = words.join(" ").replace('\'', "''");
    let file = file.replace('\'', "''");
    let window = if hidden { " -WindowStyle Hidden" } else { "" };
    Ok(format!("Start-Process -Verb RunAs -FilePath '{file}' -ArgumentList '{line}'{window} -ErrorAction Stop"))
}

/// A window title safe to hand to `cmd /C start "<title>"`. cmd reads `"`, `&`, `|`, `<`, `>`,
/// `^`, `%` and `!` as its own syntax even inside the quotes Rust adds, and a thread's title is
/// written by the agent, so only plain characters stay.
pub fn window_title(title: &str) -> String {
    let kept: String = title.chars().filter(|c| c.is_alphanumeric() || " -_.,:·#@+".contains(*c)).collect();
    let kept: String = kept.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(60).collect();
    if kept.is_empty() { "Hub".to_owned() } else { kept }
}

/// Whether `args` can go through `cmd /C` as they are: cmd does not read Rust's quoting, so an
/// argument with one of its own characters would be run as a command of its own.
pub fn cmd_safe<'a>(args: impl IntoIterator<Item = &'a str>) -> bool {
    args.into_iter().all(|a| !a.contains(['"', '&', '|', '<', '>', '^', '%', '!']))
}

fn quoted(arg: &str) -> String {
    if !arg.is_empty() && arg.chars().all(|c| c.is_ascii_alphanumeric() || "-_./@:=".contains(c)) {
        arg.to_owned()
    } else {
        format!("\"{}\"", arg.replace('"', "\\\""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_administrator_line_quotes_for_windows_terminal_and_powershell() {
        let line = admin_script("wt", &["new-tab", "--title", "PC", "-d", r"C:\Users\Kalle Anka", r"C:\Program Files\Hub\hubd.exe", "cli", "claude"], false).unwrap();
        assert_eq!(
            line,
            r#"Start-Process -Verb RunAs -FilePath 'wt' -ArgumentList 'new-tab --title PC -d "C:\Users\Kalle Anka" "C:\Program Files\Hub\hubd.exe" cli claude' -ErrorAction Stop"#
        );
        let quote = admin_script("wt", &["new-tab", "--title", "it's"], false).unwrap();
        assert!(quote.contains("it''s"));
        assert!(admin_script("wt", &["a;b"], false).is_err());
        assert!(admin_script("wt", &["a\"b"], false).is_err());
        // The helper: no window, the program with spaces in its path as the file.
        let host = admin_script(r"C:\Program Files\Hub\hubd.exe", &["admin-host", "127.0.0.1:7777", r"C:\Users\Kalle Anka\Hub", "abc", "s3cret", "120", "32"], true).unwrap();
        assert_eq!(
            host,
            r#"Start-Process -Verb RunAs -FilePath 'C:\Program Files\Hub\hubd.exe' -ArgumentList 'admin-host 127.0.0.1:7777 "C:\Users\Kalle Anka\Hub" abc s3cret 120 32' -WindowStyle Hidden -ErrorAction Stop"#
        );
    }

    #[test]
    fn the_engine_cli_is_the_first_word_of_its_terminal_command() {
        assert_eq!(engine_cli("claude --resume {session}"), Some("claude"));
        assert_eq!(engine_cli("codex resume {session}"), Some("codex"));
        assert_eq!(engine_cli("{session}"), None);
    }

    #[test]
    fn a_window_title_keeps_nothing_cmd_would_run() {
        assert_eq!(window_title("Hub · Fix the build"), "Hub · Fix the build");
        assert_eq!(window_title("x\" & calc & \"y"), "x calc y");
        assert_eq!(window_title("%PATH% | more ^ !x!"), "PATH more x");
        assert_eq!(window_title("   "), "Hub");
        assert!(window_title(&"a".repeat(200)).len() <= 60);
        assert!(cmd_safe(["new-tab", "-d", "D:\\Dev\\hub"]));
        assert!(!cmd_safe(["-d", "D:\\Dev\\a&b"]));
    }

    #[test]
    fn remote_commands_go_over_ssh_without_a_semicolon() {
        let shell = command(Some("clanker"), MachineOpen::Shell, None).unwrap();
        assert_eq!(shell, ["ssh", "clanker"]);
        let root = command(Some("clanker"), MachineOpen::Root, None).unwrap();
        assert_eq!(&root[..3], ["ssh", "-t", "clanker"]);
        // Hub's own sign-in goes with root when the machine's hubd has `root`, else plain sudo.
        assert!(root[3].contains("&& exec $H root || exec sudo -i env PATH=$HOME/.local/bin:"), "{}", root[3]);
        assert!(root[3].contains("grep -qa hubd-root-signed-in-v1 $H"), "{}", root[3]);
        assert!(root[3].contains("/hub/tools/bin:") && root[3].ends_with(":/bin bash"), "{}", root[3]);
        assert!(root.iter().all(|a| !a.contains(';') && !a.contains('"')));
        let claude = command(Some("clanker"), MachineOpen::Engine, Some("claude")).unwrap();
        assert_eq!(&claude[..3], ["ssh", "-t", "clanker"]);
        assert!(claude[3].ends_with("exec claude'"));
        assert!(claude.iter().all(|a| !a.contains(';')));
        // With its engine named, through hubd so it gets Hub's sign-in, bare on an old hubd.
        let signed = command_for(Some("mac"), MachineOpen::Engine, Some("claude"), Some("claude")).unwrap();
        assert!(signed[3].contains("&& exec $H cli claude || exec claude'"), "{}", signed[3]);
        assert!(signed.iter().all(|a| !a.contains(';') && !a.contains('"')));
        let sign_in = command(Some("tincan"), MachineOpen::SignIn, Some("claude")).unwrap();
        assert!(sign_in[3].ends_with("/hubd sign-in claude'"), "{}", sign_in[3]);
        assert!(sign_in.iter().all(|a| !a.contains(';') && !a.contains('"')));
        let here = command(None, MachineOpen::SignIn, Some("claude")).unwrap();
        assert_eq!(&here[1..], ["sign-in", "claude"]);
        assert!(command(Some("tincan"), MachineOpen::SignIn, Some("x; rm")).is_err());
    }

    #[test]
    fn taking_a_thread_over_runs_hubd_attach_there() {
        let id = "0b9c5a1e-27c4-4f7e-9d0a-5f3e8c2a1b7d";
        let here = attach_command(None, id).unwrap();
        assert_eq!(&here[1..], ["attach", id, "--if-idle"]);
        let there = attach_command(Some("tincan"), id).unwrap();
        assert_eq!(&there[..3], ["ssh", "-t", "tincan"]);
        assert!(there[3].ends_with(&format!("/hubd attach {id} --if-idle'")), "{}", there[3]);
        assert!(there.iter().all(|a| !a.contains(';') && !a.contains('"')));
        assert!(attach_command(Some("tincan"), "x'; rm -rf ~").is_err());
        assert!(attach_command(Some("-oProxyCommand=x"), id).is_err());
        assert!(command(Some("tincan"), MachineOpen::Attach, None).is_err());
    }

    #[test]
    fn odd_aliases_and_cli_names_are_refused() {
        assert!(command(Some("-oProxyCommand=x"), MachineOpen::Shell, None).is_err());
        assert!(command(Some("clanker"), MachineOpen::Engine, Some("rm -rf")).is_err());
        assert!(command(Some("clanker"), MachineOpen::Engine, None).is_err());
    }

    #[test]
    fn this_computer_opens_without_ssh() {
        assert!(command(None, MachineOpen::Shell, None).unwrap().is_empty());
        assert_eq!(command(None, MachineOpen::Engine, Some("claude")).unwrap(), ["claude"]);
    }
}

//! Opening a project folder on the core's machine, in the editor, a terminal or the file manager
//! (or a command from the settings with `{dir}`), and `split_command`, the quote-aware split of a
//! command line that engine launches use too.

use super::*;

/// Opens the project folder in the editor, a terminal or the file manager of the machine the
/// core runs on.
pub fn open_folder(root: &Path, with: &str, open: &crate::config::OpenConfig) -> Result<()> {
    if !root.is_dir() {
        bail!("{} is not a folder", root.display());
    }
    let dir = root.display().to_string();
    let custom = match with {
        "editor" => open.editor.as_deref(),
        "terminal" => open.terminal.as_deref(),
        _ => None,
    };
    if let Some(command) = custom.filter(|c| !c.trim().is_empty()) {
        return run_custom(command, root);
    }
    #[cfg(windows)]
    let tries: Vec<(&str, Vec<&str>)> = match with {
        // `code` is a .cmd script, so it goes through cmd.
        "editor" => vec![("cmd", vec!["/C", "code", &dir])],
        "terminal" => vec![("wt", vec!["-d", &dir]), ("cmd", vec!["/C", "start", "", "/D", &dir, "cmd"])],
        "files" => vec![("explorer", vec![&dir])],
        _ => bail!("unknown way to open a folder: {with}"),
    };
    #[cfg(target_os = "macos")]
    let tries: Vec<(&str, Vec<&str>)> = match with {
        "editor" => vec![("code", vec![&dir])],
        "terminal" => vec![("open", vec!["-a", "Terminal", &dir])],
        "files" => vec![("open", vec![&dir])],
        _ => bail!("unknown way to open a folder: {with}"),
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let tries: Vec<(&str, Vec<&str>)> = match with {
        "editor" => vec![("code", vec![&dir])],
        "terminal" => vec![("x-terminal-emulator", vec!["--working-directory", &dir])],
        "files" => vec![("xdg-open", vec![&dir])],
        _ => bail!("unknown way to open a folder: {with}"),
    };
    let mut last = None;
    for (program, args) in tries {
        // cmd does not read Rust's quoting: a folder called `a&b` would run `b`.
        if program == "cmd" && !crate::machine::cmd_safe(args.iter().copied()) {
            bail!("could not open {dir}: its name has a character cmd would run as a command");
        }
        let mut cmd = crate::proc::command(program);
        cmd.args(&args).current_dir(root);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            if program == "cmd" {
                cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
            }
        }
        match cmd.spawn() {
            Ok(_) => return Ok(()),
            Err(e) => last = Some(e),
        }
    }
    Err(anyhow::anyhow!("could not open {dir}: {}", last.map(|e| e.to_string()).unwrap_or_default()))
}

/// A command from the settings, with the folder where it says `{dir}` or at the end.
pub(super) fn run_custom(command: &str, root: &Path) -> Result<()> {
    let dir = root.display().to_string();
    let mut words = split_command(command);
    if words.iter().any(|w| w.contains("{dir}")) {
        words.iter_mut().for_each(|w| *w = w.replace("{dir}", &dir));
    } else {
        words.push(dir.clone());
    }
    let Some((program, args)) = words.split_first() else { bail!("the command is empty") };
    // Through cmd on Windows, so .cmd scripts such as `code` and `cursor` start too. cmd does not
    // read Rust's quoting, so a folder with one of its characters cannot go through it.
    if cfg!(windows) && !crate::machine::cmd_safe(words.iter().map(String::as_str)) {
        bail!("could not open {dir}: its name has a character cmd would run as a command");
    }
    #[cfg(windows)]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        let mut c = crate::proc::command("cmd");
        c.arg("/C").arg(program).args(args).creation_flags(0x0800_0000);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = crate::proc::command(program);
        c.args(args);
        c
    };
    cmd.current_dir(root).spawn().with_context(|| format!("starting {program}"))?;
    Ok(())
}

/// Splits a command line on spaces, keeping "quoted parts" together.
pub fn split_command(command: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut any = false;
    for c in command.trim().chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    words.push(std::mem::take(&mut word));
                    any = false;
                }
            }
            c => {
                word.push(c);
                any = true;
            }
        }
    }
    if any {
        words.push(word);
    }
    words
}

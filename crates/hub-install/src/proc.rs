//! The one way Hub starts a program.
//!
//! On Windows a program started from a process that has no console of its own (Hub's `hubd` and
//! window both run that way) gets a console window of its own, which flashes on screen and
//! vanishes. `CREATE_NO_WINDOW` stops that. Every spawn goes through [`command`] so none can be
//! forgotten; `cargo test -p hub-core --test it no_window::` fails on a raw `Command::new`.

use std::ffi::OsStr;
use std::process::Command;

/// Windows process creation flag: no console window for a console program.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A command that never opens a console window. A caller that sets its own `creation_flags`
/// replaces this one and must include [`CREATE_NO_WINDOW`] itself.
///
/// A `git` also never asks the person to sign in: see [`no_credential_prompt`].
pub fn command(program: impl AsRef<OsStr>) -> Command {
    let program = program.as_ref();
    let mut cmd = Command::new(program);
    quiet(&mut cmd);
    if is_git(program) {
        no_credential_prompt(&mut cmd);
    }
    cmd
}

/// True for `git`, `git.exe` and a path to either.
fn is_git(program: &OsStr) -> bool {
    std::path::Path::new(program).file_stem().is_some_and(|name| name.eq_ignore_ascii_case("git"))
}

/// Make git fail instead of asking for a password. Hub runs git in the background (the update
/// check every minute, the merge queue, worktrees, the GitHub page), and on Windows a repository
/// that needs a sign-in it does not have makes Git Credential Manager open its "Connect to GitHub"
/// window, which git then waits on, again at every check. With this, git uses whatever sign-in is
/// already stored and otherwise fails with an error the caller can show.
pub fn no_credential_prompt(cmd: &mut Command) -> &mut Command {
    cmd.env("GIT_TERMINAL_PROMPT", "0").env("GCM_INTERACTIVE", "never").env("GIT_ASKPASS", "").env("SSH_ASKPASS", "")
}

/// A command that keeps the console it is started from, or shows its own window on purpose: a
/// terminal opened for the person (`wt`, `cmd /C start`), and a program run in a terminal that
/// already is the person's (`hubd attach`, `hubd sign-in`, `hub-shell`). `CREATE_NO_WINDOW` there
/// would take the terminal away. Never use it for a program started in the background.
pub fn command_with_console(program: impl AsRef<OsStr>) -> Command {
    Command::new(program)
}

/// Make an existing command windowless (for one built some other way). Does nothing off Windows.
pub fn quiet(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_git_gets_the_no_prompt_environment() {
        let set = |cmd: &Command| cmd.get_envs().any(|(k, v)| k == "GCM_INTERACTIVE" && v == Some(OsStr::new("never")));
        assert!(set(&command("git")));
        assert!(set(&command("git.exe")));
        assert!(set(&command("/usr/bin/Git.exe")));
        assert!(!set(&command("gh")));
        assert!(!set(&command("cargo")));
        assert!(!set(&command_with_console("git")));
    }
}

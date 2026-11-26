//! Getting into a fresh machine: its password once, to put this machine's SSH key there, and
//! keys from then on. Hub asks nothing more after that, as with the sudo password: the password
//! goes to ssh and is kept nowhere.
//!
//! ssh reads a password only from a terminal or from an askpass program. hubd is that program:
//! started by ssh with `SECRET_ENV` set, it prints the password and exits (see hubd's `main`).

use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use tokio::io::AsyncWriteExt;

/// Set only for the ssh that logs in with a password, and so for the askpass it starts.
pub const SECRET_ENV: &str = "HUB_ASKPASS_SECRET";

/// What ssh says when keys do not get in and a password would.
pub fn wants_password(error: &str) -> bool {
    let e = error.to_lowercase();
    e.contains("permission denied") || e.contains("too many authentication failures")
}

/// This machine's public key, made first when there is none (ed25519, no passphrase, as
/// `ssh-keygen` would make it by default).
pub async fn public_key() -> Result<String> {
    let dir = crate::folders::home_dir().context("no home folder")?.join(".ssh");
    for name in ["id_ed25519.pub", "id_ecdsa.pub", "id_rsa.pub"] {
        if let Ok(key) = std::fs::read_to_string(dir.join(name))
            && dir.join(name.trim_end_matches(".pub")).exists()
        {
            return Ok(key.trim().to_owned());
        }
    }
    std::fs::create_dir_all(&dir)?;
    let file = dir.join("id_ed25519");
    let out = crate::proc::tokio_command("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-C", "hub", "-f"])
        .arg(&file)
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .context("running ssh-keygen; is the OpenSSH client installed?")?;
    if !out.status.success() {
        bail!("ssh-keygen: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(std::fs::read_to_string(file.with_extension("pub"))?.trim().to_owned())
}

/// Run on the target with the key on stdin. A file that does not end in a newline gets one first:
/// appending straight to it would glue the new key onto the last line and break both keys.
const ADD_KEY: &str = "umask 077; mkdir -p ~/.ssh && k=$(cat) && f=~/.ssh/authorized_keys && \
    { grep -qxF \"$k\" \"$f\" 2>/dev/null || { [ -z \"$(tail -c1 \"$f\" 2>/dev/null)\" ] || echo >> \"$f\"; echo \"$k\" >> \"$f\"; }; }";

/// Logs in to `target` with `password` once and adds this machine's key to its
/// `authorized_keys`, unless it is there already.
pub async fn give_key(target: &str, password: &str) -> Result<()> {
    let key = public_key().await?;
    let askpass = std::env::current_exe().context("finding hubd for ssh's password prompt")?;
    let mut cmd = crate::proc::tokio_command("ssh");
    cmd.args(["-o", "PubkeyAuthentication=no", "-o", "PreferredAuthentications=password,keyboard-interactive"])
        .args(["-o", "NumberOfPasswordPrompts=1", "-o", "ConnectTimeout=10", "-o", "StrictHostKeyChecking=accept-new"])
        .arg(target)
        .arg(ADD_KEY)
        .env("SSH_ASKPASS", askpass)
        .env("SSH_ASKPASS_REQUIRE", "force")
        // Older ssh uses askpass only with a display and no terminal.
        .env("DISPLAY", std::env::var("DISPLAY").unwrap_or_else(|_| ":0".to_owned()))
        .env(SECRET_ENV, password)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().context("starting ssh; is the OpenSSH client installed?")?;
    let mut stdin = child.stdin.take().context("ssh's stdin")?;
    stdin.write_all(format!("{key}\n").as_bytes()).await?;
    drop(stdin);
    let out = tokio::time::timeout(Duration::from_secs(30), child.wait_with_output())
        .await
        .map_err(|_| anyhow!("ssh {target} did not finish"))??;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let line = err.lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("ssh failed");
        if wants_password(line) {
            bail!("{target} did not take that password");
        }
        bail!("{line}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_when_a_password_would_get_in() {
        assert!(wants_password("pi@192.168.0.174: Permission denied (publickey,password)."));
        assert!(!wants_password("ssh: connect to host 192.168.0.9 port 22: Connection timed out"));
    }
}

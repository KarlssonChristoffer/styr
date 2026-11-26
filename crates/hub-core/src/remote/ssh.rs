//! What Hub does to another machine over SSH: the tunnel to its hubd, reading its connection
//! token, probing what it has of hubd and what `install-host.sh --check` says it lacks, the
//! installer with Unix line ends, the Tailscale join script and its sign-in link, and `ssh_run`.

use super::*;

pub(super) fn free_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

pub(super) fn ssh_command(alias: &str) -> tokio::process::Command {
    let mut cmd = crate::proc::tokio_command("ssh");
    // Keys only: a password prompt would hang where nobody sees it.
    cmd.args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=10", "-o", "ServerAliveInterval=15"])
        .args(["-o", "ServerAliveCountMax=3", "-o", "StrictHostKeyChecking=accept-new"])
        .arg(alias)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// `ssh -N -L port:127.0.0.1:remote alias`, alive as long as the returned child.
pub(super) fn open_tunnel(alias: &str, port: u16, remote: u16) -> Result<tokio::process::Child> {
    let mut cmd = ssh_command(alias);
    cmd.args(["-N", "-o", "ExitOnForwardFailure=yes", "-L", &format!("{port}:127.0.0.1:{remote}")])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    cmd.spawn().context("starting ssh; is the OpenSSH client installed?")
}

/// The other hubd's connection token, from its data folder. A connection ssh itself drops (a Mac's
/// sshd turns some away at once) is tried again twice; why it ends without a token is logged, since
/// the row can only say that the token was refused.
pub(super) async fn read_token(alias: &str) -> Option<String> {
    for attempt in 0..3 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
        let mut cmd = ssh_command(alias);
        cmd.arg("cat \"${XDG_DATA_HOME:-$HOME/.local/share}/hub/token\" 2>/dev/null");
        let out = match tokio::time::timeout(Duration::from_secs(20), cmd.output()).await {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => {
                tracing::info!("reading the token on {alias}: {e}");
                return None;
            }
            Err(_) => {
                tracing::info!("reading the token on {alias}: ssh did not answer in 20 seconds");
                continue;
            }
        };
        // 255 is ssh's own: it did not get in. Any other status is the command's.
        if out.status.code() == Some(255) {
            let why = String::from_utf8_lossy(&out.stderr).lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("ssh failed").to_owned();
            tracing::info!("reading the token on {alias}: {why}");
            continue;
        }
        let token = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        if token.is_empty() {
            tracing::info!("reading the token on {alias}: there is no token file in its data folder");
            return None;
        }
        return Some(token);
    }
    None
}

/// The installer as bash on the host needs it. A hubd built from a Windows checkout may have
/// it with CRLF line ends, and bash then reads `set -o pipefail\r` as an unknown option.
pub(super) fn install_script() -> String {
    INSTALL_SCRIPT.replace("\r\n", "\n")
}

/// What a host that SSH reaches has of hubd.
#[derive(Debug, PartialEq)]
pub(super) enum Probe {
    Installed,
    Missing,
    /// The installer runs; its latest line.
    Installing(String),
    /// The installer ran and stopped without installing; its last line.
    Failed(String),
    Unknown,
}

pub(super) async fn probe(alias: &str) -> Probe {
    let script = format!(
        "{REMOTE_DATA}; \
         if [ -f \"$D/install.pid\" ] && kill -0 \"$(cat \"$D/install.pid\")\" 2>/dev/null; then echo installing; \
         elif [ -x \"$D/run-hubd\" ]; then echo installed; \
         elif [ -f \"$D/install.log\" ]; then echo failed; \
         else echo missing; fi; \
         grep -v '^[[:space:]]*$' \"$D/install.log\" 2>/dev/null | tail -n 1"
    );
    match ssh_run(alias, &script, None, Duration::from_secs(60)).await {
        Ok(out) => read_probe(&out),
        Err(_) => Probe::Unknown,
    }
}

pub(super) fn read_probe(out: &str) -> Probe {
    let mut lines = out.lines().map(str::trim);
    let state = lines.next().unwrap_or_default();
    let step = lines.next().unwrap_or("").to_owned();
    match state {
        "installed" => Probe::Installed,
        "missing" => Probe::Missing,
        "installing" => Probe::Installing(if step.is_empty() { "starting".to_owned() } else { step }),
        "failed" => Probe::Failed(if step.is_empty() { "see install.log in Hub's folder there".to_owned() } else { step }),
        _ => Probe::Unknown,
    }
}

/// What `install-host.sh --check` found missing on a host.
#[derive(Debug, Default, PartialEq)]
pub(super) struct Lacks {
    /// apt packages to install.
    pub(super) packages: Vec<String>,
    /// hubd would stop when the user logs out.
    pub(super) linger: bool,
    /// Root is at hand there without a password.
    pub(super) sudo: bool,
    /// What a person has to fix.
    pub(super) problems: Vec<String>,
}

impl Lacks {
    pub(super) fn needs_root(&self) -> bool {
        !self.packages.is_empty() || self.linger
    }

    pub(super) fn sudo_for(&self, name: &str) -> String {
        let mut what = Vec::new();
        if !self.packages.is_empty() {
            // A Mac's one package has a name only Hub's installer knows.
            let names: Vec<&str> =
                self.packages.iter().map(|p| if p == "xcode-clt" { "the Xcode Command Line Tools" } else { p.as_str() }).collect();
            what.push(format!("install {}", names.join(", ")));
        }
        if self.linger {
            what.push("let hubd run without a login".to_owned());
        }
        format!("Hub needs sudo on {name} once to {}.", what.join(" and "))
    }
}

pub(super) fn read_check(out: &str) -> Lacks {
    let mut lacks = Lacks::default();
    for line in out.lines().map(str::trim) {
        let (kind, rest) = line.split_once(' ').unwrap_or((line, ""));
        match kind {
            // Only plain package names reach the root shell.
            "package" if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "+-.".contains(c)) => {
                lacks.packages.push(rest.to_owned())
            }
            "linger" => lacks.linger = true,
            "sudo" => lacks.sudo = true,
            "problem" => lacks.problems.push(rest.to_owned()),
            _ => {}
        }
    }
    lacks
}

/// Runs a command over SSH, feeding it `input`, and gives back what it printed.
/// Whether the machine is on a tailnet, has Tailscale, and has root without a password.
/// A Mac says `mac` instead: Tailscale there is its app, which Hub neither installs nor starts over
/// SSH (its command line cannot reach the app from an SSH session, error -600).
pub(super) const JOIN_CHECK: &str = "if [ \"$(uname -s)\" = Darwin ]; then echo mac; \
    elif command -v tailscale >/dev/null 2>&1; then \
    if tailscale status --json 2>/dev/null | grep -q '\"BackendState\": *\"Running\"'; then echo on; else echo installed; fi; \
    else echo missing; fi; \
    if [ \"$(id -u)\" = 0 ] || sudo -n true 2>/dev/null; then echo root; fi";

/// Run as root on the machine, with the user who may run `tailscale` without sudo afterwards
/// (for `hubd pair --tailscale`). `tailscale up` prints a sign-in link and waits until it is used.
pub(super) const JOIN_SCRIPT: &str = r#"set -e
operator="$1"
if ! command -v curl >/dev/null 2>&1; then
  echo "Installing curl"
  apt-get update -qq && apt-get install -y -qq curl
fi
if ! command -v tailscale >/dev/null 2>&1; then
  echo "Installing Tailscale"
  curl -fsSL https://tailscale.com/install.sh | sh
fi
systemctl enable --now tailscaled >/dev/null 2>&1 || true
echo "Starting Tailscale"
tailscale up --operator="$operator" --timeout=25m
echo "On the tailnet"
"#;

/// The sign-in link `tailscale up` prints: `To authenticate, visit: https://login.tailscale.com/a/…`.
pub(super) fn login_url(log: &str) -> Option<String> {
    log.split_whitespace().rev().find(|w| w.starts_with("https://login.tailscale.com/")).map(str::to_owned)
}

pub(super) async fn ssh_run(alias: &str, command: &str, input: Option<Vec<u8>>, timeout: Duration) -> Result<String> {
    use tokio::io::AsyncWriteExt;
    let mut cmd = ssh_command(alias);
    cmd.arg(command).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    if input.is_some() {
        cmd.stdin(std::process::Stdio::piped());
    }
    let mut child = cmd.spawn().context("starting ssh; is the OpenSSH client installed?")?;
    if let Some(input) = input {
        let mut stdin = child.stdin.take().context("ssh's stdin")?;
        stdin.write_all(&input).await?;
        drop(stdin);
    }
    let out = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| anyhow!("ssh {alias} did not finish"))??;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("{}", err.lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("ssh failed"));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

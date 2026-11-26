//! Installing a harness from Settings > Engines: the background install jobs and their last
//! lines of output, the shell an installer runs in (`run_shell`), the npm fallback into Hub's own
//! tools folder (Node.js first where there is no npm), and the PATH that finds what Hub installed.

use super::*;

/// An install that runs or ran since the core started, by harness.
#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(export)]
pub struct InstallJob {
    pub harness: String,
    /// What runs, as you would type it.
    pub command: String,
    pub running: bool,
    /// Done and the program is found (true), or it failed (false). None while it runs.
    pub ok: Option<bool>,
    /// One line on how it went.
    pub message: String,
    /// The installer's last lines of output.
    pub lines: Vec<String>,
    #[ts(type = "number")]
    pub started: i64,
}

pub(super) static JOBS: LazyLock<Mutex<BTreeMap<String, InstallJob>>> = LazyLock::new(Default::default);
pub(super) const KEEP_LINES: usize = 60;
pub(super) const INSTALL_TIMEOUT: Duration = Duration::from_secs(20 * 60);

pub fn install_jobs() -> Vec<InstallJob> {
    JOBS.locked().values().cloned().collect()
}

pub(super) fn update_job(harness: &str, f: impl FnOnce(&mut InstallJob)) {
    if let Some(job) = JOBS.locked().get_mut(harness) {
        f(job);
    }
}

/// Starts installing a harness from the catalog, in the background. Pressing Install is the ask:
/// the command is the harness's own installer, shown on the page before it runs.
pub fn start_install(harness_id: &str) -> Result<InstallJob> {
    let h = harnesses().into_iter().find(|h| h.id == harness_id).with_context(|| format!("Hub knows no harness {harness_id:?}"))?;
    let install = h.install.clone().with_context(|| format!("Hub doesn't know how to install {}", h.label))?;
    let job = {
        let mut jobs = JOBS.locked();
        if jobs.get(&h.id).is_some_and(|j| j.running) {
            bail!("{} is being installed already", h.label);
        }
        let job = InstallJob {
            harness: h.id.clone(),
            command: install.clone(),
            running: true,
            ok: None,
            message: format!("Installing {}…", h.label),
            lines: Vec::new(),
            started: crate::model::now_ms(),
        };
        jobs.insert(h.id.clone(), job.clone());
        job
    };
    tokio::spawn(async move {
        let result = tokio::time::timeout(INSTALL_TIMEOUT, run_install(&h.id, &install)).await;
        let (ok, message) = match result {
            Err(_) => (false, format!("The installer was still going after {} minutes and was stopped.", INSTALL_TIMEOUT.as_secs() / 60)),
            Ok(Err(e)) => (false, format!("The installer failed: {e}")),
            Ok(Ok(())) => {
                // The installer may have changed PATH for new shells: look again now.
                let command = h.command.clone();
                let found = tokio::task::spawn_blocking(move || {
                    forget_lookup();
                    resolve(&command)
                })
                .await
                .ok()
                .flatten();
                match found {
                    Some(_) => (true, format!("{} is installed.", h.label)),
                    None => (
                        false,
                        format!("The installer finished, but Hub can't find {} yet. Restarting Hub (or signing out and in) usually fixes that.", h.label),
                    ),
                }
            }
        };
        update_job(&h.id, |j| {
            j.running = false;
            j.ok = Some(ok);
            j.message = message;
        });
    });
    Ok(job)
}

/// `fresh_lookup` keeps answers for half a minute; an install changes them.
pub fn forget_lookup() {
    CLEAR.store(true, std::sync::atomic::Ordering::SeqCst);
}

pub(super) static CLEAR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Runs a harness's installer, keeping its last `KEEP_LINES` lines of output on its job.
pub(super) async fn run_install(harness: &str, install: &str) -> Result<()> {
    let id = harness.to_owned();
    run_shell(install, move |line: String| {
        update_job(&id, |j| {
            j.lines.push(line);
            let extra = j.lines.len().saturating_sub(KEEP_LINES);
            j.lines.drain(..extra);
        })
    })
    .await
}

/// Runs a command in the machine's shell (PowerShell on Windows, a login bash elsewhere) with no
/// one to answer it, handing each line of output, without colours or secrets, to `sink`.
pub async fn run_shell(install: &str, sink: impl Fn(String) + Send + Sync + Clone + 'static) -> Result<()> {
    use tokio::io::AsyncBufReadExt;
    let mut cmd = if cfg!(windows) {
        let mut c = crate::proc::tokio_command("powershell");
        c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", install]);
        c
    } else {
        let mut c = crate::proc::tokio_command("bash");
        c.args(["-lc", install]);
        c
    };
    // Nobody can answer a question the installer asks: it gets an empty answer and goes on with
    // its defaults, or stops and says why.
    cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    cmd.env("CI", "1").env("NONINTERACTIVE", "1").env("NO_COLOR", "1").kill_on_drop(true);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.as_std_mut().creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().with_context(|| format!("starting {}", if cfg!(windows) { "powershell" } else { "bash" }))?;
    let out = tokio::io::BufReader::new(child.stdout.take().context("no output")?).lines();
    let err = tokio::io::BufReader::new(child.stderr.take().context("no output")?).lines();
    let (_, _, status) = tokio::join!(keep(sink.clone(), out), keep(sink, err), child.wait());
    let status = status?;
    if !status.success() {
        bail!("it ended with {status}");
    }
    Ok(())
}

/// Hands an installer's output lines to `sink`.
pub(super) async fn keep<R: tokio::io::AsyncBufRead + Unpin>(sink: impl Fn(String), mut lines: tokio::io::Lines<R>) {
    while let Ok(Some(line)) = lines.next_line().await {
        let line = plain(&line);
        let line = crate::leak::redact(&line, &BTreeMap::new()).unwrap_or(line);
        if line.trim().is_empty() {
            continue;
        }
        sink(line);
    }
}

/// A line of terminal output without its colour codes.
pub(super) fn plain(line: &str) -> String {
    static ESC: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b\][^\x07]*\x07|\r").expect("fixed pattern"));
    ESC.replace_all(line, "").into_owned()
}

/// How to install the engine's own CLI: its harness's installer, else its npm package with the
/// npm Hub finds (Hub's installer puts Node in its data folder, off a service's PATH).
pub fn install_command(config: &crate::config::Config, engine: &str, cmd: &str) -> Option<String> {
    let id = config.engines.get(engine).and_then(|e| e.harness.clone()).unwrap_or_else(|| engine.to_owned());
    let h = harnesses().into_iter().find(|h| h.id == id)?;
    let program = crate::project::split_command(cmd).first().cloned()?;
    if h.cli.as_deref() != Some(program.as_str()) && h.id != program {
        return None;
    }
    h.install.or_else(|| {
        let package = h.package?;
        if cfg!(windows) {
            let npm = crate::proc::find_program("npm")?;
            return Some(format!("\"{}\" install -g {package}", npm.display()));
        }
        Some(npm_install_script(&package))
    })
}

/// Installs `package` into Hub's own tools folder with npm, and Node.js 22 into Hub's folder first
/// where there is no npm (a Debian `nodejs` has none, a Raspberry Pi often only that). Nothing
/// needs sudo, and `install -g` into a system prefix would fail without it. The Node.js step is
/// the one in `scripts/install-host.sh`.
fn npm_install_script(package: &str) -> String {
    format!(
        r#"D="${{XDG_DATA_HOME:-$HOME/.local/share}}/hub"
PATH="$D/node/bin:$PATH"
if ! command -v npm >/dev/null 2>&1; then
  case "$(uname -m)" in x86_64) A=x64 ;; aarch64|arm64) A=arm64 ;; armv7l) A=armv7l ;; *) echo "Node.js has no build for $(uname -m); install Node.js 20 or newer by hand" >&2; exit 1 ;; esac
  case "$(uname -s)" in Darwin) P="darwin-$A"; E='tar\.gz'; U=-xzf ;; *) P="linux-$A"; E='tar\.xz'; U=-xJf ;; esac
  DIST=https://nodejs.org/dist/latest-v22.x
  T="$(mktemp -d "${{TMPDIR:-/tmp}}/hub-node.XXXXXX")" || exit 1
  echo "npm is missing here, so Hub installs Node.js 22 first (in $D/node)"
  curl -fsSL -o "$T/SHASUMS256.txt" "$DIST/SHASUMS256.txt" || {{ rm -rf "$T"; exit 1; }}
  LINE="$(grep -E " node-v[0-9.]+-$P\.$E$" "$T/SHASUMS256.txt" | head -n1)"
  [ -n "$LINE" ] || {{ echo "Could not find Node.js for $P at $DIST" >&2; rm -rf "$T"; exit 1; }}
  F="${{LINE##* }}"
  curl -fsSL -o "$T/$F" "$DIST/$F" || {{ rm -rf "$T"; exit 1; }}
  (cd "$T" && printf '%s
' "$LINE" | {{ sha256sum -c --status - 2>/dev/null || shasum -a 256 -c --status -; }}) || {{ echo "The Node.js download did not match its checksum." >&2; rm -rf "$T"; exit 1; }}
  rm -rf "$D/node" && mkdir -p "$D/node" && tar $U "$T/$F" -C "$D/node" --strip-components=1 || {{ rm -rf "$T"; exit 1; }}
  rm -rf "$T"
fi
echo "Installing {package} (a few minutes on a small computer; nothing is wrong while it is quiet)…"
npm install --global --prefix "$D/tools" --no-fund --no-audit --no-update-notifier --fetch-timeout=60000 --fetch-retries=3 {package}"#
    )
}

/// A command with the folders Hub's installer puts programs in first on its PATH.
pub fn with_path(cmd: &str) -> String {
    if cfg!(windows) {
        return cmd.to_owned();
    }
    format!("export PATH=$HOME/.local/bin:${{XDG_DATA_HOME:-$HOME/.local/share}}/hub/tools/bin:${{XDG_DATA_HOME:-$HOME/.local/share}}/hub/node/bin:$HOME/.npm-global/bin:$PATH && {cmd}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn the_cli_install_is_valid_shell_and_needs_no_sudo() {
        let script = npm_install_script("@openai/codex");
        let checked = std::process::Command::new("sh").arg("-n").arg("-c").arg(&script).status().unwrap();
        assert!(checked.success(), "not valid shell:\n{script}");
        assert!(script.contains("--prefix \"$D/tools\""), "installs into Hub's tools folder, not a system prefix");
        assert!(script.trim_end().ends_with("@openai/codex"));
        assert!(script.contains("--fetch-timeout"), "a stalled download ends with an error instead of waiting for minutes");
        assert!(script.contains("command -v npm"), "Node.js comes first where there is no npm");
    }
}

//! Watching a command's output while it runs, for engines that report it only when the command
//! ends. Claude Code is one: its adapter sends a Bash command's output in one piece at the end.
//!
//! Claude Code runs every shell command through the program in `CLAUDE_CODE_SHELL_PREFIX` when it
//! is set, as `<prefix> '<script>'`, and it points the command's stdout and stderr at a file it
//! reads back when the command ends. Hub sets the prefix to `hub-shell`, a link to hubd that
//! notes where that file is (a line in `<data dir>/output/<thread>/`) and then runs the script
//! with the same shell, its streams untouched: no pipe sits between the command and the file, so
//! a server started in the background, the exit code and what Claude reads are as they were.
//! While the command's tool call runs, the core reads the file as it grows and logs each new
//! piece as an `output` event on that call. Claude's own output replaces it when the call ends.
//!
//! Hooks and MCP servers go through the prefix too. Those are not Claude's Bash script, which
//! always ends by writing its folder with `pwd -P >|`, and run as they are, noting nothing.

use crate::locked::{LockExt, ResultExt};
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::config::Config;
use crate::model::OutputData;

/// The file name hubd answers to as the wrapper.
pub const WRAPPER: &str = "hub-shell";
/// Where the wrapper notes the output files of the thread's commands.
pub const DIR_VAR: &str = "HUB_OUTPUT_DIR";
const PREFIX_VAR: &str = "CLAUDE_CODE_SHELL_PREFIX";
/// What only Claude's Bash script holds: it ends by writing the folder it ended in to a file.
const BASH_MARK: &str = "pwd -P >|";
/// Most of one command's output logged as it runs; the rest comes with the engine's own output.
const MAX_LIVE: u64 = 2 * 1024 * 1024;
/// How long a note waits for the tool call it belongs to.
const MATCH_WAIT: Duration = Duration::from_secs(5);

/// Whether this process was started as the wrapper.
pub fn is_wrapper() -> bool {
    std::env::args_os()
        .next()
        .and_then(|a| Path::new(&a).file_stem().map(|s| s.to_string_lossy().eq_ignore_ascii_case(WRAPPER)))
        .unwrap_or(false)
}

/// The wrapper: runs `<shell> -c <script>` with this process's streams and returns its exit code,
/// after noting where its output goes when it is Claude's Bash script.
pub fn run_wrapper(args: &[String]) -> i32 {
    let script = args.join(" ");
    let shell = shell();
    if script.contains(BASH_MARK)
        && let Some(dir) = std::env::var_os(DIR_VAR)
        && let Some(output) = stdout_file()
    {
        note(Path::new(&dir), &script, &output);
    }
    let mut command = crate::proc::command_with_console(&shell);
    command.arg("-c").arg(&script);
    exec(command, &shell)
}

#[cfg(unix)]
fn exec(mut command: std::process::Command, shell: &Path) -> i32 {
    use std::os::unix::process::CommandExt;
    // Becomes the shell, so signals and the process group are exactly as without the wrapper.
    let err = command.exec();
    eprintln!("hub-shell: could not run {}: {err}", shell.display());
    127
}

#[cfg(not(unix))]
fn exec(mut command: std::process::Command, shell: &Path) -> i32 {
    match command.status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(err) => {
            eprintln!("hub-shell: could not run {}: {err}", shell.display());
            127
        }
    }
}

/// The shell that started the wrapper (the one Claude Code chose), else `$SHELL` or `bash`.
fn shell() -> PathBuf {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let me = Pid::from_u32(std::process::id());
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[me]), true, ProcessRefreshKind::nothing());
    let parent = sys.process(me).and_then(|p| p.parent());
    if let Some(parent) = parent {
        sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[parent]), true, ProcessRefreshKind::nothing().with_exe(UpdateKind::Always));
        if let Some(exe) = sys.process(parent).and_then(|p| p.exe())
            && is_shell(exe)
        {
            return exe.to_path_buf();
        }
    }
    match std::env::var_os("SHELL").map(PathBuf::from) {
        Some(sh) if is_shell(&sh) && sh.is_absolute() && sh.exists() => sh,
        _ => git_bash().unwrap_or_else(|| PathBuf::from("bash")),
    }
}

/// Git Bash, which Claude Code uses on Windows, found the way Claude Code finds it. Started by
/// Claude itself (an MCP server) the wrapper has no shell above it and `bash` may not be on the
/// path. Never WSL's `bash.exe` in System32.
#[cfg(windows)]
fn git_bash() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("CLAUDE_CODE_GIT_BASH_PATH").map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(p);
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    let dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
    let wsl = |p: &Path| p.to_string_lossy().to_lowercase().contains("system32");
    if let Some(p) = dirs.iter().map(|d| d.join("bash.exe")).find(|p| p.is_file() && !wsl(p.as_path())) {
        return Some(p);
    }
    // git.exe in <Git>\cmd: bash is in <Git>\bin.
    let from_git = dirs.iter().filter(|d| d.join("git.exe").is_file()).filter_map(|d| d.parent().map(|g| g.join("bin").join("bash.exe")));
    let defaults = [r"C:\Program Files\Git\bin\bash.exe", r"C:\Program Files (x86)\Git\bin\bash.exe"].map(PathBuf::from);
    from_git.chain(defaults).find(|p| p.is_file())
}

#[cfg(not(windows))]
fn git_bash() -> Option<PathBuf> {
    None
}

fn is_shell(path: &Path) -> bool {
    let name = path.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
    matches!(name.as_str(), "bash" | "zsh" | "sh")
}

/// Where this process's stdout goes, when it is a file.
#[cfg(target_os = "linux")]
fn stdout_file() -> Option<PathBuf> {
    let path = std::fs::read_link("/proc/self/fd/1").ok()?;
    path.is_file().then_some(path)
}

#[cfg(windows)]
fn stdout_file() -> Option<PathBuf> {
    use std::os::windows::io::AsRawHandle;
    #[link(name = "kernel32", kind = "raw-dylib")]
    unsafe extern "system" {
        fn GetFinalPathNameByHandleW(file: *mut std::ffi::c_void, path: *mut u16, len: u32, flags: u32) -> u32;
    }
    let handle = std::io::stdout().as_raw_handle();
    let mut buf = vec![0u16; 1024];
    // SAFETY: the buffer is as long as said; a handle that is not a file makes the call fail.
    let n = unsafe { GetFinalPathNameByHandleW(handle.cast(), buf.as_mut_ptr(), buf.len() as u32, 0) } as usize;
    if n == 0 || n >= buf.len() {
        return None;
    }
    let path = String::from_utf16_lossy(&buf[..n]);
    let path = PathBuf::from(path.strip_prefix(r"\\?\").unwrap_or(&path));
    path.is_file().then_some(path)
}

#[cfg(not(any(target_os = "linux", windows)))]
fn stdout_file() -> Option<PathBuf> {
    None
}

/// One line in a file of its own: the script and its output file. Nothing is said when it fails;
/// the command runs all the same.
fn note(dir: &Path, script: &str, output: &Path) {
    let ms = crate::model::now_ms();
    let path = dir.join(format!("{ms}-{}.json", std::process::id()));
    let line = json!({ "script": script, "output": output });
    std::fs::write(path, line.to_string()).warn_on_err("write file");
}

/// The wrapper next to Hub's data: a hard link to this hubd (a copy where links fail), renewed
/// when hubd changes. None when this process is not hubd (tests).
pub fn wrapper(data_dir: &Path) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    if exe.file_stem()?.to_string_lossy() != "hubd" {
        return None;
    }
    renew(&exe, &data_dir.join("bin"))
}

/// The wrapper in `dir` as a link to (or copy of) `exe`, made anew when `exe` changed.
fn renew(exe: &Path, dir: &Path) -> Option<PathBuf> {
    std::fs::create_dir_all(dir).ok()?;
    let path = dir.join(format!("{WRAPPER}{}", std::env::consts::EXE_SUFFIX));
    let same = |a: &Path, b: &Path| {
        let (Ok(a), Ok(b)) = (std::fs::metadata(a), std::fs::metadata(b)) else { return false };
        a.len() == b.len() && a.modified().ok() == b.modified().ok()
    };
    if !same(exe, &path) {
        // Made beside it and moved into place in one step, so a command an engine starts just
        // now finds the old wrapper or the new one, never none.
        let fresh = dir.join(format!("{WRAPPER}.new{}", std::env::consts::EXE_SUFFIX));
        crate::locked::clean_file(&fresh);
        let made = std::fs::hard_link(exe, &fresh).is_ok() || std::fs::copy(exe, &fresh).is_ok();
        if !made || std::fs::rename(&fresh, &path).is_err() {
            tracing::debug!("the shell wrapper {} was not renewed", path.display());
        }
        // Also after a rename that did nothing: two links to one file, when two engines start at once.
        crate::locked::clean_file(&fresh);
    }
    // An old one still running (Windows keeps it locked) is as good: the wrapper changes rarely.
    path.exists().then_some(path)
}

/// The prefix as Claude Code reads it: one program path. Git Bash on Windows takes forward slashes.
fn prefix(path: &Path) -> String {
    let p = path.display().to_string();
    if cfg!(windows) { p.replace('\\', "/") } else { p }
}

/// What one thread's engine needs to have its commands watched, and the watching.
#[derive(Clone)]
pub struct Live {
    thread: String,
    dir: PathBuf,
    wrapper: Option<PathBuf>,
    calls: Arc<Mutex<Calls>>,
}

/// The thread's command tool calls that are running, oldest first: id and command line.
#[derive(Default)]
struct Calls {
    running: Vec<(String, Option<String>)>,
}

impl Live {
    pub fn new(config: &Config, thread: &str) -> Self {
        let wrapper = if config.live_output { wrapper(&config.data_dir) } else { None };
        Self::with_wrapper(config, thread, wrapper)
    }

    /// With a given wrapper, or none; tests run one that is not this process.
    pub fn with_wrapper(config: &Config, thread: &str, wrapper: Option<PathBuf>) -> Self {
        let dir = config.data_dir.join("output").join(thread);
        if wrapper.is_some() {
            crate::locked::clean_dir(&dir);
            std::fs::create_dir_all(&dir).warn_on_err("create the output folder");
        }
        Self { thread: thread.to_owned(), dir, wrapper, calls: Arc::default() }
    }

    /// The engine's environment: the prefix and where to note output files.
    pub fn env(&self) -> Vec<(String, String)> {
        match &self.wrapper {
            // Claude Code splits a prefix at its last " -" into a program and a flag.
            Some(w) if prefix(w).contains(" -") => Vec::new(),
            Some(w) => vec![(PREFIX_VAR.into(), prefix(w)), (DIR_VAR.into(), self.dir.display().to_string())],
            None => Vec::new(),
        }
    }

    /// Follows the engine's tool calls: which commands are running.
    pub fn saw(&self, update: &Value) {
        if !matches!(crate::model::update_kind(update), Some("tool_call" | "tool_call_update")) {
            return;
        }
        let Some(id) = update["toolCallId"].as_str() else { return };
        let mut calls = self.calls.locked();
        let known = calls.running.iter().position(|(c, _)| c == id);
        let command = command_of(&update["rawInput"]);
        match update["status"].as_str() {
            Some("completed" | "failed") => {
                if let Some(i) = known {
                    calls.running.remove(i);
                }
            }
            _ => match known {
                Some(i) => {
                    if command.is_some() {
                        calls.running[i].1 = command;
                    }
                }
                None if update["kind"] == "execute" => calls.running.push((id.to_owned(), command)),
                None => {}
            },
        }
    }

    /// Reads the noted output files while the engine runs and hands what they grow by to `log`
    /// (the caller logs it as an `output` event). Ends when aborted (the engine stopped); the
    /// folder goes with it.
    pub async fn watch<F, Fut>(self, log: F)
    where
        F: Fn(OutputData) -> Fut,
        Fut: std::future::Future<Output = anyhow::Result<()>>,
    {
        if self.wrapper.is_none() {
            return;
        }
        let _gone = Cleanup(self.dir.clone());
        let mut files: HashMap<PathBuf, Followed> = HashMap::new();
        loop {
            tokio::time::sleep(Duration::from_millis(250)).await;
            let dir = self.dir.clone();
            let notes = tokio::task::spawn_blocking(move || read_notes(&dir)).await.unwrap_or_default();
            for (note, (script, output)) in notes {
                files.entry(note).or_insert_with(|| Followed { script, output, call: None, offset: 0, since: Instant::now(), done: false });
            }
            let mut taken: Vec<String> = files.values().filter_map(|f| f.call.clone()).collect();
            for (note, f) in files.iter_mut() {
                if f.done {
                    continue;
                }
                if f.call.is_none() {
                    f.call = self.claim(&f.script, &taken);
                    taken.extend(f.call.clone());
                    if f.call.is_none() {
                        if f.since.elapsed() > MATCH_WAIT {
                            f.done = true;
                            crate::locked::clean_file(note);
                        }
                        continue;
                    }
                }
                let call = f.call.clone().unwrap_or_default();
                let running = self.calls.locked().running.iter().any(|(c, _)| *c == call);
                let (path, from) = (f.output.clone(), f.offset);
                let read = tokio::task::spawn_blocking(move || read_from(&path, from)).await.ok().flatten();
                if let Some((text, upto)) = read {
                    f.offset = upto;
                    if !text.is_empty() && from < MAX_LIVE {
                        let data = OutputData { tool_call_id: call, data: text };
                        if let Err(err) = log(data).await {
                            tracing::warn!(thread = %self.thread, "live output not logged: {err:#}");
                        }
                    }
                }
                if !running {
                    f.done = true;
                    crate::locked::clean_file(note);
                }
            }
            files.retain(|_, f| !f.done || f.since.elapsed() < Duration::from_secs(60));
        }
    }

    /// The running command a script belongs to: the one whose command line it holds, else the
    /// only one, else the newest. Each call is claimed once.
    fn claim(&self, script: &str, taken: &[String]) -> Option<String> {
        let calls = self.calls.locked();
        let free: Vec<&(String, Option<String>)> = calls.running.iter().filter(|(id, _)| !taken.contains(id)).collect();
        let holds = |c: &str| script.contains(c) || script.contains(&c.replace('\'', r"'\''"));
        if let Some((id, _)) = free.iter().find(|(_, c)| c.as_deref().is_some_and(|c| !c.is_empty() && holds(c))) {
            return Some(id.clone());
        }
        free.last().map(|(id, _)| id.clone())
    }
}

struct Followed {
    script: String,
    output: PathBuf,
    call: Option<String>,
    offset: u64,
    since: Instant,
    done: bool,
}

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        crate::locked::clean_dir(&self.0);
    }
}

/// The notes in the folder: file, and the script and output file each names.
fn read_notes(dir: &Path) -> Vec<(PathBuf, (String, PathBuf))> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut notes: Vec<_> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .filter_map(|p| {
            let v: Value = serde_json::from_str(&std::fs::read_to_string(&p).ok()?).ok()?;
            let script = v["script"].as_str()?.to_owned();
            let output = PathBuf::from(v["output"].as_str()?);
            Some((p, (script, output)))
        })
        .collect();
    notes.sort_by(|a, b| a.0.cmp(&b.0));
    notes
}

/// What a file holds from `from` on, as text up to its last whole character, and where that ends.
fn read_from(path: &Path, from: u64) -> Option<(String, u64)> {
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    if len < from {
        // Rewritten from the start: read it again.
        return read_from_start(file);
    }
    if len == from {
        return Some((String::new(), from));
    }
    file.seek(SeekFrom::Start(from)).ok()?;
    let mut bytes = Vec::new();
    file.take((len - from).min(256 * 1024)).read_to_end(&mut bytes).ok()?;
    let whole = whole_chars(&bytes);
    Some((String::from_utf8_lossy(&bytes[..whole]).into_owned(), from + whole as u64))
}

fn read_from_start(mut file: std::fs::File) -> Option<(String, u64)> {
    let mut bytes = Vec::new();
    (&mut file).take(256 * 1024).read_to_end(&mut bytes).ok()?;
    let whole = whole_chars(&bytes);
    Some((String::from_utf8_lossy(&bytes[..whole]).into_owned(), whole as u64))
}

/// How many bytes end on a whole UTF-8 character: a character cut by the read waits for the next.
fn whole_chars(bytes: &[u8]) -> usize {
    match std::str::from_utf8(bytes) {
        Ok(_) => bytes.len(),
        Err(e) if e.error_len().is_none() => e.valid_up_to(),
        Err(_) => bytes.len(),
    }
}

fn command_of(input: &Value) -> Option<String> {
    match &input["command"] {
        Value::String(s) => Some(s.clone()),
        Value::Array(a) => Some(a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" ")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wrapper_is_renewed_in_one_step_and_leaves_nothing_beside_it() {
        let dir = tempfile::tempdir().unwrap();
        let (exe, bin) = (dir.path().join("hubd"), dir.path().join("bin"));
        std::fs::write(&exe, "one").unwrap();
        let path = renew(&exe, &bin).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "one");
        // A new hubd: the wrapper follows it, and the old file was replaced, not removed first.
        std::fs::remove_file(&exe).unwrap();
        std::fs::write(&exe, "two, longer").unwrap();
        assert_eq!(renew(&exe, &bin).unwrap(), path);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "two, longer");
        assert_eq!(std::fs::read_dir(&bin).unwrap().count(), 1, "no half-made wrapper is left");
        // It cannot be renewed (hubd is gone): the old one stays and is still the answer.
        std::fs::remove_file(&exe).unwrap();
        std::fs::write(bin.join(format!("{WRAPPER}.new{}", std::env::consts::EXE_SUFFIX)), "stale").unwrap();
        assert_eq!(renew(&exe, &bin).unwrap(), path);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "two, longer");
        assert_eq!(std::fs::read_dir(&bin).unwrap().count(), 1);
    }

    fn live() -> Live {
        Live { thread: "t".into(), dir: PathBuf::new(), wrapper: None, calls: Arc::default() }
    }

    #[test]
    fn a_script_is_matched_to_the_command_it_runs() {
        let live = live();
        live.saw(&json!({ "sessionUpdate": "tool_call", "toolCallId": "a", "kind": "execute", "status": "pending", "rawInput": { "command": "npm test" } }));
        live.saw(&json!({ "sessionUpdate": "tool_call", "toolCallId": "b", "kind": "execute", "status": "pending", "rawInput": { "command": "echo 'it''s'" } }));
        live.saw(&json!({ "sessionUpdate": "tool_call", "toolCallId": "r", "kind": "read", "status": "pending" }));
        let script = r"source snap && eval 'npm test' < /dev/null && pwd -P >| /tmp/cwd";
        assert_eq!(live.claim(script, &[]).as_deref(), Some("a"));
        let quoted = r"eval 'echo '\''it'\'''\''s'\''' && pwd -P >| /tmp/cwd";
        assert_eq!(live.claim(quoted, &[]).as_deref(), Some("b"));
        // Nothing matches: the newest running command that no file has yet.
        assert_eq!(live.claim("eval 'other' && pwd -P >| x", &["b".into()]).as_deref(), Some("a"));
        live.saw(&json!({ "sessionUpdate": "tool_call_update", "toolCallId": "a", "status": "completed" }));
        live.saw(&json!({ "sessionUpdate": "tool_call_update", "toolCallId": "b", "status": "failed" }));
        assert_eq!(live.claim(script, &[]), None);
    }

    #[test]
    fn a_read_stops_before_a_cut_character() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out");
        let mut bytes = "ok å".as_bytes().to_vec();
        bytes.pop();
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(read_from(&path, 0), Some(("ok ".into(), 3)));
        std::fs::write(&path, "ok å\n").unwrap();
        assert_eq!(read_from(&path, 3), Some(("å\n".into(), 6)));
        std::fs::write(&path, "new").unwrap();
        assert_eq!(read_from(&path, 6), Some(("new".into(), 3)));
    }
}

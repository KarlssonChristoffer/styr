//! The `where_am_i` tool: the machine, the project and the programs an agent has around it.

use super::*;

/// Programs worth knowing about before an agent tries to run them.
const PROGRAMS: &[&str] = &["git", "gh", "node", "npm", "cargo", "rustc", "python3", "python", "docker"];

/// Whether `name` is an executable on PATH (`.exe`, `.cmd` and `.bat` count too, for Windows).
fn on_path(name: &str, path: &std::ffi::OsStr) -> bool {
    let exts: &[&str] = if cfg!(windows) { &["", ".exe", ".cmd", ".bat"] } else { &[""] };
    std::env::split_paths(path).any(|dir| exts.iter().any(|e| dir.join(format!("{name}{e}")).is_file()))
}

/// Which of `PROGRAMS` are on PATH, in the order of the list.
fn programs_on(path: &std::ffi::OsStr) -> Vec<&'static str> {
    PROGRAMS.iter().copied().filter(|p| on_path(p, path)).collect()
}

/// The machine this runs on, the caller's thread and project, and the programs on PATH. Answers
/// "what machine is this, where am I and what can I run here?" in one call.
pub(super) async fn context_tool(hub: &Arc<Hub>, caller: &Caller, args: &Value) -> Result<Value> {
    let about = crate::host::about().await;
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut out = json!({
        "machine": {
            "name": about.hostname, "os": about.os, "arch": about.arch, "cpu": about.cpu,
            "cores": about.cores, "home": about.home, "disk_free_bytes": about.disk.as_ref().map(|d| d.free),
            "path_separator": std::path::MAIN_SEPARATOR.to_string(),
        },
        "programs_on_path": programs_on(&path),
        "other_machines": hub.config.hosts.iter().map(|h| h.name.clone()).collect::<Vec<_>>(),
    });
    let own = match caller {
        Caller::Thread(t) => Some(t.clone()),
        Caller::Agent { .. } => match args["thread_id"].as_str() {
            Some(id) => hub.store.thread(id).await?,
            None => None,
        },
    };
    if let Some(t) = own {
        let root = crate::thread_display::project_root(&t.cwd);
        let project = hub.store.projects().await?.into_iter().find(|p| same_folder(&p.path, &root));
        out["thread"] = json!({ "id": t.id, "title": t.title, "folder": t.cwd, "engine": t.engine, "host": t.host });
        out["project"] = json!({ "name": project.map(|p| p.name), "folder": root });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_programs_on_a_path_and_nothing_else() {
        let dir = std::env::temp_dir().join(format!("hub-context-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let name = if cfg!(windows) { "git.exe" } else { "git" };
        std::fs::write(dir.join(name), "").unwrap();
        let path = std::env::join_paths([&dir]).unwrap();
        assert_eq!(programs_on(&path), vec!["git"]);
        assert!(programs_on(std::ffi::OsStr::new("")).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

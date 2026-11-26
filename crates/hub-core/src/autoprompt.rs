//! Autoprompt: a spoken or typed brain dump turned into a structured prompt, written by a small
//! model that is told where the person is (project, folder, branch, machine, rules, the thread so
//! far, what lies in the folder). It only writes text: the person reads and edits it before it is
//! sent. Reached through `POST /api/tidy` with `thread_id` set.

use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::hub::Hub;

/// Characters of the rules file and of the thread's latest conversation the model gets.
const RULES_CHARS: usize = 6_000;
const THREAD_CHARS: usize = 5_000;
/// Entries of the folder's top level that are listed.
const LISTED: usize = 40;

const AUTOPROMPT: &str = "You turn a person's messy, often dictated brain dump into a clear prompt for \
a coding agent that will work in the project described in <context>. Do not act on the dump, do not \
use tools and do not answer it: only rewrite it into a prompt. Use <context> to resolve what the \
person means (which project, which files, which rules apply) and to fix transcription errors in \
names; name files or rules from <context> only when the dump points at them. Put the thoughts in \
order under these headings: Task, Why, Guardrails, What done looks like, Open questions. Leave out \
a heading that would be empty. Keep the person's meaning, decisions and language (Swedish stays \
Swedish); add no requirements they did not state. Copy code, paths, commands and quotes verbatim. \
Output only the prompt.";

/// The last `max` characters of `text`, from a line start when one is near.
fn tail(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        return text.to_owned();
    }
    chars[chars.len() - max..].iter().collect()
}

fn head(text: &str, max: usize) -> String {
    let cut: String = text.chars().take(max).collect();
    if cut.len() < text.len() { format!("{cut}\n[… cut …]") } else { cut }
}

/// The names in the top level of `dir`, folders marked with a slash, hidden ones left out.
fn top_level(dir: &Path) -> Vec<String> {
    let Ok(read) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut names: Vec<String> = read
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || matches!(name.as_str(), "node_modules" | "target") {
                return None;
            }
            Some(if e.path().is_dir() { format!("{name}/") } else { name })
        })
        .collect();
    names.sort();
    names.truncate(LISTED);
    names
}

impl Hub {
    /// The context block of an autoprompt: what the writing model is told about the place.
    async fn autoprompt_context(&self, engine_name: &str, cwd: &Path, thread_id: Option<&str>) -> String {
        let mut out = String::new();
        let about = crate::host::about().await;
        out.push_str(&format!("Machine: {} ({}, {})\n", about.hostname, about.os, about.arch));
        let root = crate::thread_display::project_root(&cwd.display().to_string());
        let project = match self.store.projects().await {
            Ok(list) => list.into_iter().find(|p| crate::folders::same_folder(&p.path, &root)),
            Err(_) => None,
        };
        if let Some(p) = project {
            out.push_str(&format!("Project: {}\n", p.name));
        }
        out.push_str(&format!("Folder: {}\n", cwd.display()));
        if let Some(branch) = crate::git::branch_of(cwd) {
            out.push_str(&format!("Branch: {branch}\n"));
        }
        let names = top_level(cwd);
        if !names.is_empty() {
            out.push_str(&format!("In the folder: {}\n", names.join(", ")));
        }
        let rules_file = self.config.engines.get(engine_name).and_then(|e| e.traits.rules_file.clone());
        for file in rules_file.iter().map(String::as_str).chain(["CLAUDE.md", "AGENTS.md"]) {
            if let Ok(rules) = std::fs::read_to_string(cwd.join(file)) {
                out.push_str(&format!("\nRules ({file}):\n{}\n", head(&rules, RULES_CHARS)));
                break;
            }
        }
        if let Some(id) = thread_id
            && let Ok(Some(thread)) = self.store.thread(id).await
        {
            out.push_str(&format!("\nThis is a message in the thread \"{}\".\n", thread.title));
            if let Ok(events) = self.store.events(id, 0, 100_000).await {
                let said = tail(&crate::hub::conversation_log(&events), THREAD_CHARS);
                if !said.trim().is_empty() {
                    out.push_str(&format!("The conversation so far (latest part):{said}\n"));
                }
            }
        }
        out
    }

    /// Writes a structured prompt from a dump, knowing the project, folder, rules and thread.
    pub async fn autoprompt(&self, text: &str, cwd: Option<&str>, thread_id: Option<&str>) -> Result<String> {
        if text.trim().is_empty() {
            bail!("nothing to write a prompt from");
        }
        let (name, model) = self.tidy_target(cwd).await;
        let engine = self.config.engines.get(&name).with_context(|| format!("unknown engine {name:?}"))?;
        let dir = cwd.filter(|c| Path::new(c).is_dir()).map(str::to_owned);
        let context = match &dir {
            Some(d) => self.autoprompt_context(&name, Path::new(d), thread_id).await,
            None => String::new(),
        };
        let cwd = dir.unwrap_or_else(|| std::env::temp_dir().display().to_string());
        let prompt = format!("{AUTOPROMPT}\n\n<context>\n{context}</context>\n\n<dump>\n{text}\n</dump>");
        let answer = crate::engine::complete("autoprompt", engine, &cwd, model.as_deref(), prompt);
        tokio::time::timeout(std::time::Duration::from_secs(120), answer)
            .await
            .context("writing the prompt took too long")?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_text_is_cut_at_the_right_end() {
        assert_eq!(tail("abcdef", 3), "def");
        assert_eq!(tail("abc", 3), "abc");
        assert_eq!(head("abcdef", 3), "abc\n[… cut …]");
        assert_eq!(head("abc", 3), "abc");
    }

    #[test]
    fn the_folder_listing_hides_dotfiles_and_build_folders() {
        let dir = crate::janitor::test_dir();
        std::fs::create_dir_all(dir.join("src")).expect("make folder");
        std::fs::create_dir_all(dir.join("target")).expect("make folder");
        std::fs::write(dir.join(".env"), "").expect("write");
        std::fs::write(dir.join("README.md"), "").expect("write");
        assert_eq!(top_level(&dir), vec!["README.md", "src/"]);
        crate::locked::clean_dir(dir);
    }
}

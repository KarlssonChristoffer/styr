//! Starting a new project: whether `gh` is ready, a repository name GitHub accepts, the first
//! commit pushed to a new GitHub repository, and the first message of the thread that sets the
//! project up (`setup_prompt`).

use super::*;

/// The GitHub CLI is installed and logged in.
pub async fn gh_ready() -> bool {
    gh_within(&std::env::temp_dir(), &["auth", "status"], Duration::from_secs(5)).await.is_ok()
}

/// A repository name GitHub accepts: letters, digits, `.`, `_` and `-`.
pub fn repo_name(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_owned();
    if out.is_empty() { "project".into() } else { out }
}

/// Makes a first commit and a GitHub repository for it (private unless `public`), and pushes. Returns its URL.
pub async fn create_github_repo(root: &Path, name: &str, public: bool) -> Result<String> {
    let dir = root.to_path_buf();
    let committed = tokio::task::spawn_blocking(move || {
        git::run(&dir, &["add", "-A"]).is_some() && git::run(&dir, &["commit", "-m", "Start the project"]).is_some()
    })
    .await?;
    let repo = repo_name(name);
    let mut args = vec!["repo", "create", repo.as_str(), if public { "--public" } else { "--private" }, "--source", ".", "--remote", "origin"];
    if committed {
        args.push("--push");
    }
    match gh_within(root, &args, Duration::from_secs(90)).await {
        Ok(out) => Ok(out
            .lines()
            .find(|l| l.contains("github.com/"))
            .map(|l| l.trim().to_owned())
            .unwrap_or_else(|| repo.clone())),
        Err(GhError::Missing) => bail!("the GitHub CLI (gh) is not installed"),
        Err(GhError::Failed(msg)) => bail!("{msg}"),
    }
}

pub use crate::projects_root::default_root;

/// The first message of the thread that sets a new project up.
pub fn setup_prompt(name: &str, about: Option<&str>, repo: Option<&str>, template: Option<&str>) -> String {
    let about = about.map(str::trim).filter(|a| !a.is_empty());
    let mut text = format!("This is a brand-new project, \"{name}\".");
    match about {
        Some(about) => text.push_str(&format!(" What it is, in my words:\n\n{about}\n")),
        None => text.push_str(" I have not said what it is yet."),
    }
    match template {
        Some(title) => text.push_str(&format!(
            "\n\nHub already wrote a starter from its \"{title}\" template: it builds, runs and passes its test. Set it up so we can start working:\n\
             1. Do not scaffold again and keep its stack. Install what it needs and run its tests once to see them pass.\n\
             2. Adapt it to what the project is: rename things, replace the example route or function with the first real piece, and keep a test for it. If you cannot tell what to build, ask me in one short message and wait.\n\
             3. Add `docs/ARCHITECTURE.md` with the stack and why, update README.md and `docs/plan.md`, and keep how to build, run and test in CLAUDE.md and AGENTS.md true.\n",
        )),
        None => text.push_str(
            "\n\nSet it up so we can start working:\n\
             1. If you cannot tell what to build or which stack fits, ask me in one short message and wait.\n\
             2. Pick a mainstream, well-supported stack for it, scaffold it with the official tools, and make it build and run.\n\
             3. Write `docs/plan.md` as a checklist of the first steps (tick what you did), `docs/ARCHITECTURE.md` with the stack and why, a README, and a .gitignore, and add how to build, run and test to CLAUDE.md.\n",
        ),
    }
    match repo {
        Some(repo) => text.push_str(&format!("4. Commit and push to the GitHub repository ({repo}).\n")),
        None => text.push_str("4. Commit the result.\n"),
    }
    text.push_str("\nAnswer in the language I write in.");
    text
}

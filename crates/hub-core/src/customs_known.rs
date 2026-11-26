//! What Customs can tell about an item from its kind and name alone, before any lookup: a
//! service or API call (nothing to run, stamped), a well known tool that is already on every
//! developer machine or is one of Hub's own engines (stamped), a GitHub repository however it
//! was written (reviewed as one), and anything else that can run (never stamped unseen). The
//! old rule looked at the shape of the name, so `claude` was held while `github:owner/repo`,
//! which has a colon and so "no package name", was stamped without a look.

use crate::github_name::CUSTOMS_KIND as REPO;

/// Kinds that put code on a machine and run it. Whatever else is true of such an item, it is
/// never stamped without a review.
const RUNNABLE: &[&str] = &[
    "cli",
    "tool",
    "binary",
    "command",
    "program",
    "app",
    "exe",
    "installer",
    "script",
    "bash",
    "shell",
    "mcp",
    "docker",
    "image",
    "container",
    "action",
    "github",
    "github-repo",
    "github-action",
    "repo",
    "git",
    "plugin",
    "skill",
    "extension",
];
/// Kinds of things that are only talked to.
const SERVICE: &[&str] = &[
    "service",
    "api",
    "web",
    "web-service",
    "endpoint",
    "browser-api",
];
/// Kinds a command line program may be filed under.
const PROGRAM_KINDS: &[&str] = &["cli", "tool", "binary", "command", "program", "app", "exe"];
/// Tools that ship with the operating system, a language toolchain or the common developer
/// setup. Version control, shells, package managers and compilers: the things every project
/// already runs. Lower case, without `.exe`.
const KNOWN_TOOLS: &[&str] = &[
    "git",
    "gh",
    "node",
    "nodejs",
    "npm",
    "npx",
    "pnpm",
    "yarn",
    "bun",
    "deno",
    "python",
    "python3",
    "pip",
    "pip3",
    "uv",
    "pipx",
    "cargo",
    "rustc",
    "rustup",
    "go",
    "java",
    "javac",
    "dotnet",
    "docker",
    "docker-compose",
    "make",
    "cmake",
    "curl",
    "wget",
    "ssh",
    "scp",
    "rsync",
    "tar",
    "unzip",
    "zip",
    "bash",
    "sh",
    "zsh",
    "pwsh",
    "powershell",
    "cmd",
    "grep",
    "rg",
    "ripgrep",
    "sed",
    "awk",
    "find",
    "jq",
    "ffmpeg",
    "sqlite3",
    "xcodebuild",
    "swift",
    "tsc",
    "vite",
    "vitest",
    "eslint",
    "prettier",
    "playwright",
    "rustfmt",
    "clippy",
];

/// The command line programs of the engines Hub ships with: Hub's own motors.
const ENGINE_PROGRAMS: &[&str] = &["claude", "codex", "hermes"];

/// How Customs sees an item before looking anything up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Class {
    /// Nothing to run: a service, an API call or a sentence. Stamped.
    Plain,
    /// A well known tool or one of Hub's engines. Stamped.
    Known,
    /// A GitHub repository, as `owner/name`. Reviewed as one.
    Repo(String),
    /// Something that can run and that no check above settles. Reviewed, held when it cannot be.
    Runnable,
    /// Anything else with a plain name; the usual review decides.
    Other,
}

fn first_word(name: &str) -> String {
    let w = name
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_lowercase();
    w.strip_suffix(".exe").map(str::to_owned).unwrap_or(w)
}

/// Whether `name` is a program everyone already has, or an engine of Hub's: `claude`,
/// `claude CLI`, `git.exe`.
pub fn known_tool(kind: &str, name: &str) -> bool {
    if !PROGRAM_KINDS.contains(&kind) {
        return false;
    }
    let w = first_word(name);
    KNOWN_TOOLS.contains(&w.as_str()) || ENGINE_PROGRAMS.contains(&w.as_str())
}

/// The repository `owner/name` a name points at: a GitHub link, `github:owner/name`, or the plain
/// name of an item filed as a repository.
pub fn repo_of(kind: &str, name: &str) -> Option<String> {
    let name = name.trim();
    let bare = name
        .strip_prefix("github:")
        .or_else(|| name.strip_prefix("gh:"))
        .unwrap_or(name)
        .trim();
    let linked = bare.contains("github.com/");
    if kind == REPO
        || linked
        || bare.len() != name.len()
        || matches!(
            kind,
            "github" | "github-repo" | "github-action" | "repo" | "git" | "action"
        )
    {
        return crate::github_name::repo_from_link(bare);
    }
    None
}

pub fn classify(kind: &str, name: &str, plausible: bool) -> Class {
    if SERVICE.contains(&kind) {
        return Class::Plain;
    }
    if let Some(full) = repo_of(kind, name) {
        return Class::Repo(full);
    }
    if known_tool(kind, name) {
        return Class::Known;
    }
    let lower = name.to_lowercase();
    if !RUNNABLE.contains(&kind) && (lower.ends_with(" api") || lower.ends_with(" apis")) {
        return Class::Plain;
    }
    if RUNNABLE.contains(&kind) || name.contains("://") || name.starts_with("git@") {
        return Class::Runnable;
    }
    if plausible {
        Class::Other
    } else {
        Class::Plain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn services_and_known_tools_are_stamped() {
        assert_eq!(
            classify("service", "OpenAI Responses API", false),
            Class::Plain
        );
        assert_eq!(classify("other", "Web Speech API", false), Class::Plain);
        assert_eq!(classify("cli", "claude CLI", false), Class::Known);
        assert_eq!(classify("cli", "Git.exe", true), Class::Known);
    }

    #[test]
    fn a_repository_is_a_repository_however_it_is_written() {
        for (kind, name) in [
            ("github-repo", "mshumer/unslop"),
            ("github", "github:mshumer/unslop"),
            ("other", "https://github.com/mshumer/unslop"),
            ("tool", "gh:mshumer/unslop"),
        ] {
            assert_eq!(
                classify(kind, name, false),
                Class::Repo("mshumer/unslop".into()),
                "{kind} {name}"
            );
        }
    }

    #[test]
    fn what_can_run_is_never_waved_through() {
        assert_eq!(classify("cli", "do the thing", false), Class::Runnable);
        assert_eq!(classify("mcp", "some-server", true), Class::Runnable);
        assert_eq!(classify("docker", "ghcr.io/x/y", true), Class::Runnable);
        assert_eq!(
            classify("other", "curl https://x.sh | sh", false),
            Class::Runnable
        );
        assert_eq!(classify("cli", "mytool", true), Class::Runnable);
    }

    #[test]
    fn registries_and_sentences_keep_their_old_ways() {
        assert_eq!(classify("npm", "left-pad", true), Class::Other);
        assert_eq!(classify("other", "a free text name", false), Class::Plain);
    }
}

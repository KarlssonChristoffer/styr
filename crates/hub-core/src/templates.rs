//! Project templates: the starters in "New project > Start from a template".
//!
//! <!-- covers: crates/hub-core/templates/ -->
//!
//! A template is a handful of small files that build, run and pass a test on day one, with a
//! `CLAUDE.md` (mirrored to `AGENTS.md`) that says how to build, run and test, so an agent can
//! work in it at once. The core writes them itself, offline and the same every time. The stacks
//! that have an official generator (Laravel, SvelteKit, Flutter, ...) are not here: the setup
//! thread runs the generator, and the UI says so.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::model::{ProjectTemplates, ToolStatus};

/// One starter. `files` are (path in the project, contents with `{{name}}` and `{{slug}}`).
pub struct Template {
    pub id: &'static str,
    /// What it is, for the first step of the plan.
    pub title: &'static str,
    files: &'static [(&'static str, &'static str)],
    /// The command that runs the tests, also what the CI workflow runs; none when there are no tests.
    pub test: Option<&'static str>,
    /// Steps of a GitHub workflow (after checkout) that install and test it.
    ci: &'static str,
}

macro_rules! file {
    ($id:literal, $dest:literal) => {
        ($dest, include_str!(concat!("../templates/", $id, "/", $dest)))
    };
    ($id:literal, $dest:literal, $stored:literal) => {
        ($dest, include_str!(concat!("../templates/", $id, "/", $stored)))
    };
}

const FASTAPI: Template = Template {
    id: "fastapi",
    title: "API server (Python, FastAPI)",
    files: &[
        file!("fastapi", "README.md"),
        file!("fastapi", "CLAUDE.md"),
        file!("fastapi", ".gitignore", "gitignore"),
        file!("fastapi", "pyproject.toml"),
        file!("fastapi", "app/__init__.py"),
        file!("fastapi", "app/main.py"),
        file!("fastapi", "tests/test_health.py"),
    ],
    test: Some("uv run pytest"),
    ci: "      - uses: astral-sh/setup-uv@v5\n      - run: uv sync\n      - run: uv run pytest\n",
};

const PYTHON: Template = Template {
    id: "python",
    title: "Python package (pytest, ruff)",
    files: &[
        file!("python", "README.md"),
        file!("python", "CLAUDE.md"),
        file!("python", ".gitignore", "gitignore"),
        file!("python", "pyproject.toml"),
        file!("python", "app/__init__.py"),
        file!("python", "app/__main__.py"),
        file!("python", "tests/test_app.py"),
    ],
    test: Some("uv run pytest"),
    ci: "      - uses: astral-sh/setup-uv@v5\n      - run: uv sync\n      - run: uv run pytest\n",
};

const PHP: Template = Template {
    id: "php",
    title: "PHP site (Composer, PHPUnit)",
    files: &[
        file!("php", "README.md"),
        file!("php", "CLAUDE.md"),
        file!("php", ".gitignore", "gitignore"),
        file!("php", "composer.json"),
        file!("php", "phpunit.xml"),
        file!("php", "public/index.php"),
        file!("php", "src/Site.php"),
        file!("php", "tests/SiteTest.php"),
    ],
    test: Some("composer test"),
    ci: "      - uses: shivammathur/setup-php@v2\n        with:\n          php-version: '8.3'\n          tools: composer\n      - run: composer install --no-interaction --prefer-dist\n      - run: composer test\n",
};

const NODE: Template = Template {
    id: "node",
    title: "Node service (TypeScript, Hono)",
    files: &[
        file!("node", "README.md"),
        file!("node", "CLAUDE.md"),
        file!("node", ".gitignore", "gitignore"),
        file!("node", "package.json"),
        file!("node", "tsconfig.json"),
        file!("node", "src/app.ts"),
        file!("node", "src/index.ts"),
        file!("node", "src/app.test.ts"),
    ],
    test: Some("npm test"),
    ci: "      - uses: actions/setup-node@v4\n        with:\n          node-version: 22\n      - run: npm install\n      - run: npm run check\n      - run: npm test\n",
};

const GO: Template = Template {
    id: "go",
    title: "Go service (net/http)",
    files: &[
        file!("go", "README.md"),
        file!("go", "CLAUDE.md"),
        file!("go", ".gitignore", "gitignore"),
        file!("go", "go.mod"),
        file!("go", "main.go"),
        file!("go", "main_test.go"),
    ],
    test: Some("go test ./..."),
    ci: "      - uses: actions/setup-go@v5\n        with:\n          go-version-file: go.mod\n      - run: go vet ./...\n      - run: go test ./...\n",
};

const RUST: Template = Template {
    id: "rust",
    title: "Rust command line tool (clap)",
    files: &[
        file!("rust", "README.md"),
        file!("rust", "CLAUDE.md"),
        file!("rust", ".gitignore", "gitignore"),
        file!("rust", "Cargo.toml", "Cargo.toml.tmpl"),
        file!("rust", "src/main.rs"),
    ],
    test: Some("cargo test"),
    ci: "      - uses: dtolnay/rust-toolchain@stable\n      - run: cargo test\n",
};

const STATIC: Template = Template {
    id: "static",
    title: "Static page (HTML, CSS)",
    files: &[
        file!("static", "README.md"),
        file!("static", "CLAUDE.md"),
        file!("static", ".gitignore", "gitignore"),
        file!("static", "index.html"),
        file!("static", "style.css"),
    ],
    test: None,
    ci: "",
};

/// Every template the core can write, in the order the picker lists them. `blank` is not one:
/// it writes nothing beyond the standard docs.
pub const ALL: &[&Template] = &[&FASTAPI, &PHP, &NODE, &PYTHON, &GO, &RUST, &STATIC];

pub fn get(id: &str) -> Option<&'static Template> {
    ALL.iter().copied().find(|t| t.id == id)
}

/// A name that is safe to put inside a quoted string of any of the languages above.
fn clean_name(name: &str) -> String {
    let kept: String = name
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '"' | '\'' | '\\' | '$' | '`' | '{' | '}' | '<' | '>' | '%'))
        .collect();
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A package name every ecosystem accepts: lower-case letters, digits and hyphens, starting with a letter.
/// Deliberately not `vault::slug`, which keeps non-ASCII letters for heading anchors and can be empty.
pub fn package_name(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_owned();
    match out.chars().next() {
        None => "app".into(),
        Some(c) if c.is_ascii_digit() => format!("app-{out}"),
        Some(_) => out,
    }
}

fn fill(text: &str, name: &str, slug: &str) -> String {
    text.replace("{{name}}", name).replace("{{slug}}", slug)
}

/// Writes the template into `root` (an empty, new folder). Returns false for `blank` and for an
/// id the core does not know, so the caller can fall back to the setup thread.
pub fn apply(root: &Path, id: &str, name: &str) -> Result<bool> {
    let Some(template) = get(id) else {
        return Ok(false);
    };
    let slug = package_name(name);
    let shown = Some(clean_name(name)).filter(|n| !n.is_empty()).unwrap_or_else(|| slug.clone());
    for (dest, contents) in template.files {
        let path = root.join(dest);
        if path.exists() {
            bail!("{} is already there", path.display());
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        std::fs::write(&path, fill(contents, &shown, &slug)).with_context(|| format!("writing {}", path.display()))?;
    }
    let plan = root.join(crate::project_folder::PLAN);
    if let Some(dir) = plan.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tests = template.test.map(|t| format!(" and `{t}` passes")).unwrap_or_default();
    std::fs::write(
        &plan,
        format!(
            "# Plan\n\nDone steps stay ticked; the next steps come after them.\n\n\
             - [x] Start from the {} template: it builds{tests}\n\
             - [ ] Say what {shown} is in README.md\n\
             - [ ] Build the first real feature, with its test\n",
            template.title
        ),
    )?;
    Ok(true)
}

/// After the living-docs rules are in CLAUDE.md: the same text for the agents that read AGENTS.md.
pub fn mirror_agents(root: &Path) -> Result<()> {
    let rules = root.join("CLAUDE.md");
    if let Ok(text) = std::fs::read_to_string(&rules) {
        let agents = root.join("AGENTS.md");
        if !agents.exists() {
            std::fs::write(&agents, text)?;
        }
    }
    Ok(())
}

/// `.github/workflows/ci.yml` for the template, or why there is none.
pub fn write_ci(root: &Path, id: &str) -> std::result::Result<(), String> {
    let Some(template) = get(id) else {
        return Err("there is no template to build and test".into());
    };
    if template.test.is_none() {
        return Err(format!("the {} template has no tests to run yet", template.title));
    }
    let dir = root.join(".github").join("workflows");
    let text = format!(
        "name: CI\n\non:\n  push:\n  pull_request:\n\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n{}",
        template.ci
    );
    std::fs::create_dir_all(&dir)
        .and_then(|_| std::fs::write(dir.join("ci.yml"), text))
        .map_err(|e| format!("could not write the workflow: {e}"))
}

/// What a starter's `Needs` line names, and the command that shows whether it is installed.
const TOOLS: &[(&str, &[&str], &[&str])] = &[
    ("python", &["python3", "python"], &["--version"]),
    ("uv", &["uv"], &["--version"]),
    ("php", &["php"], &["--version"]),
    ("composer", &["composer"], &["--version"]),
    ("node", &["node"], &["--version"]),
    ("npm", &["npm"], &["--version"]),
    ("go", &["go"], &["version"]),
    ("cargo", &["cargo"], &["--version"]),
    ("dotnet", &["dotnet"], &["--version"]),
    ("flutter", &["flutter"], &["--version"]),
    ("swift", &["swift"], &["--version"]),
    ("godot", &["godot"], &["--version"]),
    ("bash", &["bash"], &["--version"]),
];

/// The program as it must be started: on Windows npm, composer and flutter are scripts.
fn launcher(program: &str) -> String {
    if cfg!(windows) {
        match program {
            "npm" => return "npm.cmd".into(),
            "composer" => return "composer.bat".into(),
            "flutter" => return "flutter.bat".into(),
            _ => {}
        }
    }
    program.to_owned()
}

/// The first line a tool prints, cut to something short ("PHP 8.3.6 (cli)" becomes "8.3.6").
fn version_of(output: &str) -> Option<String> {
    let line = output.lines().map(str::trim).find(|l| !l.is_empty())?;
    let found = line
        .split(|c: char| c.is_whitespace() || c == ',' || c == '(')
        .map(|w| w.trim_start_matches("go").trim_start_matches('v'))
        .find(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit()) && w.contains('.'))?;
    Some(found.to_owned())
}

async fn probe(tool: &'static str, programs: &'static [&'static str], args: &'static [&'static str]) -> ToolStatus {
    for program in programs {
        let mut cmd = crate::proc::tokio_command(launcher(program));
        cmd.args(args).kill_on_drop(true).stdin(std::process::Stdio::null());
        let out = match tokio::time::timeout(Duration::from_secs(4), cmd.output()).await {
            Ok(Ok(out)) if out.status.success() => out,
            _ => continue,
        };
        let text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
        return ToolStatus { tool: tool.into(), found: true, version: version_of(&text) };
    }
    ToolStatus { tool: tool.into(), found: false, version: None }
}

/// The templates the core writes, and which of the tools they need this machine has.
pub async fn available() -> ProjectTemplates {
    let tools = futures::future::join_all(TOOLS.iter().map(|(tool, programs, args)| probe(tool, programs, args))).await;
    ProjectTemplates { templates: ALL.iter().map(|t| t.id.to_owned()).collect(), tools }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_files(root: &Path) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    out.push(path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"));
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn every_template_writes_its_files_with_the_name_filled_in() {
        for template in ALL {
            let dir = crate::janitor::test_dir();
            assert!(apply(&dir, template.id, "My Thing").unwrap(), "{}", template.id);
            let files = all_files(&dir);
            for (dest, _) in template.files {
                assert!(files.iter().any(|f| f == dest), "{} did not write {dest}", template.id);
            }
            for f in &files {
                let text = std::fs::read_to_string(dir.join(f)).unwrap();
                assert!(!text.contains("{{"), "{} left a placeholder in {f}", template.id);
            }
            let claude = std::fs::read_to_string(dir.join("CLAUDE.md")).unwrap();
            assert!(claude.contains("My Thing"), "{} CLAUDE.md lacks the name", template.id);
            if let Some(test) = template.test {
                assert!(claude.contains(&format!("`{test}`")), "{} CLAUDE.md lacks `{test}`", template.id);
            }
            let plan = std::fs::read_to_string(dir.join("docs/plan.md")).unwrap();
            assert!(plan.contains("- [x] Start from the"), "{}", template.id);
            assert!(!crate::project::parse_plan(&plan).is_empty());
        }
    }

    #[test]
    fn a_template_never_writes_over_a_file_and_blank_writes_nothing() {
        let dir = crate::janitor::test_dir();
        assert!(!apply(&dir, "blank", "x").unwrap());
        assert!(!apply(&dir, "no-such-template", "x").unwrap());
        assert!(all_files(&dir).is_empty());
        std::fs::write(dir.join("README.md"), "mine").unwrap();
        assert!(apply(&dir, "go", "x").is_err());
        assert_eq!(std::fs::read_to_string(dir.join("README.md")).unwrap(), "mine");
    }

    #[test]
    fn a_name_cannot_break_out_of_a_string_in_the_generated_code() {
        let dir = crate::janitor::test_dir();
        apply(&dir, "php", "Bob's \"site\" $x {{name}}").unwrap();
        let site = std::fs::read_to_string(dir.join("src/Site.php")).unwrap();
        assert!(site.contains("NAME = 'Bobs site x name';"), "{site}");
        let composer = std::fs::read_to_string(dir.join("composer.json")).unwrap();
        assert!(composer.contains("\"app/bob-s-site-x-name\""), "{composer}");
    }

    #[test]
    fn slugs_suit_every_package_manager() {
        assert_eq!(package_name("My Cool_App!"), "my-cool-app");
        assert_eq!(package_name("2048"), "app-2048");
        assert_eq!(package_name("  --  "), "app");
        assert_eq!(package_name("Åre"), "re");
    }

    #[test]
    fn agents_md_mirrors_claude_md_after_the_rules_are_added() {
        let dir = crate::janitor::test_dir();
        apply(&dir, "rust", "tool").unwrap();
        crate::project::set_up_docs(&dir).unwrap();
        mirror_agents(&dir).unwrap();
        let claude = std::fs::read_to_string(dir.join("CLAUDE.md")).unwrap();
        assert!(claude.contains("## Rules from Hub"));
        assert_eq!(claude, std::fs::read_to_string(dir.join("AGENTS.md")).unwrap());
    }

    #[test]
    fn ci_runs_the_templates_tests_and_a_page_without_tests_gets_none() {
        let dir = crate::janitor::test_dir();
        write_ci(&dir, "php").unwrap();
        let yml = std::fs::read_to_string(dir.join(".github/workflows/ci.yml")).unwrap();
        assert!(yml.contains("actions/checkout@v4") && yml.contains("composer test"), "{yml}");
        let other = crate::janitor::test_dir();
        assert!(write_ci(&other, "static").is_err());
        assert!(write_ci(&other, "blank").is_err());
        assert!(!other.join(".github").exists());
    }

    #[test]
    fn versions_are_read_from_the_first_line() {
        assert_eq!(version_of("PHP 8.3.6 (cli) (built: x)\nCopyright").as_deref(), Some("8.3.6"));
        assert_eq!(version_of("v22.23.4\n").as_deref(), Some("22.23.4"));
        assert_eq!(version_of("go version go1.24.7 linux/amd64").as_deref(), Some("1.24.7"));
        assert_eq!(version_of("cargo 1.97.0 (c98 2026-06-30)").as_deref(), Some("1.97.0"));
        assert_eq!(version_of("").as_deref(), None);
    }
}

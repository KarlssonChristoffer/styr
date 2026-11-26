//! Plain facts about a project folder, read from its files and nothing of Hub: where its plan is
//! (`PLAN`), a path that stays inside it (`inside`), the stack its manifests name (`stack`), a
//! markdown checkbox line (`checkbox`) and a rule added to its CLAUDE.md (`add_rule`).
//! Low on purpose: a module that needs only these does not have to name `project`, which sits in
//! the dependency ring (`tests/it/frozen/ring.txt`).

use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};

/// The plan, relative to the project folder.
pub const PLAN: &str = "docs/plan.md";

/// A path inside the project, refusing anything that climbs out of it.
pub(crate) fn inside(root: &Path, rel: &str) -> Result<PathBuf> {
    let rel = Path::new(rel);
    if rel.components().any(|c| !matches!(c, Component::Normal(_) | Component::CurDir)) {
        bail!("{} is outside the project", rel.display());
    }
    Ok(root.join(rel))
}

/// Languages and frameworks named by the manifests in the folder and its direct subfolders.
pub fn stack(root: &Path) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut add = |name: &str| {
        if !found.iter().any(|f| f == name) {
            found.push(name.to_owned());
        }
    };
    let mut dirs = vec![root.to_path_buf()];
    if let Ok(entries) = std::fs::read_dir(root) {
        let mut subs: Vec<PathBuf> = entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.path())
            .filter(|p| {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                !name.starts_with('.') && !SKIP_DIRS.contains(&name.as_str())
            })
            .collect();
        subs.sort();
        dirs.extend(subs);
    }
    for dir in &dirs {
        let read = |name: &str| std::fs::read_to_string(dir.join(name)).ok();
        if let Some(text) = read("Cargo.toml") {
            add("Rust");
            for (dep, label) in CARGO_DEPS {
                if declares(&text, dep) {
                    add(label);
                }
            }
        }
        if let Some(text) = read("package.json") {
            let deps = package_deps(&text);
            if deps.iter().any(|d| d == "typescript") || dir.join("tsconfig.json").is_file() {
                add("TypeScript");
            } else {
                add("JavaScript");
            }
            for (dep, label) in NPM_DEPS {
                if deps.iter().any(|d| d == dep) {
                    add(label);
                }
            }
        }
        let python = [read("pyproject.toml"), read("requirements.txt"), read("setup.py")];
        if python.iter().any(Option::is_some) {
            add("Python");
            let text = python.into_iter().flatten().collect::<String>().to_lowercase();
            for (dep, label) in PY_DEPS {
                if text.contains(dep) {
                    add(label);
                }
            }
        }
        if dir.join("go.mod").is_file() {
            add("Go");
        }
        if dir.join("project.godot").is_file() {
            add("Godot");
        }
        if dir.join("pom.xml").is_file() || dir.join("build.gradle").is_file() || dir.join("build.gradle.kts").is_file() {
            add("JVM");
        }
        if has_extension(dir, &["csproj", "sln", "fsproj"]) {
            add(".NET");
        }
        if dir.join("Dockerfile").is_file()
            || ["docker-compose.yml", "docker-compose.yaml", "compose.yml", "compose.yaml"]
                .iter()
                .any(|f| dir.join(f).is_file())
        {
            add("Docker");
        }
    }
    found
}

const SKIP_DIRS: &[&str] = &["node_modules", "target", "dist", "build", "vendor", "venv", "__pycache__"];

const CARGO_DEPS: &[(&str, &str)] = &[
    ("tauri", "Tauri"),
    ("axum", "axum"),
    ("actix-web", "Actix"),
    ("tokio", "Tokio"),
    ("bevy", "Bevy"),
    ("rusqlite", "SQLite"),
    ("sqlx", "sqlx"),
    ("leptos", "Leptos"),
];

const NPM_DEPS: &[(&str, &str)] = &[
    ("svelte", "Svelte"),
    ("react", "React"),
    ("vue", "Vue"),
    ("next", "Next.js"),
    ("astro", "Astro"),
    ("vite", "Vite"),
    ("tailwindcss", "Tailwind"),
    ("@tauri-apps/api", "Tauri"),
    ("electron", "Electron"),
    ("express", "Express"),
    ("discord.js", "discord.js"),
    ("prisma", "Prisma"),
];

const PY_DEPS: &[(&str, &str)] = &[
    ("fastapi", "FastAPI"),
    ("django", "Django"),
    ("flask", "Flask"),
    ("torch", "PyTorch"),
    ("discord.py", "discord.py"),
];

/// True when a Cargo manifest lists `dep` as a dependency, in any of the usual spellings.
fn declares(manifest: &str, dep: &str) -> bool {
    manifest.lines().any(|line| {
        let line = line.trim_start();
        line.strip_prefix(dep)
            .is_some_and(|rest| rest.trim_start().starts_with('=') || rest.starts_with(".workspace"))
    })
}

fn package_deps(text: &str) -> Vec<String> {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else { return Vec::new() };
    ["dependencies", "devDependencies"]
        .iter()
        .filter_map(|key| json.get(key)?.as_object())
        .flat_map(|deps| deps.keys().cloned())
        .collect()
}

fn has_extension(dir: &Path, exts: &[&str]) -> bool {
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().any(|e| {
            e.path().extension().is_some_and(|x| exts.iter().any(|ext| x.eq_ignore_ascii_case(ext)))
        })
    })
}

/// A markdown checkbox line (`- [ ] text`, `* [x] text`): whether it is ticked, and its text.
pub(crate) fn checkbox(line: &str) -> Option<(bool, &str)> {
    let rest = line.trim_start().strip_prefix(['-', '*', '+'])?.strip_prefix(' ')?;
    let (done, rest) = if let Some(r) = rest.strip_prefix("[ ]") {
        (false, r)
    } else {
        (true, rest.strip_prefix("[x]").or_else(|| rest.strip_prefix("[X]"))?)
    };
    let text = rest.trim();
    (!text.is_empty()).then_some((done, text))
}

const RULES_HEADING: &str = "## Rules from Hub";

/// Adds `rule` as the last line under `## Rules from Hub` in the CLAUDE.md at `file`, creating the
/// section or the file when missing. A rule that is already there is left alone.
pub(crate) fn add_rule(file: &Path, rule: &str) -> Result<()> {
    let line = format!("- {rule}\n");
    let mut body = std::fs::read_to_string(file).unwrap_or_default();
    if body.lines().any(|l| l.trim() == line.trim()) {
        return Ok(());
    }
    match body.find(RULES_HEADING) {
        Some(at) => {
            // Last in the section: before the next heading, or at the end.
            let from = at + RULES_HEADING.len();
            let end = body[from..].find("\n#").map(|i| from + i + 1).unwrap_or(body.len());
            let mut head = body[..end].trim_end().to_owned();
            head.push('\n');
            head.push_str(&line);
            let tail = &body[end..];
            body = if tail.is_empty() { head } else { format!("{head}\n{tail}") };
        }
        None => {
            if !body.is_empty() {
                body = format!("{}\n\n", body.trim_end());
            }
            body.push_str(RULES_HEADING);
            body.push_str("\n\n");
            body.push_str(&line);
        }
    }
    std::fs::write(file, body).with_context(|| format!("writing {}", file.display()))
}

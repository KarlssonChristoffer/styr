//! `npx -y <package>@<version>` without npx. An engine's command is often that line, and every
//! start then pays for npm finding the package it already has (0.7 s on Linux, about 2 s of a
//! 3.3 s start on Christoffer's PC, 2026-11-10; nearly every other turn starts an engine). Once
//! npx has installed the package, its program is in npm's cache, and `direct` gives the line that
//! runs it with `node` straight away. Before that, or when anything looks off, the line stays as
//! it is and npx does its work.

use std::path::{Path, PathBuf};

/// `words` as a line that runs the package's program directly, when `words` is
/// `npx -y <package>@<version> [args]` and npx has that package in its cache. None otherwise.
pub fn direct(words: &[String]) -> Option<Vec<String>> {
    direct_in(words, &cache()?)
}

/// npm's cache folder: `npm_config_cache`, else where npm keeps it on this system. Read from
/// Hub's own environment; a cache set elsewhere (`.npmrc`, an engine's own variables) is not
/// found, and npx starts the engine as before.
fn cache() -> Option<PathBuf> {
    if let Some(set) = std::env::var_os("npm_config_cache").filter(|v| !v.is_empty()) {
        return Some(set.into());
    }
    if cfg!(windows) {
        return std::env::var_os("LOCALAPPDATA").map(|local| PathBuf::from(local).join("npm-cache"));
    }
    crate::home_files::home_dir().map(|home| home.join(".npm"))
}

/// `direct` with npm's cache at `cache`.
pub(crate) fn direct_in(words: &[String], cache: &Path) -> Option<Vec<String>> {
    let [program, yes, spec, rest @ ..] = words else { return None };
    let stem = Path::new(program).file_stem()?.to_str()?.to_lowercase();
    if stem != "npx" || !matches!(yes.as_str(), "-y" | "--yes") || spec.starts_with('-') {
        return None;
    }
    // "@scope/name@1.2": the version is after the last `@`, which is not the first character.
    let at = spec.rfind('@').filter(|i| *i > 0)?;
    let (name, version) = (&spec[..at], &spec[at + 1..]);
    if version.is_empty() || name.contains("..") {
        return None;
    }
    // npx keeps one folder per asked package, with what it asked for in its package.json (written
    // when the install is done). The installed version has to be the asked one or a later part
    // of it ("0.99" has 0.99.3, never a pre-release); of several folders the highest wins.
    let (_, bin) = std::fs::read_dir(cache.join("_npx"))
        .ok()?
        .flatten()
        .filter_map(|dir| {
            crate::home_files::read_json(&dir.path().join("package.json"))?["dependencies"].get(name)?;
            let package = dir.path().join("node_modules").join(name);
            let manifest = crate::home_files::read_json(&package.join("package.json"))?;
            let has = manifest["version"].as_str()?;
            if has != version && !(has.starts_with(&format!("{version}.")) && !has.contains('-')) {
                return None;
            }
            let bin = match &manifest["bin"] {
                serde_json::Value::String(one) => one.clone(),
                serde_json::Value::Object(many) => {
                    let own = name.rsplit('/').next().unwrap_or(name);
                    many.get(own).or_else(|| many.values().next())?.as_str()?.to_owned()
                }
                _ => return None,
            };
            let bin = package.join(bin);
            // Only a JavaScript program runs with `node`; anything else is npx's to start.
            let script = bin.extension().is_some_and(|e| matches!(e.to_str(), Some("js" | "mjs" | "cjs")));
            let parts: Vec<u64> = has.split('.').map(|p| p.parse().unwrap_or(0)).collect();
            (script && bin.is_file()).then_some((parts, bin))
        })
        .max_by(|a, b| a.0.cmp(&b.0))?;
    let mut line = vec![node(program)?, bin.to_str()?.to_owned()];
    line.extend(rest.iter().cloned());
    Some(line)
}

/// The Node that npx itself runs with: the one beside it when npx is named by its path (its
/// folder may not be on the PATH, and another Node may be), else the one Hub finds (`proc::find_program`).
fn node(npx: &str) -> Option<String> {
    let beside = Path::new(npx).parent().filter(|dir| !dir.as_os_str().is_empty()).map(|dir| dir.join(format!("node{}", std::env::consts::EXE_SUFFIX)));
    match beside {
        Some(node) if node.is_file() => Some(node.to_str()?.to_owned()),
        Some(_) => None,
        None => crate::proc::find_program("node")?.to_str().map(str::to_owned),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split(' ').map(str::to_owned).collect()
    }

    #[test]
    fn a_package_npx_already_has_runs_with_node_and_anything_else_stays_with_npx() {
        let cache = tempfile::tempdir().unwrap();
        // npx named by its path, as when Node was installed after Hub started: its own Node is used.
        let tools = tempfile::tempdir().unwrap();
        let npx = tools.path().join("npx.cmd").to_string_lossy().into_owned();
        let node = tools.path().join(format!("node{}", std::env::consts::EXE_SUFFIX));
        let line = vec![npx.clone(), "-y".to_owned(), "@acp/agent@0.99".to_owned(), "--flag".to_owned()];
        let with = |spec: &str| vec![npx.clone(), "-y".to_owned(), spec.to_owned()];
        // The program as a path: npm names it with `/` (`@acp/agent`, `dist/index.js`), which
        // Windows takes as `\`, so the line holds `…\@acp/agent\dist/index.js` there. Node runs it.
        let direct = |line: &[String]| direct_in(line, cache.path()).map(|l| (l[0].clone(), PathBuf::from(&l[1]), l[2..].to_vec()));

        let install = |folder: &str, version: &str, bin: &str| {
            let dir = cache.path().join("_npx").join(folder);
            let package = dir.join("node_modules").join("@acp").join("agent");
            std::fs::create_dir_all(package.join("dist")).unwrap();
            std::fs::write(dir.join("package.json"), r#"{ "dependencies": { "@acp/agent": "^0.99.1" } }"#).unwrap();
            std::fs::write(package.join("package.json"), format!(r#"{{ "version": "{version}", "bin": {bin} }}"#)).unwrap();
            std::fs::write(package.join("dist").join("index.js"), "").unwrap();
            package.join("dist").join("index.js")
        };
        assert_eq!(direct_in(&line, cache.path()), None, "nothing installed yet");
        let old = install("abc", "0.99.3", r#"{ "agent": "dist/index.js" }"#);
        assert_eq!(direct_in(&line, cache.path()), None, "no Node beside npx: npx knows how to start");
        std::fs::write(&node, "").unwrap();
        let node = node.to_string_lossy().into_owned();
        let flag = vec!["--flag".to_owned()];
        assert_eq!(direct(&line), Some((node.clone(), old, flag.clone())));

        let new = install("def", "0.99.10", r#""dist/index.js""#);
        install("pre", "0.99.11-beta.1", r#""dist/index.js""#);
        assert_eq!(direct(&line), Some((node.clone(), new, flag)), "the highest release of several");
        assert_eq!(direct_in(&with("@acp/agent@1.0"), cache.path()), None, "another version was asked");
        assert_eq!(direct_in(&with("@acp/agent"), cache.path()), None, "no version: npx looks for the latest");
        assert_eq!(direct_in(&[npx.clone(), "@acp/agent@0.99".to_owned()], cache.path()), None, "not the line Hub knows");
        assert_eq!(direct_in(&words("agent --acp"), cache.path()), None);
    }
}

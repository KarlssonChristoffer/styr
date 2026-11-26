//! Finds files in a folder for @-mentions, skipping what git ignores.

use std::path::Path;

use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct FileMatch {
    /// Relative to the searched folder, with forward slashes.
    pub rel: String,
    pub path: String,
    pub dir: bool,
}

/// Visits at most this many entries, so a huge folder answers quickly with what it found.
const MAX_VISITED: usize = 50_000;

/// Paths whose relative path contains every word of `query`, best matches first: a hit in the
/// file name beats a hit in the folders, and shorter paths beat longer ones.
pub fn search(root: &Path, query: &str, limit: usize) -> Vec<FileMatch> {
    let words: Vec<String> = query.to_lowercase().split_whitespace().map(str::to_owned).collect();
    let mut found: Vec<(u32, FileMatch)> = Vec::new();
    let walk = ignore::WalkBuilder::new(root).hidden(true).max_depth(Some(12)).build();
    for entry in walk.flatten().skip(1).take(MAX_VISITED) {
        let Ok(rel) = entry.path().strip_prefix(root) else { continue };
        let rel = rel.to_string_lossy().replace('\\', "/");
        let lower = rel.to_lowercase();
        if !words.iter().all(|w| lower.contains(w.as_str())) {
            continue;
        }
        let name = lower.rsplit('/').next().unwrap_or(&lower);
        let in_name = words.iter().filter(|w| name.contains(w.as_str())).count() as u32;
        let score = (words.len() as u32 - in_name) * 1000 + rel.len() as u32;
        let dir = entry.file_type().is_some_and(|t| t.is_dir());
        found.push((score, FileMatch { rel, path: entry.path().display().to_string(), dir }));
    }
    found.sort_by_key(|(score, m)| (*score, m.rel.clone()));
    found.into_iter().take(limit).map(|(_, m)| m).collect()
}

/// What a path named by a destructive command holds.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Reach {
    /// The path as the command names it.
    pub target: String,
    pub exists: bool,
    pub dir: bool,
    /// Files under it, counting ignored ones too, since a delete does not skip them.
    #[ts(type = "number")]
    pub files: u64,
    /// True when counting stopped early.
    pub more: bool,
    /// A few of the files, relative to the thread's folder.
    pub sample: Vec<String>,
}

const MAX_REACH: u64 = 20_000;

/// Counts what each target holds, resolved against `root`. A `*` in the last part of a target
/// matches names in its folder.
pub fn reach(root: &Path, targets: &[String]) -> Vec<Reach> {
    targets
        .iter()
        .map(|target| {
            let full = root.join(target.trim_matches(|c| c == '"' || c == '\''));
            let paths: Vec<std::path::PathBuf> = match full.file_name().and_then(|n| n.to_str()) {
                Some(name) if name.contains('*') => {
                    let parent = full.parent().unwrap_or(root);
                    std::fs::read_dir(parent)
                        .map(|rd| {
                            rd.flatten()
                                .filter(|e| wildcard(name, &e.file_name().to_string_lossy()))
                                .map(|e| e.path())
                                .collect()
                        })
                        .unwrap_or_default()
                }
                _ => vec![full],
            };
            let mut r = Reach {
                target: target.clone(),
                exists: false,
                dir: false,
                files: 0,
                more: false,
                sample: Vec::new(),
            };
            for path in paths.iter().filter(|p| p.exists()) {
                r.exists = true;
                r.dir |= path.is_dir();
                let walk = ignore::WalkBuilder::new(path).standard_filters(false).build();
                for entry in walk.flatten() {
                    if !entry.file_type().is_some_and(|t| t.is_file()) {
                        continue;
                    }
                    if r.files >= MAX_REACH {
                        r.more = true;
                        break;
                    }
                    r.files += 1;
                    if r.sample.len() < 8 {
                        let rel = entry.path().strip_prefix(root).unwrap_or(entry.path());
                        r.sample.push(rel.to_string_lossy().replace('\\', "/"));
                    }
                }
            }
            r
        })
        .collect()
}

fn wildcard(pattern: &str, name: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !name.starts_with(first) || name.len() < first.len() + last.len() || !name.ends_with(last) {
        return false;
    }
    let mut rest = &name[first.len()..name.len() - last.len()];
    for part in &parts[1..parts.len() - 1] {
        match rest.find(part) {
            Some(i) => rest = &rest[i + part.len()..],
            None => return false,
        }
    }
    true
}

/// Diffs larger than this are cut, so one attachment cannot flood the context.
const MAX_DIFF: usize = 200_000;

/// The uncommitted changes in `root` as one patch: tracked changes against HEAD, then the names
/// of new files. None when the folder is not in a git repository or nothing changed.
pub fn uncommitted(root: &Path) -> Option<String> {
    let git = |args: &[&str]| crate::git::run(root, args);
    // A repository without commits has no HEAD; its changes are all new files.
    let mut patch = git(&["diff", "HEAD", "--no-color"]).unwrap_or_default();
    let untracked = git(&["ls-files", "--others", "--exclude-standard"])?;
    if !untracked.trim().is_empty() {
        patch.push_str("\n# New files, not yet tracked:\n");
        for name in untracked.lines() {
            patch.push_str("#   ");
            patch.push_str(name);
            patch.push('\n');
        }
    }
    if patch.trim().is_empty() {
        return None;
    }
    if patch.len() > MAX_DIFF {
        let mut cut = MAX_DIFF;
        while !patch.is_char_boundary(cut) {
            cut -= 1;
        }
        patch.truncate(cut);
        patch.push_str("\n# … cut, the diff is larger than 200 KB\n");
    }
    Some(patch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reach_counts_files_including_ignored_ones() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("build/deep")).unwrap();
        std::fs::write(root.join("build/a.o"), "").unwrap();
        std::fs::write(root.join("build/deep/b.o"), "").unwrap();
        std::fs::write(root.join("x.log"), "").unwrap();
        std::fs::write(root.join("y.log"), "").unwrap();
        std::fs::write(root.join(".gitignore"), "build\n").unwrap();
        std::fs::create_dir(root.join(".git")).unwrap();

        let r = reach(root, &["build".into(), "*.log".into(), "nope".into()]);
        assert_eq!((r[0].exists, r[0].dir, r[0].files), (true, true, 2));
        assert_eq!(r[1].files, 2);
        assert!(!r[2].exists);
    }

    #[test]
    fn finds_by_name_and_skips_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src/lib")).unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::write(root.join("src/lib/composer.ts"), "").unwrap();
        std::fs::write(root.join("src/main.ts"), "").unwrap();
        std::fs::write(root.join("target/composer.ts"), "").unwrap();
        std::fs::write(root.join(".gitignore"), "target\n").unwrap();
        // The ignore crate only reads .gitignore inside a git repository.
        std::fs::create_dir(root.join(".git")).unwrap();

        let found = search(root, "compo", 10);
        assert_eq!(found.iter().map(|m| m.rel.as_str()).collect::<Vec<_>>(), ["src/lib/composer.ts"]);
        let all = search(root, "", 10);
        assert!(all.iter().any(|m| m.rel == "src" && m.dir));
    }

    #[test]
    fn uncommitted_lists_changes_and_new_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let git = |args: &[&str]| {
            let ok = std::process::Command::new("git").arg("-C").arg(root).args(args).output().unwrap().status.success();
            assert!(ok, "git {args:?}");
        };
        git(&["init", "-q"]);
        std::fs::write(root.join("a.txt"), "one\n").unwrap();
        git(&["add", "."]);
        git(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "init"]);
        assert_eq!(uncommitted(root), None);
        std::fs::write(root.join("a.txt"), "two\n").unwrap();
        std::fs::write(root.join("b.txt"), "new\n").unwrap();
        let patch = uncommitted(root).unwrap();
        assert!(patch.contains("-one") && patch.contains("+two"), "{patch}");
        assert!(patch.contains("#   b.txt"), "{patch}");
    }
}

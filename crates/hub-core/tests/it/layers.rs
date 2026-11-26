//! Layer guard for `hub-core`'s modules. Every `crate::x` a top-level module (`x.rs` or `x/`) names
//! is an edge `from -> x`. The modules are ranked in layers (`LAYERS`, lowest first); an edge may
//! point to the same layer or a lower one. An edge that points UP is a low-level module knowing a
//! high-level one, and it must be in `frozen/layers.txt`. The list holds the upward edges that
//! already exist, so it only gets shorter: a new upward edge fails here, and removing an old one
//! leaves a stale line that `LAYERS_BLESS=1` tidies away. A module not named in `LAYERS` sits in
//! the top layer (functions), so a new feature module needs no line.
//!
//! The ring: modules that depend on each other in a circle are listed in `frozen/ring.txt`, and no
//! module may join them (`no_module_joins_the_ring`). The same `LAYERS_BLESS=1` drops the ones that left.
//!
//! Test code is left out (each item under a `#[cfg(test)]`, wherever it sits, see `test_code.rs`; and
//! the `tests.rs` files that are a module's `#[cfg(test)] mod tests;`), since a test may reach anywhere.
//! When you move a module between layers or add an upward edge on purpose:
//! `LAYERS_BLESS=1 cargo test -p hub-core --test it layers::`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Lowest first. A module may use its own layer and the ones above this line (lower in the list).
const LAYERS: &[&[&str]] = &[
    // 0: plain helpers that know nothing of Hub
    &["proc", "npx", "locked", "text", "git", "tempdirs", "voice_config", "blobs", "home_files", "project_folder", "http", "process_marks", "hubd_args", "update_signals"],
    // 1: types and settings
    &["model", "config", "thread_display", "thread_owner", "sim", "registry_address", "problem_inbox", "dev_state", "metric_samples"],
    // 2: the database, and the borrowed view of it a function takes instead of the whole Hub
    &["store", "ctx"],
    // 3: infrastructure the platform stands on (the token in `auth`; the door that checks requests is `guard`, above `hub`),
    // and a feature's data half whose `impl Hub` half is in `hub/` (`hub/reach.rs` and so on), and
    // the settings Hub reads when it starts work (where projects go, who does which task), and the
    // processes a project runs (`runtime`, `workers`, `services`; their `Hub` half is `hub/services.rs`),
    // who owns a thread Hub starts (`system`), and the Overview widgets' list (`widgets`; the build is `widget_builds`),
    // and what an agent is told as it connects (`tool_instructions`) with the rules it reads from (`pages`, `library`
    // and its `activity`/`vault`, `desk`), and the reader of a thread's last reply (`replies`)
    &["secrets", "auth", "engine_home", "reach", "reactions", "recipes", "focus", "projects_root", "handoff", "runtime", "workers", "services", "system", "widgets", "tool_instructions", "pages", "library", "activity", "vault", "desk", "replies", "folders", "own_tools", "machine", "vakten_thread"],
    // 4: the platform: engines, tools they reach and the Hub that runs them
    &["engine", "mcp", "hub"],
];

fn rank(module: &str) -> usize {
    LAYERS.iter().position(|l| l.contains(&module)).unwrap_or(LAYERS.len())
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else { return };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The source without `//` comments and without test-only items (`test_code.rs`).
fn code(text: &str) -> String {
    let mut out = String::new();
    for (_, line) in crate::test_code::code_lines(text) {
        out.push_str(line.split("//").next().unwrap_or(""));
        out.push('\n');
    }
    out
}

fn ident(s: &str) -> &str {
    let end = s.find(|c: char| !(c.is_alphanumeric() || c == '_')).unwrap_or(s.len());
    &s[..end]
}

/// Every top-level module named after a `crate::` in `text`, also inside `crate::{a, b::c}`.
fn targets(text: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut from = 0;
    while let Some(i) = text[from..].find("crate::") {
        let at = from + i;
        from = at + "crate::".len();
        let before = text[..at].chars().next_back();
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$') {
            continue;
        }
        let rest = &text[from..];
        if let Some(group) = rest.strip_prefix('{') {
            let (mut depth, mut item_start) = (0usize, true);
            for (n, c) in group.char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' if depth == 0 => break,
                    '}' => depth -= 1,
                    ',' if depth == 0 => item_start = true,
                    c if c.is_whitespace() => {}
                    _ if item_start && depth == 0 => {
                        item_start = false;
                        let name = ident(&group[n..]);
                        if !name.is_empty() && name != "self" {
                            found.insert(name.to_string());
                        }
                    }
                    _ => {}
                }
            }
        } else {
            let name = ident(rest);
            if !name.is_empty() {
                found.insert(name.to_string());
            }
        }
    }
    found
}

/// `from -> to` for every module pair, skipping a module naming itself.
fn edges() -> BTreeSet<(String, String)> {
    let src = hub_core::source_tree::hub_core().join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    let modules: BTreeSet<String> = std::fs::read_dir(&src)
        .unwrap()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.is_dir() { p.file_name().map(|n| n.to_string_lossy().into_owned()) } else if p.extension().is_some_and(|x| x == "rs") { p.file_stem().map(|n| n.to_string_lossy().into_owned()) } else { None }
        })
        .collect();
    let mut all = BTreeSet::new();
    for f in files.iter().filter(|f| !is_test_file(f)) {
        let rel = f.strip_prefix(&src).unwrap();
        let from = rel.components().next().unwrap().as_os_str().to_string_lossy().trim_end_matches(".rs").to_string();
        let text = code(&std::fs::read_to_string(f).unwrap_or_else(|e| panic!("{}: {e}", f.display())));
        for to in targets(&text) {
            if to != from && modules.contains(&to) {
                all.insert((from.clone(), to));
            }
        }
    }
    all
}

/// A `tests.rs` beside or under a module: test code, as long as its parent takes it in with
/// `#[cfg(test)] mod tests;` (`test_files_are_only_compiled_for_tests` checks that).
fn is_test_file(f: &Path) -> bool {
    f.file_name().is_some_and(|n| n == "tests.rs")
}

/// Every `tests.rs` the edges skip is compiled only for tests: its parent module (`x.rs` beside the
/// folder `x/`, or `x/mod.rs`) declares it right under `#[cfg(test)]`.
#[test]
fn test_files_are_only_compiled_for_tests() {
    let src = hub_core::source_tree::hub_core().join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    for f in files.iter().filter(|f| is_test_file(f)) {
        let dir = f.parent().expect("a file has a folder");
        let parent = [dir.with_extension("rs"), dir.join("mod.rs"), dir.join("lib.rs")].into_iter().find(|p| p.is_file());
        let text = parent.and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default().replace("\r\n", "\n");
        assert!(text.contains("#[cfg(test)]\nmod tests;"), "{} is skipped as test code, but its module does not take it in with `#[cfg(test)] mod tests;`", f.display());
    }
}

fn upward() -> BTreeSet<String> {
    edges().into_iter().filter(|(a, b)| rank(b) > rank(a)).map(|(a, b)| format!("{a} -> {b}")).collect()
}

#[test]
fn modules_only_point_down_the_layers() {
    let now = upward();
    let path = hub_core::source_tree::hub_core().join("tests/it/frozen/layers.txt");
    if std::env::var_os("LAYERS_BLESS").is_some() {
        std::fs::write(&path, now.iter().map(|l| format!("{l}\n")).collect::<String>()).unwrap();
        return;
    }
    let text = std::fs::read_to_string(&path).expect("frozen/layers.txt is missing: LAYERS_BLESS=1 writes it").replace("\r\n", "\n");
    let frozen: BTreeSet<String> = text.lines().map(str::to_string).collect();
    let new: Vec<&String> = now.difference(&frozen).collect();
    assert!(
        new.is_empty(),
        "a lower layer now names a higher one: {new:?}\nMove the type or function the other way (the layers are LAYERS in tests/it/layers.rs), or pass the value in instead of importing it.\nIf it must stay: LAYERS_BLESS=1 cargo test -p hub-core --test it layers::"
    );
}

/// Every module that sits in a ring: it reaches itself again by following edges (Tarjan's strongly
/// connected components, the ones with more than one module).
fn in_a_ring(edges: &BTreeSet<(String, String)>) -> BTreeSet<String> {
    struct Walk<'a> {
        out: BTreeMap<&'a str, Vec<&'a str>>,
        index: BTreeMap<&'a str, usize>,
        low: BTreeMap<&'a str, usize>,
        stack: Vec<&'a str>,
        on_stack: BTreeSet<&'a str>,
        found: BTreeSet<String>,
    }
    impl<'a> Walk<'a> {
        fn visit(&mut self, v: &'a str) {
            let n = self.index.len();
            self.index.insert(v, n);
            self.low.insert(v, n);
            self.stack.push(v);
            self.on_stack.insert(v);
            for w in self.out.get(v).cloned().unwrap_or_default() {
                if !self.index.contains_key(w) {
                    self.visit(w);
                    self.low.insert(v, self.low[v].min(self.low[w]));
                } else if self.on_stack.contains(w) {
                    self.low.insert(v, self.low[v].min(self.index[w]));
                }
            }
            if self.low[v] == self.index[v] {
                let at = self.stack.iter().rposition(|m| *m == v).unwrap_or(0);
                let ring = self.stack.split_off(at);
                for m in &ring {
                    self.on_stack.remove(m);
                }
                if ring.len() > 1 {
                    self.found.extend(ring.iter().map(|m| m.to_string()));
                }
            }
        }
    }
    let mut walk = Walk { out: BTreeMap::new(), index: BTreeMap::new(), low: BTreeMap::new(), stack: Vec::new(), on_stack: BTreeSet::new(), found: BTreeSet::new() };
    for (a, b) in edges {
        walk.out.entry(a.as_str()).or_default().push(b.as_str());
    }
    let starts: Vec<&str> = walk.out.keys().copied().collect();
    for v in starts {
        if !walk.index.contains_key(v) {
            walk.visit(v);
        }
    }
    walk.found
}

/// The ring is the modules that all depend on each other, so none of them can be understood, built
/// or moved to its own crate alone. `frozen/ring.txt` names the ones in it; a module not named there
/// that ends up in a ring fails, so the ring only gets smaller.
#[test]
fn no_module_joins_the_ring() {
    let now = in_a_ring(&edges());
    let path = hub_core::source_tree::hub_core().join("tests/it/frozen/ring.txt");
    if std::env::var_os("LAYERS_BLESS").is_some() {
        // Blessing only takes modules out; a list that does not exist yet gets what is there now.
        let keep: BTreeSet<String> = match std::fs::read_to_string(&path) {
            Ok(text) => text.replace("\r\n", "\n").lines().filter(|l| now.contains(*l)).map(str::to_string).collect(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => now.clone(),
            Err(e) => panic!("{}: {e}", path.display()),
        };
        std::fs::write(&path, keep.iter().map(|l| format!("{l}\n")).collect::<String>()).expect("writing frozen/ring.txt");
    }
    let text = std::fs::read_to_string(&path).expect("frozen/ring.txt is missing: LAYERS_BLESS=1 writes it").replace("\r\n", "\n");
    let frozen: BTreeSet<String> = text.lines().map(str::to_string).collect();
    let new: Vec<&String> = now.difference(&frozen).collect();
    assert!(
        new.is_empty(),
        "these modules now depend on modules that depend back on them (a ring): {new:?}\nA module joins the ring when it names a ring module with `crate::x` and one of those already reaches it.\nTake what it needs as an argument (`Ctx`, a `&Store`, a path) or move the shared piece to a lower module. The list (tests/it/frozen/ring.txt) never grows."
    );
}

#[test]
fn the_ring_finder_finds_rings_and_only_rings() {
    let e = |pairs: &[(&str, &str)]| pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<BTreeSet<_>>();
    // a -> b -> c -> a is a ring, d only points in, e and h are only on the way out, f <-> g is a second ring.
    let found = in_a_ring(&e(&[("a", "b"), ("b", "c"), ("c", "a"), ("d", "a"), ("c", "e"), ("f", "g"), ("g", "f"), ("e", "h")]));
    assert_eq!(found.into_iter().collect::<Vec<_>>(), ["a", "b", "c", "f", "g"]);
    assert!(in_a_ring(&e(&[("a", "b"), ("b", "c"), ("a", "c")])).is_empty());
}

#[test]
fn the_layer_table_names_real_modules() {
    let src = hub_core::source_tree::hub_core().join("src");
    for m in LAYERS.iter().flat_map(|l| l.iter()) {
        assert!(src.join(format!("{m}.rs")).exists() || src.join(m).is_dir(), "LAYERS names {m}, which is not a module");
    }
}

#[test]
fn the_import_reader_finds_groups_and_skips_comments() {
    let t = targets(&code("use crate::{model::A, store::{self, B}, proc};\nlet x = crate::git::run(); // crate::voice::X\nfn f() { my_crate::nope(); }\n#[cfg(test)]\nmod t { use crate::hub::H; }\n"));
    assert_eq!(t.into_iter().collect::<Vec<_>>(), ["git", "model", "proc", "store"]);
}

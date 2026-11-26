//! `AGENTS.md` is a plain copy of `CLAUDE.md`, so Codex and the other engines that read it see the
//! same instructions as Claude. Change `CLAUDE.md`, then `cp CLAUDE.md AGENTS.md`.


#[test]
fn agents_md_mirrors_claude_md() {
    let root = hub_core::source_tree::root();
    let claude = std::fs::read_to_string(root.join("CLAUDE.md")).unwrap();
    let agents = std::fs::read_to_string(root.join("AGENTS.md")).expect("AGENTS.md is missing");
    assert_eq!(claude, agents, "AGENTS.md must be a copy of CLAUDE.md: run `cp CLAUDE.md AGENTS.md`");
}

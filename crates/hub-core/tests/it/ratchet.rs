//! The comparison behind the debt ratchets (`rules::`, `clippy::`): their frozen lists hold one
//! `rule path count` line per file, but what may not grow is the TOTAL per rule across the
//! workspace (count summed over files; a line without a count, like `header`, counts as one).
//! So moving a case from one file to another passes without a bless; a rule absent from the frozen
//! list has total 0, so its first case fails. Bless rewrites the lines to what is true now, and is
//! refused while any rule's total grew. Pure, so its tests run everywhere (clippy's guard only on Linux).

use std::collections::BTreeMap;

/// `(rule, path) -> count`, as in a frozen list.
pub type Lines = BTreeMap<(String, String), usize>;

/// A rule whose total grew, with the files that likely did it.
#[derive(Debug, PartialEq)]
pub struct Grown {
    pub rule: String,
    pub frozen: usize,
    pub now: usize,
    /// `(path, frozen count or None when the file had no line, count now)` for each file that rose or is new.
    pub files: Vec<(String, Option<usize>, usize)>,
}

/// What a line weighs in its rule's total: its count, and at least one (a `header` line has 0).
fn weight(n: usize) -> usize {
    n.max(1)
}

/// The total per rule.
pub fn totals(lines: &Lines) -> BTreeMap<String, usize> {
    let mut t = BTreeMap::new();
    for ((rule, _), n) in lines {
        *t.entry(rule.clone()).or_default() += weight(*n);
    }
    t
}

/// The rules whose total now is above the frozen total.
pub fn grown(frozen: &Lines, now: &Lines) -> Vec<Grown> {
    let before = totals(frozen);
    totals(now)
        .into_iter()
        .filter_map(|(rule, total)| {
            let old = before.get(&rule).copied().unwrap_or(0);
            (total > old).then(|| Grown {
                files: now
                    .iter()
                    .filter(|((r, _), _)| *r == rule)
                    .filter_map(|((r, p), n)| {
                        let was = frozen.get(&(r.clone(), p.clone())).copied();
                        was.is_none_or(|w| weight(*n) > weight(w)).then(|| (p.clone(), was, *n))
                    })
                    .collect(),
                rule,
                frozen: old,
                now: total,
            })
        })
        .collect()
}

/// What a bless writes: the lines as they are now, or the grown rules when a total went up (then
/// nothing is written). With no frozen list yet, a first write takes everything.
pub fn bless(frozen: Option<&Lines>, now: &Lines) -> Result<Lines, Vec<Grown>> {
    match frozen {
        None => Ok(now.clone()),
        Some(f) => {
            let g = grown(f, now);
            if g.is_empty() { Ok(now.clone()) } else { Err(g) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(v: &[(&str, &str, usize)]) -> Lines {
        v.iter().map(|(r, p, n)| ((r.to_string(), p.to_string()), *n)).collect()
    }

    #[test]
    fn unchanged_lower_and_moved_pass() {
        let frozen = lines(&[("let_discard", "a.rs", 2), ("raw_git", "b.rs", 1)]);
        assert!(grown(&frozen, &frozen).is_empty());
        assert!(grown(&frozen, &lines(&[("let_discard", "a.rs", 1)])).is_empty());
        // One `let _ =` moved from a.rs to c.rs, the git call from b.rs to a.rs: totals unchanged.
        let moved = lines(&[("let_discard", "a.rs", 1), ("let_discard", "c.rs", 1), ("raw_git", "a.rs", 1)]);
        assert!(grown(&frozen, &moved).is_empty());
        assert_eq!(bless(Some(&frozen), &moved), Ok(moved.clone()));
    }

    #[test]
    fn a_total_one_higher_fails_and_names_the_file_that_rose_or_is_new() {
        let frozen = lines(&[("unwrap", "a.rs", 3), ("unwrap", "b.rs", 2)]);
        let now = lines(&[("unwrap", "a.rs", 4), ("unwrap", "b.rs", 2)]);
        assert_eq!(grown(&frozen, &now), vec![Grown { rule: "unwrap".into(), frozen: 5, now: 6, files: vec![("a.rs".into(), Some(3), 4)] }]);
        // Moved one and added one: b.rs went down, c.rs is new; c.rs is named, a.rs (unchanged) is not.
        let now = lines(&[("unwrap", "a.rs", 3), ("unwrap", "b.rs", 1), ("unwrap", "c.rs", 2)]);
        let g = grown(&frozen, &now);
        assert_eq!((g[0].frozen, g[0].now), (5, 6));
        assert_eq!(g[0].files, vec![("c.rs".into(), None, 2)]);
    }

    #[test]
    fn a_rule_missing_from_the_frozen_list_has_total_zero() {
        let frozen = lines(&[("unwrap", "a.rs", 3)]);
        let now = lines(&[("unwrap", "a.rs", 2), ("own_clip", "a.rs", 1)]);
        assert_eq!(grown(&frozen, &now), vec![Grown { rule: "own_clip".into(), frozen: 0, now: 1, files: vec![("a.rs".into(), None, 1)] }]);
    }

    #[test]
    fn lines_without_a_count_weigh_one_file_each() {
        let frozen = lines(&[("header", "a.rs", 0), ("header", "b.rs", 0)]);
        assert_eq!(totals(&frozen)["header"], 2);
        assert!(grown(&frozen, &lines(&[("header", "b.rs", 0), ("header", "c.rs", 0)])).is_empty());
        let g = grown(&frozen, &lines(&[("header", "a.rs", 0), ("header", "b.rs", 0), ("header", "c.rs", 0)]));
        assert_eq!((g[0].frozen, g[0].now, g[0].files.clone()), (2, 3, vec![("c.rs".into(), None, 0)]));
    }

    #[test]
    fn bless_rewrites_the_lines_but_never_raises_a_total() {
        let frozen = lines(&[("let_discard", "a.rs", 2), ("let_discard", "gone.rs", 1)]);
        let lower = lines(&[("let_discard", "a.rs", 1), ("let_discard", "b.rs", 1)]);
        assert_eq!(bless(Some(&frozen), &lower), Ok(lower.clone()));
        let higher = lines(&[("let_discard", "a.rs", 2), ("let_discard", "gone.rs", 1), ("let_discard", "b.rs", 1)]);
        let refused = bless(Some(&frozen), &higher).unwrap_err();
        assert_eq!(refused[0].files, vec![("b.rs".into(), None, 1)]);
        assert_eq!(bless(None, &higher), Ok(higher.clone()));
    }
}

//! One case over its repeats, and the regression flag between two such summaries (this version of
//! Hub against the one before, same suite hash). Plain arithmetic over `Measured` and pass/fail.
//!
//! The flag, as the plan defines it: the pass count down by 2 or more is red, down by 1 amber; a
//! median (wall time, first word, tokens) above the previous median by more than the previous
//! min-max spread or 20 %, whichever is larger, is amber. Amber twice in a row turning red needs
//! the history of runs, which the runner keeps (PR 3): it takes the previous flag and this one
//! and raises the level there; nothing here remembers.

use serde::{Deserialize, Serialize};

use super::measure::Measured;
/// The range, level and flag are API types (`model/bench.rs`): `BenchRun.summary` carries them.
pub use crate::model::{BenchFlag as Flag, BenchLevel as Level, BenchRange as Range};

/// The median and the extremes of one measurement over the valid repeats.
impl Range {
    /// `None` for no values. The median is `timings::spread`'s.
    pub fn of(values: Vec<i64>) -> Option<Range> {
        let (min, max) = (*values.iter().min()?, *values.iter().max()?);
        let (median, _) = crate::timings::spread(values)?;
        Some(Range { median, min, max })
    }
}

/// A case over its repeats. Invalid runs (`Measured.invalid`) count only in `invalid`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    pub case: String,
    /// Valid runs whose checks all passed: k of `runs`.
    pub passed: u32,
    pub runs: u32,
    pub invalid: u32,
    pub wall_ms: Option<Range>,
    pub first_word_ms: Option<Range>,
    pub tokens: Option<Range>,
}

/// Sums up one case: each run is whether all its checks passed, and what it measured.
pub fn summarize(case: &str, runs: &[(bool, &Measured)]) -> Summary {
    let valid: Vec<&(bool, &Measured)> = runs.iter().filter(|(_, m)| m.invalid.is_none()).collect();
    let of = |f: fn(&Measured) -> Option<i64>| Range::of(valid.iter().filter_map(|(_, m)| f(m)).collect());
    Summary {
        case: case.to_owned(),
        passed: valid.iter().filter(|(ok, _)| *ok).count() as u32,
        runs: valid.len() as u32,
        invalid: (runs.len() - valid.len()) as u32,
        wall_ms: of(|m| m.wall_ms),
        first_word_ms: of(|m| m.first_word_ms),
        tokens: of(Measured::total_tokens),
    }
}

/// The flag of `now` against `before`, the same case under the previous version.
pub fn flag(before: &Summary, now: &Summary) -> Flag {
    let mut level = Level::Green;
    let mut why = Vec::new();
    let mut raise = |to: Level, line: String| {
        level = level.max(to);
        why.push(line);
    };
    let fewer = before.passed.saturating_sub(now.passed);
    if fewer > 0 {
        let to = if fewer >= 2 { Level::Red } else { Level::Amber };
        raise(to, format!("passed {} of {}, was {} of {}", now.passed, now.runs, before.passed, before.runs));
    }
    for (what, was, is) in [("wall time", before.wall_ms, now.wall_ms), ("first word", before.first_word_ms, now.first_word_ms), ("tokens", before.tokens, now.tokens)] {
        if let (Some(was), Some(is)) = (was, is) {
            let allowed = (was.max - was.min).max(was.median / 5);
            if is.median - was.median > allowed {
                raise(Level::Amber, format!("{what} median {} was {} (allowed up to {})", is.median, was.median, was.median + allowed));
            }
        }
    }
    Flag { level, why }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(wall: i64, words: i64, tokens: i64) -> Measured {
        Measured {
            wall_ms: Some(wall),
            first_word_ms: Some(words),
            tokens: Some(crate::model::TurnUsage { input: Some(tokens), output: None, cache_read: None, cache_write: None }),
            ..Default::default()
        }
    }

    #[test]
    fn a_summary_counts_valid_runs_and_takes_the_median() {
        let (a, b, c) = (run(100, 10, 1_000), run(300, 30, 3_000), run(200, 20, 2_000));
        let mut bad = run(9_000, 9, 9);
        bad.invalid = Some("another model".into());
        let s = summarize("x", &[(true, &a), (false, &b), (true, &c), (true, &bad)]);
        assert_eq!((s.passed, s.runs, s.invalid), (2, 3, 1));
        assert_eq!(s.wall_ms, Some(Range { median: 200, min: 100, max: 300 }));
        assert_eq!(s.tokens.map(|t| t.median), Some(2_000));
    }

    fn summary(passed: u32, wall: Range) -> Summary {
        Summary { case: "x".into(), passed, runs: 3, invalid: 0, wall_ms: Some(wall), first_word_ms: None, tokens: None }
    }

    #[test]
    fn the_flag_follows_the_plan() {
        let steady = Range { median: 1_000, min: 950, max: 1_050 };
        let before = summary(3, steady);
        assert_eq!(flag(&before, &summary(3, steady)).level, Level::Green);
        assert_eq!(flag(&before, &summary(2, steady)).level, Level::Amber);
        assert_eq!(flag(&before, &summary(1, steady)).level, Level::Red);
        assert_eq!(flag(&before, &summary(0, steady)).level, Level::Red);
        // More passes is never a flag.
        assert_eq!(flag(&summary(1, steady), &summary(3, steady)).level, Level::Green);
        // The allowance is 20 % (200) here, more than the spread of 100.
        assert_eq!(flag(&before, &summary(3, Range { median: 1_200, min: 1_200, max: 1_200 })).level, Level::Green);
        assert_eq!(flag(&before, &summary(3, Range { median: 1_201, min: 1_201, max: 1_201 })).level, Level::Amber);
        // A wide spread before allows more than 20 %.
        let wide = summary(3, Range { median: 1_000, min: 500, max: 1_500 });
        assert_eq!(flag(&wide, &summary(3, Range { median: 1_900, min: 1_900, max: 1_900 })).level, Level::Green);
        assert_eq!(flag(&wide, &summary(3, Range { median: 2_001, min: 2_001, max: 2_001 })).level, Level::Amber);
        // Slower and two fewer passes: red, with both reasons.
        let both = flag(&before, &summary(1, Range { median: 5_000, min: 5_000, max: 5_000 }));
        assert_eq!((both.level, both.why.len()), (Level::Red, 2));
    }
}

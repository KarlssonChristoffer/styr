//! The built-in suites as `GET /api/bench/suites` lists them: each one's version, cases and
//! defaults, and what earlier finished runs of it took. An estimate is only ever history (medians
//! over stored runs of the same suite version, engine, model and repeats, over all its cases);
//! with no such run there is none, never a formula.

use super::measure::Measured;
use super::results;
use crate::ctx::Ctx;
use crate::model::{BenchEstimate, BenchRun, BenchSuiteInfo};

/// Every built-in suite that parses (`suite::BUILTIN`, a new one shows up here by itself).
pub fn suites(ctx: &Ctx) -> Vec<BenchSuiteInfo> {
    let runs = results::list(ctx);
    super::builtin_names()
        .filter_map(|name| super::builtin(name).ok())
        .map(|suite| {
            let cases: Vec<String> = suite.cases.iter().map(|c| c.id.clone()).collect();
            let finished: Vec<&BenchRun> = runs.iter().filter(|r| r.state == "done" && r.suite_hash == suite.hash && r.cases == cases).collect();
            BenchSuiteInfo {
                name: suite.name,
                hash: suite.hash,
                cases,
                engine: suite.engine,
                model: suite.model,
                repeats: suite.repeats,
                timeout_s: suite.timeout_s,
                cost_cap_usd: suite.cost_cap_usd,
                estimates: estimates(&finished),
            }
        })
        .collect()
}

/// One estimate per engine, model and repeats among `runs` (newest first, as `results::list`
/// gives them), in that order.
pub fn estimates(runs: &[&BenchRun]) -> Vec<BenchEstimate> {
    let key = |r: &BenchRun| (r.engine.clone(), r.model_asked.clone(), r.repeats);
    let mut groups: Vec<Vec<&BenchRun>> = Vec::new();
    for run in runs {
        match groups.iter_mut().find(|g| key(g[0]) == key(run)) {
            Some(group) => group.push(run),
            None => groups.push(vec![run]),
        }
    }
    groups
        .into_iter()
        .map(|group| BenchEstimate {
            engine: group[0].engine.clone(),
            model: group[0].model_asked.clone(),
            repeats: group[0].repeats,
            runs: group.len() as u32,
            tokens: median(group.iter().filter_map(|r| tokens(r)).collect()),
            // In millionths of a dollar, so the median is `timings::spread`'s like every other.
            cost_usd: median(group.iter().filter_map(|r| results::cost(r)).map(|c| (c * 1e6).round() as i64).collect()).map(|m| m as f64 / 1e6),
        })
        .collect()
}

/// A run's total tokens over every repeat (side jobs included, as each row's `Measured` holds
/// them); `None` when no repeat said.
fn tokens(run: &BenchRun) -> Option<i64> {
    let counts: Vec<i64> = run
        .rows
        .iter()
        .filter_map(|r| serde_json::from_value::<Measured>(r.measured.clone()).ok()?.total_tokens())
        .collect();
    (!counts.is_empty()).then(|| counts.iter().sum())
}

fn median(values: Vec<i64>) -> Option<i64> {
    crate::timings::spread(values).map(|(median, _)| median)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BenchRow;
    use serde_json::json;

    fn run(engine: &str, repeats: u32, rows: &[(i64, f64)]) -> BenchRun {
        BenchRun {
            engine: engine.into(),
            model_asked: "small".into(),
            repeats,
            rows: rows
                .iter()
                .map(|(tokens, cost)| BenchRow { measured: json!({ "tokens": { "input": tokens }, "cost_usd": cost }), ..Default::default() })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn an_estimate_is_the_median_of_earlier_runs_and_none_without_one() {
        assert!(estimates(&[]).is_empty());
        let (a, b, c) = (run("e", 3, &[(100, 0.1), (100, 0.1)]), run("e", 3, &[(500, 0.5)]), run("e", 3, &[(50, 0.05)]));
        let other = run("f", 1, &[(7, 0.0)]);
        let silent = run("g", 3, &[]);
        let got = estimates(&[&a, &other, &b, &c, &silent]);
        assert_eq!(got.len(), 3);
        assert_eq!((got[0].engine.as_str(), got[0].runs, got[0].tokens, got[0].cost_usd), ("e", 3, Some(200), Some(0.2)));
        assert_eq!((got[1].engine.as_str(), got[1].repeats, got[1].tokens), ("f", 1, Some(7)));
        assert_eq!((got[2].tokens, got[2].cost_usd), (None, None), "a run that said nothing gives no number");
    }
}

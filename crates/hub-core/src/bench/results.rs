//! Where bench runs are kept and how one reads as a report. One JSON file per run under
//! `<data>/bench/runs/<id>.json` (`BenchRun`) and a markdown report beside it (`<id>.md`), written
//! again after every repeat, so a run that stops half-way still has both. "Before" is the newest
//! earlier run with the same suite hash, engine and answering model; the report puts each case
//! beside it, with the flag `bench::flag` gives. Files, not events: the event log stays the
//! threads' own.

use std::path::PathBuf;

use anyhow::{Context, Result};

use super::measure::Measured;
use super::summary::{Flag, Level, Range, Summary, flag, summarize};
use crate::ctx::Ctx;
use crate::model::{BenchCaseSummary, BenchCount, BenchRow, BenchRun, BenchRunRef, BenchSummary, TurnUsage};

/// The folder of the runs' files.
pub fn runs_dir(ctx: &Ctx) -> PathBuf {
    ctx.config.data_dir.join("bench").join("runs")
}

/// Writes the run's JSON with its summary (against the runs before it) and the report rendered
/// from that summary.
pub fn save(ctx: &Ctx, run: &BenchRun) -> Result<()> {
    let dir = runs_dir(ctx);
    std::fs::create_dir_all(&dir).with_context(|| format!("making {}", dir.display()))?;
    let mut run = run.clone();
    let summary = summary_among(&read_all(ctx), &run);
    run.summary = Some(summary.clone());
    let json = dir.join(format!("{}.json", run.id));
    std::fs::write(&json, serde_json::to_vec_pretty(&run)?).with_context(|| format!("writing {}", json.display()))?;
    let md = dir.join(format!("{}.md", run.id));
    std::fs::write(&md, report(&run, &summary)).with_context(|| format!("writing {}", md.display()))?;
    Ok(())
}

/// The run called `id`, if its file is there and reads. A run stored before `BenchRun.summary`
/// gets it worked out here, when it has rows.
pub fn load(ctx: &Ctx, id: &str) -> Option<BenchRun> {
    let mut run = read(ctx, id)?;
    if run.summary.is_none() && !run.rows.is_empty() {
        run.summary = Some(summary_among(&read_all(ctx), &run));
    }
    Some(run)
}

/// The run's file as stored.
fn read(ctx: &Ctx, id: &str) -> Option<BenchRun> {
    if id.contains(['/', '\\']) || id.contains("..") {
        return None;
    }
    serde_json::from_slice(&std::fs::read(runs_dir(ctx).join(format!("{id}.json"))).ok()?).ok()
}

/// The report of run `id` as it was last written.
pub fn load_report(ctx: &Ctx, id: &str) -> Option<String> {
    read(ctx, id)?;
    std::fs::read_to_string(runs_dir(ctx).join(format!("{id}.md"))).ok()
}

/// Every run that reads, newest first, each with its summary (worked out for a run stored before
/// the field, as [`load`] does).
pub fn list(ctx: &Ctx) -> Vec<BenchRun> {
    let mut runs = read_all(ctx);
    let missing: Vec<(usize, BenchSummary)> = runs
        .iter()
        .enumerate()
        .filter(|(_, r)| r.summary.is_none() && !r.rows.is_empty())
        .map(|(i, r)| (i, summary_among(&runs, r)))
        .collect();
    for (i, summary) in missing {
        runs[i].summary = Some(summary);
    }
    runs
}

/// Every run file that reads, as stored, newest first.
fn read_all(ctx: &Ctx) -> Vec<BenchRun> {
    let mut runs: Vec<BenchRun> = std::fs::read_dir(runs_dir(ctx))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| serde_json::from_slice(&std::fs::read(e.path()).ok()?).ok())
        .collect();
    runs.sort_by_key(|r: &BenchRun| std::cmp::Reverse(r.started));
    runs
}

/// The run `run` is compared with: the newest one started before it, finished, on the same suite
/// version, engine and answering model.
pub fn before<'a>(runs: &'a [BenchRun], run: &BenchRun) -> Option<&'a BenchRun> {
    runs.iter()
        .filter(|r| r.id != run.id && r.started < run.started && r.state != "running")
        .filter(|r| r.suite_hash == run.suite_hash && r.engine == run.engine && r.model_answered == run.model_answered)
        .max_by_key(|r| r.started)
}

/// One case of a run summed up (`bench::summarize`); `None` when the run has no row of it.
pub fn case_summary(run: &BenchRun, case: &str) -> Option<Summary> {
    let rows: Vec<(bool, Measured)> = run
        .rows
        .iter()
        .filter(|r| r.case == case && r.state != "interrupted")
        .map(|r| {
            let mut m: Measured = serde_json::from_value(r.measured.clone()).unwrap_or_default();
            if m.invalid.is_none() {
                m.invalid = r.invalid.clone();
            }
            (r.pass, m)
        })
        .collect();
    if rows.is_empty() {
        return None;
    }
    let refs: Vec<(bool, &Measured)> = rows.iter().map(|(ok, m)| (*ok, m)).collect();
    Some(summarize(case, &refs))
}

/// The run's summary against the runs in `all`: the run before it, and the one before that (for
/// amber twice in a row).
pub fn summary_among(all: &[BenchRun], run: &BenchRun) -> BenchSummary {
    let before = before(all, run);
    let earlier = before.and_then(|b| self::before(all, b));
    summary(run, before, earlier)
}

/// Per case, what the report's table prints: k of n now and before, the medians and min-max, the
/// cost and the flag. [`report`] renders from this and nothing else, so the two agree.
pub fn summary(run: &BenchRun, before: Option<&BenchRun>, earlier: Option<&BenchRun>) -> BenchSummary {
    BenchSummary {
        before: before.map(|b| BenchRunRef { id: b.id.clone(), hub_version: b.hub_version.clone() }),
        cases: run.cases.iter().map(|case| case_line(run, case, before, earlier)).collect(),
    }
}

/// One case of [`summary`]. Not run: only the name and the cost.
fn case_line(run: &BenchRun, case: &str, before: Option<&BenchRun>, earlier: Option<&BenchRun>) -> BenchCaseSummary {
    let costs: Vec<f64> = run.rows.iter().filter(|r| r.case == case).filter_map(|r| r.measured["cost_usd"].as_f64()).collect();
    let cost_usd = (!costs.is_empty()).then(|| costs.iter().sum());
    let Some(now) = case_summary(run, case) else {
        return BenchCaseSummary { case: case.to_owned(), cost_usd, ..Default::default() };
    };
    let was = before.and_then(|b| case_summary(b, case));
    let mut flagged = was.as_ref().map(|w| flag(w, &now));
    // Amber twice in a row is red: the run before also flagged this case against its own before.
    let twice = earlier.zip(before).and_then(|(e, b)| Some(flag(&case_summary(e, case)?, &case_summary(b, case)?)));
    if let (Some(f), Some(prev)) = (flagged.as_mut(), twice)
        && f.level == Level::Amber
        && prev.level >= Level::Amber
    {
        f.level = Level::Red;
        f.why.push("amber in the run before too".into());
    }
    BenchCaseSummary {
        case: now.case,
        passed: now.passed,
        runs: now.runs,
        invalid: now.invalid,
        wall_ms: now.wall_ms,
        first_word_ms: now.first_word_ms,
        tokens: now.tokens,
        cost_usd,
        before: was.map(|w| BenchCount { passed: w.passed, runs: w.runs }),
        flag: flagged,
    }
}

/// What the run cost, from the engine's own figures; `None` when no repeat had one.
pub fn cost(run: &BenchRun) -> Option<f64> {
    let costs: Vec<f64> = run.rows.iter().filter_map(|r| r.measured["cost_usd"].as_f64()).collect();
    (!costs.is_empty()).then(|| costs.iter().sum())
}

/// Each turn's count at least the one before it in every field: an engine that reports the
/// session's running total rather than each turn's own.
pub fn looks_cumulative(turns: &[Option<TurnUsage>]) -> bool {
    let counted: Vec<&TurnUsage> = turns.iter().flatten().collect();
    let field = |u: &TurnUsage| [u.input, u.output, u.cache_read, u.cache_write];
    counted.len() >= 2
        && counted.windows(2).all(|w| field(w[0]).iter().zip(field(w[1])).all(|(a, b)| match (a, b) {
            (Some(a), Some(b)) => b >= *a,
            _ => true,
        }))
}

/// What three repeats can say and what they cannot.
pub const HOW_TO_READ: &str = "Three repeats show whether a case passes reliably and roughly how long it takes and what it \
costs. They cannot show small changes: a time or token count inside the earlier min-max spread is noise, and a single \
amber flag is a hint to run again, not a verdict.";

/// The markdown report: per case k/n now and before, median wall time, first word and tokens,
/// cost, the flag (the table from `summary`, see [`summary`]), and the failed repeats with their
/// threads.
pub fn report(run: &BenchRun, summary: &BenchSummary) -> String {
    let mut out = format!("# Bench: {} on {}\n\n", run.suite, run.engine);
    out.push_str(&format!(
        "Run `{}` · suite version `{}` · model {} (asked {}) · Hub {} · {} · {}\n\n",
        run.id,
        run.suite_hash,
        run.model_answered.as_deref().unwrap_or("not reported"),
        run.model_asked,
        run.hub_version,
        run.machine,
        state_words(run)
    ));
    match &summary.before {
        Some(b) => out.push_str(&format!("Compared with run `{}` (Hub {}).\n\n", b.id, b.hub_version)),
        None => out.push_str("No earlier run of this suite version, engine and model: a new series.\n\n"),
    }
    out.push_str("| Case | Passed | Before | Wall (median, min-max) | First word | Tokens | Flag |\n|---|---|---|---|---|---|---|\n");
    for c in &summary.cases {
        let case = &c.case;
        if c.runs + c.invalid == 0 {
            out.push_str(&format!("| {case} | not run | | | | | |\n"));
            continue;
        }
        out.push_str(&format!(
            "| {case} | {}/{}{} | {} | {} | {} | {} | {} |\n",
            c.passed,
            c.runs,
            if c.invalid > 0 { format!(" ({} invalid)", c.invalid) } else { String::new() },
            c.before.map_or("new".into(), |w| format!("{}/{}", w.passed, w.runs)),
            range(c.wall_ms, ms),
            range(c.first_word_ms, ms),
            range(c.tokens, |n| n.to_string()),
            c.flag.as_ref().map_or("new series".into(), flag_words),
        ));
    }
    out.push_str(&format!("\nCost: {}.\n\n", cost(run).map_or("the engine did not say".into(), |c| format!("${c:.2}"))));
    let planned = run.cases.len() * run.repeats as usize;
    let ran = run.rows.iter().filter(|r| r.state != "interrupted").count();
    if run.state != "running" && ran < planned {
        let why = if run.state == "done" { String::new() } else { format!(" ({})", state_words(run)) };
        out.push_str(&format!("{} of {planned} repeats were not run{why}.\n\n", planned - ran));
    }
    let refused: Vec<String> = run
        .rows
        .iter()
        .filter(|r| r.measured["perm_refused"].as_u64().is_some_and(|n| n > 0))
        .map(|r| {
            let why: Vec<&str> = r.measured["refused"].as_array().into_iter().flatten().filter_map(|w| w.as_str()).collect();
            format!("{} {}/{}: {}", r.case, r.repeat, run.repeats, if why.is_empty() { "refused".to_owned() } else { why.join("; ") })
        })
        .collect();
    if !refused.is_empty() {
        out.push_str(&format!("Hub refused the agent something in {} repeats (a stricter guard than usual runs here): {}.\n\n", refused.len(), refused.join(" · ")));
    }
    out.push_str(HOW_TO_READ);
    out.push('\n');
    let failed: Vec<&BenchRow> = run.rows.iter().filter(|r| !r.pass && r.invalid.is_none()).collect();
    if !failed.is_empty() {
        out.push_str("\n## Failed repeats\n\n");
        for r in failed {
            let why: Vec<String> = r.checks.iter().filter(|c| !c.ok).map(|c| format!("{} ({})", c.check, c.why.as_deref().unwrap_or("failed"))).collect();
            out.push_str(&format!(
                "- {} {}/{}: {}; thread `{}`{}\n",
                r.case,
                r.repeat,
                run.repeats,
                if why.is_empty() { r.state.replace('_', " ") } else { why.join("; ") },
                r.thread.as_deref().unwrap_or("none"),
                r.folder.as_deref().map_or(String::new(), |f| format!(", folder `{f}`")),
            ));
        }
    }
    let invalid: Vec<&BenchRow> = run.rows.iter().filter(|r| r.invalid.is_some()).collect();
    if !invalid.is_empty() {
        out.push_str("\n## Not counted\n\n");
        for r in invalid {
            out.push_str(&format!("- {} {}/{}: {}\n", r.case, r.repeat, run.repeats, r.invalid.as_deref().unwrap_or_default()));
        }
    }
    if run.rows.iter().any(|r| looks_cumulative(&r.turn_usage)) {
        out.push_str("\nNote: in repeats with several turns every turn's token count was at least the one before it, so this engine \
may report the session's running total rather than each turn's own; the tokens column may count early turns more than once.\n");
    }
    out
}

fn state_words(run: &BenchRun) -> String {
    let state = match run.state.as_str() {
        "running" => "still running".to_owned(),
        "done" => "finished".to_owned(),
        other => other.replace('_', " "),
    };
    match &run.why {
        Some(why) => format!("{state}: {why}"),
        None => state,
    }
}

fn ms(n: i64) -> String {
    format!("{:.1} s", n as f64 / 1000.0)
}

fn range(r: Option<Range>, show: fn(i64) -> String) -> String {
    r.map_or("-".into(), |r| format!("{} ({}-{})", show(r.median), show(r.min), show(r.max)))
}

fn flag_words(f: &Flag) -> String {
    let level = match f.level {
        Level::Green => "green",
        Level::Amber => "amber",
        Level::Red => "red",
    };
    if f.why.is_empty() { level.into() } else { format!("{level}: {}", f.why.join("; ")) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BenchCheck;
    use serde_json::json;

    fn row(case: &str, n: u32, pass: bool, wall: i64) -> BenchRow {
        BenchRow {
            case: case.into(),
            repeat: n,
            thread: Some(format!("t{n}")),
            state: "done".into(),
            pass,
            checks: vec![BenchCheck { check: "ended = \"end_turn\"".into(), ok: pass, why: (!pass).then(|| "it stopped".into()) }],
            measured: json!({ "wall_ms": wall, "first_word_ms": 100, "tokens": { "input": 10, "output": 5 }, "cost_usd": 0.01, "turns": 1 }),
            ..Default::default()
        }
    }

    fn run(id: &str, started: i64, rows: Vec<BenchRow>) -> BenchRun {
        BenchRun {
            id: id.into(),
            suite: "smoke".into(),
            suite_hash: "abc".into(),
            engine: "e".into(),
            model_asked: "small".into(),
            machine: "pc".into(),
            hub_version: "1".into(),
            started,
            state: "done".into(),
            repeats: 3,
            cases: vec!["a".into(), "b".into()],
            rows,
            ..Default::default()
        }
    }

    /// The report as `save` writes it: rendered from the run's summary.
    fn report(run: &BenchRun, before: Option<&BenchRun>, earlier: Option<&BenchRun>) -> String {
        super::report(run, &summary(run, before, earlier))
    }

    /// The case table's cells by case (header and rule left out).
    fn table(report: &str) -> std::collections::BTreeMap<String, Vec<String>> {
        report
            .lines()
            .map(|l| l.split('|').map(|c| c.trim().to_owned()).collect::<Vec<_>>())
            .filter(|cells| cells.len() == 9 && cells[1] != "Case" && !cells[1].starts_with("---"))
            .map(|cells| (cells[1].clone(), cells))
            .collect()
    }

    #[test]
    fn the_summary_and_the_report_table_agree() {
        let before_run = run("1-smoke", 1, (1..=3).map(|n| row("a", n, true, 1_000)).collect());
        let mut invalid = row("a", 3, false, 9_000);
        invalid.invalid = Some("another model answered".into());
        let mut now = run("2-smoke", 2, vec![row("a", 1, true, 1_000), row("a", 2, false, 1_000), invalid, row("b", 1, true, 2_000)]);
        now.cases.push("c".into());
        let s = summary(&now, Some(&before_run), None);
        let text = super::report(&now, &s);

        assert_eq!(s.before, Some(BenchRunRef { id: "1-smoke".into(), hub_version: "1".into() }));
        let [a, b, c] = &s.cases[..] else { panic!("{s:#?}") };
        assert_eq!((a.passed, a.runs, a.invalid, a.before), (1, 2, 1, Some(BenchCount { passed: 3, runs: 3 })));
        assert_eq!(a.flag.as_ref().map(|f| (f.level, f.why.clone())), Some((Level::Red, vec!["passed 1 of 2, was 3 of 3".to_owned()])));
        assert_eq!(a.wall_ms, Some(Range { median: 1_000, min: 1_000, max: 1_000 }), "the invalid repeat's 9 s is left out");
        assert!(a.cost_usd.is_some_and(|c| (c - 0.03).abs() < 1e-9), "every repeat's cost counts: {:?}", a.cost_usd);
        // b has no earlier run, so no flag; c never ran.
        assert_eq!((b.passed, b.runs, b.before, b.flag.clone()), (1, 1, None, None));
        assert_eq!((c.runs + c.invalid, c.flag.clone(), c.cost_usd), (0, None, None));

        let cells = table(&text);
        assert_eq!(cells.len(), 3, "{text}");
        for c in &s.cases {
            let row = &cells[&c.case];
            if c.runs + c.invalid == 0 {
                assert_eq!(row[2], "not run");
                continue;
            }
            let invalid = if c.invalid > 0 { format!(" ({} invalid)", c.invalid) } else { String::new() };
            assert_eq!(row[2], format!("{}/{}{invalid}", c.passed, c.runs));
            assert_eq!(row[3], c.before.map_or("new".into(), |w| format!("{}/{}", w.passed, w.runs)));
            assert_eq!(row[4], range(c.wall_ms, ms));
            assert_eq!(row[6], range(c.tokens, |n| n.to_string()));
            assert_eq!(row[7], c.flag.as_ref().map_or("new series".into(), flag_words));
        }
        assert_eq!(cells["a"][7], "red: passed 1 of 2, was 3 of 3");
        assert_eq!(cells["b"][7], "new series");
    }

    #[test]
    fn a_run_stored_before_the_summary_still_reads() {
        let mut stored = serde_json::to_value(run("1-smoke", 1, vec![row("a", 1, true, 1_000)])).unwrap();
        assert!(stored.get("summary").is_none(), "a run without one writes no key");
        stored.as_object_mut().unwrap().remove("summary");
        let read: BenchRun = serde_json::from_value(stored).unwrap();
        assert_eq!(read.summary, None);
        assert_eq!(summary_among(std::slice::from_ref(&read), &read).cases[0].passed, 1);
    }

    #[test]
    fn a_repeats_side_jobs_count_in_the_summary_as_in_the_report() {
        let usage = |input| Some(TurnUsage { input: Some(input), output: None, cache_read: None, cache_write: None });
        let parent = Measured { wall_ms: Some(1_000), tokens: usage(100), cost_usd: Some(0.5), ..Default::default() };
        let job = Measured { tokens: usage(40), cost_usd: Some(0.25), ..Default::default() };
        let mut r = row("a", 1, true, 1_000);
        r.measured = serde_json::to_value(parent.with_side_jobs(&[job])).unwrap();
        let ran = run("1-s", 1, vec![r]);
        let s = summary(&ran, None, None);
        assert_eq!((s.cases[0].tokens.map(|t| t.median), s.cases[0].cost_usd), (Some(140), Some(0.75)));
        assert_eq!(table(&super::report(&ran, &s))["a"][6], "140 (140-140)");
        assert_eq!(cost(&ran), Some(0.75));
    }

    #[test]
    fn the_report_says_k_of_n_and_compares_with_the_run_before() {
        let first = run("1-smoke", 1, vec![row("a", 1, true, 1_000), row("a", 2, true, 1_100), row("a", 3, true, 1_200)]);
        let report1 = report(&first, None, None);
        assert!(report1.contains("a new series") && report1.contains("| a | 3/3 | new |"), "{report1}");
        assert!(report1.contains("| b | not run |") && report1.contains(HOW_TO_READ));
        assert!(report1.contains("Cost: $0.03"), "{report1}");

        let second = run("2-smoke", 2, vec![row("a", 1, true, 1_000), row("a", 2, false, 5_000), row("a", 3, false, 5_000)]);
        let runs = vec![second.clone(), first.clone()];
        assert_eq!(before(&runs, &second).map(|r| r.id.as_str()), Some("1-smoke"));
        let report2 = report(&second, before(&runs, &second), None);
        assert!(report2.contains("| a | 1/3 | 3/3 |") && report2.contains("red"), "{report2}");
        assert!(report2.contains("## Failed repeats") && report2.contains("thread `t2`") && report2.contains("it stopped"), "{report2}");
    }

    #[test]
    fn only_a_like_run_counts_as_before() {
        let first = run("1-smoke", 1, vec![]);
        let mut other_suite = run("2-smoke", 2, vec![]);
        other_suite.suite_hash = "new".into();
        let mut running = run("3-smoke", 3, vec![]);
        running.state = "running".into();
        let now = run("4-smoke", 4, vec![]);
        let runs = vec![now.clone(), running, other_suite, first];
        assert_eq!(before(&runs, &now).map(|r| r.id.as_str()), Some("1-smoke"));
    }

    #[test]
    fn amber_twice_in_a_row_is_red() {
        let a = |id: &str, at: i64, wall: i64| run(id, at, (1..=3).map(|n| row("a", n, true, wall)).collect());
        let (r1, r2, r3) = (a("1-s", 1, 1_000), a("2-s", 2, 1_500), a("3-s", 3, 2_000));
        assert!(report(&r2, Some(&r1), None).contains("amber"));
        assert!(report(&r3, Some(&r2), Some(&r1)).contains("red: wall time"), "{}", report(&r3, Some(&r2), Some(&r1)));
    }

    #[test]
    fn invalid_repeats_are_not_failures() {
        let mut bad = row("a", 1, false, 1_000);
        bad.invalid = Some("the quota guard moved it".into());
        let r = run("1-s", 1, vec![bad, row("a", 2, true, 1_000)]);
        let text = report(&r, None, None);
        assert!(text.contains("| a | 1/1 (1 invalid) |") && text.contains("## Not counted") && !text.contains("## Failed"), "{text}");
    }

    #[test]
    fn the_report_says_what_was_not_run_and_what_hub_refused() {
        let mut refused = row("a", 1, false, 1_000);
        refused.measured["perm_refused"] = json!(1);
        refused.measured["refused"] = json!(["a bench case stays inside its own folder: it never pushes"]);
        let mut r = run("1-s", 1, vec![refused]);
        r.state = "plan_limit".into();
        r.why = Some("the plan limit was reached in a 1/3".into());
        let text = report(&r, None, None);
        assert!(text.contains("5 of 6 repeats were not run (plan limit: the plan limit was reached in a 1/3)."), "{text}");
        assert!(text.contains("Hub refused the agent something in 1 repeats") && text.contains("a 1/3: a bench case stays inside its own folder: it never pushes"), "{text}");
    }

    #[test]
    fn rising_counts_in_every_field_look_cumulative() {
        let u = |i, o| Some(TurnUsage { input: Some(i), output: Some(o), cache_read: None, cache_write: None });
        assert!(looks_cumulative(&[u(10, 5), u(30, 9)]));
        assert!(!looks_cumulative(&[u(10, 5), u(8, 9)]));
        assert!(!looks_cumulative(&[u(10, 5)]));
    }
}

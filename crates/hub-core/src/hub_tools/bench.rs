//! The bench's two tools (`bench_run`, `bench_report`; declared in `registry.rs`): start a run and
//! read its report. The work is `bench/run.rs` and `bench/results.rs`.

use std::sync::Arc;

use serde_json::{Value, json};

use crate::hub::Hub;

/// `bench_run`: starts the run and says at once what it will do.
pub(super) async fn run_tool(hub: &Arc<Hub>, args: &Value) -> anyhow::Result<Value> {
    let ask: crate::model::BenchAsk = serde_json::from_value(args.clone()).map_err(|e| anyhow::anyhow!("{e}"))?;
    let run = crate::bench::run::start(hub, &ask).await?;
    let threads = run.cases.len() as u32 * run.repeats;
    Ok(json!({
        "id": run.id,
        "will": format!(
            "run the {} cases of suite {} (version {}) {} times each on {} with model {}: {threads} threads, one at a time, each archived when it is checked",
            run.cases.len(), run.suite, run.suite_hash, run.repeats, run.engine, run.model_asked
        ),
        "next": "call bench_report with this id to see how far it is; the report is complete once it no longer says still running",
    }))
}

/// `bench_report`: the markdown of a run (the latest by default), with how far it is while it goes.
pub(super) async fn report_tool(hub: &Arc<Hub>, args: &Value) -> Result<String, String> {
    let ctx = hub.ctx();
    let id = match args["id"].as_str().map(str::trim).filter(|s| !s.is_empty()) {
        Some(id) => id.to_owned(),
        None => crate::bench::results::list(&ctx).first().map(|r| r.id.clone()).ok_or("no bench run yet; start one with bench_run")?,
    };
    let report = crate::bench::results::load_report(&ctx, &id).ok_or_else(|| format!("no bench run {id}"))?;
    match crate::bench::run::progress(hub) {
        Some((going, at)) if going == id => Ok(format!("Still running: {at}. The report so far:\n\n{report}")),
        _ => Ok(report),
    }
}

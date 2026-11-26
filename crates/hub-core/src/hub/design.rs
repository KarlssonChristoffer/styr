//! The `Hub` half of `design.rs`: switching design mode, logging what a design turn changed
//! outside `docs/design/`, and running the fresh-session review of a design document. They emit
//! events, start engines and send messages, so they live here; `design.rs` keeps the notes, the
//! checks and the pure readers of the log, and names nothing above it.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use serde_json::json;

use super::Hub;
use crate::design;
use crate::locked::{LockExt, ResultExt};
use crate::model::{DesignCheck, DesignData, DesignReviewAsk, DesignReviewStarted, Event, EventKind};

impl Hub {
    /// Turns design mode on or off. Nothing is logged when it already is.
    pub async fn switch_design(&self, thread_id: &str, on: bool) -> Result<()> {
        self.store.thread(thread_id).await?.context("no such thread")?;
        if design::is_on(&self.ctx(), thread_id).await == on {
            return Ok(());
        }
        self.emit_typed(thread_id, &DesignData::switched(on, "you")).await?;
        Ok(())
    }

    /// After a turn in design mode: logs the files it changed outside `docs/design/`. Engines that
    /// edit without asking never come past Hub's permission answers, so this is the only sign.
    pub(crate) async fn design_after_turn(&self, event: &Event) -> Result<()> {
        let marks = self.store.events_of(&event.thread_id, 0, &[EventKind::Design, EventKind::TurnStart]).await?;
        if !design::on_in(&marks) {
            return Ok(());
        }
        let start = marks.iter().rev().filter(|e| e.seq < event.seq).find(|e| e.kind == EventKind::TurnStart).map(|e| e.seq).unwrap_or(0);
        let events = self.store.events_of(&event.thread_id, start, &[EventKind::Update]).await?;
        let thread = self.store.thread(&event.thread_id).await?.context("no such thread")?;
        let cwd = Path::new(&thread.cwd);
        let mut strayed: Vec<String> = events
            .iter()
            .filter(|e| e.seq > start && e.seq < event.seq && e.kind == EventKind::Update)
            .flat_map(|e| crate::clash::edited(&e.data, cwd))
            .filter(|p| !design::in_design_folder(p, cwd))
            .collect();
        strayed.sort();
        strayed.dedup();
        if !strayed.is_empty() {
            self.emit_typed(&event.thread_id, &DesignData { strayed: Some(strayed), seq: Some(event.seq), ..Default::default() }).await?;
        }
        Ok(())
    }

    /// Has fresh sessions review the thread's design document. Returns at once; progress and the
    /// result are `design` events under one id, and unless every check finds nothing the findings
    /// go to the agent as a message marked `design: id`.
    pub async fn design_review(self: &Arc<Self>, thread_id: &str, ask: DesignReviewAsk, by: &str) -> Result<DesignReviewStarted> {
        let thread = self.store.thread(thread_id).await?.context("no such thread")?;
        // What the readers below look at: review states, edited files, and the model in use.
        let events = self.store.events_of(thread_id, 0, &[EventKind::Design, EventKind::Update, EventKind::SessionState, EventKind::Setting]).await?;
        if design::review_running(&events) {
            bail!("a design review is already running in this thread");
        }
        let cwd = PathBuf::from(&thread.cwd);
        let doc = design::find_doc(&events, &cwd, ask.doc.as_deref())?;
        let text = std::fs::read_to_string(cwd.join(&doc)).with_context(|| format!("reading {doc}"))?;
        if text.trim().is_empty() {
            bail!("{doc} is empty");
        }
        let engine = self.config.engines.get(&thread.engine).context("the thread's engine is not configured")?.clone();
        let strong = self.reviewer(&thread.engine, &events).await.ok();
        let id = uuid::Uuid::now_v7().to_string();
        let checks: Vec<DesignCheck> = design::CHECKS
            .iter()
            .map(|c| DesignCheck {
                name: c.name.to_owned(),
                title: c.title.to_owned(),
                model: Some(if c.strong { strong.clone() } else { None }),
                state: "running".into(),
                ..Default::default()
            })
            .collect();
        let data = DesignData {
            review: Some(id.clone()),
            state: Some("running".into()),
            doc: Some(doc.clone()),
            checks: Some(checks),
            by: Some(by.to_owned()),
            ..Default::default()
        };
        self.emit_typed(thread_id, &data).await?;

        let data = Arc::new(Mutex::new(data));
        let body = format!("<design path=\"{doc}\">\n{}\n</design>", crate::text::clip_counted(&text, 150_000));
        let cwd_text = thread.cwd.clone();
        let mut tasks = Vec::new();
        for (i, check) in design::CHECKS.iter().enumerate() {
            let (hub, thread_id, engine, data) = (self.clone(), thread_id.to_owned(), engine.clone(), data.clone());
            let model = if check.strong { strong.clone() } else { None };
            let prompt = format!("{}\n\n{body}", check.prompt);
            let cwd = cwd_text.clone();
            tasks.push(tokio::spawn(run_check(hub, thread_id, engine, cwd, model, prompt, data, i)));
        }

        let (hub, thread_id_owned, review_id) = (self.clone(), thread_id.to_owned(), id.clone());
        tokio::spawn(async move {
            for task in tasks {
                task.await.warn_on_err("a design check");
            }
            let (final_data, findings) = design::settle(&data);
            hub.emit_typed(&thread_id_owned, &final_data).await.warn_on_err("emit Design");
            if final_data.sent == Some(true) {
                let message = format!("{}\n\n{findings}", design::SEND_BACK);
                if let Err(e) = hub.send_from(&thread_id_owned, message, Vec::new(), None, Some(("design", json!(review_id))), None).await {
                    tracing::warn!(thread = %thread_id_owned, "could not send the design review back: {e:#}");
                }
            }
        });
        Ok(DesignReviewStarted { id, doc })
    }
}

/// Runs check number `i` on the engine and records how it went in the review's data.
#[allow(clippy::too_many_arguments)]
async fn run_check(hub: Arc<Hub>, thread_id: String, engine: crate::config::EngineConfig, cwd: String, model: Option<String>, prompt: String, data: Arc<Mutex<DesignData>>, i: usize) {
    let answer = crate::engine::complete("design_check", &engine, &cwd, model.as_deref(), prompt);
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(15 * 60), answer).await;
    let snapshot = {
        let mut d = data.locked();
        if let Some(c) = d.checks.as_mut().and_then(|checks| checks.get_mut(i)) {
            match outcome {
                Ok(Ok(text)) => {
                    c.state = "done".into();
                    c.verdict = Some(design::verdict(&text));
                    c.text = Some(text.trim().to_owned());
                }
                Ok(Err(e)) => {
                    c.state = "failed".into();
                    c.error = Some(format!("{e:#}"));
                }
                Err(_) => {
                    c.state = "failed".into();
                    c.error = Some("took more than 15 minutes".into());
                }
            }
        }
        d.clone()
    };
    hub.emit_typed(&thread_id, &snapshot).await.warn_on_err("emit Design");
}

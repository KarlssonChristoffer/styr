//! Hub's side of Customs' review (`customs_review.rs`): which engine and model judge an item,
//! and starting the monthly update threads. The review asks through `customs_review::Officer`,
//! so it never names Hub or the engines.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use futures::future::BoxFuture;

use crate::customs_review::Officer;
use crate::hub::Hub;
use crate::model::{NewThread, Owner};

// On `Arc<Hub>`, since a new thread needs the `Arc` (`create_thread_as`).
impl Officer for Arc<Hub> {
    fn ctx(&self) -> crate::ctx::Ctx<'_> {
        Hub::ctx(self)
    }

    fn judge<'a>(&'a self, kind: &'a str, name: &'a str, prompt: String, cwd: Option<&'a str>) -> BoxFuture<'a, Result<String>> {
        Box::pin(async move {
            // Who the user chose for plugin vetting in this kind of thread (Settings > Handoffs)
            // judges it; otherwise the tidy engine with the review model, as before.
            let chosen = crate::handoff::pick(&self.ctx(), "vetting", cwd).await;
            let engine_name = match &chosen {
                Some(p) => &p.engine,
                None => self.config.tidy.engine.as_ref().unwrap_or(&self.config.default_engine),
            };
            let engine = self.config.engines.get(engine_name).with_context(|| format!("unknown engine {engine_name:?}"))?;
            let model = match chosen.as_ref().and_then(|p| p.model.clone()) {
                Some(m) => m,
                None => self.guard_prefs().await.ok().and_then(|g| g.review_model).unwrap_or_else(|| "sonnet".into()),
            };
            let cwd = std::env::temp_dir().display().to_string();
            crate::customs_review::asking(kind, name);
            let answer = crate::engine::complete("customs_review", engine, &cwd, Some(&model), prompt);
            tokio::time::timeout(Duration::from_secs(180), answer).await.context("the review took too long")?
        })
    }

    fn start_thread(&self, new: NewThread, owner: Owner) -> BoxFuture<'_, Result<()>> {
        Box::pin(async move { self.create_thread_as(new, None, Some(owner)).await.map(drop) })
    }
}

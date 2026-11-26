//! The `Hub` half of handing a thread's work to one of your tools (`own_tools/handoff.rs`): the
//! thread's engine drafts the brief and answers what the tool asks, and the handoff, its offer and
//! its end are noted in the thread. What the tool is handed and answers stays in `own_tools/`.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use super::Hub;
use crate::model::{HandedOffData, HandoffBrief, OwnToolRan, Thread, ToolAsk};
use crate::own_tools;

impl Hub {
    /// A brief for handing the thread's work to the tool, written from the thread's log with the
    /// tool's guide by the thread's own engine. The user reads and edits it before anything runs.
    pub async fn draft_handoff(&self, thread_id: &str, folder: &str) -> Result<HandoffBrief> {
        let (dir, manifest) = own_tools::found(&self.ctx(), folder).await?;
        manifest.handoff.as_ref().with_context(|| format!("{} takes no handoffs", manifest.name))?;
        let thread = self.store.thread(thread_id).await?.context("no such thread")?;
        let log = super::conversation_log(&self.store.events(thread_id, 0, 100_000).await?);
        let prompt = own_tools::draft_prompt(&manifest, &thread, &log)?;
        let engine = self.config.engines.get(&thread.engine).with_context(|| format!("unknown engine {:?}", thread.engine))?;
        let answer = crate::engine::complete("tool_handoff", engine, &thread.cwd, None, prompt);
        let answer = tokio::time::timeout(Duration::from_secs(300), answer).await.context("drafting took too long")??;
        let (title, brief) = own_tools::split_draft(&answer, &thread.title);
        Ok(HandoffBrief { folder: dir.display().to_string(), title, brief, build: false, answers: BTreeMap::new() })
    }

    /// One press on the agent's card ("Hand to Knegarn"), which is the approval: the brief is drafted,
    /// what the tool asks first is answered from the repository, and the work goes over with "Let it
    /// build" on. Nothing to read or fill in. Whatever stops it is noted in the thread as refused.
    pub async fn hand_off_now(&self, thread_id: &str, folder: &str) {
        if let Err(e) = self.try_hand_off_now(thread_id, folder).await {
            let tool = own_tools::found(&self.ctx(), folder).await.map(|(_, m)| m.name).unwrap_or_else(|_| folder.to_owned());
            let refused = HandedOffData { tool, title: String::new(), ok: false, output: format!("{e:#}"), folder: Some(folder.to_owned()), work: None };
            if let Err(e) = self.emit_typed(thread_id, &refused).await {
                tracing::warn!(thread = %thread_id, "noting a handoff that did not go over: {e:#}");
            }
        }
    }

    async fn try_hand_off_now(&self, thread_id: &str, folder: &str) -> Result<()> {
        let mut brief = self.draft_handoff(thread_id, folder).await?;
        brief.build = true;
        let mut ran = self.hand_off(thread_id, &brief).await?;
        if !ran.asks.is_empty() {
            let thread = self.store.thread(thread_id).await?.context("no such thread")?;
            brief.answers = self.handoff_answers(&thread, &ran.asks).await;
            ran = self.hand_off(thread_id, &brief).await?;
        }
        if !ran.asks.is_empty() {
            bail!("{}", ran.output);
        }
        Ok(())
    }

    /// What the tool asked before it takes the work, answered by the thread's engine from the
    /// folder's top files, else the tool's own example.
    async fn handoff_answers(&self, thread: &Thread, asks: &[ToolAsk]) -> BTreeMap<String, String> {
        let prompt = own_tools::answers_prompt(thread, asks);
        let said = match self.config.engines.get(&thread.engine) {
            Some(engine) => tokio::time::timeout(Duration::from_secs(120), crate::engine::complete("tool_answers", engine, &thread.cwd, None, prompt))
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or_default(),
            None => String::new(),
        };
        own_tools::read_answers(asks, &said)
    }

    /// Offers the user a handoff to the tool in a card in the thread, with the agent's reason. Nothing
    /// is handed over until the user presses the card.
    pub async fn offer_handoff(&self, thread_id: &str, folder: &str, why: &str) -> Result<String> {
        let offer = own_tools::offer(&self.ctx(), folder, why).await?;
        self.emit_typed(thread_id, &offer).await?;
        Ok(offer.label)
    }

    /// Hands the thread's work to the tool with the brief the user approved, and notes it in the thread.
    pub async fn hand_off(&self, thread_id: &str, approved: &HandoffBrief) -> Result<OwnToolRan> {
        let (ran, handed) = own_tools::hand_off(&self.ctx(), thread_id, approved).await?;
        if let Some(handed) = handed {
            self.emit_typed(thread_id, &handed).await?;
        }
        Ok(ran)
    }

    /// Asks the tools that took a thread's work how it went, and notes the end in the thread once:
    /// a `handoff_result` with what the tool says it proved and spent (`own_tools::follow`).
    pub async fn follow_handoffs(self: Arc<Self>) {
        tokio::time::sleep(Duration::from_secs(60)).await;
        loop {
            if let Err(err) = self.note_settled_handoffs().await {
                tracing::warn!("handoffs: {err:#}");
            }
            tokio::time::sleep(own_tools::FOLLOW_EVERY).await;
        }
    }

    async fn note_settled_handoffs(&self) -> Result<()> {
        for (thread_id, handoff) in own_tools::open_handoffs(&self.ctx()).await? {
            if let Some(result) = own_tools::follow(&self.ctx(), &handoff).await? {
                self.emit_typed(&thread_id, &result).await?;
            }
        }
        Ok(())
    }
}

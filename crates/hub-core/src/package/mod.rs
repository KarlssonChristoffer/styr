//! What Hub sends to an AI, in one place.
//!
//! Hub writes a lot of text to models on its own: the notes it puts before your messages, the
//! conversation it hands a fresh session after a model switch, the one-shot questions behind
//! titles, commit messages and reviews, the decisions it asks Jev for. This module is where that
//! is counted and, step by step, built. The plan is in `docs/ARCHITECTURE.md` ("What Hub sends
//! to an AI"); what exists now is the **slip**:
//!
//! - every package that goes to an engine gets a [`Slip`] (`ledger.rs`): what it was for, who
//!   got it, and how big each part was in tokens. Written when the package goes, so it is there
//!   when something fails after;
//! - a slip keeps sizes and a pointer (the thread and the message seq), never the text. The
//!   text is rebuilt from its sources, the thread's log first of all, so nothing is stored twice;
//! - nothing here changes what is sent. Building the same text another way comes later, as
//!   recipes, and each recipe's slip says so (`recipe`).
//!
//! <!-- see docs/ARCHITECTURE.md "What Hub sends to an AI" -->

pub mod ledger;

use crate::locked::Ignore;
use crate::model::{Slip, SlipSection, now_ms};

/// How many tokens a text is, estimated: one for about four characters, which is close for
/// English and code and a little low for Swedish. An engine's own count replaces it when it
/// reports one; until then every figure is marked `estimated`.
pub fn tokens(text: &str) -> u32 {
    let chars = text.chars().count();
    u32::try_from(chars.div_ceil(4)).unwrap_or(u32::MAX)
}

/// Characters of a text, the unit a part is measured in before it is turned into tokens.
pub(crate) fn chars(text: &str) -> usize {
    text.chars().count()
}

/// A package on its way to an engine. Parts are `(id, characters)`; whatever the parts do not
/// account for (the blank lines between them) shows as `other`, so the sections always add up.
pub struct Sent<'a> {
    pub purpose: &'a str,
    pub thread: Option<&'a str>,
    pub seq: Option<i64>,
    pub engine: &'a str,
    pub model: Option<&'a str>,
    /// The engine's session it goes into, when that is known by now.
    pub session: Option<&'a str>,
    pub text: &'a str,
    pub parts: &'a [(String, usize)],
    pub attachments: usize,
    /// The recipe that built it; `legacy` for text Hub assembled the old way.
    pub recipe: &'a str,
}

impl Sent<'_> {
    /// The slip for it, to be handed to [`ledger::record`].
    pub fn slip(&self) -> Slip {
        let total = chars(self.text);
        let named: usize = self.parts.iter().map(|(_, n)| *n).sum::<usize>().min(total);
        let mut sections: Vec<SlipSection> = self.parts.iter().map(|(id, n)| section(id, *n)).collect();
        if sections.is_empty() {
            sections.push(section("prompt", total));
        } else if total > named {
            sections.push(section("other", total - named));
        }
        Slip {
            id: uuid::Uuid::now_v7().to_string(),
            at: now_ms(),
            purpose: self.purpose.to_owned(),
            thread: self.thread.map(str::to_owned),
            seq: self.seq,
            engine: self.engine.to_owned(),
            model: self.model.map(str::to_owned),
            session: self.session.map(str::to_owned),
            tokens: tokens(self.text),
            estimated: true,
            sections,
            attachments: u32::try_from(self.attachments).unwrap_or(u32::MAX),
            recipe: self.recipe.to_owned(),
            fallback: false,
            // A thread's text is rebuilt from its log; only a one-shot needs a few words kept.
            head: if self.thread.is_some() { String::new() } else { self.text.chars().take(160).collect() },
        }
    }
}

fn section(id: &str, chars: usize) -> SlipSection {
    let chars = u32::try_from(chars).unwrap_or(u32::MAX);
    SlipSection { id: id.to_owned(), tokens: chars.div_ceil(4), chars, cut: false }
}

/// Hands a slip to the ledger's writer (`Hub::slips`); none outside an async runtime.
pub(crate) fn send(slips: Option<&tokio::sync::mpsc::UnboundedSender<Slip>>, slip: Slip) {
    if let Some(tx) = slips {
        tx.send(slip).ignore("the writer ends with the runtime of its hub (a test's may be gone)");
    }
}

/// Writes the slip for a one-shot question to a model, which has no thread.
pub(crate) fn asked(purpose: &str, engine: &crate::config::EngineConfig, model: Option<&str>, session: &str, prompt: &str) {
    ledger::record(question(purpose, engine, model, Some(session), prompt));
}

fn question(purpose: &str, engine: &crate::config::EngineConfig, model: Option<&str>, session: Option<&str>, prompt: &str) -> Slip {
    let model = model.or(engine.model.as_deref());
    Sent { purpose, thread: None, seq: None, engine: &engine_name(engine), model, session, text: prompt, parts: &[], attachments: 0, recipe: "legacy" }.slip()
}

/// The name Hub shows for the engine a one-shot ran on: its label, else its harness, else the
/// program it starts.
pub fn engine_name(engine: &crate::config::EngineConfig) -> String {
    engine
        .label
        .clone()
        .or_else(|| engine.harness.clone())
        .unwrap_or_else(|| {
            std::path::Path::new(engine.command.split_whitespace().next().unwrap_or_default())
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_characters_make_a_token_and_a_part_is_never_free() {
        assert_eq!(tokens(""), 0);
        assert_eq!(tokens("abcd"), 1);
        assert_eq!(tokens("abcde"), 2);
        // Swedish letters count as one character each, not as their bytes.
        assert_eq!(tokens("åäöå"), 1);
    }

    #[test]
    fn the_sections_add_up_to_the_whole() {
        let text = "note\n\nconversation\n\nhello";
        let parts = vec![("handover".to_owned(), 4 + 2 + 12), ("you".to_owned(), 5)];
        let r = Sent {
            purpose: "switch",
            thread: Some("t1"),
            seq: Some(7),
            engine: "claude",
            model: None,
            session: None,
            text,
            parts: &parts,
            attachments: 0,
            recipe: "legacy",
        }
        .slip();
        let sum: u32 = r.sections.iter().map(|s| s.chars).sum();
        assert_eq!(sum as usize, text.chars().count());
        assert_eq!(r.sections.last().unwrap().id, "other");
        assert_eq!(r.recipe, "legacy");
        assert!(r.estimated && !r.fallback);
        assert!(r.head.is_empty(), "a thread's text is rebuilt from its log");
    }

    #[test]
    fn parts_that_claim_too_much_do_not_break_the_sum() {
        let parts = vec![("you".to_owned(), 500)];
        let r = Sent { purpose: "message", thread: None, seq: None, engine: "e", model: None, session: None, text: "short", parts: &parts, attachments: 2, recipe: "legacy" }.slip();
        assert_eq!(r.attachments, 2);
        assert_eq!(r.head, "short", "a one-shot keeps its first words");
        assert!(r.sections.iter().all(|s| s.id != "other"));
    }

    #[test]
    fn a_one_shot_names_its_engine_by_label_then_harness_then_program() {
        use crate::config::EngineConfig;
        let mut e = EngineConfig { command: "/usr/bin/claude-agent-acp --flag".into(), ..Default::default() };
        assert_eq!(engine_name(&e), "claude-agent-acp");
        e.harness = Some("hermes".into());
        assert_eq!(engine_name(&e), "hermes");
        e.label = Some("Hermes · DeepSeek".into());
        e.model = Some("deepseek:deepseek-flash".into());
        let r = question("gist", &e, None, None, "hello");
        assert_eq!((r.engine.as_str(), r.model.as_deref(), r.purpose.as_str()), ("Hermes · DeepSeek", Some("deepseek:deepseek-flash"), "gist"));
        assert_eq!(question("gist", &e, Some("haiku"), None, "hello").model.as_deref(), Some("haiku"), "a helper's own model wins");
    }

    #[test]
    fn an_unsplit_prompt_is_one_section() {
        let r = Sent { purpose: "gist", thread: None, seq: None, engine: "e", model: Some("haiku"), session: Some("s-1"), text: "hello", parts: &[], attachments: 0, recipe: "legacy" }.slip();
        assert_eq!(r.sections.len(), 1);
        assert_eq!(r.sections[0].id, "prompt");
        assert_eq!(r.model.as_deref(), Some("haiku"));
    }
}

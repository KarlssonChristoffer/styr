//! What Hub puts in front of a message: notes about rolled-back work, clashes, design mode and handovers.

use super::*;

/// The engine switch, rewind or move since the last finished turn that the engine's session
/// has not been told about yet.
pub(super) fn switch_since(since_turn: &[&Event]) -> Option<Event> {
    since_turn
        .iter()
        .enumerate()
        .find(|(i, e)| {
            // A thread moved here carried its conversation with the message that carried on
            // its cut-off turn; only the ones after that are new to the session.
            let told = e.kind == EventKind::Moved
                && since_turn[..*i].iter().any(|l| l.kind == EventKind::UserMessage && l.mark("moved").as_i64() == Some(e.seq));
            matches!(e.kind, EventKind::EngineSwitch | EventKind::Rewind | EventKind::Moved) && !told
        })
        .map(|(_, e)| (**e).clone())
}

/// What `add_notes` made of a message.
pub(super) struct Notes {
    pub(super) text: String,
    /// How many characters Hub added, by what for (`package/`).
    pub(super) parts: Vec<(String, usize)>,
    /// Set when a handover decides what the message is for; else it is the sender's own.
    pub(super) purpose: Option<&'static str>,
    pub(super) recipe: &'static str,
}

/// What Hub adds in front of the words: that rolled-back work is gone, that another thread changes
/// the same files, that design mode changed, and the conversation after a handover.
pub(super) fn add_notes(mut text: String, since: &[&Event], switched: Option<&Event>, previous: &[Event], thread_id: &str) -> Notes {
    // What Hub adds in front of the words, for the slip (`package/`).
    let mut parts: Vec<(String, usize)> = Vec::new();
    let rewound = switched.is_some_and(|e| e.kind == EventKind::Rewind);
    if !rewound && since.iter().any(|e| e.kind == EventKind::Restored) {
        let before = text.chars().count();
        text = format!("{ROLLED_BACK}\n\n{text}");
        parts.push(("rolled_back".into(), text.chars().count() - before));
    }
    // Another thread is changing the same files; say so once, with the next message.
    let mut clashes: Vec<(String, Vec<String>)> = Vec::new();
    for clash in since.iter().rev().filter_map(|e| e.data_as::<ClashData>()) {
        let title = clash.other_title.unwrap_or_else(|| "another thread".to_owned());
        let path = clash.path;
        match clashes.iter_mut().find(|(t, _)| *t == title) {
            Some((_, paths)) => paths.push(path),
            None => clashes.push((title, vec![path])),
        }
    }
    for (title, paths) in clashes {
        let before = text.chars().count();
        text = format!(
            "Hub: the thread \"{title}\" is also changing {} in this folder. Check the current contents before you change them again, and do not undo its work.\n\n{text}",
            paths.join(", ")
        );
        parts.push(("clash".into(), text.chars().count() - before));
    }
    // Design mode changed, or a side job ended without waking the agent: said once, with this one.
    for (what, note) in [("design", crate::design::note_since(since).map(str::to_owned)), ("child", crate::tool_instructions::quiet_side_jobs(since))].into_iter().filter_map(|(w, n)| Some((w, n?))) {
        let before = text.chars().count();
        text = format!("{note}\n\n{text}");
        parts.push((what.into(), text.chars().count() - before));
    }
    let mut purpose = None;
    let mut recipe = "legacy";
    if let Some(switch) = switched {
        let (log, made_by) = handover_log(thread_id, previous);
        recipe = made_by;
        purpose = match switch.kind {
            EventKind::Rewind => "rewind",
            EventKind::Moved => "moved",
            _ => "switch",
        }
        .into();
        let note = if switch.kind == EventKind::Rewind {
            rewound_note(switch.data_as::<RewindData>().is_some_and(|r| r.files))
        } else if switch.kind == EventKind::Moved {
            moved_note(written(&switch.data_as::<MovedData>().unwrap_or_default().from).unwrap_or("another machine"))
        } else {
            switched_note(written(&switch.data_as::<EngineSwitchData>().unwrap_or_default().from).unwrap_or("another agent"))
        };
        let before = text.chars().count();
        text = format!("{note}\n\n<conversation>{log}\n</conversation>\n\n{text}");
        parts.push(("handover".into(), text.chars().count() - before));
    }
    Notes { text, parts, purpose, recipe }
}

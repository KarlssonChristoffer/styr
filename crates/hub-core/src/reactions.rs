//! Reactions: an emoji on a message, from the user or from the agent, and the user's own emojis.
//!
//! A reaction is a `reaction` event (`{ seq, emoji, by, on }`) on the message it is about, a user
//! message or the `turn_end` closing an agent's reply. The latest event per message, emoji and
//! `by` is the state, so taking one away is another event, never an edit of the old one. A
//! reaction never starts a turn, changes the thread's time or counts as news.
//!
//! The user's own emojis are small pictures kept in `<data folder>/emoji/<name>.<ext>`, named in
//! a reaction as `:name:`. Which emojis the user reaches for most is counted here too, so the
//! quick row of the picker is theirs on every device.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ctx::Ctx;
use crate::model::{Event, EventKind, ReactionData};

/// What a new user's quick row holds until their own use says otherwise.
pub const STARTERS: [&str; 6] = ["👍", "❤️", "🎉", "👀", "🚀", "🙏"];
/// How many emojis the quick row shows.
pub const QUICK: usize = 7;
/// The biggest picture of an emoji, decoded. The app shrinks them to about 96 px first.
pub const MAX_BYTES: usize = 96 * 1024;
/// How many own emojis there can be.
pub const MAX_CUSTOM: usize = 300;
const USAGE_KEY: &str = "emoji:usage";

/// One of the user's own emojis, with its picture.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CustomEmoji {
    pub name: String,
    /// `image/png`, `image/webp`, `image/gif` or `image/jpeg`.
    pub mime: String,
    /// The picture, base64.
    pub data: String,
}

/// What `GET /api/emoji` answers: the user's own emojis and the ones they use most.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EmojiState {
    pub custom: Vec<CustomEmoji>,
    /// Most used first, at most [`QUICK`], filled up with [`STARTERS`].
    pub quick: Vec<String>,
}

/// `{ seq, emoji, on? }` to `POST /api/threads/{id}/react`.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct React {
    #[ts(type = "number")]
    pub seq: i64,
    pub emoji: String,
    /// False takes the reaction away. Missing means true.
    #[serde(default)]
    pub on: Option<bool>,
}

fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("emoji")
}

/// A name a custom emoji can have: lower-case letters, digits, `_` and `-`, up to 32.
pub fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && name.len() <= 32
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

fn ext_of(mime: &str) -> Option<&'static str> {
    match mime {
        "image/png" => Some("png"),
        "image/webp" => Some("webp"),
        "image/gif" => Some("gif"),
        "image/jpeg" => Some("jpg"),
        _ => None,
    }
}

fn mime_of(ext: &str) -> Option<&'static str> {
    match ext {
        "png" => Some("image/png"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        "jpg" => Some("image/jpeg"),
        _ => None,
    }
}

/// Whether the picture's first bytes are what its type says, so a file of another kind is not
/// kept under an image name.
fn looks_like(mime: &str, bytes: &[u8]) -> bool {
    match mime {
        "image/png" => bytes.starts_with(&[0x89, b'P', b'N', b'G']),
        "image/webp" => bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP",
        "image/gif" => bytes.starts_with(b"GIF8"),
        "image/jpeg" => bytes.starts_with(&[0xff, 0xd8]),
        _ => false,
    }
}

/// The `:name:` of a custom emoji, or None for anything else.
pub fn custom_name(emoji: &str) -> Option<&str> {
    emoji.strip_prefix(':')?.strip_suffix(':').filter(|n| valid_name(n))
}

/// The user's own emojis, by name.
pub fn list_custom(data_dir: &Path) -> Vec<CustomEmoji> {
    let Ok(entries) = std::fs::read_dir(dir(data_dir)) else { return Vec::new() };
    let mut found: Vec<CustomEmoji> = entries
        .filter_map(|e| {
            let path = e.ok()?.path();
            let name = path.file_stem()?.to_str()?.to_owned();
            let mime = mime_of(path.extension()?.to_str()?)?;
            valid_name(&name).then_some(())?;
            let bytes = std::fs::read(&path).ok()?;
            Some(CustomEmoji { name, mime: mime.into(), data: base64::engine::general_purpose::STANDARD.encode(bytes) })
        })
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// The names only, for an agent's tool description.
pub fn custom_names(data_dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir(data_dir)) else { return Vec::new() };
    let mut names: Vec<String> = entries
        .filter_map(|e| {
            let path = e.ok()?.path();
            mime_of(path.extension()?.to_str()?)?;
            let name = path.file_stem()?.to_str()?.to_owned();
            valid_name(&name).then_some(name)
        })
        .collect();
    names.sort();
    names
}

/// Keeps a picture under `name`, replacing one of the same name.
pub fn save_custom(data_dir: &Path, name: &str, mime: &str, data: &str) -> Result<()> {
    if !valid_name(name) {
        bail!("An emoji name is lower-case letters, digits, _ and -, up to 32 characters.");
    }
    let ext = ext_of(mime).with_context(|| format!("{mime} is not a picture type an emoji can be (png, webp, gif or jpeg)"))?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(data.trim()).context("the picture is not valid base64")?;
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        bail!("An emoji picture is at most {} KB; shrink it first.", MAX_BYTES / 1024);
    }
    if !looks_like(mime, &bytes) {
        bail!("That is not a {mime} picture.");
    }
    let folder = dir(data_dir);
    std::fs::create_dir_all(&folder)?;
    if list_names_in(&folder).len() >= MAX_CUSTOM && !list_names_in(&folder).iter().any(|n| n == name) {
        bail!("There are already {MAX_CUSTOM} emojis; remove one first.");
    }
    remove_files(&folder, name);
    std::fs::write(folder.join(format!("{name}.{ext}")), bytes)?;
    Ok(())
}

fn list_names_in(folder: &Path) -> Vec<String> {
    std::fs::read_dir(folder)
        .map(|d| d.filter_map(|e| Some(e.ok()?.path().file_stem()?.to_str()?.to_owned())).collect())
        .unwrap_or_default()
}

fn remove_files(folder: &Path, name: &str) {
    for ext in ["png", "webp", "gif", "jpg"] {
        crate::locked::clean_file(folder.join(format!("{name}.{ext}")));
    }
}

/// Removes one of the user's emojis. Reactions that name it stay in the log and show as the name.
pub fn delete_custom(data_dir: &Path, name: &str) -> Result<()> {
    if !valid_name(name) {
        bail!("No such emoji.");
    }
    remove_files(&dir(data_dir), name);
    Ok(())
}

/// What a surface needs to draw its picker: own emojis and the quick row.
pub async fn state(ctx: &Ctx<'_>) -> Result<EmojiState> {
    let data_dir = ctx.config.data_dir.clone();
    let custom = tokio::task::spawn_blocking(move || list_custom(&data_dir)).await?;
    Ok(EmojiState { custom, quick: quick_row(&usage(ctx).await?) })
}

/// The quick row for these counts: most used first, then the starters that are missing.
pub fn quick_row(counts: &BTreeMap<String, u64>) -> Vec<String> {
    let mut used: Vec<(&String, &u64)> = counts.iter().filter(|(_, n)| **n > 0).collect();
    used.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let mut row: Vec<String> = used.into_iter().take(QUICK).map(|(e, _)| e.clone()).collect();
    for starter in STARTERS {
        if row.len() >= QUICK {
            break;
        }
        if !row.iter().any(|e| e == starter) {
            row.push(starter.to_owned());
        }
    }
    row
}

async fn usage(ctx: &Ctx<'_>) -> Result<BTreeMap<String, u64>> {
    Ok(ctx.store.get_value(USAGE_KEY).await?.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default())
}

/// One more use of `emoji` by the user, for the quick row.
pub async fn count(ctx: &Ctx<'_>, emoji: &str) -> Result<()> {
    let mut counts = usage(ctx).await?;
    *counts.entry(emoji.to_owned()).or_default() += 1;
    ctx.store.set_value(USAGE_KEY, &serde_json::to_string(&counts)?).await
}

/// Whether `emoji` can be a reaction: one Unicode emoji (no letters or spaces, a few characters
/// at most) or the `:name:` of an existing custom emoji.
pub fn check_emoji(data_dir: &Path, emoji: &str) -> Result<()> {
    if emoji.is_empty() || emoji.len() > 64 {
        bail!("Give one emoji.");
    }
    if emoji.starts_with(':') {
        let name = custom_name(emoji).with_context(|| format!("{emoji} is not an emoji name"))?;
        if !custom_names(data_dir).iter().any(|n| n == name) {
            bail!("There is no emoji named :{name}:.");
        }
        return Ok(());
    }
    if emoji.chars().any(|c| c.is_ascii_alphanumeric() || c.is_whitespace()) || emoji.chars().count() > 12 {
        bail!("Give one emoji, or the :name: of one of the user's own.");
    }
    Ok(())
}

/// Whether the log already says `emoji` is on the message at `seq` from `by`.
pub fn is_on(events: &[Event], seq: i64, emoji: &str, by: &str) -> bool {
    events
        .iter()
        .rev()
        .filter_map(Event::data_as::<ReactionData>)
        .find(|r| r.seq == Some(seq) && r.emoji.as_deref() == Some(emoji) && r.by.as_deref() == Some(by))
        .is_some_and(|r| r.on.unwrap_or(true))
}

/// The reaction to log for putting `emoji` on the message at `seq`, or taking it away. `by` is
/// "you" or "agent". None when it would change nothing (it was on already, or never on).
/// `Hub::react` (`hub/reactions.rs`) logs it.
pub async fn reaction(ctx: &Ctx<'_>, thread_id: &str, seq: i64, emoji: &str, on: bool, by: &str) -> Result<Option<ReactionData>> {
    check_emoji(&ctx.config.data_dir, emoji)?;
    ctx.store.thread(thread_id).await?.context("no such thread")?;
    let target = ctx.store.events(thread_id, seq - 1, 1).await?.into_iter().find(|e| e.seq == seq).context("no such message")?;
    if !matches!(target.kind, EventKind::UserMessage | EventKind::TurnEnd) {
        bail!("A reaction goes on a message of the user or an agent's reply.");
    }
    if is_on(&ctx.store.events_of(thread_id, 0, &[EventKind::Reaction]).await?, seq, emoji, by) == on {
        return Ok(None);
    }
    Ok(Some(ReactionData { seq: Some(seq), emoji: Some(emoji.to_owned()), by: Some(by.to_owned()), on: Some(on) }))
}

/// The user message the thread's latest turn answers: what an agent's `react` goes on.
pub fn latest_user_message(events: &[Event]) -> Option<i64> {
    events
        .iter()
        .rev()
        .find(|e| {
            e.kind == EventKind::UserMessage
                && !["review", "design", "leak", "proof", "customs", "docs", "quota", "restart", "terminal", "handoff", "rewind", "child"]
                    .iter()
                    .any(|k| !e.mark(k).is_null())
        })
        .map(|e| e.seq)
}

/// What an agent is told about reacting, with the user's own emoji names.
pub fn tool_description(data_dir: &Path) -> String {
    let names = custom_names(data_dir);
    let own = if names.is_empty() {
        String::new()
    } else {
        format!(" The user's own emojis you can also use: {}.", names.iter().map(|n| format!(":{n}:")).collect::<Vec<_>>().join(" "))
    };
    format!(
        "Puts an emoji reaction on the user's latest message, the way a colleague gives a thumbs up. Use it when a reaction alone is the \
answer, for example 👍 when the user writes \"merge\" or \"go ahead\", then do the work, or 👀 when you start on something that will take \
a while. Do not use it to avoid answering a question, and never more than one or two per message. It is quiet: the user gets no \
notification.{own}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use serde_json::Value;

    fn ev(seq: i64, kind: EventKind, data: Value) -> Event {
        Event { thread_id: "t".into(), seq, ts: seq, kind, data }
    }

    #[test]
    fn the_latest_event_is_the_state() {
        let mut events = vec![ev(1, EventKind::UserMessage, json!({ "text": "merge" }))];
        assert!(!is_on(&events, 1, "👍", "agent"));
        events.push(ev(2, EventKind::Reaction, json!({ "seq": 1, "emoji": "👍", "by": "agent", "on": true })));
        assert!(is_on(&events, 1, "👍", "agent"));
        assert!(!is_on(&events, 1, "👍", "you"), "each side has its own");
        assert!(!is_on(&events, 1, "🎉", "agent"));
        events.push(ev(3, EventKind::Reaction, json!({ "seq": 1, "emoji": "👍", "by": "agent", "on": false })));
        assert!(!is_on(&events, 1, "👍", "agent"));
    }

    #[test]
    fn names_and_emojis_are_checked() {
        assert!(valid_name("merged") && valid_name("ship-it_2"));
        assert!(!valid_name("") && !valid_name("Merged") && !valid_name("a b") && !valid_name("-x") && !valid_name(&"a".repeat(33)));
        let dir = tempfile::tempdir().unwrap();
        assert!(check_emoji(dir.path(), "👍").is_ok());
        assert!(check_emoji(dir.path(), "❤️").is_ok());
        assert!(check_emoji(dir.path(), "🏳️‍🌈").is_ok());
        assert!(check_emoji(dir.path(), "").is_err());
        assert!(check_emoji(dir.path(), "yes").is_err());
        assert!(check_emoji(dir.path(), "👍 ok").is_err());
        assert!(check_emoji(dir.path(), ":merged:").is_err(), "not made yet");
    }

    #[test]
    fn own_emojis_are_kept_checked_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let png = base64::engine::general_purpose::STANDARD.encode([0x89, b'P', b'N', b'G', 1, 2, 3]);
        save_custom(dir.path(), "merged", "image/png", &png).unwrap();
        assert!(save_custom(dir.path(), "Bad Name", "image/png", &png).is_err());
        assert!(save_custom(dir.path(), "x", "text/html", &png).is_err());
        assert!(save_custom(dir.path(), "x", "image/gif", &png).is_err(), "the bytes must match the type");
        assert!(save_custom(dir.path(), "x", "image/png", "!!!").is_err());
        let big = base64::engine::general_purpose::STANDARD.encode(vec![0x89; MAX_BYTES + 1]);
        assert!(save_custom(dir.path(), "x", "image/png", &big).is_err());
        let list = list_custom(dir.path());
        assert_eq!((list.len(), list[0].name.as_str(), list[0].mime.as_str()), (1, "merged", "image/png"));
        assert!(check_emoji(dir.path(), ":merged:").is_ok());
        // The same name again replaces it, also with another type.
        let gif = base64::engine::general_purpose::STANDARD.encode(b"GIF89a....");
        save_custom(dir.path(), "merged", "image/gif", &gif).unwrap();
        assert_eq!(list_custom(dir.path()).len(), 1);
        assert_eq!(list_custom(dir.path())[0].mime, "image/gif");
        delete_custom(dir.path(), "merged").unwrap();
        assert!(list_custom(dir.path()).is_empty());
    }

    #[test]
    fn the_quick_row_is_what_you_use_then_the_starters() {
        assert_eq!(quick_row(&BTreeMap::new()), STARTERS.map(String::from).to_vec());
        let counts: BTreeMap<String, u64> = [("✅", 9), ("🔥", 4), ("🚀", 4), ("👍", 1)].into_iter().map(|(e, n)| (e.to_owned(), n)).collect();
        let row = quick_row(&counts);
        assert_eq!(&row[..4], ["✅", "🔥", "🚀", "👍"], "most used first, ties by name");
        assert_eq!(row.len(), QUICK);
        assert!(row.contains(&"❤️".to_string()), "filled up with starters");
        assert_eq!(row.iter().filter(|e| *e == "🚀").count(), 1);
    }

    #[test]
    fn the_agent_reacts_to_the_users_own_message() {
        let events = vec![
            ev(1, EventKind::UserMessage, json!({ "text": "fix it" })),
            ev(2, EventKind::TurnEnd, json!({})),
            ev(3, EventKind::UserMessage, json!({ "text": "merge" })),
            ev(4, EventKind::UserMessage, json!({ "text": "Hub's own request", "proof": 2 })),
        ];
        assert_eq!(latest_user_message(&events), Some(3));
    }
}

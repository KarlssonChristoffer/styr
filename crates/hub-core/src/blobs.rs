//! Generated pictures kept once, as files. An engine sends a picture as base64 inside an ACP
//! image block, and Codex repeats it in `rawOutput.result` of the same tool call, so one picture
//! used to sit twice in one row of the event log (60 % of a 424 MB database). Now the store lifts
//! the picture out when it writes the event: the bytes go to `<data dir>/blobs/<sha256>.<ext>`
//! (named by their content, so the same picture is one file), the row keeps `blob` instead of
//! `data`, and the repeat is dropped. Readers do not notice: the store puts `data` back when it
//! reads the row (`inline`), so the app, the phone, side jobs and a moved thread see what they
//! always saw. Rows written before this keep their `data` until the one-time job in `migrate`.
//! <!-- covers: crates/hub-core/src/blobs.rs -->

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use base64::Engine as _;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// A picture smaller than this stays in the row: the file and its name would cost more.
const MIN_BASE64: usize = 4096;

/// The folder of a database file: `blobs/` beside it.
pub fn dir_of(db: &Path) -> Option<PathBuf> {
    db.parent().map(|p| p.join("blobs"))
}

fn is_name(name: &str) -> bool {
    let Some((stem, ext)) = name.split_once('.') else { return false };
    stem.len() == 64 && stem.bytes().all(|b| b.is_ascii_hexdigit()) && (1..=5).contains(&ext.len()) && ext.bytes().all(|b| b.is_ascii_alphanumeric())
}

fn ext_of(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "bin",
    }
}

fn is_picture(o: &serde_json::Map<String, Value>) -> bool {
    o.get("type").and_then(Value::as_str) == Some("image")
}

fn has_big_picture(v: &Value) -> bool {
    match v {
        Value::Object(o) => {
            (is_picture(o) && o.get("data").and_then(Value::as_str).is_some_and(|d| d.len() >= MIN_BASE64)) || o.values().any(has_big_picture)
        }
        Value::Array(a) => a.iter().any(has_big_picture),
        _ => false,
    }
}

/// Writes the bytes under their content name (once) and returns the name.
fn keep(dir: &Path, bytes: &[u8], mime: &str) -> Result<String> {
    let digest = Sha256::digest(bytes);
    let name = format!("{}.{}", digest.iter().map(|b| format!("{b:02x}")).collect::<String>(), ext_of(mime));
    let path = dir.join(&name);
    if path.metadata().is_ok_and(|m| m.len() == bytes.len() as u64) {
        return Ok(name);
    }
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let tmp = dir.join(format!("{name}.{}.{}.tmp", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    std::fs::write(&tmp, bytes).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("placing {}", path.display()))?;
    Ok(name)
}

fn lift_in(dir: &Path, v: &mut Value, lifted: &mut Vec<String>) -> Result<()> {
    match v {
        Value::Object(o) => {
            if is_picture(o)
                && let Some(data) = o.get("data").and_then(Value::as_str).filter(|d| d.len() >= MIN_BASE64) {
                    // Only a picture that encodes back to the very same text is lifted, so what
                    // `inline` returns is exactly what the engine sent.
                    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data)
                        && base64::engine::general_purpose::STANDARD.encode(&bytes) == data {
                            let mime = o.get("mimeType").and_then(Value::as_str).unwrap_or("image/png").to_owned();
                            let name = keep(dir, &bytes, &mime)?;
                            if let Some(Value::String(text)) = o.remove("data") {
                                lifted.push(text);
                            }
                            o.insert("blob".into(), Value::String(name));
                            return Ok(());
                        }
                }
            for child in o.values_mut() {
                lift_in(dir, child, lifted)?;
            }
        }
        Value::Array(a) => {
            for child in a {
                lift_in(dir, child, lifted)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Removes every string equal to a lifted picture (Codex's `rawOutput.result`).
fn drop_copies(v: &mut Value, lifted: &[String]) {
    match v {
        Value::Object(o) => {
            o.retain(|_, child| !child.as_str().is_some_and(|s| lifted.iter().any(|l| l == s)));
            for child in o.values_mut() {
                drop_copies(child, lifted);
            }
        }
        Value::Array(a) => {
            a.retain(|child| !child.as_str().is_some_and(|s| lifted.iter().any(|l| l == s)));
            for child in a {
                drop_copies(child, lifted);
            }
        }
        _ => {}
    }
}

/// The event data as the log keeps it: pictures as files, none twice. None when there is nothing
/// to lift (the usual case, and then nothing is copied). When writing a file fails the picture
/// stays in the row.
pub fn lift(dir: &Path, data: &Value) -> Option<Value> {
    if !has_big_picture(data) {
        return None;
    }
    let mut out = data.clone();
    let mut lifted = Vec::new();
    if let Err(err) = lift_in(dir, &mut out, &mut lifted) {
        tracing::warn!("keeping a picture in the log, not as a file: {err:#}");
        return None;
    }
    if lifted.is_empty() {
        return None;
    }
    drop_copies(&mut out, &lifted);
    Some(out)
}

/// Adds the file name of every picture lifted out of `v` (a `blob` in an image block).
pub fn names_in(v: &Value, out: &mut std::collections::HashSet<String>) {
    match v {
        Value::Object(o) => {
            if is_picture(o)
                && let Some(name) = o.get("blob").and_then(Value::as_str).filter(|n| is_name(n)) {
                    out.insert(name.to_owned());
                }
            o.values().for_each(|c| names_in(c, out));
        }
        Value::Array(a) => a.iter().for_each(|c| names_in(c, out)),
        _ => {}
    }
}

/// Puts the pictures back into event data read from the log. A file that is gone leaves the block
/// without `data`, which the app already treats as nothing to show.
pub fn inline(dir: &Path, v: &mut Value) {
    match v {
        Value::Object(o) => {
            if is_picture(o) && !o.contains_key("data") {
                if let Some(name) = o.get("blob").and_then(Value::as_str).filter(|n| is_name(n)) {
                    match std::fs::read(dir.join(name)) {
                        Ok(bytes) => {
                            let text = base64::engine::general_purpose::STANDARD.encode(bytes);
                            o.insert("data".into(), Value::String(text));
                            o.remove("blob");
                        }
                        Err(err) => tracing::warn!("picture file {name} is missing: {err}"),
                    }
                }
                return;
            }
            for child in o.values_mut() {
                inline(dir, child);
            }
        }
        Value::Array(a) => {
            for child in a {
                inline(dir, child);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn big() -> String {
        base64::engine::general_purpose::STANDARD.encode((0..6000u32).map(|i| (i * 7 % 251) as u8).collect::<Vec<_>>())
    }

    fn tool_call(data: &str) -> Value {
        json!({
            "sessionUpdate": "tool_call_update",
            "toolCallId": "t1",
            "content": [{ "type": "content", "content": { "type": "image", "mimeType": "image/png", "data": data, "uri": "/tmp/a.png" } }],
            "rawOutput": { "result": data, "status": "ok" },
        })
    }

    #[test]
    fn a_picture_is_kept_once_as_a_file_and_comes_back_as_it_was() {
        let dir = tempfile::tempdir().unwrap();
        let data = big();
        let before = tool_call(&data);
        let lifted = lift(dir.path(), &before).expect("lifted");
        let text = lifted.to_string();
        assert!(!text.contains(&data), "no copy of the picture is left in the row");
        assert!(lifted["rawOutput"].get("result").is_none());
        assert_eq!(lifted["rawOutput"]["status"], "ok");
        let name = lifted["content"][0]["content"]["blob"].as_str().unwrap().to_owned();
        assert!(name.ends_with(".png") && dir.path().join(&name).is_file());
        let mut back = lifted.clone();
        inline(dir.path(), &mut back);
        assert_eq!(back["content"][0]["content"]["data"], data.as_str());
        assert_eq!(back["content"][0]["content"]["uri"], "/tmp/a.png");
        assert!(back["content"][0]["content"].get("blob").is_none());
    }

    #[test]
    fn the_same_picture_is_one_file_and_small_ones_stay_in_the_row() {
        let dir = tempfile::tempdir().unwrap();
        let data = big();
        lift(dir.path(), &tool_call(&data)).unwrap();
        lift(dir.path(), &tool_call(&data)).unwrap();
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        let small = json!({ "content": { "type": "image", "data": "iVBORw0KGgo=" } });
        assert!(lift(dir.path(), &small).is_none());
        assert!(lift(dir.path(), &json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "hi" } })).is_none());
    }

    #[test]
    fn text_that_does_not_encode_back_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let odd = format!("{}\n", big());
        assert!(lift(dir.path(), &json!({ "content": { "type": "image", "data": odd } })).is_none());
    }

    #[test]
    fn a_missing_file_or_a_strange_name_leaves_the_block_without_data() {
        let dir = tempfile::tempdir().unwrap();
        let mut v = json!({ "content": { "type": "image", "blob": format!("{}.png", "a".repeat(64)) } });
        inline(dir.path(), &mut v);
        assert!(v["content"].get("data").is_none());
        let mut v = json!({ "content": { "type": "image", "blob": "../../etc/passwd" } });
        inline(dir.path(), &mut v);
        assert!(v["content"].get("data").is_none());
    }
}

//! The leak guard: spots what looks like an API key, a token or a private key in what a thread
//! wrote, before it is committed or pushed. After each turn the core scans the lines the turn
//! added to the repository (ignored files are never in that diff, so a key in an ignored `.env`
//! is fine), and the same patterns keep keys out of Hub's own log.
//!
//! Known shapes are matched by prefix (Anthropic, OpenAI, GitHub, AWS, Slack, Google, Stripe,
//! Hugging Face, private keys). Anything else counts only when it is assigned to a name that says
//! key, secret, token or password and looks random. The values in Hub's vault are matched as
//! they are, since they are secrets by definition.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;

/// One thing that looks like a secret.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    /// What it looks like, as a person would say it: "Anthropic API key".
    pub kind: String,
    /// The secret with all but its start and end hidden, safe to show and log.
    pub hint: String,
    /// Byte range in the text it was found in.
    pub start: usize,
    pub end: usize,
}

static KNOWN: LazyLock<Vec<(&'static str, Regex)>> = LazyLock::new(|| {
    [
        ("Anthropic API key", r"sk-ant-[A-Za-z0-9_\-]{20,}"),
        ("OpenAI API key", r"sk-(?:proj-|svcacct-)?[A-Za-z0-9_\-]{32,}"),
        ("GitHub token", r"(?:gh[pousr]_[A-Za-z0-9]{36,}|github_pat_[A-Za-z0-9_]{40,})"),
        ("AWS access key", r"(?:AKIA|ASIA)[0-9A-Z]{16}"),
        ("Slack token", r"xox[abprs]-[A-Za-z0-9\-]{10,}"),
        ("Google API key", r"AIza[0-9A-Za-z_\-]{35}"),
        ("Stripe key", r"(?:sk|rk)_live_[0-9A-Za-z]{20,}"),
        ("Hugging Face token", r"hf_[A-Za-z0-9]{30,}"),
        ("private key", r"-----BEGIN (?:[A-Z]+ )*PRIVATE KEY-----"),
    ]
    .into_iter()
    .map(|(kind, re)| (kind, Regex::new(re).expect("leak pattern")))
    .collect()
});

/// `api_key = "…"`, `"token": "…"`, `PASSWORD=…`: a name that says secret, then a quoted or bare value.
static ASSIGNED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)\b[A-Za-z0-9_.\-]*(?:api[_\-]?key|secret|token|passw(?:or)?d|private[_\-]?key|access[_\-]?key)[A-Za-z0-9_\-]*["']?\s*[:=]\s*["']?([A-Za-z0-9+/_\-.=]{20,})"#,
    )
    .expect("assignment pattern")
});

/// Every secret-looking string in `text`, known kinds first, without overlaps.
pub fn find(text: &str, vault: &BTreeMap<String, String>) -> Vec<Found> {
    let mut found: Vec<Found> = Vec::new();
    let mut add = |kind: String, start: usize, end: usize| {
        if found.iter().any(|f| start < f.end && f.start < end) {
            return;
        }
        found.push(Found { kind, hint: hint(&text[start..end]), start, end });
    };
    for (name, value) in vault {
        if value.len() < 12 {
            continue;
        }
        for (start, _) in text.match_indices(value.as_str()) {
            add(format!("{name} from Hub's vault"), start, start + value.len());
        }
    }
    for (kind, re) in KNOWN.iter() {
        for m in re.find_iter(text) {
            if stands_alone(text, m.start(), m.end()) {
                add((*kind).to_owned(), m.start(), m.end());
            }
        }
    }
    for c in ASSIGNED.captures_iter(text) {
        let value = c.get(1).expect("group");
        if looks_random(value.as_str()) {
            add("secret".to_owned(), value.start(), value.end());
        }
    }
    found.sort_by_key(|f| f.start);
    found
}

/// Replaces each secret in `text` by its hint, so a key the agent wrote or echoed is not kept in
/// the log. Vault values are named instead, as `[secret NAME]`.
pub fn redact(text: &str, vault: &BTreeMap<String, String>) -> Option<String> {
    let found = find(text, vault);
    if found.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for f in &found {
        out.push_str(&text[at..f.start]);
        match f.kind.strip_suffix(" from Hub's vault") {
            Some(name) => out.push_str(&format!("[secret {name}]")),
            None => out.push_str(&format!("[{} {}]", f.kind, f.hint)),
        }
        at = f.end;
    }
    out.push_str(&text[at..]);
    Some(out)
}

/// A known key only counts when it is not part of a longer run of key or base64 characters, so
/// `AIza…` in the middle of an image's base64 is left alone. The regex crate has no lookaround,
/// hence the check by hand.
fn stands_alone(text: &str, start: usize, end: usize) -> bool {
    let run = |c: char| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '_' | '-');
    let before = text[..start].chars().next_back().is_none_or(|c| !run(c));
    let after = text[end..].chars().next().is_none_or(|c| !run(c));
    before && after
}

/// Every string in `data`, redacted in place. Binary content is skipped: the base64 `data` of an
/// attachment or image (an object that also says its `mime` or `mimeType`) and a resource's
/// `blob`. Rewriting it would only corrupt the file, and it goes to the engine again as logged.
pub fn redact_json(data: &mut serde_json::Value, vault: &BTreeMap<String, String>) {
    match data {
        serde_json::Value::String(text) => {
            if let Some(clean) = redact(text, vault) {
                *text = clean;
            }
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(|v| redact_json(v, vault)),
        serde_json::Value::Object(map) => {
            let binary = map.contains_key("mime") || map.contains_key("mimeType");
            for (key, v) in map.iter_mut() {
                if key == "blob" || (binary && key == "data" && v.is_string()) {
                    continue;
                }
                redact_json(v, vault);
            }
        }
        _ => {}
    }
}

/// A secret in a diff: the file, the line in the new version, and what it looks like.
#[derive(Debug, Clone, PartialEq)]
pub struct InFile {
    pub path: String,
    pub line: usize,
    pub kind: String,
    pub hint: String,
}

/// Secrets on the lines a unified diff of one file adds.
pub fn in_diff(path: &str, diff: &str, vault: &BTreeMap<String, String>) -> Vec<InFile> {
    let mut out = Vec::new();
    let mut line = 0usize;
    for l in diff.lines() {
        if let Some(hunk) = l.strip_prefix("@@ ") {
            // @@ -a,b +c,d @@: the next line is line c of the new file.
            line = hunk
                .split_whitespace()
                .find_map(|p| p.strip_prefix('+'))
                .and_then(|p| p.split(',').next()?.parse().ok())
                .unwrap_or(1);
            continue;
        }
        if l.starts_with("+++") || l.starts_with("---") {
            continue;
        }
        if let Some(added) = l.strip_prefix('+') {
            for f in find(added, vault) {
                out.push(InFile { path: path.to_owned(), line, kind: f.kind, hint: f.hint });
            }
            line += 1;
        } else if !l.starts_with('-') && !l.starts_with('\\') {
            line += 1;
        }
    }
    out
}

/// The first four and last four characters, enough to recognise a key without giving it away.
fn hint(secret: &str) -> String {
    let chars: Vec<char> = secret.chars().collect();
    if chars.len() <= 12 {
        return "…".into();
    }
    let head: String = chars[..4.min(chars.len())].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}…{tail}")
}

/// Placeholders and words are not secrets: a value counts when it mixes letters and digits, has
/// no telltale placeholder in it, and is not one repeated character.
fn looks_random(value: &str) -> bool {
    let lower = value.to_lowercase();
    let placeholder = ["example", "xxxx", "your", "changeme", "placeholder", "dummy", "test", "fake", "redacted", "process.env", "secret"]
        .iter()
        .any(|p| lower.contains(p));
    let letters = value.chars().any(|c| c.is_ascii_alphabetic());
    let digits = value.chars().filter(|c| c.is_ascii_digit()).count();
    let distinct = value.chars().collect::<std::collections::BTreeSet<_>>().len();
    !placeholder && letters && digits >= 2 && distinct >= 10
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none() -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    #[test]
    fn finds_known_keys_and_random_assignments() {
        let text = r#"const KEY = "sk-ant-api03-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789";
aws = AKIAIOSFODNN7EXAMPLQ
let api_key = "q8Zr4LmN0pX2vB7wK9tY3hJ6";"#;
        let found = find(text, &none());
        let kinds: Vec<&str> = found.iter().map(|f| f.kind.as_str()).collect();
        assert_eq!(kinds, ["Anthropic API key", "AWS access key", "secret"]);
        assert_eq!(found[0].hint, "sk-a…6789");
    }

    #[test]
    fn placeholders_and_words_are_not_secrets() {
        let text = r#"api_key = "your-api-key-goes-here-123"
token: process.env.GITHUB_TOKEN_VALUE_123
password = "aaaaaaaaaaaaaaaaaaaaaaaa11"
const tokenizer = createTokenizerFromConfig()"#;
        assert!(find(text, &none()).is_empty(), "{:?}", find(text, &none()));
    }

    #[test]
    fn vault_values_are_named_and_redacted() {
        let vault = BTreeMap::from([("MY_SERVICE".to_owned(), "plainvaluewithoutshape".to_owned())]);
        let clean = redact("export X=plainvaluewithoutshape and sk-ant-api03-AbCdEfGhIjKlMnOpQrStUv", &vault).unwrap();
        assert_eq!(clean, "export X=[secret MY_SERVICE] and [Anthropic API key sk-a…StUv]");
        assert!(redact("nothing here", &vault).is_none());
    }

    /// Deterministic pseudo-random base64, like an image's bytes.
    fn base64_noise(len: usize, seed: u64) -> String {
        const ABC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut x = seed;
        (0..len)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                ABC[(x % 64) as usize] as char
            })
            .collect()
    }

    // A fake key, split so secret scanners do not flag this test fixture.
    const GOOGLE: &str = concat!("AI", "zaSyA1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q");

    #[test]
    fn a_key_shape_inside_base64_is_left_alone() {
        let blob = format!("{}{GOOGLE}{}", base64_noise(4000, 7), base64_noise(4000, 11));
        assert!(find(&blob, &none()).is_empty(), "{:?}", find(&blob, &none()));
        // A message's attachment, an engine's image block and a resource blob are never
        // rewritten, even when the shape stands alone.
        let mut event = serde_json::json!({
            "text": "see picture",
            "attachments": [{ "name": "a.png", "mime": "image/png", "data": blob }],
            "content": { "type": "image", "mimeType": "image/png", "data": GOOGLE },
            "resource": { "uri": "file:///a.bin", "blob": GOOGLE },
        });
        let before = event.clone();
        redact_json(&mut event, &none());
        assert_eq!(event, before);
    }

    #[test]
    fn a_real_key_in_text_is_still_redacted() {
        let hidden = "[Google API key AIza…5p6Q]";
        assert_eq!(redact(&format!("key: {GOOGLE}, ok"), &none()).unwrap(), format!("key: {hidden}, ok"));
        assert_eq!(redact(&format!("https://x.test/?key={GOOGLE}&a=1"), &none()).unwrap(), format!("https://x.test/?key={hidden}&a=1"));
        let mut event = serde_json::json!({
            "text": format!("use {GOOGLE}"),
            "attachments": [{ "name": "notes.txt", "mime": "text/plain", "text": format!("GOOGLE={GOOGLE}") }],
            "tool_call_id": "t1",
            "data": format!("{GOOGLE}\n"),
        });
        redact_json(&mut event, &none());
        assert_eq!(event["text"], format!("use {hidden}"));
        assert_eq!(event["attachments"][0]["text"], format!("GOOGLE={hidden}"));
        assert_eq!(event["data"], format!("{hidden}\n"));
    }

    #[test]
    fn diff_lines_are_numbered_in_the_new_file() {
        let diff = "diff --git a/src/c.ts b/src/c.ts\n--- a/src/c.ts\n+++ b/src/c.ts\n@@ -10,2 +10,3 @@\n const a = 1;\n-old\n+const k = \"ghp_0123456789abcdefghijABCDEFGHIJ0123456789\";\n+ok\n";
        let found = in_diff("src/c.ts", diff, &none());
        assert_eq!(found, vec![InFile { path: "src/c.ts".into(), line: 11, kind: "GitHub token".into(), hint: "ghp_…6789".into() }]);
    }
}

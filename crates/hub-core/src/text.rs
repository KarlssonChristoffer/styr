//! Cutting text short, saying how long ago something was, and naming a key by its end. Every place that hands an agent or a
//! list a piece of a longer text used to carry its own `clip`; these are the shapes that were
//! actually different. Do not add a private `clip`, `shorten`, `short` or `ago` elsewhere: use
//! these (the title makers `widgets::title_of`, `ideas::idea_line` and `attempts::branch_stem` cut
//! on words or build ids rather than characters).

/// The first `max` characters, with `…` when something was cut.
pub fn clip(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.to_owned(),
    }
}

/// The end of `text`, where a reply usually sums up, with `[…] ` when something was cut.
pub fn tail(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_owned();
    }
    let at = text.char_indices().nth(count - max).map(|(i, _)| i).unwrap_or(0);
    format!("[…] {}", &text[at..])
}

/// `clip` over the text as one line: runs of whitespace, newlines included, become one space.
pub fn clip_line(text: &str, max: usize) -> String {
    clip(&text.split_whitespace().collect::<Vec<_>>().join(" "), max)
}

/// The first `max` characters followed by `note` on its own paragraph when something was cut, for
/// text a reader should be told is not all of it and where the rest is.
pub fn clip_note(text: &str, max: usize, note: &str) -> String {
    match text.char_indices().nth(max) {
        Some((at, _)) => format!("{}\n\n{note}", &text[..at]),
        None => text.to_owned(),
    }
}

/// The first `max` characters followed by how many bytes were left out.
pub fn clip_counted(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((at, _)) => format!("{}\n[… {} more characters left out …]", &text[..at], text.len() - at),
        None => text.to_owned(),
    }
}

/// The start of `text`, or its end, in at most `max` characters.
pub fn cut(text: &str, max: usize, end: bool) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.trim().to_owned();
    }
    if end {
        let at = text.char_indices().nth(count - max).map_or(0, |(i, _)| i);
        format!("[…] {}", text[at..].trim())
    } else {
        let at = text.char_indices().nth(max).map_or(text.len(), |(i, _)| i);
        format!("{} […]", text[..at].trim())
    }
}

/// How long ago, from the milliseconds that have passed: "just now", "5 min ago", "3 h ago" (up to
/// two days) and "4 days ago". A negative span counts as now.
pub fn ago(elapsed_ms: i64) -> String {
    let min = elapsed_ms.max(0) / 60_000;
    match min {
        0 => "just now".into(),
        1..60 => format!("{min} min ago"),
        60..2880 => format!("{} h ago", min / 60),
        _ => format!("{} days ago", min / 1440),
    }
}

/// A version (a commit hash) as a person says it: its first 12 characters.
pub fn short_version(version: &str) -> &str {
    version.char_indices().nth(12).map_or(version, |(at, _)| &version[..at])
}

/// A secret by its last four characters, which is how most dashboards name a key too.
pub fn key_hint(secret: &str) -> String {
    let chars: Vec<char> = secret.trim().chars().collect();
    if chars.len() < 12 {
        return "set".into();
    }
    format!("…{}", chars[chars.len() - 4..].iter().collect::<String>())
}

/// The first version number in a program's `--version` output: `2.1.3 (Claude Code)` → `2.1.3`.
pub fn parse_version(text: &str) -> Option<String> {
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"\d+\.\d+(?:\.\d+)?(?:-[0-9A-Za-z.]+)?").expect("fixed pattern"));
    RE.find(text).map(|m| m.as_str().to_owned())
}

/// The text a terminal showed, without its colours and cursor moves (CSI and OSC sequences).
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            // CSI: parameters, then one final byte from @ to ~.
            Some('[') => {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            // OSC: up to BEL or ESC \.
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\u{7}' || (c == '\u{1b}' && chars.next_if_eq(&'\\').is_some()) {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_is_left_alone() {
        assert_eq!(clip("abc", 3), "abc");
        assert_eq!(clip_note("abc", 3, "n"), "abc");
        assert_eq!(clip_counted("abc", 3), "abc");
        assert_eq!(clip("", 0), "");
    }

    #[test]
    fn cuts_on_characters_not_bytes() {
        assert_eq!(clip("åäö!", 3), "åäö…");
        assert_eq!(clip_note("åäö!", 3, "more"), "åäö\n\nmore");
        assert_eq!(clip_counted("åäö!", 3), "åäö\n[… 1 more characters left out …]");
    }

    #[test]
    fn a_line_collapses_whitespace_first() {
        assert_eq!(clip_line("a \n  b\tc", 100), "a b c");
        assert_eq!(clip_line("a \n  b\tc", 3), "a b…");
    }

    #[test]
    fn ago_counts_minutes_hours_and_days() {
        assert_eq!(ago(-5), "just now");
        assert_eq!(ago(59_999), "just now");
        assert_eq!(ago(60_000), "1 min ago");
        assert_eq!(ago(59 * 60_000), "59 min ago");
        assert_eq!(ago(60 * 60_000), "1 h ago");
        assert_eq!(ago(47 * 3_600_000), "47 h ago");
        assert_eq!(ago(48 * 3_600_000), "2 days ago");
    }

    #[test]
    fn a_version_is_cut_to_twelve_characters() {
        assert_eq!(short_version("0123456789abcdef"), "0123456789ab");
        assert_eq!(short_version("abc"), "abc");
        assert_eq!(short_version("åäö"), "åäö");
        assert_eq!(short_version(""), "");
    }
}

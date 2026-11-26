//! What goes into whisper as text and what comes out of it: the prompt (the text before the
//! recording, the folder's names and the scene-setting vocabulary), the language it is told to
//! hear, and the cleaning of its output (markers, phantom subtitle credits, echoes of the prompt,
//! and the line of whisper-server's log that says why it stopped).

use std::path::Path;

/// Whisper only looks at the last 224 tokens of a prompt; keep well inside that.
pub(super) const PROMPT_CHARS: usize = 400;

/// Sets the scene when the box is empty. Whisper copies the style of its prompt, so this is
/// written as punctuated sentences rather than a word list, which made it leave out full stops,
/// and it carries the tech terms that should come out spelled right.
pub(super) const VOCABULARY: &str = "Hej! Kan du titta på Hub? Claude ska göra en commit, pusha branchen och öppna en pull request. Sedan mergar vi och kör en deploy av frontend och backend, enligt README och API:t.";

/// The folder's name, its branch and up to eight changed files, as one short line.
pub(super) async fn folder_line(cwd: &str) -> String {
    let dir = Path::new(cwd);
    if !dir.is_dir() {
        return String::new();
    }
    let mut words: Vec<String> = dir.file_name().map(|n| n.to_string_lossy().into_owned()).into_iter().collect();
    // git.rs's git waits for git to finish, so it runs off the async threads.
    let git = |args: &'static [&'static str]| {
        let dir = dir.to_path_buf();
        async move { tokio::task::spawn_blocking(move || crate::git::run(&dir, args)).await.ok().flatten() }
    };
    if let Some(branch) = git(&["rev-parse", "--abbrev-ref", "HEAD"]).await {
        let branch = branch.trim();
        // A thread's own branch ends in a made-up suffix that only gets in whisper's way.
        let branch = branch.rsplit('/').next().unwrap_or(branch);
        if !branch.is_empty() && branch != "HEAD" && !branch.chars().any(|c| c.is_ascii_digit()) {
            words.push(branch.replace(['-', '_'], " "));
        }
    }
    if let Some(status) = git(&["status", "--porcelain"]).await {
        for line in status.lines().take(8) {
            let path = line.get(3..).unwrap_or_default().trim().trim_matches('"');
            let path = path.rsplit(" -> ").next().unwrap_or(path);
            if let Some(name) = Path::new(path).file_name() {
                words.push(name.to_string_lossy().into_owned());
            }
        }
    }
    words.dedup();
    let line = words.join(", ");
    if line.is_empty() { line } else { format!("{line}.") }
}

/// The end of the text before the recording, cut at a word, or the vocabulary when there is none,
/// after the folder's names (`context`).
pub(super) fn prompt(before: Option<&str>, context: &str) -> String {
    let context: String = context.chars().take(PROMPT_CHARS / 3).collect();
    let room = PROMPT_CHARS - context.chars().count();
    let before = before.unwrap_or_default().trim();
    let text = if before.is_empty() {
        VOCABULARY.to_owned()
    } else {
        let chars: Vec<char> = before.chars().collect();
        if chars.len() <= room {
            before.to_owned()
        } else {
            let tail: String = chars[chars.len() - room..].iter().collect();
            match tail.split_once(char::is_whitespace) {
                Some((_, rest)) => rest.trim_start().to_owned(),
                None => tail,
            }
        }
    };
    if context.is_empty() { text } else { format!("{context} {text}") }
}

/// The language whisper is told to hear: Swedish unless English is chosen. See `VoiceConfig::language`.
pub(super) fn pinned_language(language: &str) -> String {
    match language.trim() {
        "" | "auto" => "sv".into(),
        other => other.to_owned(),
    }
}

/// Over silence or a breath, whisper sometimes just says the scene-setting prompt back. Text
/// that is wholly inside it, and long enough not to be a real "ja" or "okej", is that echo. The
/// user's own earlier text is not checked (the caller asks only when there was none): saying
/// something again is allowed.
pub(super) fn echoes(prompt: &str, text: &str) -> bool {
    let norm = |s: &str| s.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ");
    let text = norm(text);
    text.chars().count() >= 12 && norm(prompt).contains(&text)
}

/// The line in whisper-server's output that says why it stopped: the last error-looking line,
/// else the last line.
pub(super) fn why_it_stopped(said: &str) -> Option<String> {
    let lines: Vec<&str> = said.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let errorish = |l: &&&str| {
        let l = l.to_lowercase();
        ["error", "failed", "couldn't", "could not", "unable", "out of memory"].iter().any(|w| l.contains(w))
    };
    lines.iter().rev().find(errorish).or(lines.last()).map(|l| l.chars().take(200).collect())
}

/// What whisper says over silence or noise, learned from subtitle credits in its training data.
/// Dropped when it is all a recording produced.
const PHANTOMS: &[&str] = &[
    "textning.nu",
    "svensktextning.nu",
    "tack för att ni tittade",
    "tack för att du tittade",
    "tack för att ni lyssnade",
    "tack för att du lyssnade",
    "tack för tittandet",
    "undertexter från amara.org-gemenskapen",
    "thank you for watching",
    "thanks for watching",
];

fn phantom(text: &str) -> bool {
    let text = text.to_lowercase();
    let bare = text.trim_matches(|c: char| c.is_whitespace() || matches!(c, '.' | ',' | '!' | '?' | '"' | '-'));
    bare.is_empty() || PHANTOMS.contains(&bare)
}

/// Joins whisper's lines and drops its markers for silence and noise, like `[BLANK_AUDIO]`.
pub(super) fn clean(output: &str) -> String {
    let text = join(output);
    if phantom(&text) { String::new() } else { text }
}

fn join(output: &str) -> String {
    let mut text = String::new();
    for line in output.lines() {
        let mut line = line.trim().to_owned();
        for (open, close) in [('[', ']'), ('(', ')')] {
            while let (Some(a), Some(b)) = (line.find(open), line.find(close)) {
                if b < a {
                    break;
                }
                line.replace_range(a..=b, "");
            }
        }
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if !line.is_empty() {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(&line);
        }
    }
    text
}

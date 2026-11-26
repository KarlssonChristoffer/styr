//! Reading one markdown doc into the index's `Doc`: front matter, headings and slugs, `[[wiki]]`
//! and markdown links, inline tags and `covers:` comments (code fences skipped), plus the text
//! previews. The index over all docs is `vault.rs`.

use super::*;

pub(super) fn parse(rel: &str, text: &str, modified: i64, len: u64, truncated: bool) -> Doc {
    let (props, body) = front_matter(text);
    let stem = rel.rsplit('/').next().unwrap_or(rel).trim_end_matches(".md").trim_end_matches(".MD").to_owned();
    let mut tags: Vec<String> = Vec::new();
    let mut headings = Vec::new();
    let mut heading_lines = Vec::new();
    let mut covers: Vec<(Option<usize>, Vec<String>)> = Vec::new();
    let mut links = Vec::new();
    let mut fence: Option<&str> = None;
    for (n, line) in body.lines().enumerate() {
        let trimmed = line.trim_start();
        if let Some(f) = fence {
            if trimmed.starts_with(f) {
                fence = None;
            }
            continue;
        }
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fence = Some(&trimmed[..3]);
            continue;
        }
        if let Some((level, text)) = heading(trimmed) {
            let text = plain_heading(text);
            headings.push(Heading { level, slug: slug(&text), text });
            heading_lines.push(n);
        }
        if let Some(list) = covers_comment(trimmed) {
            covers.push((headings.len().checked_sub(1), list));
        }
        let prose = strip_code(line);
        for link in wikilinks(&prose).into_iter().chain(md_links(&prose)) {
            links.push((link, n));
        }
        if heading(trimmed).is_none() {
            for tag in inline_tags(&prose) {
                if !tags.iter().any(|t| t.eq_ignore_ascii_case(&tag)) {
                    tags.push(tag);
                }
            }
        }
    }
    for p in &props {
        if p.key.eq_ignore_ascii_case("tags") || p.key.eq_ignore_ascii_case("tag") {
            for tag in p.value.split([',', ' ']).map(|t| t.trim().trim_start_matches('#')).filter(|t| !t.is_empty()) {
                if !tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                    tags.push(tag.to_owned());
                }
            }
        }
    }
    let prop = |keys: &[&str]| props.iter().find(|p| keys.iter().any(|k| p.key.eq_ignore_ascii_case(k))).map(|p| p.value.clone()).filter(|v| !v.is_empty());
    // README and agent files usually carry the project's name as heading; their name says more.
    let top_level = !rel.contains('/');
    let title = prop(&["title"]).unwrap_or_else(|| {
        if top_level {
            stem.clone()
        } else {
            headings.iter().find(|h| h.level == 1).map(|h| h.text.clone()).unwrap_or_else(|| stem.clone())
        }
    });
    let summary = prop(&["summary", "description"]).unwrap_or_else(|| preview(body, 220));
    if let Some(list) = prop(&["covers"]) {
        covers.insert(0, (None, cover_list(&list)));
    }
    covers.retain(|(_, list)| !list.is_empty());
    let lower = body.lines().map(|l| l.to_lowercase()).collect();
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let offset = text.len().checked_sub(body.len()).map(|n| text[..n].matches('\n').count()).unwrap_or_default();
    Doc {
        rel: rel.to_owned(),
        stem,
        title,
        summary,
        props,
        tags,
        headings,
        links,
        body: body.to_owned(),
        lower,
        modified,
        len,
        truncated,
        offset,
        heading_lines,
        covers,
    }
}

/// The paths in a `<!-- covers: a, b -->` line.
pub(super) fn covers_comment(line: &str) -> Option<Vec<String>> {
    let inner = line.trim().strip_prefix("<!--")?.strip_suffix("-->")?.trim();
    let list = inner.strip_prefix("covers:")?;
    Some(cover_list(list))
}

pub(super) fn cover_list(list: &str) -> Vec<String> {
    list.split([',', ' ', '\t'])
        .map(|p| p.trim().trim_matches(['`', '"', '\'']).trim_start_matches("./"))
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect()
}

/// A part of a doc that says which code it describes: a whole doc, or one heading's section.
#[derive(Debug, Clone)]
pub struct Covering {
    pub rel: String,
    /// The heading, or None for the whole doc.
    pub heading: Option<Heading>,
    /// Lines in the file, from 1, inclusive.
    pub first: usize,
    pub last: usize,
    /// The section's markdown (the whole body for a doc).
    pub text: String,
    pub patterns: Vec<String>,
}

/// Every `covers:` in the project's docs, with the text it belongs to.
pub fn coverings(root: &Path) -> Vec<Covering> {
    let index = index(root);
    let mut out = Vec::new();
    for d in &index.docs {
        let lines: Vec<&str> = d.body.lines().collect();
        for (at, patterns) in &d.covers {
            let (heading, from, to) = match at {
                Some(i) => {
                    let level = d.headings[*i].level;
                    let from = d.heading_lines[*i];
                    let to = d
                        .headings
                        .iter()
                        .zip(&d.heading_lines)
                        .skip(i + 1)
                        .find(|(h, _)| h.level <= level)
                        .map(|(_, &l)| l)
                        .unwrap_or(lines.len());
                    (Some(d.headings[*i].clone()), from, to)
                }
                None => (None, 0, lines.len()),
            };
            let text = lines[from.min(lines.len())..to.min(lines.len())].join("\n").trim_end().to_owned();
            let last = (from + text.lines().count()).max(from + 1);
            out.push(Covering {
                rel: d.rel.clone(),
                heading,
                first: from + d.offset + 1,
                last: last + d.offset,
                text,
                patterns: patterns.clone(),
            });
        }
    }
    out
}

/// `key: value` lines between `---` fences at the very top. Lists, inline (`[a, b]`) or
/// indented (`- a`), become a comma separated value. Anything fancier is kept as text.
pub(super) fn front_matter(text: &str) -> (Vec<Prop>, &str) {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let Some(rest) = text.strip_prefix("---\n").or_else(|| text.strip_prefix("---\r\n")) else {
        return (Vec::new(), text);
    };
    let Some(end) = rest.find("\n---").filter(|&i| rest[i + 4..].starts_with(['\n', '\r']) || rest.len() == i + 4) else {
        return (Vec::new(), text);
    };
    let body = rest[end + 4..].trim_start_matches(['\r', '\n']);
    let mut props: Vec<Prop> = Vec::new();
    for line in rest[..end].lines() {
        let item = line.trim_start().strip_prefix("- ");
        match (item, props.last_mut()) {
            (Some(item), Some(last)) if line.starts_with([' ', '-', '\t']) => {
                let item = unquote(item.trim());
                if last.value.is_empty() {
                    last.value = item.to_owned();
                } else {
                    last.value = format!("{}, {item}", last.value);
                }
            }
            _ => {
                if let Some((key, value)) = line.split_once(':')
                    && !key.trim().is_empty()
                    && !key.starts_with([' ', '\t', '#'])
                {
                    let value = value.trim();
                    let value = match value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
                        Some(list) => list.split(',').map(|v| unquote(v.trim())).collect::<Vec<_>>().join(", "),
                        None => unquote(value).to_owned(),
                    };
                    props.push(Prop { key: key.trim().to_owned(), value });
                }
            }
        }
    }
    (props, body)
}

pub(super) fn unquote(v: &str) -> &str {
    v.strip_prefix('"').and_then(|v| v.strip_suffix('"')).or_else(|| v.strip_prefix('\'').and_then(|v| v.strip_suffix('\''))).unwrap_or(v)
}

pub(super) fn heading(line: &str) -> Option<(u8, &str)> {
    let hashes = line.bytes().take_while(|&b| b == b'#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let text = line[hashes..].strip_prefix(' ')?;
    Some((hashes as u8, text.trim().trim_end_matches('#').trim()))
}

/// A heading as it reads: links show their text, emphasis and code marks go.
pub(super) fn plain_heading(text: &str) -> String {
    link_text(text).replace(['*', '`'], "").replace("__", "").trim().to_owned()
}

/// Text with its `[[links]]` read as what they show.
pub fn link_text(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("[[") {
        out.push_str(&rest[..at]);
        let Some(end) = rest[at..].find("]]") else { break };
        let inner = &rest[at + 2..at + end];
        out.push_str(inner.split_once('|').map(|(_, a)| a).unwrap_or(inner));
        rest = &rest[at + end + 2..];
    }
    out.push_str(rest);
    out
}

/// The anchor for a heading: lower case, letters and digits kept, the rest a dash. The reader
/// makes the same anchors, so `[[Doc#Heading]]` lands on it (`templates::package_name` is another thing).
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_owned()
}

/// The line without its inline code, which can hold anything that looks like a link or a tag.
pub(super) fn strip_code(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_code = false;
    for c in line.chars() {
        if c == '`' {
            in_code = !in_code;
            out.push(' ');
        } else if !in_code {
            out.push(c);
        }
    }
    out
}

/// `[[Target]]`, `[[Target#Heading]]`, `[[Target|text]]` and embeds `![[Target]]`.
pub(super) fn wikilinks(line: &str) -> Vec<RawLink> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(at) = rest.find("[[") {
        let after = &rest[at + 2..];
        let Some(end) = after.find("]]") else { break };
        let inner = &after[..end];
        rest = &after[end + 2..];
        if inner.contains('[') || inner.trim().is_empty() {
            continue;
        }
        let inner = inner.split_once('|').map(|(t, _)| t).unwrap_or(inner);
        let (target, heading) = match inner.split_once('#') {
            Some((t, h)) => (t.trim(), Some(h.trim().trim_start_matches('^').to_owned()).filter(|h| !h.is_empty())),
            None => (inner.trim(), None),
        };
        out.push(RawLink { target: target.to_owned(), heading, relative: false });
    }
    out
}

/// `[text](other.md)` and `[text](other.md#heading)` links to markdown in the project.
pub(super) fn md_links(line: &str) -> Vec<RawLink> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(at) = rest.find("](") {
        let after = &rest[at + 2..];
        let Some(end) = after.find(')') else { break };
        let href = after[..end].trim().trim_start_matches('<').trim_end_matches('>');
        rest = &after[end + 1..];
        let href = href.split_once(' ').map(|(h, _)| h).unwrap_or(href);
        if href.contains("://") || href.starts_with("mailto:") || href.starts_with('#') {
            continue;
        }
        let (file, heading) = match href.split_once('#') {
            Some((f, h)) => (f, Some(h.to_owned())),
            None => (href, None),
        };
        if !file.to_lowercase().ends_with(".md") {
            continue;
        }
        let file = file.replace("%20", " ");
        out.push(RawLink { target: file, heading, relative: true });
    }
    out
}

/// `#tag` and `#area/tag`: after a space or at the start, not only digits (issue numbers).
pub(super) fn inline_tags(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '#' && (i == 0 || chars[i - 1].is_whitespace() || chars[i - 1] == '(') {
            let start = i + 1;
            let mut j = start;
            while j < chars.len() && (chars[j].is_alphanumeric() || matches!(chars[j], '-' | '_' | '/')) {
                j += 1;
            }
            let tag: String = chars[start..j].iter().collect();
            let tag = tag.trim_end_matches('/');
            if !tag.is_empty() && !tag.chars().all(|c| c.is_ascii_digit()) {
                out.push(tag.to_owned());
            }
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

pub(super) fn context(line: &str) -> String {
    let line = line.replace("**", "");
    let line = line.trim().trim_start_matches(['-', '*', '>', ' ']);
    if line.chars().count() <= 200 {
        return line.to_owned();
    }
    let cut: String = line.chars().take(200).collect();
    format!("{}…", cut.rsplit_once(' ').map(|(h, _)| h).unwrap_or(&cut))
}

/// The first prose of a markdown text, without headings, badges, code or markup, cut at about
/// `max` characters on a word boundary.
pub fn preview(text: &str, max: usize) -> String {
    let mut out = String::new();
    let mut in_code = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code || line.is_empty() && out.is_empty() {
            continue;
        }
        if line.is_empty() {
            break;
        }
        if line.starts_with('#') || line.starts_with("![") || line.starts_with("[![") || line.starts_with('<') || line.starts_with('|') || line.starts_with("---") {
            if out.is_empty() {
                continue;
            }
            break;
        }
        let line = line.trim_start_matches(['-', '*', '>', ' ']);
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(line);
        if out.len() > max {
            break;
        }
    }
    let plain = link_text(&out.replace("**", "").replace('`', ""));
    if plain.chars().count() <= max {
        return plain;
    }
    let cut: String = plain.chars().take(max).collect();
    let cut = cut.rsplit_once(' ').map(|(head, _)| head).unwrap_or(&cut);
    format!("{}…", cut.trim_end_matches([',', '.', ':', ';']))
}

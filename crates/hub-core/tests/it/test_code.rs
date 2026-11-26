//! Which lines of a Rust file are test code, for the guards that read source (`rules::`,
//! `layers::`, `no_window::`). A test-only ITEM is skipped, not the rest of the file: a
//! `#[cfg(test)]` or `#[cfg(all(test, ..))]` attribute at the start of a line, with any further
//! attributes, hides the item under it: a `mod x;` or `use ..;` to its `;`, an inline `mod x { .. }`,
//! `fn` or `impl` to its matching `}`, a field or arm to its `,`. The code after it is read again,
//! so `#[cfg(test)] mod tests;` may sit anywhere in a file. Braces inside strings, chars and
//! comments do not count, and `#[cfg(not(test))]` is code. A line that holds any of a hidden item
//! is hidden whole. Whole files of tests (`tests.rs`, `*_tests.rs`) are each guard's own business.

/// Per line of `text` (as `str::lines` splits it): `true` when it belongs to a test-only item.
pub fn test_lines(text: &str) -> Vec<bool> {
    let b = text.as_bytes();
    let mut hidden = vec![false; text.lines().count()];
    let (mut i, mut line, mut blank_so_far) = (0usize, 0usize, true);
    // The test-only item being skipped: its first line, the depth of (), [] and {}, whether the
    // attributes are behind us, whether a `{` opened at depth 0, and the last line it reached.
    let mut item: Option<Item> = None;
    while i < b.len() {
        let c = b[i];
        if c == b'\n' {
            line += 1;
            blank_so_far = true;
            i += 1;
            continue;
        }
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        // Comments and literals are skipped whole; inside an item they only extend it.
        if let Some(end) = skip_comment_or_literal(b, i) {
            let lines_in = b[i..end].iter().filter(|&&x| x == b'\n').count();
            if let Some(it) = item.as_mut()
                && it.body
                && !is_comment(b, i)
            {
                it.last = line + lines_in;
            }
            line += lines_in;
            blank_so_far = false;
            i = end;
            continue;
        }
        let at_line_start = blank_so_far;
        blank_so_far = false;
        match item.as_mut() {
            None => {
                if at_line_start && (b[i..].starts_with(b"#[cfg(test)]") || b[i..].starts_with(b"#[cfg(all(test")) {
                    item = Some(Item { first: line, depth: 0, body: false, braced: false, comma_ends: false, last: line });
                    continue; // the `#` is read again as the item's first attribute
                }
            }
            Some(it) => {
                let mut end = false;
                if !it.body && it.depth == 0 && c != b'#' && c != b'[' {
                    it.body = true;
                    it.comma_ends = !starts_with_keyword(b, i);
                }
                match c {
                    b'(' | b'[' => it.depth += 1,
                    b'{' => {
                        if it.depth == 0 && it.body {
                            it.braced = true;
                        }
                        it.depth += 1;
                    }
                    b')' | b']' | b'}' if it.depth == 0 => {
                        // Closes what holds the item (a struct's field list, a match): it ended before.
                        mark(&mut hidden, it.first, it.last);
                        item = None;
                        i += 1;
                        continue;
                    }
                    b')' | b']' => it.depth -= 1,
                    b'}' => {
                        it.depth -= 1;
                        end = it.depth == 0 && it.braced;
                    }
                    b';' if it.depth == 0 && it.body => end = true,
                    b',' if it.depth == 0 && it.body && it.comma_ends => end = true,
                    _ => {}
                }
                it.last = line;
                if end {
                    mark(&mut hidden, it.first, it.last);
                    item = None;
                }
            }
        }
        i += 1;
    }
    if let Some(it) = item {
        let last = line.min(hidden.len().saturating_sub(1));
        mark(&mut hidden, it.first, last);
    }
    hidden
}

/// The lines of `text` that are not test code, with their 0-based index.
pub fn code_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let hidden = test_lines(text);
    text.lines().enumerate().filter(move |(n, _)| !hidden.get(*n).copied().unwrap_or(false))
}

struct Item {
    first: usize,
    depth: usize,
    body: bool,
    braced: bool,
    /// A field, variant or arm ends at its `,`; a `fn`, `impl` or `let` does not (`fn f<A, B>()`).
    comma_ends: bool,
    last: usize,
}

/// Whether the item at `i` opens with a keyword among its first words (`pub(crate) async fn`).
fn starts_with_keyword(b: &[u8], mut i: usize) -> bool {
    const KEYWORDS: &[&[u8]] = &[b"fn", b"mod", b"impl", b"use", b"struct", b"enum", b"trait", b"const", b"static", b"type", b"union", b"macro_rules", b"let", b"extern"];
    for _ in 0..4 {
        while b.get(i).is_some_and(|c| c.is_ascii_whitespace()) {
            i += 1;
        }
        let start = i;
        while b.get(i).is_some_and(|&c| ident_byte(c)) {
            i += 1;
        }
        let word = &b[start..i];
        if word.is_empty() {
            return false;
        }
        if KEYWORDS.contains(&word) {
            return true;
        }
        if word == b"pub" && b.get(i) == Some(&b'(') {
            i += b[i..].iter().position(|&c| c == b')').map_or(b.len() - i, |p| p + 1);
        }
    }
    false
}

fn mark(hidden: &mut [bool], first: usize, last: usize) {
    for h in hidden.iter_mut().take(last + 1).skip(first) {
        *h = true;
    }
}

fn is_comment(b: &[u8], i: usize) -> bool {
    b[i] == b'/' && matches!(b.get(i + 1), Some(b'/') | Some(b'*'))
}

fn ident_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// When a comment, string, raw string or char literal starts at `i`: the index just past it.
fn skip_comment_or_literal(b: &[u8], i: usize) -> Option<usize> {
    let at = |k: usize| b.get(k).copied();
    match b[i] {
        b'/' if at(i + 1) == Some(b'/') => Some(b[i..].iter().position(|&x| x == b'\n').map_or(b.len(), |p| i + p)),
        b'/' if at(i + 1) == Some(b'*') => {
            let (mut k, mut depth) = (i + 2, 1);
            while k < b.len() && depth > 0 {
                if b[k..].starts_with(b"/*") {
                    depth += 1;
                    k += 2;
                } else if b[k..].starts_with(b"*/") {
                    depth -= 1;
                    k += 2;
                } else {
                    k += 1;
                }
            }
            Some(k)
        }
        b'"' => {
            let mut k = i + 1;
            while k < b.len() && b[k] != b'"' {
                k += if b[k] == b'\\' { 2 } else { 1 };
            }
            Some((k + 1).min(b.len()))
        }
        // r"..", r#".."#, br#".."#: not when the `r` ends an identifier (`for"` is not code anyway).
        b'r' if i == 0 || !ident_byte(b[i - 1]) || (b[i - 1] == b'b' && (i < 2 || !ident_byte(b[i - 2]))) => {
            let hashes = b[i + 1..].iter().take_while(|&&x| x == b'#').count();
            if at(i + 1 + hashes) != Some(b'"') {
                return None;
            }
            let close = format!("\"{}", "#".repeat(hashes)).into_bytes();
            let from = i + 2 + hashes;
            Some(b[from..].windows(close.len()).position(|w| w == close.as_slice()).map_or(b.len(), |p| from + p + close.len()))
        }
        // A char literal ('{', '\'', '\u{7b}', 'é'); a lifetime ('a) is not one.
        b'\'' => {
            if at(i + 1) == Some(b'\\') {
                // The escaped char at i + 2 may itself be a quote (`'\''`).
                let k = b.get(i + 3..).and_then(|r| r.iter().position(|&x| x == b'\'')).map_or(b.len(), |p| i + 3 + p);
                return Some((k + 1).min(b.len()));
            }
            let len = match at(i + 1)? {
                x if x < 0x80 => 1,
                x if x >= 0xF0 => 4,
                x if x >= 0xE0 => 3,
                _ => 2,
            };
            (at(i + 1 + len) == Some(b'\'')).then_some(i + 2 + len)
        }
        _ => None,
    }
}

#[cfg(test)]
mod scanner {
    use super::*;

    fn code(text: &str) -> Vec<&str> {
        code_lines(text).map(|(_, l)| l.trim()).collect()
    }

    #[test]
    fn a_declaration_at_the_top_hides_only_itself() {
        let text = "mod a;\n#[cfg(test)]\nmod tests;\nmod b;\nfn f() { let _ = x(); }\n";
        assert_eq!(code(text), ["mod a;", "mod b;", "fn f() { let _ = x(); }"]);
        assert_eq!(code("#[cfg(test)] mod tests;\nfn g() {}\n"), ["fn g() {}"]);
    }

    #[test]
    fn an_inline_test_module_at_the_bottom_is_hidden_to_its_brace() {
        let text = "fn f() {}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn t() { if a { b() } }\n}\n";
        assert_eq!(code(text), ["fn f() {}", ""]);
    }

    #[test]
    fn a_test_only_fn_in_the_middle_is_hidden_and_the_code_after_it_is_read() {
        let text = "fn a() {}\n#[cfg(test)]\n#[allow(dead_code)]\npub(crate) fn helper<T: Into<String>>(x: T) -> Vec<u8> {\n    vec![1]\n}\nfn b() {}\n";
        assert_eq!(code(text), ["fn a() {}", "fn b() {}"]);
    }

    #[test]
    fn a_test_only_use_ends_at_its_semicolon() {
        assert_eq!(code("#[cfg(test)]\nuse std::{io, fmt::{self, Write}};\nuse std::fs;\n"), ["use std::fs;"]);
        assert_eq!(code("#[cfg(all(test, unix))]\nuse std::os::unix;\nfn f() {}\n"), ["fn f() {}"]);
    }

    #[test]
    fn nested_braces_and_braces_in_literals_do_not_end_an_item_early() {
        let text = "#[cfg(test)]\nfn t() {\n    let s = \"}\";\n    let r = r#\"}}\"#;\n    let c = '}';\n    let l: &'static str = \"\";\n    // }\n    /* } */\n    match x { _ => { {} } }\n}\nfn after() {}\n";
        assert_eq!(code(text), ["fn after() {}"]);
        assert_eq!(code("#[cfg(test)]\nconst X: &[&str] = &[\"{\", \"[\"];\nconst Y: u8 = 1;\n"), ["const Y: u8 = 1;"]);
    }

    #[test]
    fn not_test_is_code_and_markers_in_strings_or_comments_are_not_markers() {
        assert_eq!(code("#[cfg(not(test))]\nfn real() {}\n"), ["#[cfg(not(test))]", "fn real() {}"]);
        assert_eq!(code("// #[cfg(test)]\nfn a() {}\n"), ["// #[cfg(test)]", "fn a() {}"]);
        assert_eq!(code("let s = \"\n#[cfg(test)]\nmod x;\";\nfn a() {}\n"), ["let s = \"", "#[cfg(test)]", "mod x;\";", "fn a() {}"]);
    }

    #[test]
    fn a_test_only_field_or_arm_ends_at_its_comma_or_the_closing_brace() {
        assert_eq!(code("struct S {\n    #[cfg(test)]\n    pub probe: u8,\n    real: u8,\n}\n"), ["struct S {", "real: u8,", "}"]);
        assert_eq!(code("struct S {\n    #[cfg(test)]\n    probe: u8\n}\n"), ["struct S {", "}"]);
        assert_eq!(code("#[cfg(test)]\nfn f<A, B>(a: A, b: B) where A: Copy, B: Copy {\n    g(a, b);\n}\nfn h() {}\n"), ["fn h() {}"]);
        assert_eq!(code("let c = '\\'';\n#[cfg(test)]\nmod t { fn x() {} }\nfn y() {}\n"), ["let c = '\\'';", "fn y() {}"]);
    }

    #[test]
    fn an_unclosed_item_hides_to_the_end() {
        assert_eq!(code("fn a() {}\n#[cfg(test)]\nmod t {\n"), ["fn a() {}"]);
    }
}

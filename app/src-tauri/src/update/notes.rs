//! Release notes as plain text blocks (spec 11: no remote HTML in the window).
//!
//! GitHub release bodies are Markdown that anyone with write access to the release repo
//! (or a mirror the user picked) controls. The window never renders them as HTML: this turns
//! them into headings, list items and paragraphs of plain text, which the UI shows as text.
//!
//! Removed: HTML tags and comments, images, link targets (the link text stays), emphasis and
//! code markers, control and bidi-override characters. Bounded: input length, number of
//! blocks, length of each block.

use crate::contract::types::{NoteBlock, NoteKind};

/// Longest body read; GitHub allows 125 000 characters.
pub const MAX_INPUT_CHARS: usize = 20_000;
/// Blocks shown in the banner; the release page has the rest.
pub const MAX_BLOCKS: usize = 40;
/// Characters per block.
pub const MAX_BLOCK_CHARS: usize = 400;

/// Markdown body -> blocks. The second value is true when anything was cut.
pub fn sanitize(body: &str) -> (Vec<NoteBlock>, bool) {
    let mut truncated = body.chars().count() > MAX_INPUT_CHARS;
    let body: String = body.chars().take(MAX_INPUT_CHARS).collect();
    let body = strip_html(&body);
    let mut blocks: Vec<NoteBlock> = Vec::new();
    let mut para: Vec<String> = Vec::new();
    let mut in_fence = false;

    fn flush(para: &mut Vec<String>, blocks: &mut Vec<NoteBlock>) {
        if !para.is_empty() {
            push(blocks, NoteKind::Text, &para.join(" "));
            para.clear();
        }
    }

    for raw in body.lines() {
        let line = raw.trim();
        if line.starts_with("```") || line.starts_with("~~~") {
            flush(&mut para, &mut blocks);
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            // Code is shown as its plain lines.
            push(&mut blocks, NoteKind::Text, line);
            continue;
        }
        if line.is_empty() || is_rule(line) {
            flush(&mut para, &mut blocks);
            continue;
        }
        if let Some(h) = heading(line) {
            flush(&mut para, &mut blocks);
            push(&mut blocks, NoteKind::Heading, h);
        } else if let Some(i) = item(line) {
            flush(&mut para, &mut blocks);
            push(&mut blocks, NoteKind::Item, i);
        } else {
            para.push(line.trim_start_matches('>').trim().to_string());
        }
    }
    flush(&mut para, &mut blocks);

    // Inline Markdown and limits, after the structure is known.
    let mut out = Vec::new();
    for b in blocks {
        let text = inline(&b.text);
        if text.is_empty() {
            continue;
        }
        if out.len() == MAX_BLOCKS {
            truncated = true;
            break;
        }
        let n = text.chars().count();
        let text = if n > MAX_BLOCK_CHARS {
            truncated = true;
            let mut t: String = text.chars().take(MAX_BLOCK_CHARS - 1).collect();
            t = t.trim_end().to_string();
            t.push('…');
            t
        } else {
            text
        };
        out.push(NoteBlock { kind: b.kind, text });
    }
    (out, truncated)
}

fn push(blocks: &mut Vec<NoteBlock>, kind: NoteKind, text: &str) {
    blocks.push(NoteBlock {
        kind,
        text: text.to_string(),
    });
}

fn is_rule(line: &str) -> bool {
    line.len() >= 3
        && (line.chars().all(|c| c == '-' || c == ' ')
            || line.chars().all(|c| c == '*' || c == ' ')
            || line.chars().all(|c| c == '_' || c == ' '))
}

fn heading(line: &str) -> Option<&str> {
    let hashes = line.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&hashes) && line[hashes..].starts_with(' ') {
        Some(line[hashes..].trim().trim_end_matches('#').trim())
    } else {
        None
    }
}

fn item(line: &str) -> Option<&str> {
    for m in ["- ", "* ", "+ "] {
        if let Some(r) = line.strip_prefix(m) {
            let r = r.trim_start();
            // Task lists: "- [x] thing".
            let r = r
                .strip_prefix("[ ] ")
                .or_else(|| r.strip_prefix("[x] "))
                .or_else(|| r.strip_prefix("[X] "))
                .unwrap_or(r);
            return Some(r);
        }
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && digits <= 3 {
        let rest = &line[digits..];
        if let Some(r) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return Some(r.trim_start());
        }
    }
    None
}

/// Drops `<!-- -->` comments and every `<...>` tag (the text between tags stays). A lone
/// `<` that never closes is kept as text.
fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        if let Some(c) = tail.strip_prefix("<!--") {
            match c.find("-->") {
                Some(j) => rest = &c[j + 3..],
                None => rest = "",
            }
            continue;
        }
        let looks_like_tag = tail[1..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '/' || c == '!' || c == '?');
        match (looks_like_tag, tail.find('>')) {
            (true, Some(j)) => {
                // Block-level tags end a line, so <br> and </p> do not glue words.
                let tag = tail[1..j].trim_start_matches('/').to_ascii_lowercase();
                let name: String = tag
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect();
                if matches!(
                    name.as_str(),
                    "br" | "p"
                        | "div"
                        | "li"
                        | "ul"
                        | "ol"
                        | "h1"
                        | "h2"
                        | "h3"
                        | "h4"
                        | "h5"
                        | "h6"
                        | "tr"
                        | "table"
                        | "details"
                        | "summary"
                        | "pre"
                ) {
                    out.push('\n');
                }
                rest = &tail[j + 1..];
            }
            _ => {
                out.push('<');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Inline Markdown -> text: images out, links to their text, autolinks and bare URLs kept as
/// text, emphasis/code markers out, entities decoded, unsafe characters out.
fn inline(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        // ![alt](url) -> nothing; [text](url) -> text; [text][ref] -> text.
        if c == '!'
            && chars.get(i + 1) == Some(&'[')
            && let Some((_, end)) = link_at(&chars, i + 1)
        {
            i = end;
            continue;
        }
        if c == '['
            && let Some((text, end)) = link_at(&chars, i)
        {
            out.push_str(&inline(&text));
            i = end;
            continue;
        }
        match c {
            '*' | '`' => {}
            // `_` only as emphasis at a word edge; snake_case names stay.
            '_' => {
                let prev = i.checked_sub(1).and_then(|p| chars.get(p));
                let next = chars.get(i + 1);
                let word = |x: Option<&char>| x.is_some_and(|c| c.is_alphanumeric());
                if word(prev) && word(next) {
                    out.push('_');
                }
            }
            '~' if chars.get(i + 1) == Some(&'~') => i += 1,
            '\\' if chars.get(i + 1).is_some_and(|n| n.is_ascii_punctuation()) => {
                out.push(chars[i + 1]);
                i += 1;
            }
            c if is_unsafe_char(c) => {}
            c => out.push(c),
        }
        i += 1;
    }
    let out = decode_entities(&out);
    let out: String = out.chars().filter(|c| !is_unsafe_char(*c)).collect();
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `[text](target)` or `[text][ref]` starting at `chars[start] == '['`: the text and the
/// index after the link.
fn link_at(chars: &[char], start: usize) -> Option<(String, usize)> {
    let mut depth = 0;
    let mut j = start;
    while j < chars.len() {
        match chars[j] {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        j += 1;
    }
    if j >= chars.len() {
        return None;
    }
    let text: String = chars[start + 1..j].iter().collect();
    let (open, close) = match chars.get(j + 1) {
        Some('(') => ('(', ')'),
        Some('[') => ('[', ']'),
        _ => return None,
    };
    let mut k = j + 2;
    let mut d = 1;
    while k < chars.len() {
        if chars[k] == open {
            d += 1;
        } else if chars[k] == close {
            d -= 1;
            if d == 0 {
                return Some((text, k + 1));
            }
        }
        k += 1;
    }
    None
}

fn decode_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
}

/// Control characters and the bidi overrides/isolates that can make text read differently
/// from what it is (Trojan Source style), plus zero-width characters.
fn is_unsafe_char(c: char) -> bool {
    (c.is_control() && c != '\t')
        || matches!(
            c,
            '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}'
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(body: &str) -> Vec<(NoteKind, String)> {
        sanitize(body)
            .0
            .into_iter()
            .map(|b| (b.kind, b.text))
            .collect()
    }

    #[test]
    fn release_please_body_becomes_plain_blocks() {
        let body = "## [0.2.0](https://github.com/voidput/starorg-codex/compare/v0.1.0...v0.2.0) (2026-11-20)\n\n\
                    ### Features\n\n\
                    * **app:** update check ([#15](https://github.com/voidput/starorg-codex/issues/15)) ([abc1234](https://github.com/x/y/commit/abc1234))\n\
                    * keep `snake_case` names\n\n\
                    ### Bug Fixes\n\n\
                    1. one\n2) two\n";
        assert_eq!(
            texts(body),
            vec![
                (NoteKind::Heading, "0.2.0 (2026-11-20)".into()),
                (NoteKind::Heading, "Features".into()),
                (NoteKind::Item, "app: update check (#15) (abc1234)".into()),
                (NoteKind::Item, "keep snake_case names".into()),
                (NoteKind::Heading, "Bug Fixes".into()),
                (NoteKind::Item, "one".into()),
                (NoteKind::Item, "two".into()),
            ]
        );
    }

    #[test]
    fn html_scripts_images_and_links_never_survive() {
        let body = "Hello <script>alert(1)</script> world\n\n\
                    <img src=x onerror=alert(1)>\n\
                    ![logo](https://evil.example/x.png)\n\n\
                    <!-- hidden\ncomment -->\n\
                    [click](javascript:alert(1)) and <a href=\"https://evil.example\">here</a>\n\n\
                    <details><summary>More</summary>inside</details>\n\
                    a < b and 2>1";
        let all: String = sanitize(body)
            .0
            .iter()
            .map(|b| b.text.clone())
            .collect::<Vec<_>>()
            .join("|");
        for bad in [
            "<script",
            "<img",
            "onerror",
            "evil.example",
            "javascript:",
            "hidden",
            "<a",
            "png",
        ] {
            assert!(!all.contains(bad), "{bad} survived in {all:?}");
        }
        assert!(all.contains("Hello alert(1) world"), "{all}");
        assert!(all.contains("click and here"), "{all}");
        assert!(all.contains("More"), "{all}");
        assert!(all.contains("a < b and 2>1"), "{all}");
    }

    #[test]
    fn entities_decode_to_text_not_markup() {
        // Decoded after tags are stripped, so this is shown as the literal text "<b>".
        let (b, _) = sanitize("x &lt;b&gt; &amp; y");
        assert_eq!(b[0].text, "x <b> & y");
    }

    #[test]
    fn bidi_overrides_and_controls_are_dropped() {
        let (b, _) = sanitize("safe\u{202E}txt.exe\u{200B} \u{7}bell\u{2066}x");
        assert_eq!(b[0].text, "safetxt.exe bellx");
    }

    #[test]
    fn limits_cut_and_flag() {
        let many = (0..100)
            .map(|i| format!("- item {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let (b, cut) = sanitize(&many);
        assert_eq!(b.len(), MAX_BLOCKS);
        assert!(cut);
        let (b, cut) = sanitize(&"word ".repeat(200));
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].text.chars().count(), MAX_BLOCK_CHARS);
        assert!(b[0].text.ends_with('…'));
        assert!(cut);
        let (_, cut) = sanitize(&"x".repeat(MAX_INPUT_CHARS + 1));
        assert!(cut);
        let (b, cut) = sanitize("short");
        assert_eq!(b.len(), 1);
        assert!(!cut);
    }

    #[test]
    fn code_fences_rules_and_quotes() {
        assert_eq!(
            texts("> quoted\n> more\n\n---\n```sh\nrm -rf <dir>\n```\n"),
            vec![
                (NoteKind::Text, "quoted more".into()),
                // Inside code too: anything shaped like a tag is dropped.
                (NoteKind::Text, "rm -rf".into()),
            ]
        );
        assert!(texts("").is_empty());
        assert!(texts("<p></p>\n\n   ").is_empty());
    }
}

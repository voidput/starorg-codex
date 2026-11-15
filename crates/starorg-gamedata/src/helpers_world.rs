//! Small JavaScript-exact helpers shared by the world/economy domain ports (commodities,
//! refining, mining, mining spawns, shops, starmap). No regex crate: each helper spells out
//! the one TypeScript expression it replaces.

use serde_json::Value;

/// `String.prototype.trim` whitespace: WhiteSpace + LineTerminator (ECMA-262), which
/// differs from Rust's `char::is_whitespace` (JS adds U+FEFF, drops U+0085).
pub fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// `s.trim()`.
pub fn js_trim(s: &str) -> &str {
    s.trim_matches(is_js_space)
}

/// `s.replace(/<[^>]*>/g, "")`: drop every `<...>` run that has a closing `>`.
pub fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find('<') {
        match rest[open..].find('>') {
            Some(close) => {
                out.push_str(&rest[..open]);
                rest = &rest[open + close + 1..];
            }
            None => break,
        }
    }
    out.push_str(rest);
    out
}

/// `s.replace(/\\n/g, "\n")`: the string table's literal backslash-n.
pub fn literal_newlines(s: &str) -> String {
    s.replace("\\n", "\n")
}

/// `s.split("/").pop()`: the last path segment (the whole string without a slash).
pub fn last_segment(s: &str) -> &str {
    s.rsplit('/').next().unwrap_or(s)
}

/// `typeof v === "object" && v !== null && !Array.isArray(v)`-ish truthy object test used
/// by `.filter((e) => !!e && typeof e === "object")`; arrays pass in JS too.
pub fn is_js_object(v: &Value) -> bool {
    v.is_object() || v.is_array()
}

/// JS truthiness of a JSON value.
pub fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_tags_and_newlines() {
        assert_eq!(js_trim("\u{FEFF} a \u{A0}"), "a");
        assert_eq!(js_trim("\u{85}a"), "\u{85}a");
        assert_eq!(strip_tags(r#"CO<font size="%d">2</font>"#), "CO2");
        assert_eq!(strip_tags("a < b"), "a < b");
        assert_eq!(strip_tags("<b>x</b> <i"), "x <i");
        assert_eq!(literal_newlines(r"a\nb"), "a\nb");
        assert_eq!(last_segment("a/b/c"), "c");
        assert_eq!(last_segment("c"), "c");
    }
}

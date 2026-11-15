//! Tokens and the validation rules that apply to generated text (spec 7.3).
//!
//! Validation compares our output with stock, not with an ideal: stock values with
//! unbalanced EM tags or `%` signs are fine as long as we keep them as they are. A generated
//! value that breaks a rule is dropped (the key keeps its stock text) and reported.
//!
//! M1's merge validates the other layers (loc patches, overrides) with the same token
//! definitions; this module covers the generated layer and the whole-file checks of `build`.

use crate::ini::{BOM, StockIni};
use std::collections::BTreeMap;

/// Values longer than this are written but reported (stock ships one of 16,026 bytes).
pub const WARN_LEN: usize = 16_000;
/// Hard limit per value.
pub const MAX_LEN: usize = 64 * 1024;

/// Per EM tag number: (unmatched opens, unmatched closes, nested opens).
pub type EmSignature = BTreeMap<u32, (u32, u32, u32)>;

/// Tokens of one value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tokens<'a> {
    /// `~name(...)` variable tokens, in order.
    pub variables: Vec<&'a str>,
    /// Format specifiers (`%s`, `%.2f`, ...), in order. `%%` is not one.
    pub formats: Vec<&'a str>,
    /// Positions of literal `%` signs (neither a specifier nor `%%`).
    pub literal_percent: Vec<usize>,
    pub em: EmSignature,
    /// True when the value ends while an EM tag is still open.
    pub open_tail: bool,
}

impl Tokens<'_> {
    /// No unmatched opens or closes and nothing nested.
    pub fn em_clean(&self) -> bool {
        self.em
            .values()
            .all(|&(o, c, n)| o == 0 && c == 0 && n == 0)
    }

    /// Signature without the nesting counts.
    fn em_balance(&self) -> BTreeMap<u32, (u32, u32)> {
        self.em
            .iter()
            .filter(|(_, (o, c, _))| *o > 0 || *c > 0)
            .map(|(k, (o, c, _))| (*k, (*o, *c)))
            .collect()
    }
}

/// `<EMn>` or `</EMn>` at `i`: (tag number, is close, byte length).
fn em_tag(b: &[u8], i: usize) -> Option<(u32, bool, usize)> {
    let rest = &b[i..];
    let (close, start) = if rest.starts_with(b"</EM") {
        (true, 4)
    } else if rest.starts_with(b"<EM") {
        (false, 3)
    } else {
        return None;
    };
    let mut j = start;
    while j < rest.len() && rest[j].is_ascii_digit() {
        j += 1;
    }
    if j == start || rest.get(j) != Some(&b'>') {
        return None;
    }
    let n = std::str::from_utf8(&rest[start..j]).ok()?.parse().ok()?;
    Some((n, close, j + 1))
}

/// Length of a format specifier starting at `i` (which holds `%`), if any.
fn format_spec(b: &[u8], i: usize) -> Option<usize> {
    let mut j = i + 1;
    while j < b.len() && b"-+ 0#".contains(&b[j]) {
        j += 1;
    }
    while j < b.len() && b[j].is_ascii_digit() {
        j += 1;
    }
    if j < b.len() && b[j] == b'.' {
        let k = j + 1;
        let mut m = k;
        while m < b.len() && b[m].is_ascii_digit() {
            m += 1;
        }
        if m == k {
            return None;
        }
        j = m;
    }
    if b[j..].starts_with(b"ll") {
        j += 2;
    } else if j < b.len() && (b[j] == b'l' || b[j] == b'h') {
        j += 1;
    }
    (j < b.len() && b"sSdiufFgGxXc".contains(&b[j])).then_some(j + 1 - i)
}

/// `~letters(...)` at `i`, if any.
fn variable(b: &[u8], i: usize) -> Option<usize> {
    let mut j = i + 1;
    while j < b.len() && b[j].is_ascii_alphabetic() {
        j += 1;
    }
    if j == i + 1 || b.get(j) != Some(&b'(') {
        return None;
    }
    let close = b[j..].iter().position(|&c| c == b')')?;
    Some(j + close + 1 - i)
}

/// Tokenise a value.
pub fn tokens(s: &str) -> Tokens<'_> {
    let b = s.as_bytes();
    let mut t = Tokens::default();
    let mut open: BTreeMap<u32, u32> = BTreeMap::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'<' => {
                if let Some((n, close, len)) = em_tag(b, i) {
                    let e = t.em.entry(n).or_default();
                    if close {
                        match open.get_mut(&n) {
                            Some(c) if *c > 0 => *c -= 1,
                            _ => e.1 += 1,
                        }
                    } else {
                        if open.values().any(|c| *c > 0) {
                            e.2 += 1;
                        }
                        *open.entry(n).or_default() += 1;
                    }
                    i += len;
                    continue;
                }
            }
            b'%' => {
                if b.get(i + 1) == Some(&b'%') {
                    i += 2;
                    continue;
                }
                if let Some(len) = format_spec(b, i) {
                    t.formats.push(&s[i..i + len]);
                    i += len;
                    continue;
                }
                t.literal_percent.push(i);
            }
            b'~' => {
                if let Some(len) = variable(b, i) {
                    t.variables.push(&s[i..i + len]);
                    i += len;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
    for (n, c) in open {
        if c > 0 {
            t.em.entry(n).or_default().0 += c;
            t.open_tail = true;
        }
    }
    // Balanced, unnested tags leave no trace in the signature.
    t.em.retain(|_, v| *v != (0, 0, 0));
    t
}

/// A format key: its stock value has at least one format specifier.
pub fn is_format_value(stock: &str) -> bool {
    !tokens(stock).formats.is_empty()
}

/// Percent rule (spec 5.1): every `%` we add is followed by the end of the value, a `\n`
/// escape or two spaces. `at_end` says whether the end of `added` is the end of the value.
pub fn percent_rule_ok(added: &str, at_end: bool) -> bool {
    let b = added.as_bytes();
    tokens(added).literal_percent.iter().all(|&i| {
        let rest = &b[i + 1..];
        (rest.is_empty() && at_end) || rest.starts_with(b"\\n") || rest.starts_with(b"  ")
    })
}

/// Why a generated value was dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    ControlChar,
    AddedEmUnbalanced,
    EmSignature,
    Variables,
    FormatSpecifiers,
    FormatKey,
    Percent,
    TooLong,
}

impl Rule {
    pub fn name(&self) -> &'static str {
        match self {
            Self::ControlChar => "control-character",
            Self::AddedEmUnbalanced => "added-em-unbalanced",
            Self::EmSignature => "em-signature",
            Self::Variables => "variables",
            Self::FormatSpecifiers => "format-specifiers",
            Self::FormatKey => "format-key",
            Self::Percent => "percent",
            Self::TooLong => "too-long",
        }
    }
}

/// Check a generated value `prefix + stock_part + suffix` against the stock value.
/// `stock_part` is the stock value, possibly with its trailing `\n` escapes and spaces
/// trimmed (spec 5.1).
pub fn check_generated(
    stock: &str,
    stock_part: &str,
    prefix: &str,
    suffix: &str,
) -> Result<(), Rule> {
    let value_len = prefix.len() + stock_part.len() + suffix.len();
    if value_len > MAX_LEN {
        return Err(Rule::TooLong);
    }
    let control = |s: &str| s.chars().any(|c| c.is_control() && c != '\t');
    if control(prefix) || control(suffix) || control(stock_part) {
        return Err(Rule::ControlChar);
    }
    let st = tokens(stock);
    if !st.formats.is_empty() {
        return Err(Rule::FormatKey);
    }
    for part in [prefix, suffix] {
        let t = tokens(part);
        if !t.em_clean() {
            return Err(Rule::AddedEmUnbalanced);
        }
        if !t.formats.is_empty() {
            return Err(Rule::FormatSpecifiers);
        }
        // Generated text never brings variable tokens.
        if !t.variables.is_empty() {
            return Err(Rule::Variables);
        }
    }
    if !percent_rule_ok(prefix, false) || !percent_rule_ok(suffix, true) {
        return Err(Rule::Percent);
    }
    let value = format!("{prefix}{stock_part}{suffix}");
    let vt = tokens(&value);
    // Unmatched opens/closes must equal stock's. Appending balanced text after an open tail
    // nests it inside CIG's tag; that is allowed (spec 7.3, counted as `open-tail`).
    if vt.em_balance() != st.em_balance() {
        return Err(Rule::EmSignature);
    }
    if !st.open_tail && vt.em != st.em {
        return Err(Rule::EmSignature);
    }
    if vt.variables != st.variables {
        return Err(Rule::Variables);
    }
    if vt.formats != st.formats {
        return Err(Rule::FormatSpecifiers);
    }
    Ok(())
}

/// A whole-file failure: a bug in our writer; nothing is written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FileError {
    #[error("output does not start with the UTF-8 BOM")]
    Bom,
    #[error("output is not valid UTF-8")]
    Utf8,
    #[error("line count or line endings differ from stock (line {0})")]
    Lines(usize),
    #[error("key sequence differs from stock (line {0})")]
    Keys(usize),
}

/// Whole-file rules (spec 7.3) on the rendered bytes.
pub fn check_file(stock: &StockIni, out: &[u8]) -> Result<(), FileError> {
    if stock.has_bom() && !out.starts_with(BOM) {
        return Err(FileError::Bom);
    }
    if std::str::from_utf8(out).is_err() {
        return Err(FileError::Utf8);
    }
    let again = StockIni::parse(out.to_vec());
    if again.line_count() != stock.line_count() {
        return Err(FileError::Lines(again.line_count().min(stock.line_count())));
    }
    let endings = |b: &[u8]| -> Vec<u8> {
        // Per line: 2 = CRLF, 1 = LF, 0 = none.
        let mut v = Vec::new();
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'\n' {
                v.push(if i > 0 && b[i - 1] == b'\r' { 2 } else { 1 });
            }
            i += 1;
        }
        v
    };
    let (a, b) = (endings(stock.as_bytes()), endings(out));
    if let Some(i) = a.iter().zip(&b).position(|(x, y)| x != y) {
        return Err(FileError::Lines(i));
    }
    if a.len() != b.len() {
        return Err(FileError::Lines(a.len().min(b.len())));
    }
    for i in 0..stock.line_count() {
        if stock.key_at(i) != again.key_at(i) {
            return Err(FileError::Keys(i));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_kinds() {
        let t =
            tokens("Go to ~mission(Location|Address) for %s and %.2f, 25% off, %% and %ls %02i");
        assert_eq!(t.variables, vec!["~mission(Location|Address)"]);
        assert_eq!(t.formats, vec!["%s", "%.2f", "%ls", "%02i"]);
        assert_eq!(t.literal_percent.len(), 1);
        assert!(is_format_value("x %d y"));
        assert!(!is_format_value("50% more"));
        assert!(!is_format_value("100%%"));
    }

    #[test]
    fn em_signature() {
        let t = tokens("<EM4>a</EM4> <EM3>b</EM3>");
        assert!(t.em_clean() && !t.open_tail);
        let t = tokens("<EM4>a <EM3>b</EM3></EM4>");
        assert_eq!(t.em[&3], (0, 0, 1));
        let t = tokens("text <EM4>open");
        assert_eq!(t.em[&4], (1, 0, 0));
        assert!(t.open_tail);
        let t = tokens("</EM1> stray");
        assert_eq!(t.em[&1], (0, 1, 0));
        // Not tags.
        assert!(tokens("<EM> <EMx> <em4>").em.is_empty());
    }

    #[test]
    fn generated_rules() {
        assert_eq!(check_generated("prose", "prose", "", "\\n\\nA: 1"), Ok(()));
        assert_eq!(
            check_generated("prose", "prose", "", "<EM4>x"),
            Err(Rule::AddedEmUnbalanced)
        );
        assert_eq!(
            check_generated("x %s", "x %s", "", " tail"),
            Err(Rule::FormatKey)
        );
        assert_eq!(
            check_generated("p", "p", "", " ~mission(X)"),
            Err(Rule::Variables)
        );
        assert_eq!(
            check_generated("p", "p", "", "\\nA: 25% off"),
            Err(Rule::Percent)
        );
        assert_eq!(check_generated("p", "p", "", "\\nA: 25%"), Ok(()));
        assert_eq!(check_generated("p", "p", "", "\\nA: 25%\\nB"), Ok(()));
        assert_eq!(check_generated("p", "p", "", "\\nA: 25%  |  B"), Ok(()));
        assert_eq!(
            check_generated("p", "p", "", "a\rb"),
            Err(Rule::ControlChar)
        );
        let long = "x".repeat(MAX_LEN);
        assert_eq!(check_generated("p", "p", "", &long), Err(Rule::TooLong));
        // Stock with CIG's unbalanced tags keeps them; our balanced suffix is fine.
        assert_eq!(
            check_generated("a </EM4> b", "a </EM4> b", "", "\\n<EM3>X</EM3>"),
            Ok(())
        );
        // Open tail: appending nests our tag inside CIG's; allowed and counted.
        assert_eq!(
            check_generated("a <EM4>b", "a <EM4>b", "", "\\n<EM3>X</EM3>"),
            Ok(())
        );
        // A prefix tag is fine.
        assert_eq!(check_generated("Gun", "Gun", "[E-S2] ", ""), Ok(()));
        // Trimmed stock part keeps variables.
        assert_eq!(check_generated("~a(b)\\n ", "~a(b)", "", "\\n\\nX"), Ok(()));
    }

    #[test]
    fn whole_file_rules() {
        let mut bytes = BOM.to_vec();
        bytes.extend_from_slice(b"a=1\r\nb,P=2\r\n");
        let stock = StockIni::parse(bytes.clone());
        assert_eq!(check_file(&stock, &bytes), Ok(()));
        assert_eq!(check_file(&stock, &bytes[3..]), Err(FileError::Bom));
        let mut changed = BOM.to_vec();
        changed.extend_from_slice(b"a=1\nb,P=2\r\n");
        assert!(matches!(
            check_file(&stock, &changed),
            Err(FileError::Lines(_))
        ));
        let mut renamed = BOM.to_vec();
        renamed.extend_from_slice(b"a=1\r\nb=2\r\n");
        assert_eq!(check_file(&stock, &renamed), Err(FileError::Keys(1)));
        let mut split = BOM.to_vec();
        split.extend_from_slice(b"a=1\r\nx\r\nb,P=2\r\n");
        assert!(matches!(
            check_file(&stock, &split),
            Err(FileError::Lines(_))
        ));
    }
}

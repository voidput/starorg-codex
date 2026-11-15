//! Validation of our output against **stock** (spec 7.3).
//!
//! Stock is not clean (unbalanced and nested EM tags, `%` everywhere), so every rule compares
//! a changed value with the stock value of the same key. Per-value rules drop one edit at a
//! time; whole-file rules stop the apply before anything is written.

use crate::ini::{BOM, StockIni};
use std::collections::BTreeMap;

/// Changed values longer than this get a warning (stock ships one of 16,026 bytes; Q9).
pub const WARN_LEN: usize = 16_000;
/// Hard limit per value.
pub const MAX_LEN: usize = 64 * 1024;

/// How a candidate value was made. Decides which rules apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// Generator added a prefix and/or suffix around the stock value.
    GeneratedParts,
    /// Generator replaced the whole value.
    GeneratedWhole,
    /// Loc patch `set`.
    Set,
    Append,
    Prepend,
    Replace,
    /// Loc patch `append_from`; `borrows_stock` when the part came from another stock value
    /// (`from_part = stock | final`), which may bring variable tokens.
    AppendFrom {
        borrows_stock: bool,
    },
    /// `overrides.ini`.
    Override,
}

impl Op {
    fn whole_replacement(self) -> bool {
        matches!(self, Op::GeneratedWhole | Op::Set | Op::Override)
    }
    fn adds_text(self) -> bool {
        matches!(
            self,
            Op::GeneratedParts | Op::Append | Op::Prepend | Op::AppendFrom { .. }
        )
    }
    fn generated(self) -> bool {
        matches!(self, Op::GeneratedParts | Op::GeneratedWhole)
    }
    fn allowed_on_format_key(self) -> bool {
        matches!(self, Op::Set | Op::Replace | Op::Override)
    }
}

/// A rule that failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rule {
    ControlChar,
    AddedEmUnbalanced,
    EmSignature,
    Variables,
    FormatSpecifiers,
    FormatKey,
    PercentRule,
    TooLong,
}

impl Rule {
    pub fn name(&self) -> &'static str {
        match self {
            Rule::ControlChar => "control-character",
            Rule::AddedEmUnbalanced => "added-em-unbalanced",
            Rule::EmSignature => "em-signature",
            Rule::Variables => "variable-tokens",
            Rule::FormatSpecifiers => "format-specifiers",
            Rule::FormatKey => "format-key",
            Rule::PercentRule => "percent-rule",
            Rule::TooLong => "too-long",
        }
    }
}

/// Per tag number: (unmatched opens, unmatched closes, nested opens).
pub type EmSignature = BTreeMap<u32, (u32, u32, u32)>;

/// Tokens of one value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tokens<'a> {
    pub variables: Vec<&'a str>,
    pub formats: Vec<&'a str>,
    pub literal_percent: usize,
    /// Per tag number; "nested" counts an open of tag n while tag n is already open.
    pub em: EmSignature,
    /// Some tag was opened while another (any number) was open.
    pub any_nesting: bool,
    /// The value ends while an EM tag is still open (CIG's "open tail" values).
    pub open_tail: bool,
}

impl Tokens<'_> {
    /// Balanced and nothing nested: what text we add must be.
    pub fn em_balanced(&self) -> bool {
        !self.any_nesting
            && self
                .em
                .values()
                .all(|&(o, c, n)| o == 0 && c == 0 && n == 0)
    }
}

fn em_tag(b: &[u8], i: usize) -> Option<(bool, u32, usize)> {
    // <EMn> or </EMn>
    let rest = &b[i..];
    let (close, start) = if rest.starts_with(b"</EM") {
        (true, 4)
    } else if rest.starts_with(b"<EM") {
        (false, 3)
    } else {
        return None;
    };
    let digits = rest[start..]
        .iter()
        .take_while(|c| c.is_ascii_digit())
        .count();
    if digits == 0 || rest.get(start + digits) != Some(&b'>') {
        return None;
    }
    let n: u32 = std::str::from_utf8(&rest[start..start + digits])
        .ok()?
        .parse()
        .ok()?;
    Some((close, n, start + digits + 1))
}

fn format_spec(b: &[u8], i: usize) -> Option<usize> {
    // % [-+ 0#]* [0-9]* (.[0-9]+)? (ll|l|h)? [sSdiufFgGxXc]
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
    if j < b.len() && b"sSdiufFgGxXc".contains(&b[j]) {
        Some(j + 1)
    } else {
        None
    }
}

/// Split a value into the tokens of spec 7.3.
pub fn tokens(s: &str) -> Tokens<'_> {
    let b = s.as_bytes();
    let mut t = Tokens::default();
    let mut open: BTreeMap<u32, u32> = BTreeMap::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'~' => {
                let letters = b[i + 1..]
                    .iter()
                    .take_while(|c| c.is_ascii_alphabetic())
                    .count();
                if letters > 0
                    && b.get(i + 1 + letters) == Some(&b'(')
                    && let Some(close) = b[i + 2 + letters..].iter().position(|&c| c == b')')
                {
                    let end = i + 2 + letters + close + 1;
                    t.variables.push(&s[i..end]);
                    i = end;
                    continue;
                }
                i += 1;
            }
            b'%' => {
                if b.get(i + 1) == Some(&b'%') {
                    i += 2;
                } else if let Some(end) = format_spec(b, i) {
                    t.formats.push(&s[i..end]);
                    i = end;
                } else {
                    t.literal_percent += 1;
                    i += 1;
                }
            }
            b'<' => {
                if let Some((close, n, len)) = em_tag(b, i) {
                    let any_open = open.values().any(|&d| d > 0);
                    let sig = t.em.entry(n).or_default();
                    let depth = open.entry(n).or_default();
                    if close {
                        if *depth > 0 {
                            *depth -= 1;
                        } else {
                            sig.1 += 1;
                        }
                    } else {
                        if *depth > 0 {
                            sig.2 += 1;
                        }
                        if any_open {
                            t.any_nesting = true;
                        }
                        *depth += 1;
                    }
                    i += len;
                } else {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    for (n, d) in open {
        t.em.entry(n).or_default().0 = d;
        t.open_tail |= d > 0;
    }
    t.em.retain(|_, v| *v != (0, 0, 0));
    t
}

/// `a` contains all of `b` in order (as a subsequence).
fn has_in_order(a: &[&str], b: &[&str]) -> bool {
    let mut it = a.iter();
    b.iter().all(|x| it.any(|y| y == x))
}

/// Check one changed value. `added` is the text we added (prefix, suffix, appended part) for
/// the ops that add text. Stock always passes; `value == stock` is never checked.
pub fn check_value(stock: &str, value: &str, op: Op, added: &[&str]) -> Result<(), Rule> {
    if value.len() > MAX_LEN {
        return Err(Rule::TooLong);
    }
    if value.chars().any(|c| c.is_control() && c != '\t') {
        return Err(Rule::ControlChar);
    }
    let st = tokens(stock);
    let vt = tokens(value);
    if op.adds_text() && added.iter().any(|a| !tokens(a).em_balanced()) {
        return Err(Rule::AddedEmUnbalanced);
    }
    // Appending balanced text after CIG's open tail nests it inside their tag: only the
    // unmatched opens and closes must stay as stock has them (spec 7.3, `open-tail`).
    let balance = |t: &Tokens<'_>| -> BTreeMap<u32, (u32, u32)> {
        t.em.iter()
            .filter(|(_, (o, c, _))| *o > 0 || *c > 0)
            .map(|(k, (o, c, _))| (*k, (*o, *c)))
            .collect()
    };
    let em_ok = vt.em == st.em
        || (op.whole_replacement() && vt.em.values().all(|&(o, c, n)| o == 0 && c == 0 && n == 0))
        || (op.adds_text() && st.open_tail && balance(&vt) == balance(&st));
    if !em_ok {
        return Err(Rule::EmSignature);
    }
    let vars_ok = match op {
        Op::AppendFrom {
            borrows_stock: true,
        } => has_in_order(&vt.variables, &st.variables),
        _ => vt.variables == st.variables,
    };
    if !vars_ok {
        return Err(Rule::Variables);
    }
    if vt.formats != st.formats {
        return Err(Rule::FormatSpecifiers);
    }
    if !st.formats.is_empty() && (!op.allowed_on_format_key() || vt.literal_percent > 0) {
        return Err(Rule::FormatKey);
    }
    if op.generated() && added.iter().any(|a| !percent_rule_ok(a, true)) {
        return Err(Rule::PercentRule);
    }
    Ok(())
}

/// Spec 5.1: a `%` we write is followed by the end of the value, the two-character escape
/// `\n`, or two spaces. `at_end` says whether the end of `s` is the end of the value (false
/// for a prefix). `%%` and format specifiers are not literal signs.
pub fn percent_rule_ok(s: &str, at_end: bool) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'%' {
            i += 1;
            continue;
        }
        if b.get(i + 1) == Some(&b'%') {
            i += 2;
            continue;
        }
        if let Some(end) = format_spec(b, i) {
            i = end;
            continue;
        }
        let rest = &b[i + 1..];
        if !((rest.is_empty() && at_end) || rest.starts_with(b"\\n") || rest.starts_with(b"  ")) {
            return false;
        }
        i += 1;
    }
    true
}

/// A format key: its stock value has at least one format specifier.
pub fn is_format_value(stock: &str) -> bool {
    !tokens(stock).formats.is_empty()
}

/// A whole-file failure: a bug in our writer. Nothing is written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FileError {
    #[error("output does not start with the UTF-8 BOM")]
    NoBom,
    #[error("output is not valid UTF-8")]
    NotUtf8,
    #[error("output has {got} lines, stock has {want}")]
    LineCount { got: usize, want: usize },
    #[error("line {0}: line ending differs from stock")]
    LineEnding(usize),
    #[error("line {0}: key differs from stock")]
    KeyChanged(usize),
}

/// Whole-file rules on the rendered bytes.
///
/// The BOM rule holds whatever stock looks like (spec 7.3): the game's own file starts with
/// one, so a stock without it means the input is wrong, and nothing is written.
pub fn check_file(stock: &StockIni, out: &[u8]) -> Result<(), FileError> {
    if !out.starts_with(BOM) {
        return Err(FileError::NoBom);
    }
    if std::str::from_utf8(out).is_err() {
        return Err(FileError::NotUtf8);
    }
    let rendered = StockIni::parse(out.to_vec());
    if rendered.line_count() != stock.line_count() {
        return Err(FileError::LineCount {
            got: rendered.line_count(),
            want: stock.line_count(),
        });
    }
    for i in 0..stock.line_count() {
        if stock.line_ending(i) != rendered.line_ending(i) {
            return Err(FileError::LineEnding(i + 1));
        }
        if stock.key_at(i) != rendered.key_at(i) {
            return Err(FileError::KeyChanged(i + 1));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_parsing() {
        let t = tokens("~mission(Location|Address) has %s and %.2f, 25% and %% <EM4>x</EM4>");
        assert_eq!(t.variables, vec!["~mission(Location|Address)"]);
        assert_eq!(t.formats, vec!["%s", "%.2f"]);
        assert_eq!(t.literal_percent, 1);
        assert!(t.em_balanced());
        let t = tokens("%ls %S %i %d %u %02i %-5d %lld %hx");
        assert_eq!(t.formats.len(), 9, "{:?}", t.formats);
        let t = tokens("<EM4>open <EM3>inner</EM3> </EM2> <EM4>again");
        assert_eq!(t.em[&4], (2, 0, 1));
        assert!(!t.em.contains_key(&3));
        assert_eq!(t.em[&2], (0, 1, 0));
        assert!(t.any_nesting && !t.em_balanced());
        assert!(!tokens("<EM3>a</EM3><EM4>b</EM4>").any_nesting);
        assert!(tokens("text <EM4>open").open_tail);
        assert!(!tokens("<EM4>a</EM4>").open_tail);
        assert!(tokens("<EM>not a tag <EMx>").em.is_empty());
        assert!(tokens("~ not a var ~x(").variables.is_empty());
    }

    #[test]
    fn stock_shapes_pass_when_unchanged_signature() {
        // Stock has an unbalanced open tag; appending balanced text keeps the signature.
        let stock = "Desc <EM4>open tail";
        let added = "\\n\\n<EM3>STATS</EM3>";
        let value = format!("{stock}{added}");
        assert_eq!(check_value(stock, &value, Op::Append, &[added]), Ok(()));
        // A full replacement may repair CIG's tags.
        assert_eq!(
            check_value(stock, "Desc <EM4>fixed</EM4>", Op::Override, &[]),
            Ok(())
        );
        // ...but a partial edit may not change the signature.
        assert_eq!(
            check_value(stock, "Desc <EM4>open</EM4>", Op::Replace, &[]),
            Err(Rule::EmSignature)
        );
    }

    #[test]
    fn each_rule_drops_the_bad_value() {
        let s = "Go to ~mission(Location) now";
        assert_eq!(
            check_value(s, "x\u{1}", Op::Override, &[]),
            Err(Rule::ControlChar)
        );
        assert_eq!(
            check_value(s, "a\tb ~mission(Location)", Op::Override, &[]),
            Ok(())
        );
        assert_eq!(
            check_value(s, "Go now", Op::Override, &[]),
            Err(Rule::Variables)
        );
        assert_eq!(
            check_value(s, &format!("{s} <EM4>x"), Op::Append, &[" <EM4>x"]),
            Err(Rule::AddedEmUnbalanced)
        );
        // Borrowed stock text may bring extra variables.
        let added = " ~action(Use)";
        assert_eq!(
            check_value(
                s,
                &format!("{s}{added}"),
                Op::AppendFrom {
                    borrows_stock: true
                },
                &[added]
            ),
            Ok(())
        );
        assert_eq!(
            check_value(s, &format!("{s}{added}"), Op::Append, &[added]),
            Err(Rule::Variables)
        );
        // Format keys.
        let f = "Deals %i damage";
        assert_eq!(check_value(f, "Deals %i dmg", Op::Override, &[]), Ok(()));
        assert_eq!(
            check_value(f, "Deals %i dmg 5%", Op::Override, &[]),
            Err(Rule::FormatKey)
        );
        assert_eq!(
            check_value(f, "Deals %d damage", Op::Override, &[]),
            Err(Rule::FormatSpecifiers)
        );
        assert_eq!(
            check_value(f, "Deals %i damage!", Op::Append, &["!"]),
            Err(Rule::FormatKey)
        );
        // Percent rule for generated text.
        assert_eq!(
            check_value("a", "a 25%  b", Op::GeneratedParts, &[" 25%  b"]),
            Ok(())
        );
        assert_eq!(
            check_value("a", "a 25% b", Op::GeneratedParts, &[" 25% b"]),
            Err(Rule::PercentRule)
        );
        assert_eq!(
            check_value("a", &"x".repeat(MAX_LEN + 1), Op::Override, &[]),
            Err(Rule::TooLong)
        );
    }

    #[test]
    fn whole_file_rules() {
        let stock = StockIni::parse(b"\xEF\xBB\xBFa=1\r\nb,P=2\r\n".to_vec());
        assert_eq!(check_file(&stock, stock.as_bytes()), Ok(()));
        assert_eq!(
            check_file(&stock, b"a=1\r\nb,P=2\r\n"),
            Err(FileError::NoBom)
        );
        assert_eq!(
            check_file(&stock, b"\xEF\xBB\xBFa=1\r\nb,P=2\n"),
            Err(FileError::LineEnding(2))
        );
        assert_eq!(
            check_file(&stock, b"\xEF\xBB\xBFa=1\r\nb=2\r\n"),
            Err(FileError::KeyChanged(2))
        );
        assert!(matches!(
            check_file(&stock, b"\xEF\xBB\xBFa=1\r\nx\r\nb,P=2\r\n"),
            Err(FileError::LineCount { .. })
        ));
        assert_eq!(
            check_file(&stock, b"\xEF\xBB\xBFa=\xFF\r\nb,P=2\r\n"),
            Err(FileError::NotUtf8)
        );
        // A BOM-less stock does not make a BOM-less output acceptable.
        let no_bom = StockIni::parse(b"a=1\r\nb,P=2\r\n".to_vec());
        assert_eq!(
            check_file(&no_bom, no_bom.as_bytes()),
            Err(FileError::NoBom)
        );
        assert!(percent_rule_ok("25%", true));
        assert!(!percent_rule_ok("25%", false));
        assert!(percent_rule_ok("%s and 100%%", false));
    }
}

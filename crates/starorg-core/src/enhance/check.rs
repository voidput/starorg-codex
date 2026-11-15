//! The generators' entry into the one set of validation rules (spec 7.3, `crate::validate`).
//!
//! Validation compares our output with stock, not with an ideal: stock values with
//! unbalanced EM tags or `%` signs are fine as long as we keep them as they are. A generated
//! value that breaks a rule is dropped (the key keeps its stock text) and reported. `build`
//! and `apply` run the same functions, so they keep exactly the same values.

pub use crate::validate::{
    FileError, MAX_LEN, Rule, Tokens, WARN_LEN, check_file, is_format_value, percent_rule_ok,
    tokens,
};
use crate::validate::{Op, check_value};

/// Check a generated value `prefix + stock_part + suffix` against the stock value.
/// `stock_part` is the stock value, possibly with its trailing `\n` escapes and spaces
/// trimmed (spec 5.1). Appending balanced text after CIG's open tail nests it inside their
/// tag; that keeps the signature and is allowed (counted as `open-tail`).
pub fn check_generated(
    stock: &str,
    stock_part: &str,
    prefix: &str,
    suffix: &str,
) -> Result<(), Rule> {
    if is_format_value(stock) {
        return Err(Rule::FormatKey);
    }
    let value = format!("{prefix}{stock_part}{suffix}");
    let added: Vec<&str> = [prefix, suffix]
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect();
    check_value(stock, &value, Op::GeneratedParts, &added)?;
    if !percent_rule_ok(prefix, false) {
        return Err(Rule::PercentRule);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ini::{BOM, StockIni};

    #[test]
    fn token_kinds() {
        let t =
            tokens("Go to ~mission(Location|Address) for %s and %.2f, 25% off, %% and %ls %02i");
        assert_eq!(t.variables, vec!["~mission(Location|Address)"]);
        assert_eq!(t.formats, vec!["%s", "%.2f", "%ls", "%02i"]);
        assert_eq!(t.literal_percent, 1);
        assert!(is_format_value("x %d y"));
        assert!(!is_format_value("50% more"));
        assert!(!is_format_value("100%%"));
    }

    #[test]
    fn em_signature() {
        let t = tokens("<EM4>a</EM4> <EM3>b</EM3>");
        assert!(t.em_balanced() && !t.open_tail);
        let t = tokens("<EM4>a <EM3>b</EM3></EM4>");
        assert!(t.any_nesting && !t.em_balanced());
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
            Err(Rule::PercentRule)
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
        assert_eq!(check_file(&stock, &bytes[3..]), Err(FileError::NoBom));
        let mut changed = BOM.to_vec();
        changed.extend_from_slice(b"a=1\nb,P=2\r\n");
        assert!(matches!(
            check_file(&stock, &changed),
            Err(FileError::LineEnding(_))
        ));
        let mut renamed = BOM.to_vec();
        renamed.extend_from_slice(b"a=1\r\nb=2\r\n");
        assert_eq!(check_file(&stock, &renamed), Err(FileError::KeyChanged(2)));
        let mut split = BOM.to_vec();
        split.extend_from_slice(b"a=1\r\nx\r\nb,P=2\r\n");
        assert!(matches!(
            check_file(&stock, &split),
            Err(FileError::LineCount { .. })
        ));
    }
}

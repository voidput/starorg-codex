//! The generators' entry into the one set of validation rules (spec 7.3, `crate::validate`).
//!
//! Validation compares our output with stock, not with an ideal: stock values with
//! unbalanced EM tags or `%` signs are fine as long as we keep them as they are. A generated
//! value that breaks a rule is dropped (the key keeps its stock text) and reported. `build`
//! and `apply` run the same functions, so they keep exactly the same values.

pub use crate::validate::{
    FileError, MAX_LEN, Rule, Tokens, WARN_LEN, added_text_ok, check_file, check_file_with,
    is_format_value, percent_rule_ok, raw_loc_refs, tokens,
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

/// Check a generated value that replaces the stock value (a ship rename with its prefix).
/// A format key stays stock; the rest are the whole-value rules (spec 7.3).
pub fn check_replaced(stock: &str, value: &str) -> Result<(), Rule> {
    if is_format_value(stock) {
        return Err(Rule::FormatKey);
    }
    check_value(stock, value, Op::GeneratedWhole, &[value])
}

/// Check the value of a key we add (stock has no line for it): the whole-value rules
/// against an empty stock value, so it holds no variables, format specifiers or EM tags out
/// of balance, and no raw `@` key or placeholder ([`added_text_ok`]).
pub fn check_added(value: &str) -> Result<(), Rule> {
    check_value("", value, Op::GeneratedWhole, &[value])?;
    if !percent_rule_ok(value, true) {
        return Err(Rule::PercentRule);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ini::{BOM, StockIni};

    #[test]
    fn generated_parts_and_added_values_never_bring_raw_keys_or_placeholders() {
        assert_eq!(
            check_generated("Gun", "Gun", "", " (@item_NameGun_01)"),
            Err(Rule::RawLocKey)
        );
        assert_eq!(
            check_generated("Gun", "Gun", "[PLACEHOLDER] ", ""),
            Err(Rule::PlaceholderText)
        );
        assert_eq!(
            check_replaced("Gun", "@vehicle_NameX_Y"),
            Err(Rule::RawLocKey)
        );
        assert_eq!(check_added("Pressurized Ice"), Ok(()));
        assert_eq!(check_added("@item_Name_x_1"), Err(Rule::RawLocKey));
        assert_eq!(check_added("<= PLACEHOLDER =>"), Err(Rule::PlaceholderText));
        assert_eq!(check_added("Box ~mission(Location)"), Err(Rule::Variables));
        assert_eq!(check_added("%s crate"), Err(Rule::FormatSpecifiers));
        assert_eq!(check_added("<EM4>Box"), Err(Rule::EmSignature));
    }

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
    fn every_colour_tag_we_emit_is_balanced() {
        use crate::enhance::COLOUR_TEST_LINE;
        use crate::enhance::profile::{EM_MAX, em};
        // Each EM number a colour setting may name, around a marker, a line and a label.
        for n in 0..=EM_MAX {
            for added in [
                format!("\\n- Filler rifle {}", em(n, "[Owned]")),
                format!("\\n- {}", em(n, "[E-S2] Filler rifle [Owned]")),
                format!(" {}", em(n, "[BP Owned]")),
                format!("\\n{} 150", em(n, "Reputation:")),
            ] {
                assert!(tokens(&added).em_balanced(), "{added}");
                assert_eq!(check_generated("prose", "prose", "", &added), Ok(()));
                // CIG's open tail keeps its signature; our closed tags nest inside it.
                assert_eq!(check_generated("a <EM4>b", "a <EM4>b", "", &added), Ok(()));
            }
        }
        // The colour test line: five closed tags, no nesting, fine on the prompt it targets.
        let t = tokens(COLOUR_TEST_LINE);
        assert!(t.em_balanced());
        for n in 1..=EM_MAX {
            assert!(COLOUR_TEST_LINE.contains(&format!("<EM{n}>EM{n}</EM{n}>")));
        }
        assert_eq!(
            check_generated(
                "Please select a contract.",
                "Please select a contract.",
                "",
                COLOUR_TEST_LINE
            ),
            Ok(())
        );
        // What we never emit is still refused: an open tag, a nested pair, a stray close.
        for bad in [
            " <EM5>[Owned]",
            " <EM2>[E-S2] <EM4>[Owned]</EM4></EM2>",
            " [Owned]</EM1>",
        ] {
            assert_eq!(
                check_generated("prose", "prose", "", bad),
                Err(Rule::AddedEmUnbalanced),
                "{bad}"
            );
        }
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

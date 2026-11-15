//! The game's string table (port of `sc-gamedata/src/localization.ts`).
//!
//! Records store display text as keys: a manufacturer's name is `@manufacturer_NameSASU`,
//! which resolves to "Sakura Sun". The table is `Data/Localization/english/global.ini`:
//! UTF-8 with a BOM, CRLF line endings, `key=value` lines whose value may contain `=`.
//!
//! [`Localization`] behaves like the JavaScript `Map` it replaces: insertion order is kept,
//! a repeated key keeps its first position and takes the last value.

use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct Localization {
    keys: Vec<String>,
    values: HashMap<String, String>,
    /// Lower-case key -> first key (in insertion order) with that lower-case form, for the
    /// case-insensitive fallback, which scans the map in order in the TypeScript.
    lower: HashMap<String, String>,
}

impl Localization {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    pub fn set(&mut self, key: &str, value: &str) {
        if let Some(v) = self.values.get_mut(key) {
            *v = value.to_string();
            return;
        }
        self.keys.push(key.to_string());
        self.values.insert(key.to_string(), value.to_string());
        self.lower
            .entry(key.to_lowercase())
            .or_insert_with(|| key.to_string());
    }

    /// Entries in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.keys
            .iter()
            .map(|k| (k.as_str(), self.values[k].as_str()))
    }

    /// The value of the first key equal to `key` ignoring case.
    fn get_ignore_case(&self, key: &str) -> Option<&str> {
        let first = self.lower.get(&key.to_lowercase())?;
        self.get(first)
    }
}

/// JavaScript `String.prototype.trim` whitespace: Unicode White_Space plus the BOM.
fn js_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

/// Parse a `global.ini` into a table.
pub fn parse_global_ini(text: &str) -> Localization {
    let mut table = Localization::new();
    // Strip the BOM so the very first key does not carry an invisible prefix.
    let body = text.strip_prefix('\u{feff}').unwrap_or(text);
    // `split(/\r?\n/)`: a lone `\r` stays in the line (and is trimmed away).
    for raw in body.split('\n') {
        let line = js_trim(raw.strip_suffix('\r').unwrap_or(raw));
        if line.is_empty()
            || line.starts_with(';')
            || line.starts_with('#')
            || line.starts_with('[')
        {
            continue;
        }
        let Some(split) = line.find('=') else {
            continue;
        };
        if split == 0 {
            continue;
        }
        // Values can contain '=', so only the first one separates key from value.
        table.set(&line[..split], &line[split + 1..]);
    }
    table
}

/// Parse `global.ini` bytes; invalid UTF-8 is replaced, as Node's `utf-8` decoding does.
pub fn parse_global_ini_bytes(bytes: &[u8]) -> Localization {
    parse_global_ini(&String::from_utf8_lossy(bytes))
}

/// Layer `overlay` over `base`; later wins. The base is not changed.
pub fn apply_overlay(base: &Localization, overlay: &Localization) -> Localization {
    let mut merged = base.clone();
    for (k, v) in overlay.iter() {
        merged.set(k, v);
    }
    merged
}

/// One string an overlay changes or adds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringChange {
    pub key: String,
    pub base: Option<String>,
    pub overlay: String,
}

/// What an overlay changes relative to a base table: `(changed, added)`.
pub fn diff_strings(
    base: &Localization,
    overlay: &Localization,
) -> (Vec<StringChange>, Vec<StringChange>) {
    let mut changed = Vec::new();
    let mut added = Vec::new();
    for (key, value) in overlay.iter() {
        match base.get(key) {
            None => added.push(StringChange {
                key: key.to_string(),
                base: None,
                overlay: value.to_string(),
            }),
            Some(original) if original != value => changed.push(StringChange {
                key: key.to_string(),
                base: Some(original.to_string()),
                overlay: value.to_string(),
            }),
            Some(_) => {}
        }
    }
    (changed, added)
}

/// Search by key or value, case-insensitively: `(total, first `limit` hits)`.
pub fn search_strings<'a>(
    table: &'a Localization,
    query: &str,
    limit: usize,
    keys_only: bool,
) -> (usize, Vec<(&'a str, &'a str)>) {
    let needle = query.to_lowercase();
    let mut hits = Vec::new();
    let mut total = 0;
    for (k, v) in table.iter() {
        let matched = k.to_lowercase().contains(&needle)
            || (!keys_only && v.to_lowercase().contains(&needle));
        if !matched {
            continue;
        }
        total += 1;
        if hits.len() < limit {
            hits.push((k, v));
        }
    }
    (total, hits)
}

/// Exact lookup, accepting a key with or without its leading `@`.
pub fn lookup_string<'a>(table: &'a Localization, key: &str) -> Option<&'a str> {
    let bare = key.strip_prefix('@').unwrap_or(key);
    table
        .get(bare)
        .or_else(|| table.get(key))
        .or_else(|| table.get_ignore_case(bare))
}

/// Resolve a possibly-localized string value.
///
/// `None` for a non-string, an empty string, `@LOC_EMPTY`, or a key resolving to an empty
/// value; plain text passes through; an unresolved `@key` comes back as the token itself
/// so the gap stays visible (see [`is_unresolved`]).
pub fn localize<'a>(
    table: &'a Localization,
    value: Option<&'a serde_json::Value>,
) -> Option<&'a str> {
    localize_str(table, value?.as_str()?)
}

/// [`localize`] for a string.
pub fn localize_str<'a>(table: &'a Localization, value: &'a str) -> Option<&'a str> {
    if value.is_empty() || value == "@LOC_EMPTY" {
        return None;
    }
    let Some(key) = value.strip_prefix('@') else {
        return Some(value);
    };
    let hit = table
        .get(key)
        .or_else(|| table.get(value))
        .or_else(|| table.get_ignore_case(key));
    match hit {
        Some(text) => (!text.is_empty()).then_some(text),
        None => Some(value),
    }
}

/// True when [`localize`] returned a token it could not resolve.
pub fn is_unresolved(value: Option<&str>) -> bool {
    value.is_some_and(|v| v.starts_with('@'))
}

/// True when a resolved string is not actually a name: unset, an unresolved key, or one of
/// CIG's placeholder markers (`<= PLACEHOLDER =>`, `< = TEST NAME = >`).
pub fn is_placeholder_name(name: Option<&str>) -> bool {
    let Some(name) = name.filter(|n| !n.is_empty()) else {
        return true;
    };
    if name.starts_with('@') {
        return true;
    }
    let lower = name.to_lowercase();
    if lower.contains("placeholder") {
        return true;
    }
    let squashed: String = lower.chars().filter(|c| !c.is_whitespace()).collect();
    squashed.contains("<=") || squashed.contains("=>") || squashed.contains("testname")
}

/// The redaction `sc-gamedata`'s `scripts/publish-dump.mjs` applies to the string table of
/// a published dump: name-shaped keys only, no prose. Off unless asked for
/// ([`crate::pipeline::Options::redact_strings`]); domain parsing never uses it.
pub mod redaction {
    use super::Localization;

    /// Value of `_redacted` in a published string table.
    pub const NOTE: &str =
        "Localisation limited to name keys; descriptive and dialogue text is not published.";
    /// A display name is never a paragraph.
    pub const MAX_VALUE_LENGTH: usize = 120;

    /// `KEEP_KEY = /(^|_)(Names?|Titles?|DisplayName|ShortName|Nickname)([A-Z0-9_]|$)/`.
    fn keep_key(key: &str) -> bool {
        const WORDS: [&str; 7] = [
            "Names",
            "Name",
            "Titles",
            "Title",
            "DisplayName",
            "ShortName",
            "Nickname",
        ];
        let b = key.as_bytes();
        for start in 0..=b.len() {
            if !(start == 0 || b[start - 1] == b'_') {
                continue;
            }
            for w in WORDS {
                if key[start..].starts_with(w) {
                    match b.get(start + w.len()) {
                        None => return true,
                        Some(c) if c.is_ascii_uppercase() || c.is_ascii_digit() || *c == b'_' => {
                            return true;
                        }
                        _ => {}
                    }
                }
            }
        }
        false
    }

    /// `DROP_KEY = /desc|tooltip|hint|subtitle|^Dlg_|^PU_|^DX[SM]/i`.
    fn drop_key(key: &str) -> bool {
        let l = key.to_ascii_lowercase();
        ["desc", "tooltip", "hint", "subtitle"]
            .iter()
            .any(|w| l.contains(w))
            || l.starts_with("dlg_")
            || l.starts_with("pu_")
            || l.starts_with("dxs")
            || l.starts_with("dxm")
    }

    /// Whether a published dump keeps this entry. `MAX_VALUE_LENGTH` counts UTF-16 units.
    pub fn keeps(key: &str, value: &str) -> bool {
        !value.is_empty()
            && value.encode_utf16().count() <= MAX_VALUE_LENGTH
            && !drop_key(key)
            && keep_key(key)
    }

    /// The table cut to what a published dump keeps.
    pub fn redact(table: &Localization) -> Localization {
        let mut out = Localization::new();
        for (k, v) in table.iter().filter(|(k, v)| keeps(k, v)) {
            out.set(k, v);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Cases from sc-gamedata src/localization.test.ts.
    #[test]
    fn parse_global_ini_cases() {
        let t = parse_global_ini("manufacturer_NameAEGS=Aegis Dynamics\nfoo=bar");
        assert_eq!(t.get("manufacturer_NameAEGS"), Some("Aegis Dynamics"));
        let t = parse_global_ini("\u{feff}first_key=value");
        assert_eq!(t.get("first_key"), Some("value"));
        let t = parse_global_ini("a=1\r\nb=2\r\n");
        assert_eq!((t.get("a"), t.get("b")), (Some("1"), Some("2")));
        let t = parse_global_ini("equation=x=y+1");
        assert_eq!(t.get("equation"), Some("x=y+1"));
        let t = parse_global_ini("; comment\n# other\n[Section]\n\nreal=yes");
        assert_eq!(t.len(), 1);
        assert_eq!(t.get("real"), Some("yes"));
        let t = parse_global_ini("=novalue\n  spaced = x  ");
        assert_eq!(t.len(), 1);
        assert_eq!(t.get("spaced "), Some(" x"));
    }

    #[test]
    fn repeated_key_keeps_position_takes_last_value() {
        let t = parse_global_ini("a=1\nb=2\na=3");
        let entries: Vec<_> = t.iter().collect();
        assert_eq!(entries, vec![("a", "3"), ("b", "2")]);
    }

    #[test]
    fn localize_cases() {
        let t = parse_global_ini("manufacturer_NameAEGS=Aegis Dynamics\nempty_value=");
        let l = |v: serde_json::Value| localize(&t, Some(&v)).map(str::to_string);
        assert_eq!(
            l(json!("@manufacturer_NameAEGS")).as_deref(),
            Some("Aegis Dynamics")
        );
        assert_eq!(l(json!("Just A Name")).as_deref(), Some("Just A Name"));
        assert_eq!(l(json!("@LOC_EMPTY")), None);
        assert_eq!(l(json!("@empty_value")), None);
        assert_eq!(
            l(json!("@MANUFACTURER_NAMEAEGS")).as_deref(),
            Some("Aegis Dynamics")
        );
        let r = l(json!("@manufacturer_NameNOPE"));
        assert_eq!(r.as_deref(), Some("@manufacturer_NameNOPE"));
        assert!(is_unresolved(r.as_deref()));
        assert_eq!(localize(&t, None), None);
        assert_eq!(l(json!(42)), None);
        assert_eq!(l(json!("")), None);
    }

    #[test]
    fn search_cases() {
        let t = parse_global_ini(
            "manufacturer_NameAEGS=Aegis Dynamics\nmanufacturer_NameDRAK=Drake Interplanetary\nStanton1=Hurston\nStanton1c=Magda\nATC_WikeloMods=Wikelo Emporium",
        );
        assert_eq!(
            search_strings(&t, "Wikelo", 25, false).1,
            vec![("ATC_WikeloMods", "Wikelo Emporium")]
        );
        assert_eq!(search_strings(&t, "manufacturer_Name", 25, false).0, 2);
        assert_eq!(search_strings(&t, "Hurston", 25, true).0, 0);
        assert_eq!(search_strings(&t, "Hurston", 25, false).0, 1);
        let (total, hits) = search_strings(&t, "manufacturer", 1, false);
        assert_eq!((total, hits.len()), (2, 1));
        assert_eq!(search_strings(&t, "aegis", 25, false).0, 1);
    }

    #[test]
    fn lookup_cases() {
        let t = parse_global_ini("Stanton1c=Magda\nmanufacturer_NameAEGS=Aegis Dynamics");
        assert_eq!(
            lookup_string(&t, "@manufacturer_NameAEGS"),
            Some("Aegis Dynamics")
        );
        assert_eq!(
            lookup_string(&t, "manufacturer_NameAEGS"),
            Some("Aegis Dynamics")
        );
        assert_eq!(lookup_string(&t, "Stanton1c"), Some("Magda"));
        assert_eq!(lookup_string(&t, "Nope"), None);
    }

    #[test]
    fn placeholder_names() {
        for p in [
            None,
            Some(""),
            Some("@item_NameFoo"),
            Some("<= PLACEHOLDER =>"),
            Some("< = TEST NAME = >"),
        ] {
            assert!(is_placeholder_name(p), "{p:?}");
        }
        assert!(!is_placeholder_name(Some("Aegis Dynamics")));
    }

    #[test]
    fn overlay_and_diff() {
        let base = parse_global_ini("a=1\nb=2");
        let over = parse_global_ini("b=3\nc=4");
        let merged = apply_overlay(&base, &over);
        assert_eq!(
            merged.iter().collect::<Vec<_>>(),
            vec![("a", "1"), ("b", "3"), ("c", "4")]
        );
        assert_eq!(base.get("b"), Some("2"));
        let (changed, added) = diff_strings(&base, &over);
        assert_eq!(changed[0].key, "b");
        assert_eq!(added[0].key, "c");
    }

    #[test]
    fn redaction_matches_publish_dump() {
        use redaction::keeps;
        assert!(keeps("vehicle_NameAEGS_Gladius", "Gladius"));
        assert!(keeps("item_Name", "x"));
        assert!(keeps("Title_1", "x"));
        assert!(!keeps("vehicle_DescAEGS", "prose"), "no prose keys");
        assert!(!keeps("PU_x_NameY", "x"), "dialogue namespace");
        assert!(!keeps("MyNamesSpecial", "x"), "segment boundary");
        assert!(!keeps("item_Namex", "x"), "lower-case after the word");
        assert!(!keeps("item_Name", ""), "empty value");
        assert!(!keeps("item_Name", &"x".repeat(121)), "long value");
    }
}

//! Fleet import and ship ordering (addendum §6).
//!
//! - [`catalog`]: the hulls of a build (`vehicle_Name*` keys a ship record names itself by).
//! - [`import`]: HangarXPLOR JSON / CSV and plain-text fleet lists, matched to hulls.
//! - [`store`]: `fleet.json` (what the user owns), `ship-names.toml` (order, groups, renames,
//!   prefix style) and the channel scope in `profile.toml`.
//! - [`names`]: the `ship-names` generator: sort prefixes and renames on the name keys.
//!
//! Everything here is a local file read; nothing goes to the network (§6.8).

pub mod catalog;
pub mod import;
pub mod names;
pub mod store;

pub use catalog::{Catalog, Hull};

/// Manufacturers: the game's code (record short name, lower case) and the names and codes
/// stores and exports use for it. The first name is the full one.
pub const MAKERS: &[(&str, &[&str])] = &[
    ("aegs", &["Aegis Dynamics", "Aegis", "AEGS"]),
    ("anvl", &["Anvil Aerospace", "Anvil", "ANVL"]),
    ("argo", &["ARGO Astronautics", "Argo", "ARGO"]),
    ("banu", &["Banu Souli", "Banu", "BANU"]),
    ("cnou", &["Consolidated Outland", "C.O.", "CNOU"]),
    ("crus", &["Crusader Industries", "Crusader", "CRUS", "CRSD"]),
    ("drak", &["Drake Interplanetary", "Drake", "DRAK"]),
    ("espr", &["Esperia", "ESPR"]),
    ("gama", &["Gatac Manufacture", "Gatac", "GAMA"]),
    ("grey", &["Grey's Market", "GREY", "GLSN"]),
    ("grin", &["Greycat Industrial", "Greycat", "GRIN"]),
    ("krig", &["Kruger Intergalactic", "Kruger", "KRIG"]),
    (
        "misc",
        &["Musashi Industrial & Starflight Concern", "Musashi", "MISC"],
    ),
    ("mrai", &["Mirai", "MRAI"]),
    ("orig", &["Origin Jumpworks", "Origin", "ORIG"]),
    ("rsi", &["Roberts Space Industries", "RSI"]),
    ("tmbl", &["Tumbril Land Systems", "Tumbril", "TMBL"]),
    ("vncl", &["Vanduul", "VNCL"]),
    ("xnaa", &["Aopoa", "Xi'an", "XNAA", "XIAN"]),
];

/// Normalise a ship name for matching (§6.3): lower case, typographic quotes to ASCII, every
/// run of non-alphanumerics to one space, trimmed.
pub fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut gap = false;
    for c in s.chars() {
        let c = match c {
            '\u{2018}' | '\u{2019}' | '\u{201B}' | '\u{2032}' => '\'',
            '\u{201C}' | '\u{201D}' | '\u{201F}' | '\u{2033}' => '"',
            c => c,
        };
        if c.is_alphanumeric() {
            if gap && !out.is_empty() {
                out.push(' ');
            }
            gap = false;
            out.extend(c.to_lowercase());
        } else {
            gap = true;
        }
    }
    out
}

/// Letters and digits only, lower case (`AEGS_Avenger_Titan` -> `aegsavengertitan`).
pub fn squash(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The game's maker code for a code or name from an export (`CRSD` -> `crus`,
/// `Roberts Space Industries` -> `rsi`). A `_2025`-style year suffix is dropped.
pub fn maker_code(s: &str) -> Option<&'static str> {
    let base = strip_year(s.trim());
    let n = normalize(base);
    if n.is_empty() {
        return None;
    }
    MAKERS
        .iter()
        .find(|(code, names)| *code == n || names.iter().any(|m| normalize(m) == n))
        .map(|(code, _)| *code)
}

/// `rsi_2025` -> `rsi`.
pub fn strip_year(code: &str) -> &str {
    match code.rsplit_once('_') {
        Some((head, y)) if y.len() == 4 && y.bytes().all(|b| b.is_ascii_digit()) => head,
        _ => code,
    }
}

/// Every maker name and code, normalised, longest first (for stripping a leading maker).
fn maker_prefixes() -> Vec<String> {
    let mut v: Vec<String> = MAKERS
        .iter()
        .flat_map(|(_, names)| names.iter().map(|n| normalize(n)))
        .collect();
    v.sort_by_key(|s| std::cmp::Reverse(s.len()));
    v.dedup();
    v
}

/// A normalised name with a leading maker name or code removed, when it has one and
/// something is left.
pub fn strip_maker(normalized: &str) -> Option<String> {
    maker_prefixes().iter().find_map(|m| {
        normalized
            .strip_prefix(m.as_str())
            .and_then(|r| r.strip_prefix(' '))
            .filter(|r| !r.is_empty())
            .map(str::to_string)
    })
}

fn is_mk_number(w: &str) -> bool {
    !w.is_empty()
        && (w.bytes().all(|b| b.is_ascii_digit())
            || w.bytes().all(|b| matches!(b, b'i' | b'v' | b'x')))
}

/// A normalised name without `Mk <roman or digits>` (also `mk2`, `mkii`).
pub fn strip_mk(normalized: &str) -> String {
    let words: Vec<&str> = normalized.split(' ').collect();
    let mut out = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        let w = words[i];
        if w == "mk" && words.get(i + 1).is_some_and(|n| is_mk_number(n)) {
            i += 2;
            continue;
        }
        if let Some(n) = w.strip_prefix("mk")
            && is_mk_number(n)
        {
            i += 1;
            continue;
        }
        out.push(w);
        i += 1;
    }
    out.join(" ")
}

/// True when the normalised name names a Mk (`mk ii`, `mk2`).
pub fn has_mk(normalized: &str) -> bool {
    strip_mk(normalized) != normalized
}

/// The words of a normalised name, sorted, without `edition`.
pub fn word_set(normalized: &str) -> Vec<String> {
    let mut w: Vec<String> = normalized
        .split(' ')
        .filter(|w| !w.is_empty() && *w != "edition")
        .map(str::to_string)
        .collect();
    w.sort();
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalising() {
        assert_eq!(normalize("  Aegis  Avenger-Titan "), "aegis avenger titan");
        assert_eq!(normalize("Xi\u{2019}an Nox"), "xi an nox");
        assert_eq!(
            normalize("F7C-M Super Hornet Mk II"),
            "f7c m super hornet mk ii"
        );
        assert_eq!(squash("AEGS_Avenger_Titan"), "aegsavengertitan");
    }

    #[test]
    fn makers() {
        assert_eq!(maker_code("CRSD"), Some("crus"));
        assert_eq!(maker_code("XIAN"), Some("xnaa"));
        assert_eq!(maker_code("Roberts Space Industries"), Some("rsi"));
        assert_eq!(maker_code("rsi_2025"), Some("rsi"));
        assert_eq!(maker_code("Nobody"), None);
        assert_eq!(
            strip_maker("aegis dynamics gladius").as_deref(),
            Some("gladius")
        );
        assert_eq!(strip_maker("aegis gladius").as_deref(), Some("gladius"));
        assert_eq!(
            strip_maker("c o mustang alpha").as_deref(),
            Some("mustang alpha")
        );
        assert_eq!(strip_maker("aegis"), None);
        assert_eq!(strip_maker("gladius"), None);
    }

    #[test]
    fn mk_and_words() {
        assert_eq!(
            strip_mk("constellation mk iv andromeda"),
            "constellation andromeda"
        );
        assert_eq!(strip_mk("hornet mk2"), "hornet");
        assert_eq!(strip_mk("mk"), "mk");
        assert!(has_mk("f7a hornet mk i") && !has_mk("f7a hornet"));
        assert_eq!(
            word_set("valkyrie liberator edition"),
            vec!["liberator", "valkyrie"]
        );
    }
}

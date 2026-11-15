//! Owned blueprints for `[Owned]` markers (spec 6.5, addendum §3).
//!
//! The owned set is per character and lives in M1's blueprint store. The mission generator
//! only asks this trait, so the store (or a test list) plugs in without the generator
//! knowing where the set came from.

use std::collections::BTreeSet;

/// One blueprint as the generator sees it.
#[derive(Debug, Clone, Copy)]
pub struct BlueprintRef<'a> {
    /// Blueprint record GUID.
    pub id: &'a str,
    /// Blueprint record short name (`BP_CRAFT_...`).
    pub record: &'a str,
    /// English display name of the produced item.
    pub name: &'a str,
}

/// The active character's owned blueprints.
pub trait OwnedBlueprints {
    fn owns(&self, bp: &BlueprintRef<'_>) -> bool;
}

/// Nobody owns anything: no markers.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoneOwned;

impl OwnedBlueprints for NoneOwned {
    fn owns(&self, _: &BlueprintRef<'_>) -> bool {
        false
    }
}

/// Fold a display name for matching: strip leading `[...]` tag groups, unify quotes and
/// apostrophes, collapse whitespace, lower case (addendum §4).
pub fn normalize_name(name: &str) -> String {
    let mut s = name.trim();
    while let Some(rest) = s.strip_prefix('[') {
        match rest.find(']') {
            Some(end) => s = rest[end + 1..].trim_start(),
            None => break,
        }
    }
    let mapped: String = s
        .chars()
        .map(|c| match c {
            '\u{2018}' | '\u{2019}' | '\u{201B}' | '`' | '\u{00B4}' => '\'',
            '\u{201C}' | '\u{201D}' => '"',
            c => c,
        })
        .collect();
    mapped
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// An owned set given as blueprint ids/record names and/or display names (normalised).
#[derive(Debug, Clone, Default)]
pub struct OwnedList {
    pub ids: BTreeSet<String>,
    pub names: BTreeSet<String>,
}

impl OwnedList {
    pub fn from_names<I: IntoIterator<Item = S>, S: AsRef<str>>(names: I) -> Self {
        Self {
            ids: BTreeSet::new(),
            names: names
                .into_iter()
                .map(|n| normalize_name(n.as_ref()))
                .filter(|n| !n.is_empty())
                .collect(),
        }
    }

    /// Read an owned-set JSON: M1's per-character file
    /// (`{"entries": {"<norm>": {"name": ..., "ids": [...]}}}`), or a plain list of names.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let v: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let mut out = Self::default();
        match &v {
            serde_json::Value::Array(a) => {
                for n in a {
                    let n = n.as_str().ok_or("list entries must be strings")?;
                    out.names.insert(normalize_name(n));
                }
            }
            serde_json::Value::Object(o) => {
                let entries = o
                    .get("entries")
                    .and_then(|e| e.as_object())
                    .ok_or("expected an \"entries\" object")?;
                for (k, e) in entries {
                    let name = e.get("name").and_then(|n| n.as_str()).unwrap_or(k);
                    out.names.insert(normalize_name(name));
                    for id in e
                        .get("ids")
                        .and_then(|i| i.as_array())
                        .into_iter()
                        .flatten()
                    {
                        if let Some(id) = id.as_str() {
                            out.ids.insert(id.to_ascii_lowercase());
                        }
                    }
                }
            }
            _ => return Err("expected a list or an object".into()),
        }
        out.names.remove("");
        Ok(out)
    }

    pub fn len(&self) -> usize {
        self.names.len().max(self.ids.len())
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty() && self.ids.is_empty()
    }
}

impl OwnedBlueprints for OwnedList {
    fn owns(&self, bp: &BlueprintRef<'_>) -> bool {
        (!bp.id.is_empty() && self.ids.contains(&bp.id.to_ascii_lowercase()))
            || self.ids.contains(&bp.record.to_ascii_lowercase())
            || self.names.contains(&normalize_name(bp.name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalising() {
        assert_eq!(
            normalize_name("[CIV-S3-A] 7CA  \u{2018}Nargun\u{2019}"),
            "7ca 'nargun'"
        );
        assert_eq!(normalize_name("[H] [x] ADP Core"), "adp core");
        assert_eq!(normalize_name("  Plain  Name "), "plain name");
    }

    #[test]
    fn list_and_json() {
        let bp = BlueprintRef {
            id: "ABC",
            record: "BP_CRAFT_Fake_Rifle",
            name: "Fake Rifle 'Ember'",
        };
        assert!(OwnedList::from_names(["[P-S3] fake rifle \u{2019}Ember\u{2019}"]).owns(&bp));
        assert!(!NoneOwned.owns(&bp));
        let j = OwnedList::from_json(
            r#"{"schema":1,"geid":"1","entries":{"x":{"name":"Other","ids":["abc"]}}}"#,
        )
        .unwrap();
        assert!(j.owns(&bp));
        let j = OwnedList::from_json(r#"["Nope"]"#).unwrap();
        assert!(!j.owns(&bp));
        assert!(OwnedList::from_json("3").is_err());
    }
}

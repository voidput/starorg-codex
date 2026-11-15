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
    /// No other blueprint of this build produces an item with the same name. A name-only
    /// match (log scan, import) marks a blueprint only when this holds: two candidates are
    /// reported, never guessed (addendum §4).
    pub name_unique: bool,
}

/// What a blueprint's marker can say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owned {
    Yes,
    No,
    /// Cannot tell: only the display name matched and another blueprint shares it, or one
    /// bullet stands for several blueprints that are not all owned. Neither marker shows.
    Unsure,
}

/// The active character's owned blueprints.
pub trait OwnedBlueprints {
    fn owns(&self, bp: &BlueprintRef<'_>) -> bool;
    /// [`Owned::Unsure`] when the name is in the set but shared with another blueprint
    /// (a name-only match is never guessed, addendum §4).
    fn state(&self, bp: &BlueprintRef<'_>) -> Owned {
        if self.owns(bp) { Owned::Yes } else { Owned::No }
    }
    /// An owned set is there at all. `[Missing]` markers need one: with nothing known,
    /// every blueprint would read as missing.
    fn known(&self) -> bool;
}

/// Nobody owns anything: no markers.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoneOwned;

impl OwnedBlueprints for NoneOwned {
    fn owns(&self, _: &BlueprintRef<'_>) -> bool {
        false
    }
    fn known(&self) -> bool {
        false
    }
}

/// Fold a display name for matching: strip leading `[...]` tag groups, unify quotes and
/// apostrophes, collapse whitespace, lower case (addendum §4). The same key the blueprint
/// store uses.
pub fn normalize_name(name: &str) -> String {
    crate::blueprints::normalize(name)
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
            || (bp.name_unique && self.names.contains(&normalize_name(bp.name)))
    }
    fn known(&self) -> bool {
        !self.is_empty()
    }
    fn state(&self, bp: &BlueprintRef<'_>) -> Owned {
        if self.owns(bp) {
            Owned::Yes
        } else if !bp.name_unique && self.names.contains(&normalize_name(bp.name)) {
            Owned::Unsure
        } else {
            Owned::No
        }
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
            name_unique: true,
        };
        assert!(OwnedList::from_names(["[P-S3] fake rifle \u{2019}Ember\u{2019}"]).owns(&bp));
        assert!(!NoneOwned.owns(&bp) && !NoneOwned.known());
        assert!(OwnedList::from_names(["x"]).known() && !OwnedList::default().known());
        let j = OwnedList::from_json(
            r#"{"schema":1,"geid":"1","entries":{"x":{"name":"Other","ids":["abc"]}}}"#,
        )
        .unwrap();
        assert!(j.owns(&bp));
        let j = OwnedList::from_json(r#"["Nope"]"#).unwrap();
        assert!(!j.owns(&bp));
        assert!(OwnedList::from_json("3").is_err());
        // A name two blueprints share is not marked from the name alone.
        let shared = BlueprintRef {
            name_unique: false,
            ..bp
        };
        assert!(!OwnedList::from_names(["Fake Rifle 'Ember'"]).owns(&shared));
        let by_id =
            OwnedList::from_json(r#"{"entries":{"x":{"name":"Other","ids":["abc"]}}}"#).unwrap();
        assert!(by_id.owns(&shared), "an id match still marks it");
        // Listed by a shared name only: unsure, not missing.
        let by_name = OwnedList::from_names(["Fake Rifle 'Ember'"]);
        assert_eq!(by_name.state(&shared), Owned::Unsure);
        assert_eq!(by_name.state(&bp), Owned::Yes);
        assert_eq!(OwnedList::from_names(["Other"]).state(&shared), Owned::No);
    }
}

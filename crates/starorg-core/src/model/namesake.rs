//! Which record owns a shared loc key: the namesake rule (spec 5.1).
//!
//! Several records often reference the same name, short-name or description key (paint
//! variants, AI copies, and CIG copy-paste such as the Bengal S6 gatling reusing the APAR
//! S4 keys). For each key:
//!
//! 1. Collect all main records that reference it (after data patches), after the filters.
//! 2. If one of them is named after the key, it owns the key.
//! 3. Otherwise, if exactly one record references the key, it owns the key.
//! 4. Otherwise, if all referencing records would produce the same text, that text is used.
//! 5. Otherwise the key is left as stock and a diagnostic lists the contenders.
//!
//! Filtered out before the rule: templates and display props (placeholder name, or short
//! name ending `_prop`). The player-hull filter of spec 5.2 is the caller's predicate.

use super::GameData;
use std::collections::BTreeMap;

/// Which loc reference of a record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyField {
    Name,
    ShortName,
    Desc,
}

/// The fold used to match keys, the same as for stock keys (spec 4.3).
pub use crate::ini::fold_key;

const PREFIXES: [&str; 5] = [
    "item_shortname",
    "item_name",
    "item_desc",
    "vehicle_name",
    "vehicle_desc",
];

fn squash(s: &str) -> String {
    s.chars()
        .filter(|c| *c != '_')
        .flat_map(char::to_lowercase)
        .collect()
}

/// True when `short_name` equals `key` with its `item_Name`/`item_Desc`/`item_ShortName`/
/// `vehicle_Name`/`vehicle_Desc` prefix removed, ignoring case and underscores.
pub fn named_after(key: &str, short_name: &str) -> bool {
    stem(key).is_some_and(|s| s == squash(short_name))
}

/// The record name a key is named after, lower case without underscores: `item_NameAEGS_X`
/// -> `aegsx`. `None` when the key has no namesake prefix or nothing after it.
pub fn stem(key: &str) -> Option<String> {
    let k = key.strip_prefix('@').unwrap_or(key);
    let lower = k.to_ascii_lowercase();
    let rest = PREFIXES
        .iter()
        .find_map(|p| lower.strip_prefix(p).map(|_| &k[p.len()..]))?;
    let s = squash(rest);
    (!s.is_empty()).then_some(s)
}

/// `short_name` folded the way [`stem`] folds keys.
pub fn squash_name(short_name: &str) -> String {
    squash(short_name)
}

/// Outcome for one key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ownership {
    /// Step 2: this record is named after the key.
    Namesake(String),
    /// Step 3: the only referencing record.
    Only(String),
    /// Step 4: all contenders produce the same text; any of them may write it.
    SameText(Vec<String>),
    /// Step 5: left as stock; the contenders, for the diagnostic.
    Contested(Vec<String>),
}

impl Ownership {
    /// The record whose text is written, if any.
    pub fn writer(&self) -> Option<&str> {
        match self {
            Self::Namesake(r) | Self::Only(r) => Some(r),
            Self::SameText(v) => v.first().map(String::as_str),
            Self::Contested(_) => None,
        }
    }
}

/// Apply steps 2 to 5 to the (already filtered) contenders of `key`. `text` gives the text
/// each record would produce; `None` counts as different from everything.
pub fn resolve<T: PartialEq>(
    key: &str,
    contenders: &[String],
    mut text: impl FnMut(&str) -> Option<T>,
) -> Option<Ownership> {
    match contenders {
        [] => None,
        [only] => Some(if named_after(key, only) {
            Ownership::Namesake(only.clone())
        } else {
            Ownership::Only(only.clone())
        }),
        many => {
            if let Some(n) = many.iter().find(|r| named_after(key, r)) {
                return Some(Ownership::Namesake(n.clone()));
            }
            let first = text(&many[0]);
            let same = first.is_some() && many[1..].iter().all(|r| text(r) == first);
            Some(if same {
                Ownership::SameText(many.to_vec())
            } else {
                Ownership::Contested(many.to_vec())
            })
        }
    }
}

/// Folded key -> short names of the item records (components, weapons, missiles) that
/// reference it in `field`, with templates and props filtered out and `keep` applied.
/// Records appear in record-name order; each record once per key.
pub fn key_users(
    gd: &GameData,
    field: KeyField,
    mut keep: impl FnMut(&super::ItemIdentity) -> bool,
) -> BTreeMap<String, Vec<String>> {
    let items = gd
        .components
        .iter()
        .map(|c| &c.item)
        .chain(gd.weapons.iter().map(|w| &w.item))
        .chain(gd.missiles.iter().map(|m| &m.item));
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for item in items {
        let short = item.short_name();
        if item.is_template || short.to_ascii_lowercase().ends_with("_prop") || !keep(item) {
            continue;
        }
        let key = match field {
            KeyField::Name => &item.loc.name,
            KeyField::ShortName => &item.loc.short_name,
            KeyField::Desc => &item.loc.desc,
        };
        let Some(key) = key.as_deref().and_then(super::loc_key) else {
            continue;
        };
        let list = out.entry(fold_key(key)).or_default();
        if !list.iter().any(|r| r == short) {
            list.push(short.to_string());
        }
    }
    for list in out.values_mut() {
        list.sort();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Component, ItemIdentity, LocKeys, Weapon};

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn named_after_rules() {
        assert!(named_after(
            "@item_NameAPAR_BallisticGatling_S4",
            "APAR_BallisticGatling_S4"
        ));
        assert!(named_after(
            "item_descapar_ballisticgatling_s4",
            "APAR_BallisticGatlingS4"
        ));
        assert!(named_after("vehicle_DescAEGS_Gladius", "AEGS_Gladius"));
        assert!(named_after("item_ShortNameFoo_S1", "Foo_S1"));
        assert!(!named_after(
            "item_NameAPAR_BallisticGatling_S4",
            "Bengal_BallisticGatling_S6"
        ));
        assert!(!named_after("item_Name", ""));
        assert!(!named_after("mission_title_x", "x"));
    }

    fn weapon(short: &str, name: &str, desc: &str) -> Weapon {
        Weapon {
            item: ItemIdentity {
                record_name: format!("EntityClassDefinition.{short}"),
                loc: LocKeys {
                    name: Some(name.into()),
                    short_name: None,
                    desc: Some(desc.into()),
                },
                ..ItemIdentity::default()
            },
            ..Weapon::default()
        }
    }

    /// Spec 4.5/13: the Bengal S6 gatling reuses the APAR S4 name and description keys;
    /// the namesake rule gives both keys to the APAR record with no patch involved.
    #[test]
    fn bengal_and_apar_gatlings() {
        let gd = GameData {
            weapons: vec![
                weapon(
                    "APAR_BallisticGatling_S4",
                    "@item_NameAPAR_BallisticGatling_S4",
                    "@item_DescAPAR_BallisticGatling_S4",
                ),
                weapon(
                    "Bengal_BallisticGatling_S6",
                    "@item_NameAPAR_BallisticGatling_S4",
                    "@item_DescAPAR_BallisticGatling_S4",
                ),
            ],
            ..GameData::default()
        };
        for field in [KeyField::Name, KeyField::Desc] {
            let users = key_users(&gd, field, |_| true);
            assert_eq!(users.len(), 1);
            let (key, list) = users.iter().next().unwrap();
            let own = resolve(key, list, |r: &str| Some(r.to_string())).unwrap();
            assert_eq!(
                own,
                Ownership::Namesake("APAR_BallisticGatling_S4".into()),
                "{field:?}"
            );
        }
    }

    #[test]
    fn only_same_text_and_contested() {
        let one = s(&["Foo_A"]);
        assert_eq!(
            resolve("item_NameBar", &one, |_| Some(1)),
            Some(Ownership::Only("Foo_A".into()))
        );
        let two = s(&["Foo_Black", "Foo_Red"]);
        let own = resolve("item_NameFoo", &two, |_| Some("same"));
        assert_eq!(own, Some(Ownership::SameText(two.clone())));
        assert_eq!(own.unwrap().writer(), Some("Foo_Black"));
        let own = resolve("item_NameFoo", &two, |r: &str| Some(r.to_string()));
        assert_eq!(own.clone(), Some(Ownership::Contested(two.clone())));
        assert_eq!(own.unwrap().writer(), None);
        let own = resolve("item_NameFoo", &two, |_| None::<u8>);
        assert_eq!(own, Some(Ownership::Contested(two)));
        assert_eq!(resolve("k", &[], |_| Some(0)), None);
    }

    #[test]
    fn filters_templates_props_and_keep() {
        let mut prop = Component {
            item: ItemIdentity {
                record_name: "EntityClassDefinition.Shield_prop".into(),
                loc: LocKeys {
                    name: Some("@item_NameShield".into()),
                    ..LocKeys::default()
                },
                ..ItemIdentity::default()
            },
            ..Component::default()
        };
        let mut tmpl = prop.clone();
        tmpl.item.record_name = "EntityClassDefinition.Shield_Template".into();
        tmpl.item.is_template = true;
        let mut real = prop.clone();
        real.item.record_name = "EntityClassDefinition.Shield".into();
        let mut ai = prop.clone();
        ai.item.record_name = "EntityClassDefinition.Shield_AI".into();
        prop.item.size = Some(1);
        let gd = GameData {
            components: vec![prop, tmpl, real, ai],
            ..GameData::default()
        };
        let users = key_users(&gd, KeyField::Name, |i| !i.record_name.ends_with("_AI"));
        assert_eq!(users["item_nameshield"], s(&["Shield"]));
        assert!(key_users(&gd, KeyField::Desc, |_| true).is_empty());
    }
}

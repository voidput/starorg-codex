//! Missing names (spec missing-names): a name for every player-visible entity whose
//! `AttachDef.Localization.Name` points at a key stock `global.ini` lacks, so the game
//! shows a name instead of the raw `@item_Name…` key.
//!
//! The key is added ([`Engine::add`]); no stock line changes. The value, in order:
//!
//! 1. **variant**: entities of the same `AttachDef` type whose record name differs only by
//!    a trailing `_NN` / `_sfNN` / `_tintNN` / `_lrNN` (or is the bare base name) all
//!    resolve to exactly one real name;
//! 2. **base**: the record name ends in a variant segment and the base entity (segment
//!    dropped, same type) has a real name: `<base name> (<SEGMENT>)`, as
//!    `P4-AR Rifle (LR01)`; a `_mag` of a weapon is `<weapon name> Magazine`;
//! 3. **description**: entities sharing the description key all resolve to exactly one
//!    real name;
//! 4. **readable**: words from the key (or the record name): known prefixes dropped,
//!    `_` and camel case split, words title-cased, manufacturer codes and short codes
//!    upper-cased, runs of numbers joined with `-` so variants stay distinct.
//!
//! Looser guesses (any sibling with a shorter record name) were measured wrong on LIVE
//! 4.10 (an iron box named "Agricium (Ore)", armour named `<= PLACEHOLDER =>`) and are not
//! made. Stock keys whose value is a placeholder are not touched (mostly hidden ship parts).

use super::{Category, Engine, stock_name};
use crate::ini::{IniError, StockIni, fold_key};
use crate::model::{GameData, ItemName, loc_key};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// `AttachDef` types the player never sees named (ship internals, doors, usables).
pub const HIDDEN_TYPES: &[&str] = &[
    "FlightController",
    "SeatDashboard",
    "ControlPanel",
    "Relay",
    "NOITEM_Vehicle",
    "UNDEFINED",
    "Door",
    "Usable",
];

/// Key prefixes dropped for the readable name, tried in order (case-insensitive).
const PREFIXES: &[&str] = &[
    "text_ui_charactercustomizer_item_",
    "items_commodities_",
    "crafting_machinename_",
    "item_name_",
    "item_name",
    "item_type_",
    "item_type",
    "item_",
];

/// Rule names, as [`super::AddedKey::rule`] and the counts report them.
pub const RULE_VARIANT: &str = "variant";
pub const RULE_BASE: &str = "base";
pub const RULE_DESCRIPTION: &str = "description";
pub const RULE_READABLE: &str = "readable";

/// A trailing variant segment: `01`, `sf02`, `tint03`, `lr01` (lower case).
fn is_variant_segment(seg: &str) -> bool {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    digits(seg)
        || ["sf", "tint", "lr"]
            .iter()
            .any(|p| seg.strip_prefix(p).is_some_and(digits))
}

/// The record name without one trailing variant segment, when it has one.
pub fn variant_base(class: &str) -> Option<&str> {
    let (base, last) = class.rsplit_once('_')?;
    (is_variant_segment(last) && !base.is_empty()).then_some(base)
}

/// Exactly one distinct value, or none.
fn only<'a>(mut it: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let first = it.next()?;
    it.all(|x| x == first).then_some(first)
}

/// Split one `_`-free token at lower-to-upper case changes (`AmmoCrate` -> `Ammo`, `Crate`).
fn camel_split(t: &str) -> Vec<&str> {
    let b = t.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    for i in 1..b.len() {
        if b[i].is_ascii_uppercase() && b[i - 1].is_ascii_lowercase() {
            out.push(&t[start..i]);
            start = i;
        }
    }
    out.push(&t[start..]);
    out
}

/// One word of a readable name.
fn word(t: &str, makers: &HashSet<String>) -> String {
    let upper = t.to_ascii_uppercase();
    if t.bytes().all(|b| b.is_ascii_digit()) {
        return t.to_string();
    }
    if t.len() <= 5 && makers.contains(&upper) {
        return upper;
    }
    let letters = t.bytes().take_while(u8::is_ascii_alphabetic).count();
    let rest_digits = letters < t.len() && t[letters..].bytes().all(|b| b.is_ascii_digit());
    // S4, LR01, SF02, MK2: a short code and its number.
    if rest_digits && (1..=2).contains(&letters) {
        return upper;
    }
    // LMG, NVS, HDH: short and without vowels, a code not a word.
    let alpha = letters == t.len();
    if alpha && t.len() <= 4 && !t.bytes().any(|b| b"aeiouyAEIOUY".contains(&b)) {
        return upper;
    }
    if t.bytes().all(|b| !b.is_ascii_lowercase()) {
        return t.to_string();
    }
    let mut c = t.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Readable name from a loc key (or a record name): `item_NameAmmoCrate_S4` ->
/// `Ammo Crate S4`, `item_Name_nvs_pilot_flightsuit_01_01_02` ->
/// `NVS Pilot Flightsuit 01-01-02`. `makers` holds manufacturer codes (upper case).
/// Empty when nothing readable is left.
pub fn readable(key: &str, makers: &HashSet<String>) -> String {
    let k = key.strip_prefix('@').unwrap_or(key);
    let lower = k.to_ascii_lowercase();
    let rest = PREFIXES
        .iter()
        .find_map(|p| lower.starts_with(p).then(|| &k[p.len()..]))
        .unwrap_or(k);
    let tokens: Vec<&str> = rest
        .split(['_', ' ', '-', '.'])
        .filter(|t| !t.is_empty())
        .flat_map(camel_split)
        .filter(|t| t.bytes().any(|b| b.is_ascii_alphanumeric()))
        .collect();
    let mut out = String::new();
    let mut prev_num = false;
    for t in tokens {
        let num = t.bytes().all(|b| b.is_ascii_digit());
        if !out.is_empty() {
            out.push(if num && prev_num { '-' } else { ' ' });
        }
        out.push_str(&word(t, makers));
        prev_num = num;
    }
    out
}

/// Manufacturer codes in stock (`manufacturer_NameRSI` -> `RSI`), upper case.
pub fn maker_codes(stock: &StockIni) -> HashSet<String> {
    stock
        .entries()
        .filter_map(|(_, k, _)| {
            let k = crate::ini::strip_flag_suffix(k);
            let (head, code) = k.split_at_checked("manufacturer_name".len())?;
            (head.eq_ignore_ascii_case("manufacturer_name") && !code.is_empty())
                .then(|| code.to_ascii_uppercase())
        })
        .collect()
}

/// Does stock lack this key (not merely have it twice)?
fn missing(stock: &StockIni, key: &str) -> bool {
    matches!(stock.find(key), Err(IniError::UnknownKey(_)))
}

/// The name for one missing key, with the rule that found it.
pub struct Found {
    pub key: String,
    pub record: String,
    pub value: String,
    pub rule: &'static str,
}

/// Names for every missing key of a player-visible entity, by folded key.
pub fn plan(gd: &GameData, stock: &StockIni) -> BTreeMap<String, Found> {
    let good = |n: &ItemName| stock_name(stock, Some(&n.name));
    // (type, variant base) -> real names of its members; description key -> real names.
    let mut by_base: HashMap<(&str, &str), BTreeSet<&str>> = HashMap::new();
    let mut by_desc: HashMap<String, BTreeSet<&str>> = HashMap::new();
    // Class -> (type, real name), for the base rule.
    let mut by_class: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
    for n in &gd.item_names {
        let Some(v) = good(n) else { continue };
        by_class.entry(&n.class).or_default().push((&n.kind, v));
        // A member of its base's family, and the bare base of its own variants.
        if let Some(base) = variant_base(&n.class) {
            by_base.entry((&n.kind, base)).or_default().insert(v);
        }
        by_base.entry((&n.kind, &n.class)).or_default().insert(v);
        if let Some(d) = n.desc.as_deref().and_then(loc_key)
            && !super::is_sentinel_key(d)
        {
            by_desc.entry(fold_key(d)).or_default().insert(v);
        }
    }
    // Missing key -> its entities.
    let mut keys: BTreeMap<String, (&str, Vec<&ItemName>)> = BTreeMap::new();
    for n in &gd.item_names {
        if HIDDEN_TYPES.contains(&n.kind.as_str()) {
            continue;
        }
        let Some(k) = loc_key(&n.name) else { continue };
        if super::is_sentinel_key(k) || !crate::ini::is_addable_key(k) || !missing(stock, k) {
            continue;
        }
        keys.entry(fold_key(k)).or_insert((k, Vec::new())).1.push(n);
    }
    let makers = maker_codes(stock);
    let mut out = BTreeMap::new();
    for (folded, (key, ents)) in keys {
        let variant = only(ents.iter().filter_map(|n| {
            let base = variant_base(&n.class)?;
            only(by_base.get(&(n.kind.as_str(), base))?.iter().copied())
        }));
        let base = || {
            let found: Vec<String> = ents
                .iter()
                .filter_map(|n| {
                    let (b, last) = n.class.rsplit_once('_')?;
                    let named = by_class.get(b)?;
                    if last == "mag" {
                        let weapon = |k: &str| k.starts_with("Weapon") && k != "WeaponAttachment";
                        let v = only(named.iter().filter(|(k, _)| weapon(k)).map(|(_, v)| *v))?;
                        return Some(format!("{v} Magazine"));
                    }
                    if !is_variant_segment(last) {
                        return None;
                    }
                    let v = only(named.iter().filter(|(k, _)| *k == n.kind).map(|(_, v)| *v))?;
                    Some(format!("{v} ({})", last.to_ascii_uppercase()))
                })
                .collect();
            only(found.iter().map(String::as_str)).map(str::to_string)
        };
        let desc = || {
            only(ents.iter().filter_map(|n| {
                let d = fold_key(loc_key(n.desc.as_deref()?)?);
                only(by_desc.get(&d)?.iter().copied())
            }))
        };
        let (value, rule) = if let Some(v) = variant {
            (v.to_string(), RULE_VARIANT)
        } else if let Some(v) = base() {
            (v, RULE_BASE)
        } else if let Some(v) = desc() {
            (v.to_string(), RULE_DESCRIPTION)
        } else {
            let mut r = readable(key, &makers);
            if r.is_empty() {
                r = readable(&ents[0].class, &makers);
            }
            (r, RULE_READABLE)
        };
        if value.trim().is_empty() {
            continue;
        }
        out.insert(
            folded,
            Found {
                key: key.to_string(),
                record: ents[0].class.clone(),
                value,
                rule,
            },
        );
    }
    out
}

/// Add a name for every missing key ([`plan`]).
pub fn generate(en: &mut Engine, gd: &GameData) {
    en.cov(Category::MissingNames);
    for f in plan(gd, en.stock).into_values() {
        en.add(
            Category::MissingNames,
            &f.key,
            Some(f.record),
            &f.value,
            f.rule,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enhance::profile::Profile;
    use crate::ini::BOM;

    fn makers() -> HashSet<String> {
        ["RSI", "BEHR"].iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn readable_names() {
        let m = makers();
        for (key, want) in [
            ("@items_commodities_pressurized_ice", "Pressurized Ice"),
            ("@item_NameAmmoCrate_S4", "Ammo Crate S4"),
            (
                "@text_ui_charactercustomizer_item_piercing_ball",
                "Piercing Ball",
            ),
            (
                "@item_Name_nvs_pilot_flightsuit_01_01_02",
                "NVS Pilot Flightsuit 01-01-02",
            ),
            (
                "@item_Namebehr_lmg_ballistic_01_lr01",
                "BEHR LMG Ballistic 01 LR01",
            ),
            ("@items_commodities_medGel", "Med Gel"),
            (
                "@item_NameMRCK_S05_RSI_Perseus_Torpedo_R",
                "MRCK S05 RSI Perseus Torpedo R",
            ),
            (
                "@crafting_machineName_fabricator_standard",
                "Fabricator Standard",
            ),
            ("@item_TypeSeatDashboard", "Seat Dashboard"),
        ] {
            assert_eq!(readable(key, &m), want, "{key}");
        }
        assert_eq!(readable("@item_Name_", &m), "");
    }

    #[test]
    fn variant_bases() {
        assert_eq!(variant_base("gun_01_tint03"), Some("gun_01"));
        assert_eq!(variant_base("gun_01_lr01"), Some("gun_01"));
        assert_eq!(variant_base("suit_01_01_02"), Some("suit_01_01"));
        assert_eq!(variant_base("gun_sf2"), Some("gun"));
        assert_eq!(variant_base("gun_black"), None);
        assert_eq!(variant_base("gun"), None);
    }

    fn stock() -> StockIni {
        let mut v = BOM.to_vec();
        v.extend_from_slice(
            b"manufacturer_NameNVS=Novikov\r\n\
              item_Name_suit_01_01_01=Pilot Suit Blue\r\n\
              item_Name_suit_01_01_03=Pilot Suit Blue\r\n\
              item_Name_armor_01_01=Redeemer Armor\r\n\
              item_Name_armor_01_02=<= PLACEHOLDER =>\r\n\
              item_Name_crate_a=Iron\r\n\
              item_Name_hidden_a=Panel\r\n",
        );
        StockIni::parse(v)
    }

    fn named(class: &str, name: &str, desc: Option<&str>, kind: &str) -> ItemName {
        ItemName {
            class: class.into(),
            name: name.into(),
            desc: desc.map(str::to_string),
            kind: kind.into(),
        }
    }

    fn data() -> GameData {
        GameData {
            item_names: vec![
                named(
                    "suit_01_01_01",
                    "@item_Name_suit_01_01_01",
                    None,
                    "Char_Armor_Torso",
                ),
                named(
                    "suit_01_01_02",
                    "@item_Name_suit_01_01_02",
                    None,
                    "Char_Armor_Torso",
                ),
                named(
                    "suit_01_01_03",
                    "@item_Name_suit_01_01_03",
                    None,
                    "Char_Armor_Torso",
                ),
                // Siblings disagree (one is a placeholder, so only one real name: still one).
                named("armor_01_01", "@item_Name_armor_01_01", None, "Armor"),
                named("armor_01_02", "@item_Name_armor_01_02", None, "Armor"),
                named("armor_01_03", "@item_Name_armor_01_03", None, "Armor"),
                // Same type, different base: no variant; shares a description key.
                named("crate_a", "@item_Name_crate_a", Some("@desc_iron"), "Cargo"),
                named(
                    "crate_b",
                    "@items_commodities_iron_box",
                    Some("@desc_iron"),
                    "Cargo",
                ),
                // Nothing to borrow: readable.
                named("nvs_box_01", "@item_Name_nvs_box_01_02", None, "Cargo"),
                // Hidden type, sentinel and placeholder-valued keys are left alone.
                named("panel_01", "@item_Name_panel_missing", None, "ControlPanel"),
                named("x", "@LOC_PLACEHOLDER", None, "Misc"),
                named("y", "@item_Name_armor_01_02", None, "Misc"),
            ],
            ..GameData::default()
        }
    }

    #[test]
    fn rules_in_order() {
        let s = stock();
        let p = plan(&data(), &s);
        let got: Vec<(&str, &str, &str)> = p
            .values()
            .map(|f| (f.key.as_str(), f.value.as_str(), f.rule))
            .collect();
        assert_eq!(
            got,
            vec![
                ("item_Name_armor_01_03", "Redeemer Armor", RULE_VARIANT),
                ("item_Name_nvs_box_01_02", "NVS Box 01-02", RULE_READABLE),
                ("item_Name_suit_01_01_02", "Pilot Suit Blue", RULE_VARIANT),
                ("items_commodities_iron_box", "Iron", RULE_DESCRIPTION),
            ]
        );
    }

    #[test]
    fn base_rule_names_a_variant_after_its_base() {
        let mut v = BOM.to_vec();
        v.extend_from_slice(
            b"item_Name_rifle_01=P4-AR Rifle\r\n\
              item_Name_rifle_01_tint01=P4-AR Rifle Black\r\n\
              item_Name_gun_01=G1\r\n\
              item_Name_box_01=Box\r\n\
              item_Name_sniper_01=Scope\r\n",
        );
        let s = StockIni::parse(v);
        let w = "WeaponPersonal";
        let gd = GameData {
            item_names: vec![
                named("rifle_01", "@item_Name_rifle_01", None, w),
                named("rifle_01_tint01", "@item_Name_rifle_01_tint01", None, w),
                // Siblings disagree, so no variant: the base's name with the segment.
                named("rifle_01_lr01", "@item_Name_rifle_01_lr01", None, w),
                // A weapon's magazine (another type) is named after the weapon.
                named(
                    "rifle_01_mag",
                    "@item_Name_rifle_01_mag",
                    None,
                    "WeaponAttachment",
                ),
                // One sibling only: the strict variant rule still wins.
                named("gun_01", "@item_Name_gun_01", None, w),
                named("gun_01_lr01", "@item_Name_gun_01_lr01", None, w),
                // Not a weapon's magazine, and a base of another type: readable.
                named("box_01", "@item_Name_box_01", None, "Cargo"),
                named("box_01_mag", "@item_Name_box_01_mag", None, "Misc"),
                named(
                    "sniper_01",
                    "@item_Name_sniper_01",
                    None,
                    "WeaponAttachment",
                ),
                named("sniper_01_sf01", "@item_Name_sniper_01_sf01", None, w),
            ],
            ..GameData::default()
        };
        let got: Vec<(String, String, &str)> = plan(&gd, &s)
            .into_values()
            .map(|f| (f.key, f.value, f.rule))
            .collect();
        let want = [
            ("item_Name_box_01_mag", "Box 01 Mag", RULE_READABLE),
            ("item_Name_gun_01_lr01", "G1", RULE_VARIANT),
            ("item_Name_rifle_01_lr01", "P4-AR Rifle (LR01)", RULE_BASE),
            ("item_Name_rifle_01_mag", "P4-AR Rifle Magazine", RULE_BASE),
            ("item_Name_sniper_01_sf01", "Sniper 01 SF01", RULE_READABLE),
        ];
        let want: Vec<(String, String, &str)> = want
            .iter()
            .map(|(k, v, r)| (k.to_string(), v.to_string(), *r))
            .collect();
        assert_eq!(got, want);
    }

    #[test]
    fn engine_adds_keys_and_render_keeps_stock_lines() {
        let s = stock();
        let gd = data();
        let mut p = Profile::off();
        p.missing_names = true;
        let g = super::super::generate(&gd, &s, &p, &crate::enhance::owned::NoneOwned);
        assert!(g.edits.is_empty());
        assert_eq!(g.count(Category::MissingNames), 4);
        let cov = g.coverage.get(Category::MissingNames);
        assert_eq!((cov.candidates, cov.written), (4, 4));
        let bytes = super::super::render(&s, &g).expect("whole-file rules pass");
        assert!(bytes.starts_with(s.as_bytes()));
        let out = StockIni::parse(bytes);
        assert_eq!(
            out.value("item_Name_suit_01_01_02"),
            Some("Pilot Suit Blue")
        );
        assert_eq!(out.value("item_name_nvs_box_01_02"), Some("NVS Box 01-02"));
        // Off: nothing added.
        let g = super::super::generate(&gd, &s, &Profile::off(), &crate::enhance::owned::NoneOwned);
        assert!(g.added.is_empty());
    }

    #[test]
    fn a_bad_value_is_dropped_with_a_warning() {
        let s = stock();
        let p = Profile::default();
        let mut en = Engine::new(&s, &p);
        assert!(!en.add(
            Category::MissingNames,
            "new_key",
            None,
            "<= PLACEHOLDER =>",
            RULE_READABLE
        ));
        assert!(!en.add(
            Category::MissingNames,
            "item_Name_crate_a",
            None,
            "X",
            RULE_READABLE
        ));
        assert!(en.add(
            Category::MissingNames,
            "@new_key",
            None,
            "Fine",
            RULE_READABLE
        ));
        assert!(!en.add(
            Category::MissingNames,
            "NEW_KEY",
            None,
            "Again",
            RULE_READABLE
        ));
        let c = en.out.coverage.get(Category::MissingNames);
        assert_eq!(c.written, 1);
        assert_eq!(c.left_for(super::super::Reason::InStock), 1);
        assert_eq!(c.left_for(super::super::Reason::DroppedByValidation), 1);
        assert!(
            en.out
                .warnings
                .iter()
                .any(|w| w.detail.contains("placeholder-text"))
        );
    }
}

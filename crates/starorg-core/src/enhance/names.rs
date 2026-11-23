//! Missing names (spec missing-names): a name for every player-visible entity whose
//! `AttachDef.Localization.Name` points at a key stock `global.ini` lacks or holds as a
//! placeholder, so the game shows a name instead of the raw `@item_Name…` key.
//!
//! One name chain ([`Resolver`]), read by every generator: **curated** (a `fill_name` patch
//! entry, while the stock name is bad), **game** (a real stock name, never replaced),
//! **generated** (the rules below), and the **readable** guard, which always has text. A
//! missing key is added ([`Engine::add`]); a placeholder stock name is renamed in place
//! ([`Engine::rename`]). The generated value, in order:
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
//! made. Placeholder stock names of hidden types (mostly ship parts) are not touched.

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
    "actor_species_creature_",
    "items_commodities_",
    "item_commodities_",
    "crafting_machinename_",
    "item_name_",
    "item_name",
    "item_type_",
    "item_type",
    // A description key used as the name (`item_Descbehr_rifle_ballistic_1`).
    "item_description_",
    "item_description",
    "item_desc_",
    "item_desc",
    "item_",
];

/// Rule names, as [`super::AddedKey::rule`] and the counts report them.
pub const RULE_VARIANT: &str = "variant";
pub const RULE_BASE: &str = "base";
pub const RULE_DESCRIPTION: &str = "description";
pub const RULE_READABLE: &str = "readable";

/// A trailing variant segment: `01`, `sf02`, `tint03`, `lr01` (lower case).
pub(crate) fn is_variant_segment(seg: &str) -> bool {
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

/// One word of a readable name. `maker_ok`: the word may be a manufacturer code (it is the
/// first word or written in capitals), so `grey` in a colour list stays a word although `GREY`
/// is Grey's Market.
fn word(t: &str, makers: &HashSet<String>, maker_ok: bool) -> String {
    let upper = t.to_ascii_uppercase();
    if t.bytes().all(|b| b.is_ascii_digit()) {
        return t.to_string();
    }
    if maker_ok && t.len() <= 5 && makers.contains(&upper) {
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
/// `NVS Pilot Flightsuit 01-01-02`. `makers` holds manufacturer codes (upper case); a code
/// counts only as the first word or written in capitals. A word repeated across a camel-case
/// split (`Mining_MiningLaser`, `itemPort_port`) is written once, a trailing `_Name` dropped.
/// Empty when nothing readable is left.
pub fn readable(key: &str, makers: &HashSet<String>) -> String {
    let k = key.strip_prefix('@').unwrap_or(key);
    let lower = k.to_ascii_lowercase();
    let mut rest = PREFIXES
        .iter()
        .find_map(|p| lower.starts_with(p).then(|| &k[p.len()..]))
        .unwrap_or(k);
    if let Some(head) = rest.len().checked_sub("_name".len())
        && rest.is_char_boundary(head)
        && rest[head..].eq_ignore_ascii_case("_name")
    {
        rest = &rest[..head];
    }
    // (word, is a number, came from a camel-case split of a longer token)
    let mut words: Vec<(String, bool, bool)> = Vec::new();
    for tok in rest.split(['_', ' ', '-', '.']).filter(|t| !t.is_empty()) {
        let parts: Vec<&str> = camel_split(tok)
            .into_iter()
            .filter(|t| t.bytes().any(|b| b.is_ascii_alphanumeric()))
            .collect();
        let split = parts.len() > 1;
        for t in parts {
            let maker_ok = words.is_empty() || t.bytes().all(|b| !b.is_ascii_lowercase());
            let w = word(t, makers, maker_ok);
            if let Some((prev, _, prev_split)) = words.last()
                && (split || *prev_split)
                && prev.eq_ignore_ascii_case(&w)
            {
                continue;
            }
            let num = t.bytes().all(|b| b.is_ascii_digit());
            words.push((w, num, split));
        }
    }
    let mut out = String::new();
    let mut prev_num = false;
    for (w, num, _) in words {
        if !out.is_empty() {
            out.push(if num && prev_num { '-' } else { ' ' });
        }
        out.push_str(&w);
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

/// Where a name came from, best first (spec missing-names, "Name tiers").
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    /// A curated name from a `fill_name` patch entry (stock lacks the key or holds a
    /// placeholder).
    Curated,
    /// The stock name, when it is real text.
    Game,
    /// Generated by a rule ([`RULE_VARIANT`], [`RULE_BASE`], [`RULE_DESCRIPTION`]).
    Generated,
    /// Generated from the key's words ([`RULE_READABLE`]).
    Readable,
}

impl Tier {
    pub fn name(self) -> &'static str {
        match self {
            Tier::Curated => "curated",
            Tier::Game => "game",
            Tier::Generated => "generated",
            Tier::Readable => "readable",
        }
    }
}

/// Curated names by folded key (`patch::curated_names`): the values of the active patch
/// set's `fill_name` entries.
pub type Curated = HashMap<String, String>;

/// Is this stock name bad (missing, empty, a placeholder, a raw `@key`)? A key stock holds
/// twice is not ours to judge.
fn bad_stock(stock: &StockIni, key: &str) -> bool {
    match stock.find(key) {
        Err(IniError::UnknownKey(_)) => true,
        Err(_) => false,
        Ok(line) => crate::model::is_placeholder_name(stock.value_at(line).map(str::trim)),
    }
}

/// The curated or game name of a loc key: tiers 1 and 2. A curated name counts only while
/// the stock name is bad, so a retired patch gives way to the game's name.
fn known<'a>(stock: &'a StockIni, curated: &'a Curated, key: &str) -> Option<(&'a str, Tier)> {
    if let Some(v) = stock_name(stock, Some(key)) {
        return Some((v, Tier::Game));
    }
    let v = curated.get(&fold_key(key))?;
    (bad_stock(stock, key) && !crate::model::is_placeholder_name(Some(v)))
        .then_some((v.as_str(), Tier::Curated))
}

/// The generated name for one key whose stock name is missing or bad, with its rule.
pub struct Found {
    pub key: String,
    pub record: String,
    pub value: String,
    pub rule: &'static str,
    /// Stock has the key (with a placeholder value): the line is renamed, not added.
    pub in_stock: bool,
}

impl Found {
    pub fn tier(&self) -> Tier {
        if self.rule == RULE_READABLE {
            Tier::Readable
        } else {
            Tier::Generated
        }
    }
}

/// One name chain for every generator: curated, game, generated (spec missing-names).
pub struct Resolver<'a> {
    stock: &'a StockIni,
    curated: &'a Curated,
    /// Generated names of visible items whose stock name is missing or bad, by folded key
    /// (empty when Fix missing item names is off).
    pub plan: BTreeMap<String, Found>,
}

impl<'a> Resolver<'a> {
    /// `generate`: run the generated tier (the missing-names switch).
    pub fn new(gd: &GameData, stock: &'a StockIni, curated: &'a Curated, generate: bool) -> Self {
        let plan = if generate {
            plan(gd, stock, curated)
        } else {
            BTreeMap::new()
        };
        Self {
            stock,
            curated,
            plan,
        }
    }

    /// The display name of a loc reference through the chain, with its tier; `None` when
    /// no tier has one (a hidden part, or the switch is off).
    pub fn resolve(&self, reference: Option<&str>) -> Option<(&str, Tier)> {
        let k = loc_key(reference?)?;
        if let Some(found) = known(self.stock, self.curated, k) {
            return Some(found);
        }
        let f = self.plan.get(&fold_key(k))?;
        Some((f.value.as_str(), f.tier()))
    }

    /// [`Self::resolve`] without the tier: the drop-in for `enhance::stock_name`.
    pub fn name(&self, reference: Option<&str>) -> Option<&str> {
        self.resolve(reference).map(|(v, _)| v)
    }
}

/// (folded key, name) of entities with a known name.
type Named<'a> = BTreeSet<(String, &'a str)>;

/// Generated names for every key of a player-visible entity whose stock name is missing or
/// bad (placeholder, empty), by folded key. Sibling and base names come through the same
/// chain (curated, then game), so a curated base names its variants; a key never borrows
/// its own curated name (that stays the patch's, so the patch keeps its outcome).
pub fn plan(gd: &GameData, stock: &StockIni, curated: &Curated) -> BTreeMap<String, Found> {
    let good = |n: &ItemName| -> Option<(String, &str)> {
        let k = loc_key(&n.name)?;
        known(stock, curated, k).map(|(v, _)| (fold_key(k), v))
    };
    // (type, variant base) -> named members; description key -> named entities.
    let mut by_base: HashMap<(&str, &str), Named> = HashMap::new();
    let mut by_desc: HashMap<String, Named> = HashMap::new();
    // Class -> (type, key, name), for the base rule.
    let mut by_class: HashMap<&str, Vec<(&str, String, &str)>> = HashMap::new();
    for n in &gd.item_names {
        let Some((k, v)) = good(n) else { continue };
        by_class
            .entry(&n.class)
            .or_default()
            .push((&n.kind, k.clone(), v));
        // A member of its base's family, and the bare base of its own variants.
        if let Some(base) = variant_base(&n.class) {
            by_base
                .entry((&n.kind, base))
                .or_default()
                .insert((k.clone(), v));
        }
        by_base
            .entry((&n.kind, &n.class))
            .or_default()
            .insert((k.clone(), v));
        if let Some(d) = n.desc.as_deref().and_then(loc_key)
            && !super::is_sentinel_key(d)
        {
            by_desc.entry(fold_key(d)).or_default().insert((k, v));
        }
    }
    // Key with a missing or bad stock name -> its visible entities.
    let mut keys: BTreeMap<String, (&str, Vec<&ItemName>)> = BTreeMap::new();
    for n in &gd.item_names {
        if HIDDEN_TYPES.contains(&n.kind.as_str()) {
            continue;
        }
        let Some(k) = loc_key(&n.name) else { continue };
        if super::is_sentinel_key(k) || !crate::ini::is_addable_key(k) || !bad_stock(stock, k) {
            continue;
        }
        keys.entry(fold_key(k)).or_insert((k, Vec::new())).1.push(n);
    }
    let makers = maker_codes(stock);
    let mut out = BTreeMap::new();
    for (folded, (key, ents)) in keys {
        // One name among the others (never this key's own).
        let others = |set: &Named<'_>| -> Option<String> {
            only(set.iter().filter(|(k, _)| *k != folded).map(|(_, v)| *v)).map(str::to_string)
        };
        let variant = only_owned(ents.iter().filter_map(|n| {
            let base = variant_base(&n.class)?;
            others(by_base.get(&(n.kind.as_str(), base))?)
        }));
        let base = || {
            only_owned(ents.iter().filter_map(|n| {
                let (b, last) = n.class.rsplit_once('_')?;
                let named: Vec<&(&str, String, &str)> = by_class
                    .get(b)?
                    .iter()
                    .filter(|(_, k, _)| *k != folded)
                    .collect();
                if last == "mag" {
                    let weapon = |k: &str| k.starts_with("Weapon") && k != "WeaponAttachment";
                    let v = only(
                        named
                            .iter()
                            .filter(|(k, _, _)| weapon(k))
                            .map(|(_, _, v)| *v),
                    )?;
                    return Some(format!("{v} Magazine"));
                }
                if !is_variant_segment(last) {
                    return None;
                }
                let v = only(
                    named
                        .iter()
                        .filter(|(k, _, _)| *k == n.kind)
                        .map(|(_, _, v)| *v),
                )?;
                Some(format!("{v} ({})", last.to_ascii_uppercase()))
            }))
        };
        let desc = || {
            only_owned(ents.iter().filter_map(|n| {
                let d = fold_key(loc_key(n.desc.as_deref()?)?);
                others(by_desc.get(&d)?)
            }))
        };
        let (value, rule) = if let Some(v) = variant {
            (v, RULE_VARIANT)
        } else if let Some(v) = base() {
            (v, RULE_BASE)
        } else if let Some(v) = desc() {
            (v, RULE_DESCRIPTION)
        } else {
            (guard_name(key, &ents[0].class, &makers), RULE_READABLE)
        };
        out.insert(
            folded,
            Found {
                key: key.to_string(),
                record: ents[0].class.clone(),
                value,
                rule,
                in_stock: !missing(stock, key),
            },
        );
    }
    out
}

/// Exactly one distinct owned value, or none.
fn only_owned(mut it: impl Iterator<Item = String>) -> Option<String> {
    let first = it.next()?;
    it.all(|x| x == first).then_some(first)
}

/// The readable name, which always has text (tier 4, the guard): from the key, else the
/// record name, else the key itself without its `@`. Never a raw `@key`.
fn guard_name(key: &str, class: &str, makers: &HashSet<String>) -> String {
    [readable(key, makers), readable(class, makers)]
        .into_iter()
        .find(|r| !r.trim().is_empty())
        .unwrap_or_else(|| key.trim_start_matches('@').to_string())
}

/// Name every key the resolver's plan holds: an added key when stock lacks it
/// ([`Engine::add`]), a renamed line when stock holds a placeholder ([`Engine::rename`]).
pub fn generate(en: &mut Engine, names: &Resolver) {
    en.cov(Category::MissingNames);
    for f in names.plan.values() {
        let record = Some(f.record.clone());
        if f.in_stock {
            en.rename(Category::MissingNames, &f.key, record, &f.value, f.rule);
        } else {
            en.add(Category::MissingNames, &f.key, record, &f.value, f.rule);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enhance::profile::Profile;
    use crate::ini::BOM;

    fn makers() -> HashSet<String> {
        ["RSI", "BEHR", "GREY", "GAMA"]
            .iter()
            .map(|s| s.to_string())
            .collect()
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
            // A maker code that is also a word (GREY: Grey's Market) is a word unless it is
            // first or in capitals; repeated colours stay.
            (
                "@item_NamePerseus_Paint_black_grey_grey",
                "Perseus Paint Black Grey Grey",
            ),
            ("@item_Name_grey_box", "GREY Box"),
            ("@item_NameBox_GREY", "Box GREY"),
            // A word repeated across a camel-case split, and the key's trailing `_Name`.
            (
                "@item_Mining_MiningLaser_GAMA_Hyun_S1_Name",
                "Mining Laser GAMA Hyun S1",
            ),
            ("@itemPort_port_SalvageArmR", "Item Port Salvage Arm R"),
            (
                "@item_NameGalaxy_Paint_Black_Red_Red",
                "Galaxy Paint Black Red Red",
            ),
            // A description key used as the name.
            ("@item_Descbehr_rifle_ballistic_1", "BEHR Rifle Ballistic 1"),
            ("@item_Desc_box_01", "Box 01"),
            ("@item_commodities_ventslug", "Ventslug"),
            ("@actor_species_creature_valakkar_apex", "Valakkar Apex"),
            ("@item_Name_name", "Name"),
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
              item_Name_hidden_a=Panel\r\n\
              item_Name_door_01=<= PLACEHOLDER =>\r\n",
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
                // Hidden types (missing or placeholder) and sentinels are left alone.
                named("panel_01", "@item_Name_panel_missing", None, "ControlPanel"),
                named("door_01", "@item_Name_door_01", None, "Door"),
                named("x", "@LOC_PLACEHOLDER", None, "Misc"),
                // A visible user of a placeholder-valued stock key: renamed (tier 3).
                named("y", "@item_Name_armor_01_02", None, "Misc"),
            ],
            ..GameData::default()
        }
    }

    #[test]
    fn rules_in_order() {
        let s = stock();
        let p = plan(&data(), &s, &Curated::new());
        let got: Vec<(&str, &str, &str)> = p
            .values()
            .map(|f| (f.key.as_str(), f.value.as_str(), f.rule))
            .collect();
        assert_eq!(
            got,
            vec![
                ("item_Name_armor_01_02", "Redeemer Armor", RULE_VARIANT),
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
        let got: Vec<(String, String, &str)> = plan(&gd, &s, &Curated::new())
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
        // The placeholder stock name is renamed in place; the others are added keys.
        let renamed: Vec<_> = g
            .edits
            .values()
            .map(|e| (e.key.as_str(), e.value("<= PLACEHOLDER =>"), e.rule))
            .collect();
        assert_eq!(
            renamed,
            vec![(
                "item_Name_armor_01_02",
                "Redeemer Armor".to_string(),
                Some(RULE_VARIANT)
            )]
        );
        assert_eq!(g.count(Category::MissingNames), 5);
        let cov = g.coverage.get(Category::MissingNames);
        assert_eq!((cov.candidates, cov.written), (5, 5));
        let bytes = super::super::render(&s, &g).expect("whole-file rules pass");
        let out = StockIni::parse(bytes);
        assert_eq!(out.value("item_Name_armor_01_02"), Some("Redeemer Armor"));
        assert_eq!(out.value("item_Name_door_01"), Some("<= PLACEHOLDER =>"));
        assert_eq!(out.value("item_Name_armor_01_01"), Some("Redeemer Armor"));
        assert_eq!(
            out.value("item_Name_suit_01_01_02"),
            Some("Pilot Suit Blue")
        );
        assert_eq!(out.value("item_name_nvs_box_01_02"), Some("NVS Box 01-02"));
        // Off: nothing added.
        let g = super::super::generate(&gd, &s, &Profile::off(), &crate::enhance::owned::NoneOwned);
        assert!(g.added.is_empty());
    }

    /// A curated base (tier 1) names its variants and magazine like a stock base would;
    /// without it the variants are readable; a real stock name beats it.
    #[test]
    fn curated_names_feed_the_rules_like_stock_names() {
        let w = "WeaponPersonal";
        let gd = GameData {
            item_names: vec![
                named("gun_01", "@item_Name_gun_01", None, w),
                named("gun_01_tint01", "@item_Name_gun_01_tint01", None, w),
                named("gun_01_tint02", "@item_Name_gun_01_tint02", None, w),
                named(
                    "gun_01_mag",
                    "@item_Name_gun_01_mag",
                    None,
                    "WeaponAttachment",
                ),
                // A named sibling that differs: the base rule, `<base> (TINT01)`.
                named("rifle_01", "@item_Name_rifle_01", None, w),
                named("rifle_01_tint01", "@item_Name_rifle_01_tint01", None, w),
                named("rifle_01_tint02", "@item_Name_rifle_01_tint02", None, w),
            ],
            ..GameData::default()
        };
        let stock_of = |extra: &str| {
            let mut v = BOM.to_vec();
            v.extend_from_slice(b"item_Name_rifle_01_tint02=Rifle Black\r\n");
            v.extend_from_slice(extra.as_bytes());
            StockIni::parse(v)
        };
        let curated: Curated = [
            (fold_key("item_Name_gun_01"), "Verdict LMG".to_string()),
            (fold_key("item_Name_rifle_01"), "Rifle".to_string()),
        ]
        .into_iter()
        .collect();
        let names = |s: &StockIni, c: &Curated| -> Vec<(String, String, bool)> {
            plan(&gd, s, c)
                .into_values()
                .map(|f| (f.key, f.value, f.in_stock))
                .collect()
        };
        let row = |k: &str, v: &str, in_stock: bool| (k.to_string(), v.to_string(), in_stock);

        // Patched base, stock lacks it: variants and the magazine use it; the base itself
        // keeps its readable name here (its patch replaces it in merge).
        let s = stock_of("");
        assert_eq!(
            names(&s, &curated),
            vec![
                row("item_Name_gun_01", "Gun 01", false),
                row("item_Name_gun_01_mag", "Verdict LMG Magazine", false),
                row("item_Name_gun_01_tint01", "Verdict LMG", false),
                row("item_Name_gun_01_tint02", "Verdict LMG", false),
                row("item_Name_rifle_01", "Rifle 01", false),
                row("item_Name_rifle_01_tint01", "Rifle (TINT01)", false),
            ]
        );
        // Patch switched off (no curated names): readable again.
        assert_eq!(
            names(&s, &Curated::new())[1..4],
            [
                row("item_Name_gun_01_mag", "Gun 01 Mag", false),
                row("item_Name_gun_01_tint01", "Gun 01 Tint01", false),
                row("item_Name_gun_01_tint02", "Gun 01 Tint02", false),
            ]
        );
        // The game names the base now: stock wins over the curated name.
        let s = stock_of("item_Name_gun_01=Stock Gun\r\n");
        let got = names(&s, &curated);
        assert_eq!(
            got[0],
            row("item_Name_gun_01_mag", "Stock Gun Magazine", false)
        );
        assert_eq!(got[1], row("item_Name_gun_01_tint01", "Stock Gun", false));
        // A placeholder stock base: the curated name feeds the variants, and the base's own
        // line is renamed (its patch sets the line in merge).
        let s = stock_of("item_Name_gun_01=<= PLACEHOLDER =>\r\n");
        let got = names(&s, &curated);
        assert_eq!(got[0], row("item_Name_gun_01", "Gun 01", true));
        assert_eq!(got[2], row("item_Name_gun_01_tint01", "Verdict LMG", false));

        // The resolver other generators read: curated, game, generated, readable.
        let s = stock_of("");
        let r = Resolver::new(&gd, &s, &curated, true);
        assert_eq!(
            r.resolve(Some("@item_Name_gun_01")),
            Some(("Verdict LMG", Tier::Curated))
        );
        assert_eq!(
            r.resolve(Some("@item_name_rifle_01_tint02")),
            Some(("Rifle Black", Tier::Game))
        );
        assert_eq!(
            r.resolve(Some("@item_Name_gun_01_mag")),
            Some(("Verdict LMG Magazine", Tier::Generated))
        );
        let empty = Curated::new();
        let off = Resolver::new(&gd, &s, &empty, true);
        assert_eq!(
            off.resolve(Some("@item_Name_gun_01")),
            Some(("Gun 01", Tier::Readable))
        );
        assert_eq!(off.resolve(Some("@item_Name_unknown")), None);
        // Switch off: no generated tier.
        let none = Resolver::new(&gd, &s, &empty, false);
        assert_eq!(none.resolve(Some("@item_Name_gun_01")), None);
    }

    #[test]
    fn the_guard_name_never_is_a_raw_key() {
        let m = makers();
        assert_eq!(guard_name("@item_Name_", "_", &m), "item_Name_");
        assert_eq!(guard_name("@item_Name_", "box_01", &m), "Box 01");
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

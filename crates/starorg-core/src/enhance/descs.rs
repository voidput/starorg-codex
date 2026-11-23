//! Missing descriptions (spec missing-names, "Descriptions"): a description for every
//! player-visible entity whose `AttachDef.Localization.Description` points at a key stock
//! `global.ini` lacks or holds as a placeholder, so the game shows text instead of the raw
//! `@item_Desc…` key. Part of Fix missing item names (`Profile::missing_names`).
//!
//! The text is a **sibling's** real stock description, never invented prose:
//!
//! 1. **siblings**: entities of the same `AttachDef` type sharing the entity's
//!    [`variant_base`] (the base entity itself first, then by record name), whose
//!    description key has real stock text. The first whose text survives the colour cut
//!    below gives it.
//! 2. **edition**: a record name with an edition segment (`…_01_01_iae2022`,
//!    `…_01_iae2022_01`, `…_01_01_scientist`) and no usable sibling borrows from its model
//!    and the model's plain variants (`…_01_01`, `…_01_01_NN`, `…_01_NN_NN`), but only when
//!    every one of them that survives the cut gives the same text.
//!
//! **Colour cut**: the text is split into paragraphs at `\n\n`; a last paragraph that names a
//! variant (`The "Lumen" edition is matte white.`, `The Grime version …`, `This version
//! features …`, `The Packrat color scheme …`: "edition", "version", "variant", "color
//! scheme" or "colorway" within its first six words, after `The` / `This`) is dropped. The
//! rest is used only when it still has prose (not only `Label: value` lines) and does not
//! name the sibling's own colour: a word of the sibling's name that the entity's name lacks
//! (`Bannock Jumpsuit Blue` for `Bannock Jumpsuit Green`: no `blue`). Raw control characters
//! are dropped from the copy.
//!
//! A key is filled only when every visible, named entity that references it gets the same
//! text, it is no sentinel and no entity uses it as its name. A placeholder stock line is
//! renamed ([`Engine::rename`]), a missing key added ([`Engine::add`]); either way the
//! description generators (stat blocks, WHERE TO GET) then build on the filled text as they
//! do on stock ([`Engine::fill`]), so this runs before them.

use super::names::{HIDDEN_TYPES, Resolver, is_variant_segment, variant_base};
use super::{Category, Engine, is_placeholder_text, is_sentinel_key, trim_tail};
use crate::ini::{IniError, StockIni, fold_key, is_addable_key};
use crate::model::{GameData, ItemName, loc_key};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Rule name of a filled description, as [`super::AddedKey::rule`] reports it.
pub const RULE_SIBLING: &str = "sibling-description";

/// Why a key with a missing or placeholder description was left alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Skip {
    /// No sibling (and no edition model) has a real stock description.
    NoSibling,
    /// Every described sibling's text names the sibling's own colour, or is only stat lines
    /// once the colour paragraph is cut.
    Colour,
    /// Entities referencing the key (or an edition's model variants) would get different
    /// text.
    Differ,
    /// The key is also some entity's name key.
    NameKey,
}

impl Skip {
    pub fn name(self) -> &'static str {
        match self {
            Skip::NoSibling => "no-sibling",
            Skip::Colour => "colour",
            Skip::Differ => "differ",
            Skip::NameKey => "name-key",
        }
    }
}

/// One description to fill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fill {
    /// The key as referenced (no `@`).
    pub key: String,
    /// The entity whose description it is (the lowest record name).
    pub record: String,
    /// Every visible, named entity that references the key, by record name.
    pub records: Vec<String>,
    /// The sibling the text was copied from.
    pub sibling: String,
    pub value: String,
    /// Stock has the key (with a placeholder value): the line is renamed, not added.
    pub in_stock: bool,
}

/// What [`plan`] found, by folded key.
#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub fills: BTreeMap<String, Fill>,
    /// Keys left alone, with the reason and the entities referencing them.
    pub skipped: BTreeMap<String, (Skip, Vec<String>)>,
}

/// The stock description of a reference, when it is real text (not missing, empty, a
/// placeholder or a shared sentinel).
fn real_desc<'s>(stock: &'s StockIni, reference: Option<&str>) -> Option<&'s str> {
    let k = loc_key(reference?)?;
    if is_sentinel_key(k) {
        return None;
    }
    let v = stock.value_at(stock.find(k).ok()?)?;
    (!v.trim().is_empty() && !bad_text(v)).then_some(v)
}

/// Placeholder text or a raw `@key` (not an empty value: an item may have a blank one).
fn bad_text(v: &str) -> bool {
    is_placeholder_text(v) || v.trim().starts_with('@')
}

/// A description key whose stock text is missing or a placeholder: `Some(in stock)`.
fn bad_desc(stock: &StockIni, key: &str) -> Option<bool> {
    match stock.find(key) {
        Err(IniError::UnknownKey(_)) => Some(false),
        Err(_) => None,
        Ok(line) => bad_text(stock.value_at(line).unwrap_or_default()).then_some(true),
    }
}

/// Lower-case alphabetic words of a name.
fn words(name: &str) -> HashSet<String> {
    name.split(|c: char| !c.is_alphabetic())
        .filter(|w| w.chars().count() >= 3)
        .map(str::to_lowercase)
        .collect()
}

/// Does `text` hold `word` (lower case) as a whole word, ignoring case?
fn has_word(text: &str, word: &str) -> bool {
    let lower = text.to_lowercase();
    lower.split(|c: char| !c.is_alphabetic()).any(|w| w == word)
}

/// A paragraph naming a variant: `The "Lumen" edition is matte white.`, `The Grime version
/// is dyed …`, `This version features …`, `The Packrat color scheme …`.
pub fn is_variant_line(p: &str) -> bool {
    let lower = p.trim().to_lowercase();
    if !(lower.starts_with("the ") || lower.starts_with("this ")) {
        return false;
    }
    let head: Vec<&str> = lower.split_whitespace().take(6).collect();
    let head = head.join(" ");
    let marks = [
        " edition",
        " version",
        " variant",
        " color scheme",
        " colour scheme",
        " colorway",
        " colourway",
    ];
    marks.iter().any(|m| head.contains(m))
}

/// `Label: value` (a stat line), not prose.
fn is_stat_line(line: &str) -> bool {
    match line.split_once(':') {
        Some((label, _)) => label.chars().count() <= 40 && !label.contains(['.', '!', '?']),
        None => false,
    }
}

/// The sibling's text for the entity: control characters dropped, a variant last paragraph
/// cut, trailing `\n` escapes and spaces trimmed; `None` when no prose is left or it names a
/// word of `sibling_name` that `name` lacks (the sibling's colour).
pub fn cut(text: &str, sibling_name: &str, name: &str) -> Option<String> {
    let clean: String = text.chars().filter(|c| !c.is_control()).collect();
    let mut paras: Vec<&str> = clean.split("\\n\\n").collect();
    if paras.len() > 1 && paras.last().is_some_and(|p| is_variant_line(trim_tail(p))) {
        paras.pop();
    }
    let out = trim_tail(paras.join("\\n\\n").trim()).to_string();
    let prose = out
        .split("\\n")
        .any(|l| !l.trim().is_empty() && !is_stat_line(l));
    if !prose {
        return None;
    }
    let own = words(name);
    let colour = words(sibling_name)
        .into_iter()
        .any(|w| !own.contains(&w) && has_word(&out, &w));
    (!colour).then_some(out)
}

/// The model of a record name with an edition segment: `x_01_01_iae2022` -> `x_01_01`,
/// `x_01_iae2022_01` -> `x_01`. The model ends in a number; the edition is no variant.
pub fn edition_model(class: &str) -> Option<&str> {
    let (head, last) = class.rsplit_once('_')?;
    let (head, edition) = if is_variant_segment(last) {
        head.rsplit_once('_')?
    } else {
        (head, last)
    };
    if is_variant_segment(edition) {
        return None;
    }
    let (_, number) = head.rsplit_once('_')?;
    is_variant_segment(number).then_some(head)
}

/// The model itself or a plain variant of it: `model`, `model_NN`, `model_NN_NN`.
fn is_model_variant(class: &str, model: &str) -> bool {
    if class == model {
        return true;
    }
    let Some(rest) = class.strip_prefix(model).and_then(|r| r.strip_prefix('_')) else {
        return false;
    };
    let segs: Vec<&str> = rest.split('_').collect();
    segs.len() <= 2 && segs.iter().all(|s| is_variant_segment(s))
}

/// Exactly one distinct value, or none.
fn only(mut it: impl Iterator<Item = String>) -> Option<String> {
    let first = it.next()?;
    it.all(|x| x == first).then_some(first)
}

/// A described entity: (record, display name, stock description).
type Described<'a> = (&'a str, &'a str, &'a str);

/// (sibling record, text) for one entity, or why there is none.
type Outcome = Result<(String, String), Skip>;

/// Folded key -> (key, the entities referencing it with their outcome).
type Keys<'a> = BTreeMap<String, (&'a str, Vec<(&'a str, Outcome)>)>;

/// Descriptions to fill (and keys left alone) for every visible, named entity whose
/// description key stock lacks or holds as a placeholder.
pub fn plan(gd: &GameData, stock: &StockIni, names: &Resolver) -> Plan {
    let name_of = |n: &ItemName| names.name(Some(&n.name));
    // (type, variant base) -> described members; the bare base counts as its own member.
    let mut by_base: HashMap<(&str, &str), Vec<Described>> = HashMap::new();
    let mut by_kind: HashMap<&str, Vec<Described>> = HashMap::new();
    let mut name_keys: HashSet<String> = HashSet::new();
    for n in &gd.item_names {
        if let Some(k) = loc_key(&n.name) {
            name_keys.insert(fold_key(k));
        }
        let (Some(name), Some(desc)) = (name_of(n), real_desc(stock, n.desc.as_deref())) else {
            continue;
        };
        let d = (n.class.as_str(), name, desc);
        if let Some(base) = variant_base(&n.class) {
            by_base.entry((&n.kind, base)).or_default().push(d);
        }
        by_base.entry((&n.kind, &n.class)).or_default().push(d);
        by_kind.entry(&n.kind).or_default().push(d);
    }
    for v in by_base.values_mut() {
        v.sort_by_key(|d| d.0);
    }
    let mut keys: Keys = BTreeMap::new();
    for n in &gd.item_names {
        if HIDDEN_TYPES.contains(&n.kind.as_str()) {
            continue;
        }
        let Some(name) = name_of(n) else { continue };
        let Some(k) = n.desc.as_deref().and_then(loc_key) else {
            continue;
        };
        if is_sentinel_key(k) || !is_addable_key(k) || bad_desc(stock, k).is_none() {
            continue;
        }
        let text = sibling_text(n, name, &by_base, &by_kind);
        keys.entry(fold_key(k))
            .or_insert((k, Vec::new()))
            .1
            .push((n.class.as_str(), text));
    }
    let mut out = Plan::default();
    for (folded, (key, mut ents)) in keys {
        ents.sort_by_key(|e| e.0);
        let records: Vec<String> = ents.iter().map(|e| e.0.to_string()).collect();
        let skip = |reason| (reason, records.clone());
        if name_keys.contains(&folded) {
            out.skipped.insert(folded, skip(Skip::NameKey));
            continue;
        }
        if let Some(Err(why)) = ents.iter().map(|e| e.1.clone()).find(|t| t.is_err()) {
            out.skipped.insert(folded, skip(why));
            continue;
        }
        let texts = ents.iter().filter_map(|e| e.1.as_ref().ok());
        let Some(value) = only(texts.clone().map(|t| t.1.clone())) else {
            out.skipped.insert(folded, skip(Skip::Differ));
            continue;
        };
        let sibling = texts.map(|t| t.0.clone()).next().unwrap_or_default();
        out.fills.insert(
            folded,
            Fill {
                key: key.to_string(),
                record: ents[0].0.to_string(),
                records,
                sibling,
                value,
                in_stock: bad_desc(stock, key) == Some(true),
            },
        );
    }
    out
}

/// (sibling record, text) for one entity, or why there is none.
fn sibling_text(
    n: &ItemName,
    name: &str,
    by_base: &HashMap<(&str, &str), Vec<Described>>,
    by_kind: &HashMap<&str, Vec<Described>>,
) -> Outcome {
    let mut seen = false;
    if let Some(base) = variant_base(&n.class)
        && let Some(members) = by_base.get(&(n.kind.as_str(), base))
    {
        // The base entity first, then by record name.
        let mut order: Vec<&Described> = members.iter().filter(|d| d.0 != n.class).collect();
        order.sort_by_key(|d| (d.0 != base, d.0));
        for (class, sib_name, text) in order {
            seen = true;
            if let Some(t) = cut(text, sib_name, name) {
                return Ok((class.to_string(), t));
            }
        }
    }
    if let Some(model) = edition_model(&n.class) {
        let members: Vec<&Described> = by_kind
            .get(n.kind.as_str())
            .into_iter()
            .flatten()
            .filter(|d| is_model_variant(d.0, model))
            .collect();
        let mut texts: BTreeMap<&str, String> = BTreeMap::new();
        for (class, sib_name, text) in &members {
            seen = true;
            if let Some(t) = cut(text, sib_name, name) {
                texts.insert(class, t);
            }
        }
        if let Some((class, _)) = texts.first_key_value() {
            let class = class.to_string();
            return match only(texts.into_values()) {
                Some(t) => Ok((class, t)),
                None => Err(Skip::Differ),
            };
        }
    }
    Err(if seen { Skip::Colour } else { Skip::NoSibling })
}

/// Fill every planned description ([`Engine::fill`]). Runs before the description
/// generators, which then build on the filled text.
pub fn generate(en: &mut Engine, gd: &GameData, names: &Resolver) {
    en.cov(Category::MissingNames);
    let p = plan(gd, en.stock, names);
    for f in p.fills.values() {
        en.fill(
            Category::MissingNames,
            &f.key,
            Some(f.record.clone()),
            &f.value,
            RULE_SIBLING,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enhance::names::Curated;
    use crate::enhance::profile::Profile;
    use crate::enhance::{Payload, Reason, check};
    use crate::ini::BOM;

    #[test]
    fn variant_lines() {
        for p in [
            "The \"Lumen\" edition is matte white.",
            "The AA Defense edition features a gray, urban camo pattern.",
            "The Golden Blossom version is mostly black.",
            "This version features a gray finish with a zigzag pattern.",
            "The Packrat color scheme complements a wide range of light armour types.",
            "The Poacher variant combines rust red and gray-brown tones.",
            "the cfp edition is a deep fjord blue",
        ] {
            assert!(is_variant_line(p), "{p}");
        }
        for p in [
            // The keyword too far in: body text, not a variant line.
            "The Omnisky IX is the size three version of Amon & Reese's autocannon.",
            "Women's sizes currently out of stock.",
            "A lustrous orange coating makes the Igniter edition colorful.",
            "",
        ] {
            assert!(!is_variant_line(p), "{p}");
        }
    }

    #[test]
    fn the_colour_paragraph_is_cut() {
        let stock = "Carrying Capacity: 1K µSCU\\n\\nA sturdy jacket.\\n\\nThe CFP edition is a deep fjord blue.\\n";
        assert_eq!(
            cut(stock, "Silverton Jacket CFP", "Silverton Jacket Red").as_deref(),
            Some("Carrying Capacity: 1K µSCU\\n\\nA sturdy jacket.")
        );
        // A raw CR in CIG's text and trailing escapes do not come along.
        assert_eq!(
            cut("A sturdy\r jacket. \\n", "Jacket", "Jacket Red").as_deref(),
            Some("A sturdy jacket.")
        );
        // One paragraph: kept whole.
        assert_eq!(
            cut("The X edition is red.", "X", "X Blue").as_deref(),
            Some("The X edition is red.")
        );
    }

    #[test]
    fn text_naming_the_siblings_colour_or_only_stats_is_skipped() {
        // The sibling's own colour word (not in the entity's name) is in the text.
        assert_eq!(
            cut(
                "A blue jumpsuit.",
                "Bannock Jumpsuit Blue",
                "Bannock Jumpsuit Green"
            ),
            None
        );
        assert_eq!(
            cut("A Blue-ish jumpsuit.", "Jumpsuit Blue", "Jumpsuit Green"),
            None
        );
        // A word inside another word is no match.
        assert_eq!(
            cut("A blueprint jumpsuit.", "Jumpsuit Blue", "Jumpsuit Green").as_deref(),
            Some("A blueprint jumpsuit.")
        );
        // Only stat lines left after the cut.
        assert_eq!(
            cut(
                "Item Type: Jacket\\nCarrying Capacity: 1K\\n\\nThe Red edition is red.",
                "Jacket",
                "Jacket Blue"
            ),
            None
        );
    }

    #[test]
    fn edition_models() {
        assert_eq!(
            edition_model("cds_undersuit_01_01_iae2022"),
            Some("cds_undersuit_01_01")
        );
        assert_eq!(
            edition_model("cds_armor_heavy_core_01_iae2022_01"),
            Some("cds_armor_heavy_core_01")
        );
        assert_eq!(
            edition_model("fio_pants_01_01_scientist"),
            Some("fio_pants_01_01")
        );
        assert_eq!(edition_model("alb_jumpsuit_01_01_04"), None);
        assert_eq!(edition_model("box_iae2022"), None);
        assert!(is_model_variant("x_01_01_02", "x_01"));
        assert!(is_model_variant("x_01", "x_01"));
        assert!(!is_model_variant("x_01_iae2022_01", "x_01"));
        assert!(!is_model_variant("x_010", "x_01"));
    }

    fn named(class: &str, name: &str, desc: &str, kind: &str) -> ItemName {
        ItemName {
            class: class.into(),
            name: name.into(),
            desc: Some(desc.into()),
            kind: kind.into(),
        }
    }

    fn stock() -> StockIni {
        let mut v = BOM.to_vec();
        v.extend_from_slice(
            "item_Name_js_01=Bannock Jumpsuit Warm Ash\r\n\
             item_Name_js_04=Bannock Jumpsuit Green\r\n\
             item_Desc_js_01=Capacity: 1K\\n\\nA comfy jumpsuit.\\n\\nThe Ash edition is grey.\r\n\
             item_Name_mask=Katla Mask\r\n\
             item_Name_mask_01=Katla Mask Red\r\n\
             item_Name_mask_04=Katla Mask Yellow\r\n\
             item_Desc_mask=A mask.\r\n\
             item_Desc_mask_01=A mask, in red.\r\n\
             item_Name_shirt_01=Shirt Blue\r\n\
             item_Name_shirt_02=Shirt Green\r\n\
             item_Desc_shirt_01=A blue shirt.\r\n\
             item_Name_lone_02=Lone Hat\r\n\
             item_Name_pants_01=Pants\r\n\
             item_Name_pants_02=Pants Red\r\n\
             item_Desc_pants_01=Good pants.\r\n\
             item_Desc_pants_02,P=PLACEHOLDER - pants_02\r\n\
             item_Name_suit_01_01_01=Suit\r\n\
             item_Name_suit_01_01_02=Suit Black\r\n\
             item_Name_suit_ed=Suit Red Alert\r\n\
             item_Desc_suit=A sealed suit.\r\n\
             item_Name_arm_01_01_01=Arms\r\n\
             item_Name_arm_01_02_01=Arms Fulcrum\r\n\
             item_Name_arm_ed=Arms Red Alert\r\n\
             item_Desc_arm_a=Arms with a helmet.\r\n\
             item_Desc_arm_b=Arms without one.\r\n\
             item_Name_door=Door\r\n"
                .as_bytes(),
        );
        StockIni::parse(v)
    }

    fn data() -> GameData {
        let c = "Char_Clothing_Torso_1";
        GameData {
            item_names: vec![
                named("js_01_01_01", "@item_Name_js_01", "@item_Desc_js_01", c),
                named("js_01_01_04", "@item_Name_js_04", "@item_Desc_js_04", c),
                // The base entity comes first, before a lower record name.
                named("mask_01", "@item_Name_mask", "@item_Desc_mask", c),
                named("mask_01_00", "@item_Name_mask_01", "@item_Desc_mask_01", c),
                named("mask_01_04", "@item_Name_mask_04", "@item_Desc_mask_04", c),
                // The only sibling names its colour.
                named(
                    "shirt_01_01",
                    "@item_Name_shirt_01",
                    "@item_Desc_shirt_01",
                    c,
                ),
                named(
                    "shirt_01_02",
                    "@item_Name_shirt_02",
                    "@item_Desc_shirt_02",
                    c,
                ),
                // No described sibling (another type does not count).
                named("lone_01_02", "@item_Name_lone_02", "@item_Desc_lone_02", c),
                named(
                    "lone_01_01",
                    "@item_Name_lone_02",
                    "@item_Desc_js_01",
                    "Other",
                ),
                // A placeholder stock description: renamed.
                named(
                    "pants_01_01",
                    "@item_Name_pants_01",
                    "@item_Desc_pants_01",
                    c,
                ),
                named(
                    "pants_01_02",
                    "@item_Name_pants_02",
                    "@item_Desc_pants_02",
                    c,
                ),
                // An edition: its model's variants agree.
                named(
                    "suit_01_01_01",
                    "@item_Name_suit_01_01_01",
                    "@item_Desc_suit",
                    c,
                ),
                named(
                    "suit_01_01_02",
                    "@item_Name_suit_01_01_02",
                    "@item_Desc_suit",
                    c,
                ),
                named(
                    "suit_01_01_iae2022",
                    "@item_Name_suit_ed",
                    "@item_Desc_suit_ed",
                    c,
                ),
                // An edition whose model's variants differ.
                named(
                    "arm_01_01_01",
                    "@item_Name_arm_01_01_01",
                    "@item_Desc_arm_a",
                    c,
                ),
                named(
                    "arm_01_02_01",
                    "@item_Name_arm_01_02_01",
                    "@item_Desc_arm_b",
                    c,
                ),
                named(
                    "arm_01_iae2022_01",
                    "@item_Name_arm_ed",
                    "@item_Desc_arm_ed",
                    c,
                ),
                // A description key some entity uses as its name.
                named("js_01_01_05", "@item_Name_js_04", "@item_Desc_js_05", c),
                named("js_01_01_06", "@item_Desc_js_05", "@item_Desc_js_01", c),
                // Hidden types and sentinels are left alone.
                named(
                    "door_01_02",
                    "@item_Name_door",
                    "@item_Desc_door_02",
                    "Door",
                ),
                named("js_01_01_07", "@item_Name_js_04", "@LOC_PLACEHOLDER", c),
            ],
            ..GameData::default()
        }
    }

    fn plan_of(gd: &GameData, s: &StockIni) -> Plan {
        let curated = Curated::new();
        plan(gd, s, &Resolver::new(gd, s, &curated, true))
    }

    #[test]
    fn siblings_fill_and_skips() {
        let s = stock();
        let p = plan_of(&data(), &s);
        let fills: Vec<(&str, &str, &str, bool)> = p
            .fills
            .values()
            .map(|f| {
                (
                    f.key.as_str(),
                    f.sibling.as_str(),
                    f.value.as_str(),
                    f.in_stock,
                )
            })
            .collect();
        assert_eq!(
            fills,
            vec![
                (
                    "item_Desc_js_04",
                    "js_01_01_01",
                    "Capacity: 1K\\n\\nA comfy jumpsuit.",
                    false
                ),
                ("item_Desc_mask_04", "mask_01", "A mask.", false),
                ("item_Desc_pants_02", "pants_01_01", "Good pants.", true),
                (
                    "item_Desc_suit_ed",
                    "suit_01_01_01",
                    "A sealed suit.",
                    false
                ),
            ]
        );
        let skipped: Vec<(&str, Skip)> = p
            .skipped
            .values()
            .map(|(why, r)| (r[0].as_str(), *why))
            .collect();
        assert_eq!(
            skipped,
            vec![
                ("arm_01_iae2022_01", Skip::Differ),
                ("js_01_01_05", Skip::NameKey),
                ("lone_01_02", Skip::NoSibling),
                ("shirt_01_02", Skip::Colour),
            ]
        );
    }

    #[test]
    fn a_shared_key_needs_one_text() {
        let s = stock();
        let mut gd = data();
        // A second user of the jumpsuit's missing key whose sibling text differs.
        gd.item_names.push(named(
            "mask_01_05",
            "@item_Name_mask_04",
            "@item_Desc_js_04",
            "Char_Clothing_Torso_1",
        ));
        let p = plan_of(&gd, &s);
        assert!(!p.fills.contains_key(&fold_key("item_Desc_js_04")));
        assert_eq!(p.skipped[&fold_key("item_Desc_js_04")].0, Skip::Differ);
    }

    #[test]
    fn fills_are_written_and_rendered() {
        let s = stock();
        let gd = data();
        let mut prof = Profile::off();
        prof.missing_names = true;
        let g = crate::enhance::generate(&gd, &s, &prof, &crate::enhance::owned::NoneOwned);
        let a = &g.added[&fold_key("item_Desc_js_04")];
        assert_eq!(
            (a.value.as_str(), a.rule),
            ("Capacity: 1K\\n\\nA comfy jumpsuit.", RULE_SIBLING)
        );
        assert_eq!(check::check_added(&a.value), Ok(()));
        // The placeholder line is renamed, the stock text replaced.
        let line = s.find("item_Desc_pants_02").unwrap();
        let e = &g.edits[&line];
        assert_eq!(
            (e.value("PLACEHOLDER - pants_02"), e.rule),
            ("Good pants.".to_string(), Some(RULE_SIBLING))
        );
        let bytes = crate::enhance::render(&s, &g).expect("whole-file rules pass");
        let out = StockIni::parse(bytes);
        assert_eq!(out.value("item_Desc_pants_02"), Some("Good pants."));
        assert_eq!(out.value("item_Desc_mask_04"), Some("A mask."));
        assert_eq!(out.value("item_Desc_lone_02"), None);
        // Off: nothing filled.
        let g =
            crate::enhance::generate(&gd, &s, &Profile::off(), &crate::enhance::owned::NoneOwned);
        assert!(g.added.is_empty() && g.edits.is_empty());
    }

    /// Stats and WHERE TO GET build on a filled description as on stock: the first owns it,
    /// an extending category appends, any other is `owned-by-other`.
    #[test]
    fn engine_builds_on_fills() {
        let s = stock();
        let prof = Profile::default();
        let mut en = Engine::new(&s, &prof);
        let cat = Category::MissingNames;
        assert!(en.fill(cat, "@item_Desc_new", None, "A new item.", RULE_SIBLING));
        assert!(en.is_filled("item_desc_new"));
        assert!(en.fill(cat, "item_Desc_pants_02", None, "Good pants.", RULE_SIBLING));
        // A real stock description is never filled; a bad value is dropped.
        assert!(!en.fill(cat, "item_Desc_pants_01", None, "X", RULE_SIBLING));
        assert!(!en.fill(cat, "item_Desc_bad", None, "See @item_Desc_x", RULE_SIBLING));
        let stats = Payload::Block(vec!["Damage: 10".into()]);
        let obtain = Payload::Append("\\n\\n<EM3>WHERE TO GET</EM3>\\nLoot".into());
        for key in ["item_Desc_new", "item_Desc_pants_02"] {
            assert!(en.offer(Category::FpsWeapons, key, None, &stats), "{key}");
            assert!(!en.offer(Category::Components, key, None, &stats), "{key}");
            assert!(en.offer(Category::ObtainHints, key, None, &obtain), "{key}");
            assert!(
                !en.offer(Category::ObtainHints, key, None, &obtain),
                "{key}"
            );
        }
        let h = prof.header.clone();
        let want = |base: &str| {
            format!("{base}\\n\\n{h}\\nDamage: 10\\n\\n<EM3>WHERE TO GET</EM3>\\nLoot")
        };
        let a = &en.out.added[&fold_key("item_Desc_new")];
        assert_eq!(a.value, want("A new item."));
        assert_eq!(a.extended_by, [Category::FpsWeapons, Category::ObtainHints]);
        let e = &en.out.edits[&s.find("item_Desc_pants_02").unwrap()];
        assert_eq!(e.value("PLACEHOLDER - pants_02"), want("Good pants."));
        assert_eq!(e.generated_part(), want("Good pants."));
        // An extension that breaks a rule is dropped; the fill and the stats stay. (Obtain
        // hints already extended it, so a second extension is tried with a raw key.)
        let mut en2 = Engine::new(&s, &prof);
        assert!(en2.fill(cat, "item_Desc_new", None, "A new item.", RULE_SIBLING));
        assert!(!en2.offer(
            Category::ObtainHints,
            "item_Desc_new",
            None,
            &Payload::Append(" @item_Name_x".into())
        ));
        assert_eq!(
            en2.out.added[&fold_key("item_Desc_new")].value,
            "A new item."
        );
        assert_eq!(
            en2.out
                .coverage
                .get(Category::ObtainHints)
                .left_for(Reason::DroppedByValidation),
            1
        );
        let c = en.out.coverage.get(cat);
        assert_eq!((c.candidates, c.written), (4, 2));
        assert_eq!(c.left_for(Reason::InStock), 1);
        assert_eq!(c.left_for(Reason::DroppedByValidation), 1);
        let g = en.out;
        assert_eq!(g.count(Category::FpsWeapons), 2);
        assert_eq!(g.count(Category::ObtainHints), 2);
        assert_eq!(g.count(cat), 2);
        crate::enhance::render(&s, &g).expect("whole-file rules pass");
        // Merge keeps the same values.
        let m = crate::merge::merge(
            &s,
            &GameData::default(),
            None,
            &crate::merge::gen_edits(&g),
            &crate::patch::PatchSet::default(),
            &[],
        );
        assert_eq!(m.counts.dropped, 0);
        assert_eq!(m.edits.added_len(), 1);
    }
}

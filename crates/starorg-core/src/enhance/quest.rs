//! Quest item hints (addendum §12): `Used in: <contract> · <system>` after the description
//! of an item a mission spawns or a contract asks for.
//!
//! Only real record references feed it ([`crate::model::quest`]); a loc key that merely
//! looks like a mission's is never enough. An item is left alone when:
//!
//! - its name is a placeholder (template or test record), or it is a general good: food,
//!   drink, currency, armour, weapons, ship parts, harvestables and trophies (a contract
//!   asking for ore does not make ore a quest item);
//! - every use is unreleased, comes from Wikelo's trade contracts (`TheCollector`, which
//!   ask for ordinary items), or has a title that is not plain text in stock (a runtime
//!   `~mission(...)` title, a placeholder);
//! - its description key is shared with an entity of its folder that is not a variant of
//!   it (`_debug`, `_1_a`/`_2_a`): the hint would show on unrelated items.
//!
//! Titles come from the stock file (never our own output), so no tag we add elsewhere can
//! end up in a hint; EM tags of the stock title are dropped so the hint cannot nest them.

use super::{Category, Claim, Engine, Payload, profile};
use crate::model::{GameData, QuestItem, is_placeholder_name, loc_key};
use std::collections::BTreeMap;

/// Contract generators whose hauling orders are trades, not quests.
const TRADE_GENERATORS: &[&str] = &["TheCollector"];

/// `AttachDef.Type`s that are quest-item shaped: chips and cards, carried mission objects,
/// containers. Everything else (weapons, armour, food, components, currency) is a general
/// good some contract happens to ask for.
const QUEST_TYPES: &[&str] = &[
    "RemovableChip",
    "FPS_Consumable",
    "Cargo",
    "Misc",
    "InventoryContainer",
];

/// Sub-types of the types above that are still general goods.
const GOODS_SUB_TYPES: &[&str] = &["Harvestable", "Trophy"];

/// Titles listed before `and N more`.
const MAX_TITLES: usize = 3;

/// The record name a variant shares with its siblings: lower case, without a trailing
/// `_debug` and a trailing `_<n>_<letter>` / `_<letter>` / `_<n>` variant suffix.
fn family(short: &str) -> String {
    let mut s = short.to_ascii_lowercase();
    if let Some(x) = s.strip_suffix("_debug") {
        s = x.to_string();
    }
    let parts: Vec<&str> = s.split('_').collect();
    let is_num = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
    let is_letter = |p: &str| p.len() == 1 && p.bytes().all(|b| b.is_ascii_lowercase());
    let mut n = parts.len();
    if n > 2 && is_letter(parts[n - 1]) && is_num(parts[n - 2]) {
        n -= 2;
    } else if n > 1 && (is_letter(parts[n - 1]) || is_num(parts[n - 1])) {
        n -= 1;
    }
    parts[..n].join("_")
}

/// Is `q` the kind of item a hint is for (type, name, shared key)?
fn is_quest_item(q: &QuestItem, stock: &crate::ini::StockIni) -> bool {
    let quest_type = q
        .item_type
        .as_deref()
        .is_some_and(|t| QUEST_TYPES.contains(&t))
        && !q
            .sub_type
            .as_deref()
            .is_some_and(|t| GOODS_SUB_TYPES.contains(&t));
    if !quest_type {
        return false;
    }
    if super::stock_name(stock, q.loc.name.as_deref()).is_none() {
        return false;
    }
    let own = family(q.short_name());
    q.desc_shared_with.iter().all(|o| family(o) == own)
}

/// Stock title text, plain: EM tags dropped, `None` unless it is real text.
fn plain_title(stock: &crate::ini::StockIni, key: Option<&str>) -> Option<String> {
    let v = super::trim_tail(stock.value(loc_key(key?)?)?).trim();
    if v.is_empty()
        || is_placeholder_name(Some(v))
        || v.contains('~')
        || v.contains("\\n")
        || super::check::is_format_value(v)
    {
        return None;
    }
    let mut out = String::with_capacity(v.len());
    let mut rest = v;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let tag = tail
            .strip_prefix("</EM")
            .or_else(|| tail.strip_prefix("<EM"))
            .and_then(|t| t.find('>').filter(|&j| j <= 2).map(|j| &t[j + 1..]));
        match tag {
            Some(after) => rest = after,
            None => {
                out.push('<');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    let out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    (!out.is_empty()).then_some(out)
}

/// The hint for one item, `None` when no use survives.
pub fn hint(q: &QuestItem, stock: &crate::ini::StockIni, label_em: u8) -> Option<String> {
    let mut titles: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for u in &q.uses {
        if u.unreleased
            || TRADE_GENERATORS
                .iter()
                .any(|g| u.source.eq_ignore_ascii_case(g))
        {
            continue;
        }
        let Some(t) = plain_title(stock, u.title_key.as_deref()) else {
            continue;
        };
        let systems = titles.entry(t).or_default();
        for s in &u.systems {
            if !systems.contains(s) {
                systems.push(s.clone());
            }
        }
    }
    if titles.is_empty() {
        return None;
    }
    let line = |t: &str, sys: &[String]| {
        let mut sys = sys.to_vec();
        sys.sort();
        if sys.is_empty() {
            t.to_string()
        } else {
            format!("{t} · {}", sys.join(", "))
        }
    };
    let label = profile::em(label_em, "Used in:");
    if titles.len() == 1 {
        let (t, sys) = titles.iter().next()?;
        return Some(format!("{label} {}", line(t, sys)));
    }
    let mut out = vec![label];
    for (t, sys) in titles.iter().take(MAX_TITLES) {
        out.push(format!("- {}", line(t, sys)));
    }
    if titles.len() > MAX_TITLES {
        out.push(format!("- and {} more", titles.len() - MAX_TITLES));
    }
    Some(out.join("\\n"))
}

/// Offer a hint for every quest item; the namesake rule decides shared keys.
pub fn generate(en: &mut Engine, gd: &GameData) {
    let stock = en.stock;
    let label_em = en.profile.mission.label_em;
    let claims: Vec<Claim> = gd
        .quest_items
        .iter()
        .filter(|q| is_quest_item(q, stock))
        .filter_map(|q| {
            let text = hint(q, stock, label_em)?;
            Some(Claim {
                record: q.short_name().to_string(),
                key: q.loc.desc.clone()?,
                payload: Some(Payload::Append(format!("\\n\\n{text}"))),
                placeholder: false,
                player: true,
            })
        })
        .collect();
    en.assign(Category::QuestHints, claims);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ini::StockIni;
    use crate::model::{LocKeys, QuestRef, QuestUse};

    fn stock() -> StockIni {
        StockIni::parse(
            "\u{feff}t_one=Overdrive Initiative: INTEL RAID\r\nt_var=~mission(Contractor|Title)\r\nt_em=<EM4>Relief</EM4>   Order\r\nn=Mainframe Access Card\r\nd=An access card.\r\n"
                .as_bytes()
                .to_vec(),
        )
    }

    fn used(source: &str, title: &str, systems: &[&str]) -> QuestUse {
        QuestUse {
            kind: QuestRef::Hauled,
            source: source.into(),
            mission: "m".into(),
            title_key: Some(format!("@{title}")),
            systems: systems.iter().map(|s| s.to_string()).collect(),
            unreleased: false,
        }
    }

    fn item(uses: Vec<QuestUse>) -> QuestItem {
        QuestItem {
            record_name: "EntityClassDefinition.none_consumable_hackingchip_DataHeist_ID".into(),
            loc: LocKeys {
                name: Some("@n".into()),
                short_name: None,
                desc: Some("@d".into()),
            },
            item_type: Some("RemovableChip".into()),
            uses,
            ..Default::default()
        }
    }

    #[test]
    fn families() {
        assert_eq!(
            family("none_consumable_hackingchip_01_debug"),
            "none_consumable_hackingchip"
        );
        assert_eq!(
            family("none_consumable_hackingchip_01"),
            "none_consumable_hackingchip"
        );
        assert_eq!(
            family("Carryable_2H_FL_MissionItem_prototype_ship_component_1_a"),
            family("Carryable_2H_FL_MissionItem_prototype_ship_component_2_a")
        );
        assert_ne!(
            family("Carryable_1H_SQ_datapad_rugged_Consignment_Boss"),
            family("Carryable_1H_SQ_datapad_rugged_Fleetweek_CommanderDatapad")
        );
    }

    #[test]
    fn hint_text() {
        let s = stock();
        let q = item(vec![
            used("MissionBrokerEntry", "t_one", &[]),
            used("MissionBrokerEntry", "t_var", &["Stanton"]),
        ]);
        assert_eq!(
            hint(&q, &s, 4).as_deref(),
            Some("<EM4>Used in:</EM4> Overdrive Initiative: INTEL RAID")
        );
        let q = item(vec![
            used("Gen", "t_em", &["Stanton"]),
            used("Gen", "t_em", &["Pyro", "Stanton"]),
            used("Gen", "t_one", &[]),
        ]);
        assert_eq!(
            hint(&q, &s, 0).as_deref(),
            Some("Used in:\\n- Overdrive Initiative: INTEL RAID\\n- Relief Order · Pyro, Stanton")
        );
        // Trades, unreleased uses and runtime titles say nothing.
        let mut u = used("Gen", "t_one", &[]);
        u.unreleased = true;
        let q = item(vec![
            u,
            used("TheCollector", "t_one", &[]),
            used("Gen", "t_var", &[]),
        ]);
        assert_eq!(hint(&q, &s, 4), None);
    }

    #[test]
    fn general_goods_and_shared_keys_are_skipped() {
        let s = stock();
        let q = item(vec![used("Gen", "t_one", &[])]);
        assert!(is_quest_item(&q, &s));
        let mut food = q.clone();
        food.item_type = Some("Food".into());
        assert!(!is_quest_item(&food, &s));
        let mut ore = q.clone();
        ore.item_type = Some("Misc".into());
        ore.sub_type = Some("Harvestable".into());
        assert!(!is_quest_item(&ore, &s));
        let mut shared = q.clone();
        shared.desc_shared_with = vec!["Carryable_1H_SQ_datapad_rugged_Other".into()];
        assert!(!is_quest_item(&shared, &s));
        let mut variant = q.clone();
        variant.desc_shared_with = vec!["none_consumable_hackingchip_DataHeist_ID_debug".into()];
        assert!(is_quest_item(&variant, &s));
        let mut placeholder = q;
        placeholder.loc.name = Some("@missing".into());
        assert!(!is_quest_item(&placeholder, &s));
    }
}

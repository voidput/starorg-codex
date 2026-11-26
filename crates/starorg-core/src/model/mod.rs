//! `GameData`: the normalised model generators read (spec 3.4).
//!
//! Local extraction from `Data.p4k` (`starorg-data`) fills it. The record shapes follow the
//! JSON schema of the published dumps of `voidput/sc-gamedata-dumps` (camelCase field names,
//! same derived numbers); local extraction adds what the dumps do not carry: loc key
//! references (`loc`), DataForge paths, contracts and reputation keys. (The dumps themselves
//! serve only Developer mode's Diff, [`crate::source::dump`].)
//!
//! Every change of meaning bumps [`GAMEDATA_SCHEMA`], which is part of the cache key.
//!
//! Loc keys are stored as DataCore gives them (with the leading `@`). Display text never
//! lives here: it comes from the stock `global.ini` at generation time, so the model holds
//! no CIG prose.

pub mod items;
pub mod missions;
pub mod namesake;
pub mod obtain;
pub mod quest;
pub mod ships;

pub use items::{
    Ammo, Capacitor, Component, Damage, Falloff, FireKind, FireMode, ItemIdentity, LocKeys, MinMax,
    Missile, SignatureDetection, Weapon, WeaponAmmo, WeaponClass,
};
pub use missions::{
    Blueprint, BlueprintPool, BlueprintPoolEntry, BlueprintReward, BlueprintTier, Contract,
    ItemReward, KeySource, ReputationReward, ReputationStanding, SpawnGroup, SpawnOption,
    SpawnRole, SpawnSlot,
};
pub use obtain::{
    ItemObtain, LootHint, MinedHint, ObtainHints, QualityHint, QualityOverride, SellHint, ShopHint,
};
pub use quest::{QuestItem, QuestRef, QuestUse};
pub use ships::{Armor, Flight, Hull, Loadout, LoadoutItem, Ship, ShipLoc, Totals};

use crate::build::BuildInfo;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Layout version of [`GameData`].
///
/// - 1: M0, build identity and DataCore counts only.
/// - 2: M2, items, ships, ammo, contracts, blueprints, reputation.
/// - 3: quest items and the missions that reference them (addendum §13).
/// - 4: description keys of every entity class and resource type (`item_descs`,
///   `resource_descs`), for the obtain hints (spec obtain-hints).
/// - 5: name key, description key and `AttachDef` type of every entity class with a name
///   (`item_names`), for the missing names (spec missing-names).
/// - 6: star systems of every contract (`Contract::systems`, from its locality
///   prerequisites), for the blueprint finder's "Earn it".
/// - 7: ship groups every contract spawns (`Contract::spawns`), with the vehicles each
///   option's tag query matches.
pub const GAMEDATA_SCHEMA: u32 = 7;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameData {
    pub schema: u32,
    /// Build identity of the channel the data came from.
    pub build: Option<BuildInfo>,
    /// Counts from the parsed DataCore, proving it was read (local extraction only).
    #[serde(default)]
    pub datacore: Option<DataCoreStats>,
    /// Ship items with a known category (shields, coolers, racks, ...).
    #[serde(default)]
    pub components: Vec<Component>,
    /// Ship and FPS weapons (ship missiles are in `missiles`).
    #[serde(default)]
    pub weapons: Vec<Weapon>,
    #[serde(default)]
    pub missiles: Vec<Missile>,
    #[serde(default)]
    pub ammo: Vec<Ammo>,
    #[serde(default)]
    pub ships: Vec<Ship>,
    /// Contract-generator contracts (spec 6.1). Local extraction only.
    #[serde(default)]
    pub contracts: Vec<Contract>,
    #[serde(default)]
    pub blueprints: Vec<Blueprint>,
    #[serde(default)]
    pub blueprint_pools: Vec<BlueprintPool>,
    #[serde(default)]
    pub reputation_standings: Vec<ReputationStanding>,
    /// Items missions spawn or contracts ask for (addendum §13). Local extraction only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quest_items: Vec<QuestItem>,
    /// Description loc reference (with `@`) of every entity class whose `AttachDef` has a
    /// real one, by class (lower case): the stock key an obtain hint is appended to (spec
    /// obtain-hints). Local extraction only.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub item_descs: BTreeMap<String, String>,
    /// Description loc reference of every resource type (commodity) in the
    /// `ResourceTypeDatabase`, by resource name (lower case, `raw_quantainium`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub resource_descs: BTreeMap<String, String>,
    /// Every entity class whose `AttachDef` names a real loc key, sorted by class: what
    /// the missing-names category reads to name keys stock lacks. Local extraction only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub item_names: Vec<ItemName>,
}

/// One entity class's `AttachDef.Localization` keys and type (spec missing-names).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemName {
    /// Record short name, lower case (`nvs_pilot_flightsuit_01_01_02`).
    pub class: String,
    /// Name reference as DataCore gives it (with `@`).
    pub name: String,
    /// Description reference (with `@`), when a real one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
    /// `AttachDef.Type` (`Char_Armor_Torso`, `Cargo`, ...), empty when none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kind: String,
}

impl Default for GameData {
    fn default() -> Self {
        Self {
            schema: GAMEDATA_SCHEMA,
            build: None,
            datacore: None,
            components: Vec::new(),
            weapons: Vec::new(),
            missiles: Vec::new(),
            ammo: Vec::new(),
            ships: Vec::new(),
            contracts: Vec::new(),
            blueprints: Vec::new(),
            blueprint_pools: Vec::new(),
            reputation_standings: Vec::new(),
            quest_items: Vec::new(),
            item_descs: BTreeMap::new(),
            resource_descs: BTreeMap::new(),
            item_names: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataCoreStats {
    pub records: u64,
    #[serde(alias = "struct_defs")]
    pub struct_defs: u64,
}

/// Record counts per category, for reports and drift checks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub components: usize,
    pub ship_weapons: usize,
    pub fps_weapons: usize,
    pub missiles: usize,
    pub ammo: usize,
    pub ships: usize,
    pub contracts: usize,
    pub blueprints: usize,
    pub blueprint_pools: usize,
    pub reputation_standings: usize,
    #[serde(default)]
    pub quest_items: usize,
}

impl GameData {
    pub fn counts(&self) -> Counts {
        let class = |c: WeaponClass| self.weapons.iter().filter(|w| w.class == c).count();
        Counts {
            components: self.components.len(),
            ship_weapons: class(WeaponClass::Ship),
            fps_weapons: class(WeaponClass::Fps),
            missiles: self.missiles.len(),
            ammo: self.ammo.len(),
            ships: self.ships.len(),
            contracts: self.contracts.len(),
            blueprints: self.blueprints.len(),
            blueprint_pools: self.blueprint_pools.len(),
            reputation_standings: self.reputation_standings.len(),
            quest_items: self.quest_items.len(),
        }
    }

    /// Sort every collection by record name, so output and caches are deterministic.
    pub fn sort(&mut self) {
        self.components
            .sort_by(|a, b| a.item.record_name.cmp(&b.item.record_name));
        self.weapons
            .sort_by(|a, b| a.item.record_name.cmp(&b.item.record_name));
        self.missiles
            .sort_by(|a, b| a.item.record_name.cmp(&b.item.record_name));
        self.ammo.sort_by(|a, b| a.record_name.cmp(&b.record_name));
        self.ships.sort_by(|a, b| a.record_name.cmp(&b.record_name));
        self.contracts.sort_by(|a, b| {
            (&a.generator, &a.debug_name, &a.id).cmp(&(&b.generator, &b.debug_name, &b.id))
        });
        self.blueprints
            .sort_by(|a, b| a.record_name.cmp(&b.record_name));
        self.blueprint_pools.sort_by(|a, b| a.name.cmp(&b.name));
        self.reputation_standings
            .sort_by(|a, b| a.record_name.cmp(&b.record_name));
        self.quest_items
            .sort_by(|a, b| a.record_name.cmp(&b.record_name));
        self.item_names.sort_by(|a, b| a.class.cmp(&b.class));
        for q in &mut self.quest_items {
            q.uses.sort_by(|a, b| {
                (&a.source, &a.mission, a.kind).cmp(&(&b.source, &b.mission, b.kind))
            });
        }
    }

    pub fn component(&self, short: &str) -> Option<&Component> {
        self.components
            .iter()
            .find(|c| short_name(&c.item.record_name).eq_ignore_ascii_case(short))
    }

    pub fn weapon(&self, short: &str) -> Option<&Weapon> {
        self.weapons
            .iter()
            .find(|c| short_name(&c.item.record_name).eq_ignore_ascii_case(short))
    }

    pub fn missile(&self, short: &str) -> Option<&Missile> {
        self.missiles
            .iter()
            .find(|c| short_name(&c.item.record_name).eq_ignore_ascii_case(short))
    }

    pub fn ship(&self, short: &str) -> Option<&Ship> {
        self.ships
            .iter()
            .find(|c| short_name(&c.record_name).eq_ignore_ascii_case(short))
    }
}

/// `"EntityClassDefinition.AEGS_Gladius"` -> `"AEGS_Gladius"`.
pub fn short_name(record_name: &str) -> &str {
    record_name
        .split_once('.')
        .map(|(_, s)| s)
        .unwrap_or(record_name)
}

/// A loc reference as written in `global.ini`: no leading `@`, `None` for empty and
/// `LOC_EMPTY`/`LOC_UNINITIALIZED` placeholders.
pub fn loc_key(reference: &str) -> Option<&str> {
    let k = reference.strip_prefix('@').unwrap_or(reference);
    if k.is_empty()
        || k.eq_ignore_ascii_case("LOC_EMPTY")
        || k.eq_ignore_ascii_case("LOC_UNINITIALIZED")
    {
        None
    } else {
        Some(k)
    }
}

/// True for names the game shows as placeholders: unresolved `@…` (also inside the text,
/// `Box @item_NameX_1`), "placeholder", the `<= … =>` forms (also spaced,
/// `< = TEST NAME = >`) and "test name" (spec 5.1). Every name lookup of the generators
/// goes through this, so such a value never reaches our text (spec missing-names).
pub fn is_placeholder_name(name: Option<&str>) -> bool {
    let Some(name) = name else { return true };
    if name.is_empty() || name.starts_with('@') || !crate::validate::raw_loc_refs(name).is_empty() {
        return true;
    }
    let lower = name.to_lowercase();
    if lower.contains("placeholder") {
        return true;
    }
    let squashed: String = lower.chars().filter(|c| !c.is_whitespace()).collect();
    squashed.contains("<=") || squashed.contains("=>") || squashed.contains("testname")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_names() {
        assert_eq!(
            short_name("EntityClassDefinition.AEGS_Gladius"),
            "AEGS_Gladius"
        );
        assert_eq!(short_name("plain"), "plain");
    }

    #[test]
    fn loc_keys() {
        assert_eq!(loc_key("@item_NameFoo"), Some("item_NameFoo"));
        assert_eq!(loc_key("item_NameFoo"), Some("item_NameFoo"));
        assert_eq!(loc_key("@LOC_EMPTY"), None);
        assert_eq!(loc_key("@LOC_UNINITIALIZED"), None);
        assert_eq!(loc_key("@"), None);
    }

    #[test]
    fn placeholders() {
        assert!(is_placeholder_name(None));
        assert!(is_placeholder_name(Some("@item_NameX")));
        assert!(is_placeholder_name(Some("<= PLACEHOLDER =>")));
        assert!(is_placeholder_name(Some("< = TEST NAME = >")));
        assert!(is_placeholder_name(Some("Placeholder gun")));
        assert!(is_placeholder_name(Some("Crate of @items_commodities_ice")));
        assert!(!is_placeholder_name(Some("Ammo @ 50%")));
        assert!(!is_placeholder_name(Some("Mantis GT-220 Gatling")));
    }

    #[test]
    fn default_round_trips() {
        let g = GameData::default();
        let s = serde_json::to_string(&g).unwrap();
        let back: GameData = serde_json::from_str(&s).unwrap();
        assert_eq!(back, g);
        assert_eq!(back.schema, GAMEDATA_SCHEMA);
    }

    /// An M0 (schema 1) worker output still deserialises; the schema number tells callers
    /// it is stale.
    #[test]
    fn schema_1_layout_reads() {
        let g: GameData = serde_json::from_str(
            r#"{"schema":1,"build":null,"datacore":{"records":1,"struct_defs":2}}"#,
        )
        .unwrap();
        assert_eq!(g.schema, 1);
        assert_eq!(g.datacore.unwrap().struct_defs, 2);
        assert!(g.ships.is_empty());
    }
}

//! Ship spawns of contracts: the tag database, the vehicles each tag is on, and the
//! `MissionPropertyValue_ShipSpawnDescriptions` groups of a contract.
//!
//! A spawn option (`SpawnDescription_Ship`) names no ship. Its `tags` are a query against
//! the vehicles' record-level tags mixed with state the game applies to the ship it spawns,
//! and `negativeTags` leave ships out. Which tags are which was read from the data (Sept 2026
//! records, 5,152 options of contract generators and templates, 1,101 vehicles):
//!
//! - Filters: `Missions/MissionType/Salvage/AvailableToSalvage/Smallest`, factions,
//!   `AI/Ship/CombatClass/...`, ship models and series. Every such tag is on vehicles.
//! - Applied ([`APPLIED`]): `AI/CargoManifest`, `AI/CrewManifest`, `AI/SkillDefinitions`,
//!   `AI/Spawning/Identifier` (on no vehicle at all) and `AI/Ship/SpawnFlags` (only three of
//!   its tags are on one entity, a boarding Cutlass; `PoweredOff` and `EngineOff`, which the
//!   salvage options ask for, are on none). Matching every tag finds no ship for 4,897 of the
//!   5,152 options; leaving the applied families out finds ships for 5,083.
//!
//! Matching is hierarchical: a selector or negative tag matches a vehicle that carries it or
//! any tag below it. Most vehicles also carry the parent tags, but 46 carry only a child (the
//! `_unmanned_salvage` hulls carry `AvailableToSalvage/Small` without `AvailableToSalvage`).
//!
//! Only vehicles a player can meet count ([`eligible`]); when an option matches AI, unmanned
//! or salvage hulls as well as player hulls, the game's own AI/salvage versions are kept.

use super::json::{array, num, referenced_name, text, type_of};
use serde_json::Value;
use starorg_core::model::{SpawnGroup, SpawnOption, SpawnRole, SpawnSlot};
use std::collections::{BTreeSet, HashMap};

/// Tag families the game applies to the ship it spawns; every other tag is a filter.
pub const APPLIED: [&str; 5] = [
    "AI/CargoManifest",
    "AI/CrewManifest",
    "AI/SkillDefinitions",
    "AI/Spawning/Identifier",
    "AI/Ship/SpawnFlags",
];

/// Whether a tag path is applied state rather than a filter.
pub fn is_applied(path: &str) -> bool {
    APPLIED.iter().any(|f| {
        path.strip_prefix(f)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    })
}

/// The id of a tag reference (`{"_RecordId_": ..., "_RecordName_": "Tag.<id>"}`), lower case.
pub fn tag_id(v: &Value) -> Option<String> {
    text(v.get("_RecordId_"))
        .or_else(|| text(v.get("_RecordName_")).map(|n| n.strip_prefix("Tag.").unwrap_or(n)))
        .map(str::to_ascii_lowercase)
}

/// Tag ids of a `TagList` (`{"tags": [...]}`) or a plain array of references.
pub fn tag_list(v: Option<&Value>) -> Vec<String> {
    let list = match v {
        Some(Value::Array(_)) => v,
        Some(o) => o.get("tags"),
        None => None,
    };
    array(list).iter().filter_map(tag_id).collect()
}

/// Tag id -> path (`Missions/MissionType/Salvage`) and parent id, from the `TagDatabase`
/// record.
#[derive(Debug, Default)]
pub struct TagDb {
    paths: HashMap<String, String>,
    parents: HashMap<String, String>,
}

impl TagDb {
    pub fn from_database(v: &Value) -> Self {
        fn walk(t: &Value, prefix: &str, parent: Option<&str>, db: &mut TagDb) {
            let Some(name) = text(t.get("tagName")) else {
                return;
            };
            let path = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{prefix}/{name}")
            };
            let id = tag_id(t);
            for c in array(t.get("children")) {
                walk(c, &path, id.as_deref(), db);
            }
            if let Some(id) = id {
                if let Some(p) = parent {
                    db.parents.insert(id.clone(), p.to_string());
                }
                db.paths.insert(id, path);
            }
        }
        let mut db = Self::default();
        for t in array(v.get("tags")) {
            walk(t, "", None, &mut db);
        }
        db
    }

    pub fn path(&self, id: &str) -> Option<&str> {
        self.paths.get(id).map(String::as_str)
    }

    /// `id` and every tag above it.
    pub fn with_ancestors(&self, id: &str) -> Vec<String> {
        let mut out = vec![id.to_string()];
        let mut cur = id;
        while let Some(p) = self.parents.get(cur) {
            if out.contains(p) {
                break;
            }
            out.push(p.clone());
            cur = p;
        }
        out
    }

    pub fn len(&self) -> usize {
        self.paths.len()
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }
}

/// Words of a class name (lower case, split at `_`).
fn segments(class: &str) -> impl Iterator<Item = &str> {
    class.split('_')
}

/// Vehicle classes a spawn can stand for: not test, template, tutorial, derelict-part,
/// piano or wreckage records.
pub fn eligible(class: &str) -> bool {
    let c = class.to_ascii_lowercase();
    !c.contains("template")
        && !segments(&c)
            .any(|s| matches!(s, "test" | "derelict" | "tutorial" | "piano" | "wreckage"))
}

/// The game's own AI, unmanned or salvage version of a hull (`..._pu_ai_crim`,
/// `..._unmanned_salvage`).
pub fn is_ai_version(class: &str) -> bool {
    segments(&class.to_ascii_lowercase()).any(|s| matches!(s, "ai" | "unmanned" | "salvage"))
}

/// Vehicle entity classes (lower case) by the tag ids on their record and every tag above
/// them.
#[derive(Debug, Default)]
pub struct ShipTags {
    by_tag: HashMap<String, BTreeSet<String>>,
}

impl ShipTags {
    pub fn add(&mut self, class: &str, tags: &[String], db: &TagDb) {
        if !eligible(class) {
            return;
        }
        for t in tags {
            for a in db.with_ancestors(t) {
                self.by_tag
                    .entry(a)
                    .or_default()
                    .insert(class.to_ascii_lowercase());
            }
        }
    }

    /// Classes with every tag of `all` (at least one; or a tag below it) and none of `none`,
    /// sorted. AI, unmanned and salvage versions win over player hulls when both match.
    pub fn matching(&self, all: &[String], none: &[String]) -> Vec<String> {
        let Some((first, rest)) = all.split_first() else {
            return Vec::new();
        };
        let Some(start) = self.by_tag.get(first) else {
            return Vec::new();
        };
        let found: Vec<String> = start
            .iter()
            .filter(|c| {
                rest.iter()
                    .all(|t| self.by_tag.get(t).is_some_and(|s| s.contains(*c)))
                    && !none
                        .iter()
                        .any(|t| self.by_tag.get(t).is_some_and(|s| s.contains(*c)))
            })
            .cloned()
            .collect();
        if found.iter().any(|c| is_ai_version(c)) && found.iter().any(|c| !is_ai_version(c)) {
            return found.into_iter().filter(|c| is_ai_version(c)).collect();
        }
        found
    }
}

/// Words in a group name or mission variable, lower case. Friendly words are checked on both
/// the name and the variable first (`FirstBeat_FriendlySupport_BP`); then the other lists,
/// on the name, then the variable, in this order: hostile before escort
/// (`SupportAttackedShip_HostileShip...`, `EscortReinforcementsWave01`), cargo and target
/// before escort.
const FRIENDLY: [&str; 3] = ["friendly", "allied", "allies"];
const ROLE_WORDS: [(SpawnRole, &[&str]); 4] = [
    (
        SpawnRole::Hostile,
        &[
            "hostile",
            "enem",
            "reinforc",
            "interdiction",
            "bomber",
            "scout",
            "ambu",
            "combatant",
            "hijacker",
            "defender",
            "defendingships",
            "wave",
            "supportships",
            "security",
        ],
    ),
    (SpawnRole::Cargo, &["cargoship"]),
    (
        SpawnRole::Target,
        &[
            "target", "salvage", "acepilot", "heist", "toboard", "boarded", "hijacked", "killship",
        ],
    ),
    (
        SpawnRole::Escort,
        &[
            "shiptodefend",
            "attackedship",
            "escortship",
            "recipientship",
            "defendship",
        ],
    ),
];

fn folded(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase()
}

fn role_in(s: &str) -> Option<SpawnRole> {
    let s = folded(s);
    ROLE_WORDS
        .iter()
        .find(|(_, words)| words.iter().any(|w| s.contains(w)))
        .map(|(r, _)| *r)
}

/// The role of a group: friendly words anywhere, else from its name, else its variable.
pub fn role_of(variable: &str, name: &str) -> SpawnRole {
    let both = format!("{} {}", folded(name), folded(variable));
    if FRIENDLY.iter().any(|w| both.contains(w)) {
        return SpawnRole::Escort;
    }
    role_in(name)
        .or_else(|| role_in(variable))
        .unwrap_or_default()
}

fn parse_option(o: &Value, tags: &TagDb, ships: &ShipTags) -> SpawnOption {
    let ids = tag_list(o.get("tags"));
    let exclude_ids = tag_list(o.get("negativeTags"));
    // Unknown ids stay filters (a query the game could not answer either) but are kept
    // apart: never shown as text.
    let (applied, filters): (Vec<&String>, Vec<&String>) = ids
        .iter()
        .partition(|id| tags.path(id).is_some_and(is_applied));
    let filter_ids: Vec<String> = filters.into_iter().cloned().collect();
    let known = |list: &[String]| -> Vec<String> {
        list.iter()
            .filter_map(|id| tags.path(id).map(str::to_string))
            .collect()
    };
    let unknown: Vec<String> = filter_ids
        .iter()
        .chain(&exclude_ids)
        .filter(|id| tags.path(id).is_none())
        .cloned()
        .collect();
    SpawnOption {
        selector: known(&filter_ids),
        exclude: known(&exclude_ids),
        unknown,
        applied: applied
            .into_iter()
            .filter_map(|id| tags.path(id).map(str::to_string))
            .collect(),
        amount: num(o.get("concurrentAmount")).unwrap_or(1.0),
        weight: num(o.get("weight")).unwrap_or(1.0),
        damage: referenced_name(o.get("initialDamageSettings")).map(str::to_ascii_lowercase),
        ships: ships.matching(&filter_ids, &exclude_ids),
    }
}

/// The ship groups of one `MissionPropertyValue_ShipSpawnDescriptions` value.
pub fn parse_groups(variable: &str, v: &Value, tags: &TagDb, ships: &ShipTags) -> Vec<SpawnGroup> {
    array(v.get("spawnDescriptions"))
        .iter()
        .filter(|g| type_of(g) == Some("SpawnDescription_ShipGroup"))
        .map(|g| {
            let name = text(g.get("Name")).unwrap_or_default().trim().to_string();
            SpawnGroup {
                variable: variable.to_string(),
                role: role_of(variable, &name),
                name,
                slots: array(g.get("ships"))
                    .iter()
                    .map(|slot| SpawnSlot {
                        options: array(slot.get("options"))
                            .iter()
                            .filter(|o| type_of(o) == Some("SpawnDescription_Ship"))
                            .map(|o| parse_option(o, tags, ships))
                            .collect(),
                    })
                    .filter(|s| !s.options.is_empty())
                    .collect(),
            }
        })
        .filter(|g| !g.slots.is_empty())
        .collect()
}

/// The ship groups of a contract, and the variables set twice within one layer (the first
/// is kept). `layers` are `MissionProperty` lists, most specific first (the contract's
/// `propertyOverrides`, the handler's, the template's `contractProperties`): the first layer
/// that sets a variable decides it, so an empty override switches a template's spawns off.
pub fn contract_spawns(
    layers: &[&[Value]],
    tags: &TagDb,
    ships: &ShipTags,
) -> (Vec<SpawnGroup>, Vec<String>) {
    let mut decided: BTreeSet<&str> = BTreeSet::new();
    let mut twice = Vec::new();
    let mut out = Vec::new();
    for layer in layers {
        let mut here: BTreeSet<&str> = BTreeSet::new();
        for p in *layer {
            let Some(value) = p.get("value") else {
                continue;
            };
            if type_of(value) != Some("MissionPropertyValue_ShipSpawnDescriptions") {
                continue;
            }
            let Some(var) = text(p.get("missionVariableName")) else {
                continue;
            };
            if !here.insert(var) {
                twice.push(var.to_string());
                continue;
            }
            if !decided.insert(var) {
                continue;
            }
            out.extend(parse_groups(var, value, tags, ships));
        }
    }
    (out, twice)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tref(id: &str) -> Value {
        json!({"_RecordPath_": "file://./../tagdatabase/tagdatabase.tagdatabase.json",
               "_RecordName_": format!("Tag.{id}"), "_RecordId_": id})
    }

    fn db() -> TagDb {
        let tag = |id: &str, name: &str, children: Vec<Value>| json!({"_Type_": "Tag", "_RecordId_": id, "tagName": name, "children": children});
        TagDb::from_database(&json!({"_Type_": "TagDatabase", "tags": [
            tag("m", "Missions", vec![tag("s", "Salvage", vec![
                tag("small", "Smallest", vec![]), tag("large", "Large", vec![])])]),
            tag("ai", "AI", vec![
                tag("sf", "Ship", vec![tag("flags", "SpawnFlags", vec![tag("off", "PoweredOff", vec![])])]),
                tag("cm", "CargoManifest", vec![tag("hv", "HighValue", vec![])]),
                tag("fac", "Faction", vec![tag("crim", "Criminal", vec![
                    tag("crimsub", "Pirates", vec![])])]),
            ]),
        ]}))
    }

    #[test]
    fn tag_paths_and_the_applied_rule() {
        let db = db();
        assert_eq!(db.path("small"), Some("Missions/Salvage/Smallest"));
        assert_eq!(db.path("off"), Some("AI/Ship/SpawnFlags/PoweredOff"));
        assert_eq!(db.with_ancestors("small"), ["small", "s", "m"]);
        assert!(is_applied("AI/Ship/SpawnFlags/PoweredOff"));
        assert!(is_applied("AI/CargoManifest"));
        assert!(!is_applied("AI/CargoManifestation"));
        assert!(!is_applied("AI/Faction/Criminal"));
    }

    #[test]
    fn selector_tags_filter_and_applied_tags_do_not() {
        let db = db();
        let mut ships = ShipTags::default();
        ships.add("AEGS_Gladius_Unmanned_Salvage", &["small".into()], &db);
        ships.add("drak_cutlass", &["large".into(), "crim".into()], &db);
        ships.add("misc_freelancer", &["large".into()], &db);
        let opt = json!({"_Type_": "SpawnDescription_Ship",
            "tags": {"_Type_": "TagList", "tags": [tref("small"), tref("off"), tref("hv")]},
            "negativeTags": {"_Type_": "TagList", "tags": []},
            "concurrentAmount": 1, "weight": 1.0,
            "initialDamageSettings": "file://./../initialdamageoverrides/salvagecontractor_lightdamage.json"});
        let o = parse_option(&opt, &db, &ships);
        assert_eq!(o.selector, ["Missions/Salvage/Smallest"]);
        assert_eq!(
            o.applied,
            [
                "AI/Ship/SpawnFlags/PoweredOff",
                "AI/CargoManifest/HighValue"
            ]
        );
        // The spawn flag and manifest are on no ship; matching them too would find none.
        assert_eq!(o.ships, ["aegs_gladius_unmanned_salvage"]);
        assert_eq!(o.damage.as_deref(), Some("salvagecontractor_lightdamage"));
        // Negative tags leave ships out.
        let opt = json!({"tags": {"tags": [tref("large")]}, "negativeTags": {"tags": [tref("crim")]},
                         "concurrentAmount": 2});
        let o = parse_option(&opt, &db, &ships);
        assert_eq!(o.ships, ["misc_freelancer"]);
        assert_eq!(o.exclude, ["AI/Faction/Criminal"]);
        assert_eq!(o.amount, 2.0);
        // Only applied tags: nothing to match (unresolved). An unknown tag matches nothing
        // and is kept apart from the shown selector.
        let o = parse_option(&json!({"tags": {"tags": [tref("off")]}}), &db, &ships);
        assert!(o.selector.is_empty() && o.ships.is_empty());
        let o = parse_option(&json!({"tags": {"tags": [tref("nope")]}}), &db, &ships);
        assert!(o.selector.is_empty());
        assert_eq!(o.unknown, ["nope"]);
        assert!(o.ships.is_empty());
    }

    #[test]
    fn parent_tags_match_vehicles_that_carry_only_a_child() {
        let db = db();
        let mut ships = ShipTags::default();
        // Carries only the child tags, not `Salvage` or `Criminal`.
        ships.add(
            "x_hull_pu_ai_pirate",
            &["small".into(), "crimsub".into()],
            &db,
        );
        ships.add("y_hull", &["large".into()], &db);
        let o = parse_option(&json!({"tags": {"tags": [tref("s")]}}), &db, &ships);
        assert_eq!(
            o.ships,
            ["x_hull_pu_ai_pirate"],
            "AI version wins over the player hull"
        );
        let o = parse_option(
            &json!({"tags": {"tags": [tref("s")]}, "negativeTags": {"tags": [tref("crim")]}}),
            &db,
            &ships,
        );
        assert_eq!(
            o.ships,
            ["y_hull"],
            "the parent negative tag excludes the child"
        );
    }

    #[test]
    fn only_vehicles_a_player_can_meet() {
        for c in [
            "aegs_gladius_test",
            "misc_template",
            "anvl_ai_template_x",
            "tutorial_ship",
            "piano_01",
            "derelict_part_a",
            "drak_wreckage_large",
        ] {
            assert!(!eligible(c), "{c}");
        }
        assert!(eligible("aegs_gladius_unmanned_salvage"));
        assert!(eligible("drak_cutlass_black_pu_ai_crim"));
    }

    #[test]
    fn roles_from_names_and_variables() {
        use SpawnRole::*;
        for (var, name, want) in [
            ("SalvageSpawnDescription_BP", "SalvageableShip", Target),
            ("MissionTargets_BP", "Target", Target),
            ("MissionTargets_BP", "Defenders", Hostile),
            ("MissionTargets_BP", "Reinforcments", Hostile),
            ("DefendingShips_BP", "Wave1", Hostile),
            (
                "SupportAttackedShip_HostileShipSpawnDescriptions_BP",
                "Wave1",
                Hostile,
            ),
            (
                "SupportAttackedShip_AttackedShipSpawnDescriptions_BP",
                "ShipToDefend",
                Escort,
            ),
            ("DefendEntities_SupportShips_BP", "Wave 1", Hostile),
            (
                "EscortShipToLandingArea_EscortShip_BP",
                "EscortShip",
                Escort,
            ),
            (
                "EscortShipToLandingArea_EscortReinforcementsWave01_BP",
                "EscortReinforcementsWave01",
                Hostile,
            ),
            (
                "EscortShipToLandingArea_InitialEnemies_BP",
                "InitialEnemies",
                Hostile,
            ),
            ("AlliedSpawnDescriptions_BP", "ShipToDefend", Escort),
            ("AlliedSpawnDescriptions_BP", "Allies", Escort),
            ("FirstBeat_FriendlyShips_BP", "", Escort),
            ("FirstBeat_FriendlySupport_BP", "Wave1", Escort),
            ("RecipientShip_BP", "RecipientShip", Escort),
            ("CargoShip_BP", "", Cargo),
            ("HeistShip_BP", "Heist Ship", Target),
            ("DecoyProbe_BP", "Probe Civillian", Other),
            ("Support1_BP", "Support1", Other),
        ] {
            assert_eq!(role_of(var, name), want, "{var} / {name}");
        }
    }

    #[test]
    fn contract_layers_and_empty_overrides() {
        let db = db();
        let mut ships = ShipTags::default();
        ships.add("x_hull", &["small".into()], &db);
        let prop = |var: &str, groups: Value| {
            json!({"_Type_": "MissionProperty", "missionVariableName": var, "extendedTextToken": "",
                   "value": {"_Type_": "MissionPropertyValue_ShipSpawnDescriptions",
                             "spawnDescriptions": groups}})
        };
        let group = json!([{"_Type_": "SpawnDescription_ShipGroup", "Name": " SalvageableShip ",
            "ships": [{"_Type_": "SpawnDescription_ShipOptions", "options": [
                {"_Type_": "SpawnDescription_Ship", "tags": {"tags": [tref("small")]},
                 "concurrentAmount": 1, "weight": 1.0}]}]}]);
        let contract = vec![
            prop("Reinforcements_BP", json!([])),
            prop("Reinforcements_BP", group.clone()),
            json!({"_Type_": "MissionProperty", "missionVariableName": "Dist_BP",
                   "value": {"_Type_": "MissionPropertyValue_Float"}}),
        ];
        let template = vec![
            prop("SalvageSpawnDescription_BP", group.clone()),
            prop("Reinforcements_BP", group),
        ];
        let (got, twice) = contract_spawns(&[&contract, &[], &template], &db, &ships);
        assert_eq!(
            got.len(),
            1,
            "the empty override switches the template's group off"
        );
        assert_eq!(
            twice,
            ["Reinforcements_BP"],
            "set twice in one layer: the first is kept"
        );
        assert_eq!(got[0].variable, "SalvageSpawnDescription_BP");
        assert_eq!(got[0].name, "SalvageableShip");
        assert_eq!(got[0].role, SpawnRole::Target);
        assert_eq!(got[0].slots[0].options[0].ships, ["x_hull"]);
    }
}

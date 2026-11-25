//! Fill [`GameData`] from DataCore records (spec 3.3 step 5, 3.4).
//!
//! Records are exported one at a time with StarBreaker's JSON exporter and read with the
//! accessors in [`json`]. That is the same JSON shape `sc-gamedata` (the producer of the
//! published dumps) parses, so both sources compute the same numbers.
//!
//! Passes, each over a few thousand records, in parallel:
//!
//! 1. ammo (`AmmoParams`) and inventory containers (cargo grid SCU),
//! 2. ship items under `entities/scitem/ships/` (components, ship weapons, missiles, flight
//!    controllers), FPS weapons under `entities/scitem/weapons/`,
//! 3. vehicles under `entities/spaceships/` and `entities/groundvehicles/`, with the
//!    loadout resolved against pass 2,
//! 4. reputation standings, blueprint pools, blueprints, contract generators,
//! 5. quest items: entities `MissionBrokerEntry` spawn descriptions and contract hauling
//!    orders reference (addendum §13),
//! 6. description keys of every entity class and resource type, for the obtain hints
//!    (spec obtain-hints): the datasets that feed them name classes, not loc keys; and
//!    the name key, description key and type of every named entity class, for the
//!    missing names (spec missing-names).
//!
//! [`drift`] then turns empty categories and missing struct types into schema-drift
//! warnings (spec 3.3 step 6).

pub mod drift;
pub mod items;
pub mod json;
pub mod missions;
pub mod ships;
pub mod store;

use rayon::prelude::*;
use serde_json::Value;
use starorg_core::ini::StockIni;
use starorg_core::model::{Ammo, GameData, ItemName, QuestItem, WeaponClass};
use starorg_core::source::Warning;
use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;
use store::{Index, RecordMeta, RecordStore};

/// Stock strings, for telling real names from placeholders.
pub struct Loc<'a> {
    ini: Option<&'a StockIni>,
}

impl<'a> Loc<'a> {
    pub fn new(ini: &'a StockIni) -> Self {
        Self { ini: Some(ini) }
    }

    /// No strings: every `@` reference stays unresolved (all names look like placeholders).
    pub fn none() -> Self {
        Self { ini: None }
    }

    /// Text of a loc reference (`@key`), or the reference itself when it is literal text.
    pub fn text(&self, reference: &str) -> Option<&'a str> {
        let key = reference.strip_prefix('@')?;
        let key = starorg_core::model::loc_key(key)?;
        self.ini?.value(key).filter(|v| !v.is_empty())
    }
}

/// Struct types the extraction reads, with the stat each one feeds. A type missing from
/// the DataCore is a schema-drift warning (the "path fails to compile" case of spec 3.3).
pub const REQUIRED_TYPES: &[(&str, &str)] = &[
    ("EntityClassDefinition", "every item and ship"),
    (
        "SAttachableComponentParams",
        "item type, size, grade, names",
    ),
    (
        "VehicleComponentParams",
        "ship identity and description key",
    ),
    (
        "SEntityComponentDefaultLoadoutParams",
        "ship loadouts, FPS magazines",
    ),
    ("IFCSParams", "ship SCM/max/boost speed and Pitch/Yaw/Roll"),
    (
        "SCItemShieldGeneratorParams",
        "shield HP, regen, resistances",
    ),
    (
        "SCItemQuantumDriveParams",
        "quantum drive speed, spool, fuel",
    ),
    ("SCItemRadarComponentParams", "radar ping and detection"),
    ("SCItemMissileRackParams", "missile rack slots"),
    ("SCItemWeaponComponentParams", "weapon fire modes"),
    ("SCItemMissileParams", "missile damage, tracking, lock"),
    (
        "SAmmoContainerComponentParams",
        "weapon ammo and magazine size",
    ),
    ("ItemResourceComponentParams", "power draw, output, EM/IR"),
    ("SHealthComponentParams", "component HP"),
    ("SCItemVehicleArmorParams", "ship armor damage multipliers"),
    ("AmmoParams", "ammo damage, speed, range"),
    (
        "ContractGenerator",
        "contract titles, descriptions, rewards",
    ),
    ("CraftingBlueprintRecord", "blueprint names"),
    ("BlueprintPoolRecord", "blueprint pools of contracts"),
    ("SReputationStandingParams", "required standing names"),
    ("SReputationRewardAmount", "contract reputation amounts"),
    (
        "ResourceTypeDatabase",
        "commodity description keys (WHERE TO GET)",
    ),
];

/// What one extraction produced.
#[derive(Debug, Clone)]
pub struct Extracted {
    pub game_data: GameData,
    pub warnings: Vec<Warning>,
}

fn load_all<'s, T: Send>(
    store: &dyn RecordStore,
    metas: Vec<&'s RecordMeta>,
    errors: &Mutex<Vec<Warning>>,
    f: impl Fn(&'s RecordMeta, &str, &Value) -> Option<T> + Sync,
) -> Vec<T> {
    metas
        .into_par_iter()
        .filter_map(|m| match store.load(m) {
            Ok(l) => f(m, &l.id, &l.value),
            Err(e) => {
                if let Ok(mut w) = errors.lock() {
                    w.push(Warning::new(
                        "record-unreadable",
                        format!("{}: {e}", m.name),
                    ));
                }
                None
            }
        })
        .collect()
}

struct Lookup<'a, 's> {
    store: &'a dyn RecordStore,
    index: &'a Index<'s>,
    rep: HashMap<String, f64>,
    name_keys: Mutex<HashMap<String, Option<String>>>,
    faction_names: Mutex<HashMap<String, Option<String>>>,
    templates: Mutex<HashMap<String, Vec<String>>>,
}

impl missions::MissionLookup for Lookup<'_, '_> {
    fn short_name(&self, path: &str) -> Option<String> {
        self.index.by_path(path).map(|m| m.short_name().to_string())
    }

    fn entity_name_key(&self, path: &str) -> Option<String> {
        if let Some(hit) = self.name_keys.lock().ok()?.get(path) {
            return hit.clone();
        }
        let key = self.index.by_path(path).and_then(|m| {
            let l = self.store.load(m).ok()?;
            json::text_at(json::attach_def(&l.value), "Localization.Name").map(str::to_string)
        });
        self.name_keys
            .lock()
            .ok()?
            .insert(path.to_string(), key.clone());
        key
    }

    fn faction_name_key(&self, path: &str) -> Option<String> {
        if let Some(hit) = self.faction_names.lock().ok()?.get(path) {
            return hit.clone();
        }
        let key = self.index.by_path(path).and_then(|m| {
            let l = self.store.load(m).ok()?;
            json::text(l.value.get("displayName"))
                .filter(|k| k.starts_with('@') && starorg_core::model::loc_key(k).is_some())
                .map(str::to_string)
        });
        self.faction_names
            .lock()
            .ok()?
            .insert(path.to_string(), key.clone());
        key
    }

    fn reputation_amount(&self, path: &str) -> Option<f64> {
        self.rep.get(path).copied()
    }

    fn template_strings(&self, path: &str) -> Vec<String> {
        if let Some(hit) = self
            .templates
            .lock()
            .ok()
            .and_then(|t| t.get(path).cloned())
        {
            return hit;
        }
        let strings: Vec<String> = self
            .index
            .by_path(path)
            .and_then(|m| self.store.load(m).ok())
            .map(|l| {
                json::array(json::at(&l.value, "contractDisplayInfo.displayString"))
                    .iter()
                    .map(|s| s.as_str().unwrap_or_default().to_string())
                    .collect()
            })
            .unwrap_or_default();
        if let Ok(mut t) = self.templates.lock() {
            t.insert(path.to_string(), strings.clone());
        }
        strings
    }
}

/// Folder part of a normalised record path (`a/b/c` -> `a/b`).
fn folder_of(path: &str) -> &str {
    path.rsplit_once('/').map(|(f, _)| f).unwrap_or("")
}

/// Description key of an entity, folded for comparison.
fn desc_key_of(entity: &Value) -> Option<String> {
    json::text_at(json::attach_def(entity), "Localization.Description")
        .and_then(starorg_core::model::loc_key)
        .map(starorg_core::ini::fold_key)
}

/// A description reference worth keeping: an `@` key, not empty or a placeholder.
fn usable_desc(r: &str) -> Option<String> {
    let k = starorg_core::model::loc_key(r.strip_prefix('@')?)?;
    (!k.eq_ignore_ascii_case("LOC_PLACEHOLDER")).then(|| r.to_string())
}

/// Description keys of pass 6.
struct DescKeys {
    items: BTreeMap<String, String>,
    resources: BTreeMap<String, String>,
    names: Vec<ItemName>,
}

/// Pass 6: entity classes (lower case) -> `AttachDef` description reference, resource
/// names (lower case) -> description reference from the `ResourceTypeDatabase` groups, and
/// every entity class with a real name reference (one record read for both).
fn desc_keys(store: &dyn RecordStore, index: &Index<'_>, errors: &Mutex<Vec<Warning>>) -> DescKeys {
    let rows = load_all(
        store,
        index.of_type("EntityClassDefinition").collect(),
        errors,
        |m, _, v| {
            let a = json::attach_def(v);
            let class = m.short_name().to_ascii_lowercase();
            let desc = json::text_at(a, "Localization.Description").and_then(usable_desc);
            let name = json::text_at(a, "Localization.Name")
                .filter(|r| r.starts_with('@'))
                .filter(|r| starorg_core::model::loc_key(r).is_some())
                .map(|name| ItemName {
                    class: class.clone(),
                    name: name.to_string(),
                    desc: desc.clone(),
                    kind: json::text_at(a, "Type").unwrap_or_default().to_string(),
                });
            (desc.is_some() || name.is_some()).then_some((class, desc, name))
        },
    );
    let mut items = BTreeMap::new();
    let mut names = Vec::new();
    for (class, desc, name) in rows {
        if let Some(d) = desc {
            items.insert(class, d);
        }
        names.extend(name);
    }
    fn walk(group: &Value, out: &mut BTreeMap<String, String>) {
        for r in json::array(group.get("resources")) {
            let name = json::text(r.get("_RecordName_")).map(starorg_core::model::short_name);
            let desc = json::text(r.get("description")).and_then(usable_desc);
            if let (Some(n), Some(d)) = (name, desc) {
                out.entry(n.to_ascii_lowercase()).or_insert(d);
            }
        }
        for g in json::array(group.get("groups")) {
            walk(g, out);
        }
    }
    let mut resources = BTreeMap::new();
    for found in load_all(
        store,
        index.of_type("ResourceTypeDatabase").collect(),
        errors,
        |_, _, v| {
            let mut out = BTreeMap::new();
            walk(v, &mut out);
            Some(out)
        },
    ) {
        for (k, v) in found {
            resources.entry(k).or_insert(v);
        }
    }
    DescKeys {
        items,
        resources,
        names,
    }
}

/// Pass 5: one [`QuestItem`] per referenced entity, with its loc keys, type and the other
/// entities of its folder that share its description key.
fn quest_items(
    store: &dyn RecordStore,
    index: &Index<'_>,
    links: Vec<missions::QuestLink>,
    errors: &Mutex<Vec<Warning>>,
) -> Vec<QuestItem> {
    let mut by_path: std::collections::BTreeMap<String, Vec<starorg_core::model::QuestUse>> =
        Default::default();
    for (path, used) in links {
        let uses = by_path.entry(path).or_default();
        if !uses.contains(&used) {
            uses.push(used);
        }
    }
    let metas: Vec<(&RecordMeta, Vec<starorg_core::model::QuestUse>)> = by_path
        .into_iter()
        .filter_map(|(p, uses)| Some((index.by_path(&p)?, uses)))
        .filter(|(m, _)| m.struct_type == "EntityClassDefinition")
        .collect();
    let mut items: Vec<QuestItem> = metas
        .into_par_iter()
        .filter_map(|(m, uses)| {
            let l = match store.load(m) {
                Ok(l) => l,
                Err(e) => {
                    if let Ok(mut w) = errors.lock() {
                        w.push(Warning::new(
                            "record-unreadable",
                            format!("{}: {e}", m.name),
                        ));
                    }
                    return None;
                }
            };
            let def = json::attach_def(&l.value);
            Some(QuestItem {
                record_name: m.name.clone(),
                path: m.path.clone(),
                loc: starorg_core::model::LocKeys {
                    name: json::text_at(def, "Localization.Name").map(str::to_string),
                    short_name: json::text_at(def, "Localization.ShortName").map(str::to_string),
                    desc: json::text_at(def, "Localization.Description").map(str::to_string),
                },
                item_type: json::text(def.get("Type")).map(str::to_string),
                sub_type: json::text(def.get("SubType"))
                    .filter(|s| *s != "UNDEFINED")
                    .map(str::to_string),
                desc_shared_with: Vec::new(),
                uses,
            })
        })
        .collect();

    // Shared description keys, among the entities of each quest item's folder.
    let folders: std::collections::BTreeSet<String> = items
        .iter()
        .map(|q| folder_of(&q.path).to_string())
        .collect();
    let neighbours: Vec<&RecordMeta> = index
        .of_type("EntityClassDefinition")
        .filter(|m| folders.contains(folder_of(&m.path)))
        .collect();
    let descs: Vec<(String, String, String)> = load_all(store, neighbours, errors, |m, _, v| {
        Some((
            folder_of(&m.path).to_string(),
            desc_key_of(v)?,
            m.short_name().to_string(),
        ))
    });
    for q in &mut items {
        let Some(key) = q
            .loc
            .desc
            .as_deref()
            .and_then(starorg_core::model::loc_key)
            .map(starorg_core::ini::fold_key)
        else {
            continue;
        };
        let folder = folder_of(&q.path);
        let own = q.short_name().to_string();
        q.desc_shared_with = descs
            .iter()
            .filter(|(f, k, n)| f == folder && *k == key && !n.eq_ignore_ascii_case(&own))
            .map(|(_, _, n)| n.clone())
            .collect();
        q.desc_shared_with.sort();
    }
    items
}

/// Run every pass over `store`. `present_types` lists DataCore struct names for the
/// "type missing" drift check (`None` skips it, e.g. for fixtures).
pub fn extract(
    store: &dyn RecordStore,
    loc: &Loc,
    present_types: Option<&dyn Fn(&str) -> bool>,
) -> Extracted {
    let index = Index::new(store.records());
    let errors = Mutex::new(Vec::new());
    let mut gd = GameData::default();
    #[allow(clippy::field_reassign_with_default)] // passes fill the model step by step
    {
        // Pass 1: ammo and containers.
        gd.ammo = load_all(
            store,
            index.of_type("AmmoParams").collect(),
            &errors,
            |m, id, v| Some(items::parse_ammo(&m.name, id, &m.path, v)),
        );
        let ammo_by: HashMap<String, Ammo> = gd
            .ammo
            .iter()
            .map(|a| (a.name.to_ascii_lowercase(), a.clone()))
            .collect();
        let containers: HashMap<String, f64> = load_all(
            store,
            index
                .of_type("InventoryContainer")
                .filter(|m| m.path.contains("inventorycontainers/"))
                .collect(),
            &errors,
            |m, _, v| {
                Some((
                    m.short_name().to_ascii_lowercase(),
                    items::container_scu(v)?,
                ))
            },
        )
        .into_iter()
        .collect();

        // Pass 2: ship items and FPS weapons.
        enum Parsed {
            Component(Box<starorg_core::model::Component>),
            Weapon(Box<starorg_core::model::Weapon>),
            Missile(Box<starorg_core::model::Missile>),
        }
        struct ItemOut {
            brief: ships::ItemBrief,
            ship_item: bool,
            parsed: Vec<Parsed>,
            flight: Option<starorg_core::model::Flight>,
        }
        let ammo_of = |n: &str| ammo_by.get(&n.to_ascii_lowercase()).cloned();
        let magazine_link = |n: &str| {
            let m = index.entity(n)?;
            let l = store.load(m).ok()?;
            items::ammo_link(&l.value)
        };
        let scu = |n: &str| containers.get(&n.to_ascii_lowercase()).copied();
        let ship_items: Vec<&RecordMeta> = index.entities_under("entities/scitem/ships/").collect();
        let fps_items: Vec<&RecordMeta> =
            index.entities_under("entities/scitem/weapons/").collect();
        let outs = load_all(
            store,
            ship_items.into_iter().chain(fps_items).collect(),
            &errors,
            |m, id, v| {
                let ident = items::identity(&m.name, id, &m.path, v, loc);
                let brief = ships::ItemBrief {
                    short: m.short_name().to_string(),
                    item_type: ident.item_type.clone(),
                    sub_type: ident.sub_type.clone(),
                    size: ident.size.map(|s| s as f64),
                };
                let mut parsed = Vec::new();
                let mut flight = None;
                if m.path.contains("entities/scitem/weapons/") {
                    if let Some(w) =
                        items::parse_weapon(ident, v, WeaponClass::Fps, &ammo_of, &magazine_link)
                    {
                        parsed.push(Parsed::Weapon(Box::new(w)));
                    }
                    return Some(ItemOut {
                        brief,
                        ship_item: false,
                        parsed,
                        flight,
                    });
                }
                if ident.item_type.as_deref() == Some("FlightController") {
                    flight = ships::parse_flight(m.short_name(), v);
                }
                if let Some(c) = items::parse_component(ident.clone(), v, &scu) {
                    parsed.push(Parsed::Component(Box::new(c)));
                }
                if m.path.contains("entities/scitem/ships/weapons/") {
                    if let Some(ms) = items::parse_missile(ident.clone(), v) {
                        parsed.push(Parsed::Missile(Box::new(ms)));
                    } else if let Some(w) =
                        items::parse_weapon(ident, v, WeaponClass::Ship, &ammo_of, &magazine_link)
                    {
                        parsed.push(Parsed::Weapon(Box::new(w)));
                    }
                }
                Some(ItemOut {
                    brief,
                    ship_item: true,
                    parsed,
                    flight,
                })
            },
        );
        let mut briefs = HashMap::new();
        let mut flights = HashMap::new();
        for o in outs {
            let key = o.brief.short.to_ascii_lowercase();
            if let Some(f) = o.flight {
                flights.insert(key.clone(), f);
            }
            for p in o.parsed {
                match p {
                    Parsed::Component(c) => gd.components.push(*c),
                    Parsed::Weapon(w) => gd.weapons.push(*w),
                    Parsed::Missile(m) => gd.missiles.push(*m),
                }
            }
            // Loadouts resolve ship items only, as the dumps do.
            if o.ship_item {
                briefs.insert(key, o.brief);
            }
        }

        // Pass 3: vehicles.
        {
            let cx = ships::ShipContext {
                items: briefs,
                components: gd
                    .components
                    .iter()
                    .filter(|c| c.item.path.contains("entities/scitem/ships/"))
                    .map(|c| (c.item.short_name().to_ascii_lowercase(), c))
                    .collect(),
                weapons: gd
                    .weapons
                    .iter()
                    .filter(|w| w.class == WeaponClass::Ship)
                    .map(|w| (w.item.short_name().to_ascii_lowercase(), w))
                    .collect(),
                missiles: gd
                    .missiles
                    .iter()
                    .map(|w| (w.item.short_name().to_ascii_lowercase(), w))
                    .collect(),
                flight: flights,
            };
            let vehicles: Vec<&RecordMeta> = index
                .entities_under("entities/spaceships/")
                .chain(index.entities_under("entities/groundvehicles/"))
                .collect();
            let ships = load_all(store, vehicles, &errors, |m, id, v| {
                ships::parse_ship(&m.name, id, &m.path, v, loc, &cx)
            });
            gd.ships = ships;
        }

        // Pass 4: reputation, blueprints, contracts.
        gd.reputation_standings = load_all(
            store,
            index.of_type("SReputationStandingParams").collect(),
            &errors,
            |m, id, v| Some(missions::parse_standing(&m.name, id, v)),
        );
        let rep: HashMap<String, f64> = load_all(
            store,
            index.of_type("SReputationRewardAmount").collect(),
            &errors,
            |m, _, v| Some((m.path.clone(), json::num(v.get("reputationAmount"))?)),
        )
        .into_iter()
        .collect();
        let lk = Lookup {
            store,
            index: &index,
            rep,
            name_keys: Mutex::new(HashMap::new()),
            faction_names: Mutex::new(HashMap::new()),
            templates: Mutex::new(HashMap::new()),
        };
        gd.blueprint_pools = load_all(
            store,
            index.of_type("BlueprintPoolRecord").collect(),
            &errors,
            |m, _, v| Some(missions::parse_pool(&m.name, v, &lk)),
        );
        gd.blueprints = load_all(
            store,
            index.of_type("CraftingBlueprintRecord").collect(),
            &errors,
            |m, id, v| missions::parse_blueprint(&m.name, id, &m.path, v, &lk),
        );
        let mut links = Vec::new();
        for (contracts, l) in load_all(
            store,
            index.of_type("ContractGenerator").collect(),
            &errors,
            |m, _, v| Some(missions::parse_contracts_with_items(&m.name, v, &lk)),
        ) {
            gd.contracts.extend(contracts);
            links.extend(l);
        }

        // Pass 5: quest items.
        links.extend(
            load_all(
                store,
                index.of_type("MissionBrokerEntry").collect(),
                &errors,
                |m, _, v| Some(missions::parse_mission_entry(&m.name, v)),
            )
            .into_iter()
            .flatten(),
        );
        gd.quest_items = quest_items(store, &index, links, &errors);

        // Pass 6: description and name keys.
        let keys = desc_keys(store, &index, &errors);
        (gd.item_descs, gd.resource_descs, gd.item_names) =
            (keys.items, keys.resources, keys.names);
    }
    gd.sort();
    let mut warnings = errors.into_inner().unwrap_or_default();
    warnings.sort_by(|a, b| a.detail.cmp(&b.detail));
    warnings.extend(drift::check(&gd, present_types));
    Extracted {
        game_data: gd,
        warnings,
    }
}

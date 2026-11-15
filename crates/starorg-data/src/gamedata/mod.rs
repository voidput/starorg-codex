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
//! 4. reputation standings, blueprint pools, blueprints, contract generators.
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
use starorg_core::model::{Ammo, GameData, WeaponClass};
use starorg_core::source::Warning;
use std::collections::HashMap;
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
        gd.contracts = load_all(
            store,
            index.of_type("ContractGenerator").collect(),
            &errors,
            |m, _, v| Some(missions::parse_contracts(&m.name, v, &lk)),
        )
        .into_iter()
        .flatten()
        .collect();
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

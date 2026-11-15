//! Developer mode in the mock backend (addendum 7.7): a synthetic 300-record DataCore (three
//! types, cross references, one 1.5 MiB record), synthetic datasets per channel build and a
//! sample folder of three files, all OS-independent. The explorer, rows and diff go through
//! the same core functions as the real backend. `mock-dev.gen.json` (written by the contract
//! tests) carries this data to the TS mock (`app/src/lib/contract/mock-dev.ts`).

use super::Progress;
use super::dev::*;
use super::mock::MockBackend;
use super::types::*;
use serde_json::{Value, json};
use starorg_core::dev::dataset::{self, Dataset, RowsQuery};
use starorg_core::dev::diff::{self, TableDiff};
use starorg_core::dev::explorer::{Entry, ExplorerIndex, MAX_PAGE};
use starorg_core::dev::readable::{self, KnownRecord, LabelKind, MapLookup};
use starorg_core::dev::record;
use starorg_core::dev::sections::{Facts, SectionIndex};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

pub const SHIPS: u32 = 40;
pub const COMPONENTS: u32 = 200;
pub const TAGS: u32 = 60;
/// Ship 39 carries a long loadout: its JSON is about 1.5 MiB.
pub const HUGE_SHIP: u32 = 39;
pub const HUGE_PORTS: u32 = 3_400;
const KINDS: [&str; 4] = ["cooler", "shield", "powerplant", "quantumdrive"];
pub const FOLDER_LABEL: &str = "sample-data";
pub const FOLDER_PATH: &str = "/synthetic/sample-data";
pub const GAMEDATA: &str = "gamedata.json";

pub fn ship_guid(n: u32) -> String {
    format!("51000000-0000-4000-8000-{n:012x}")
}
pub fn component_guid(n: u32) -> String {
    format!("52000000-0000-4000-8000-{n:012x}")
}
pub fn tag_guid(n: u32) -> String {
    format!("53000000-0000-4000-8000-{n:012x}")
}

fn ship_path(n: u32) -> String {
    format!("libs/foundry/records/entities/spaceships/syn_ship_{n:03}.xml")
}
fn component_path(n: u32) -> String {
    format!(
        "libs/foundry/records/entities/scitem/ships/{}/syn_comp_{n:03}.xml",
        KINDS[(n % 4) as usize]
    )
}
const TAG_PATH: &str = "libs/foundry/records/tagdatabase/syn_tags.xml";

/// Mock string keys (see `synthetic_strings` in mock.rs): ships are rows 8k, coolers 8k+1.
fn ship_key(n: u32) -> String {
    format!("@vehicle_DescSYN_Ship_{:05}", n * 8)
}
fn component_key(n: u32) -> String {
    let i = n * 8 + 1;
    format!("@item_NameSYN_Cooler_S{}_{i:05}", i % 4 + 1)
}

fn reference(guid: String, name: String, ty: &str, path: String) -> Value {
    json!({ record::REF_KEY: guid, "name": name, "type": ty, "path": path })
}

/// The synthetic DataCore: index entries and record values (same order), and what the
/// section index knows about them (7.17).
pub struct MockCore {
    pub index: ExplorerIndex,
    pub values: Vec<Value>,
    pub facts: Vec<Option<Facts>>,
    pub sections: SectionIndex,
}

/// The item catalogue (7.17): synthetic records spread over the sections, so the item
/// finder has something to show. (struct type, item type, sub-type, size, maker, display
/// name, folder below the records root, grade, class, model, sounds). No CIG data.
type CatRow = (
    &'static str,
    &'static str,
    &'static str,
    i64,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    bool,
    bool,
);
const E: &str = "EntityClassDefinition";
pub const CATALOGUE: &[CatRow] = &[
    (
        E,
        "NOITEM_Vehicle",
        "Vehicle_Spaceship",
        1,
        "SYNA",
        "Alpha Sparrow",
        "entities/spaceships",
        "",
        "",
        true,
        true,
    ),
    (
        E,
        "NOITEM_Vehicle",
        "Vehicle_Spaceship",
        2,
        "SYNA",
        "Alpha Kestrel",
        "entities/spaceships",
        "",
        "",
        true,
        true,
    ),
    (
        E,
        "NOITEM_Vehicle",
        "Vehicle_Spaceship",
        3,
        "SYNB",
        "Beta Hauler",
        "entities/spaceships",
        "",
        "",
        true,
        true,
    ),
    (
        E,
        "NOITEM_Vehicle",
        "Vehicle_Spaceship",
        4,
        "SYNC",
        "Gamma Carrier",
        "entities/spaceships",
        "",
        "",
        true,
        true,
    ),
    (
        E,
        "NOITEM_Vehicle",
        "Vehicle_Spaceship",
        2,
        "SYNA",
        "Alpha Kestrel PU AI CIV",
        "entities/spaceships",
        "",
        "",
        true,
        true,
    ),
    (
        E,
        "NOITEM_Vehicle",
        "Vehicle_Spaceship",
        3,
        "SYNB",
        "Beta Hauler Template",
        "entities/spaceships",
        "",
        "",
        false,
        false,
    ),
    (
        E,
        "NOITEM_Vehicle",
        "Vehicle_GroundVehicle",
        1,
        "SYNB",
        "Beta Rover",
        "entities/groundvehicles",
        "",
        "",
        true,
        true,
    ),
    (
        E,
        "Paints",
        "UNDEFINED",
        1,
        "SYNA",
        "Kestrel Ember Livery",
        "entities/scitem/ships/paints",
        "",
        "",
        true,
        false,
    ),
    (
        E,
        "WeaponGun",
        "Gun",
        1,
        "SYNA",
        "Ember Repeater S1",
        "entities/scitem/ships/weapons",
        "",
        "",
        true,
        true,
    ),
    (
        E,
        "WeaponGun",
        "Gun",
        2,
        "SYNA",
        "Ember Repeater S2",
        "entities/scitem/ships/weapons",
        "",
        "",
        true,
        true,
    ),
    (
        E,
        "WeaponGun",
        "Gun",
        3,
        "SYNB",
        "Torrent Cannon S3",
        "entities/scitem/ships/weapons",
        "",
        "",
        true,
        true,
    ),
    (
        E,
        "WeaponGun",
        "Rocket",
        4,
        "SYNC",
        "Breaker Scattergun S4",
        "entities/scitem/ships/weapons",
        "",
        "",
        true,
        true,
    ),
    (
        E,
        "WeaponDefensive",
        "CountermeasureLauncher",
        1,
        "SYNB",
        "Chaff Launcher",
        "entities/scitem/ships/countermeasures",
        "",
        "",
        true,
        false,
    ),
    (
        E,
        "Cooler",
        "UNDEFINED",
        1,
        "SYNA",
        "Frost Cooler",
        "entities/scitem/ships/cooler",
        "C",
        "Military",
        true,
        false,
    ),
    (
        E,
        "Cooler",
        "UNDEFINED",
        2,
        "SYNB",
        "Glacier Cooler",
        "entities/scitem/ships/cooler",
        "A",
        "Civilian",
        true,
        false,
    ),
    (
        E,
        "PowerPlant",
        "Power",
        1,
        "SYNA",
        "Spark Power Plant",
        "entities/scitem/ships/powerplant",
        "B",
        "Industrial",
        true,
        true,
    ),
    (
        E,
        "PowerPlant",
        "Power",
        2,
        "SYNC",
        "Dynamo Power Plant",
        "entities/scitem/ships/powerplant",
        "D",
        "Civilian",
        true,
        true,
    ),
    (
        E,
        "Shield",
        "UNDEFINED",
        1,
        "SYNB",
        "Aegis Shell Shield",
        "entities/scitem/ships/shieldgenerator",
        "A",
        "Military",
        true,
        true,
    ),
    (
        E,
        "Shield",
        "UNDEFINED",
        3,
        "SYNC",
        "Bastion Shield",
        "entities/scitem/ships/shieldgenerator",
        "C",
        "Stealth",
        true,
        true,
    ),
    (
        E,
        "QuantumDrive",
        "UNDEFINED",
        1,
        "SYNA",
        "Leap Quantum Drive",
        "entities/scitem/ships/quantumdrive",
        "B",
        "Competition",
        true,
        true,
    ),
    (
        E,
        "QuantumDrive",
        "UNDEFINED",
        2,
        "SYNB",
        "Vault Quantum Drive",
        "entities/scitem/ships/quantumdrive",
        "C",
        "Civilian",
        true,
        true,
    ),
    (
        E,
        "QuantumDrive",
        "UNDEFINED",
        1,
        "SYNA",
        "Leap Quantum Drive Template",
        "entities/scitem/ships/quantumdrive",
        "",
        "",
        false,
        false,
    ),
    (
        E,
        "Missile",
        "Missile",
        1,
        "SYNC",
        "Wasp Missile",
        "entities/scitem/ships/missiles",
        "",
        "",
        true,
        true,
    ),
    (
        E,
        "Missile",
        "Torpedo",
        3,
        "SYNC",
        "Hammer Torpedo",
        "entities/scitem/ships/missiles",
        "",
        "",
        true,
        true,
    ),
    (
        E,
        "MissileLauncher",
        "MissileRack",
        2,
        "SYNB",
        "Quiver Missile Rack",
        "entities/scitem/ships/missileracks",
        "",
        "",
        true,
        false,
    ),
    (
        E,
        "WeaponPersonal",
        "Medium",
        2,
        "SYNA",
        "Viper Rifle",
        "entities/scitem/weapons/fps_weapons",
        "",
        "",
        true,
        true,
    ),
    (
        E,
        "WeaponPersonal",
        "Small",
        1,
        "SYNB",
        "Hornet Pistol",
        "entities/scitem/weapons/fps_weapons",
        "",
        "",
        true,
        true,
    ),
    (
        E,
        "WeaponPersonal",
        "Large",
        3,
        "SYNC",
        "Thumper Launcher",
        "entities/scitem/weapons/fps_weapons",
        "",
        "",
        true,
        true,
    ),
    (
        E,
        "Gadget",
        "Gadget",
        1,
        "SYNB",
        "Tracker Gadget",
        "entities/scitem/weapons/gadgets",
        "",
        "",
        true,
        false,
    ),
    (
        E,
        "Char_Armor_Helmet",
        "UNDEFINED",
        1,
        "SYNA",
        "Sentinel Helmet",
        "entities/scitem/characters/human/armor",
        "",
        "",
        true,
        false,
    ),
    (
        E,
        "Char_Armor_Torso",
        "UNDEFINED",
        1,
        "SYNA",
        "Sentinel Core",
        "entities/scitem/characters/human/armor",
        "",
        "",
        true,
        false,
    ),
    (
        E,
        "Char_Clothing_Feet",
        "UNDEFINED",
        1,
        "SYNC",
        "Trail Boots",
        "entities/scitem/characters/human/clothing",
        "",
        "",
        true,
        false,
    ),
    (
        E,
        "WeaponAttachment",
        "Barrel",
        1,
        "SYNA",
        "Hush Suppressor",
        "entities/scitem/weapons/attachments",
        "",
        "",
        true,
        false,
    ),
    (
        E,
        "WeaponAttachment",
        "IronSight",
        2,
        "SYNB",
        "Clear Scope 4x",
        "entities/scitem/weapons/attachments",
        "",
        "",
        true,
        false,
    ),
    (
        E,
        "WeaponAttachment",
        "Magazine",
        1,
        "SYNA",
        "Viper Magazine",
        "entities/scitem/weapons/attachments",
        "",
        "",
        true,
        false,
    ),
    (
        E,
        "Food",
        "Bar",
        1,
        "",
        "Ration Bar",
        "entities/scitem/consumables/food",
        "",
        "",
        true,
        false,
    ),
    (
        E,
        "Drink",
        "Can",
        1,
        "",
        "Fizz Can",
        "entities/scitem/consumables/drink",
        "",
        "",
        true,
        false,
    ),
    (
        E,
        "FPS_Consumable",
        "Medical",
        1,
        "SYNB",
        "Mend Pen",
        "entities/scitem/consumables/medical",
        "",
        "",
        true,
        true,
    ),
    (
        E,
        "WeaponMining",
        "Gun",
        1,
        "SYNC",
        "Drill Head S1",
        "entities/scitem/ships/mining",
        "",
        "",
        true,
        true,
    ),
    (
        E,
        "MiningModifier",
        "Gun",
        1,
        "SYNC",
        "Focus Module",
        "entities/scitem/ships/mining",
        "",
        "",
        true,
        false,
    ),
    (
        E,
        "Cargo",
        "UNDEFINED",
        1,
        "",
        "Copper Crate",
        "entities/commodities/metals",
        "",
        "",
        true,
        false,
    ),
    (
        E,
        "ShopDisplay",
        "UNDEFINED",
        1,
        "",
        "Rifle Rack Display",
        "entities/scitem/shopdisplays",
        "",
        "",
        true,
        false,
    ),
    (
        E,
        "NOITEM_Player",
        "UNDEFINED",
        1,
        "",
        "Player Body",
        "entities/scitem/characters/human/body",
        "",
        "",
        true,
        true,
    ),
    (
        E,
        "",
        "",
        0,
        "",
        "Guard NPC 01",
        "actor/actors/npc_archetypes/guards",
        "",
        "",
        true,
        true,
    ),
    (
        E,
        "Usable",
        "UNDEFINED",
        1,
        "",
        "Bench Seat",
        "entities/scitem/usables",
        "",
        "",
        true,
        false,
    ),
    (
        E,
        "Door",
        "UNDEFINED",
        1,
        "",
        "Hangar Door",
        "entities/scitem/doors",
        "",
        "",
        true,
        true,
    ),
    (
        "ContractGenerator",
        "",
        "",
        0,
        "",
        "Delivery Contracts",
        "contracts/contractgenerator",
        "",
        "",
        false,
        false,
    ),
    (
        "MissionBrokerEntry",
        "",
        "",
        0,
        "",
        "Salvage Run",
        "missionbroker/pu_missions",
        "",
        "",
        false,
        false,
    ),
    (
        "StarMapObject",
        "",
        "",
        0,
        "",
        "Synthetic Moon",
        "starmap/pu/system",
        "",
        "",
        false,
        false,
    ),
    (
        "StarMapObject",
        "",
        "",
        0,
        "",
        "Synthetic Station",
        "starmap/pu/system",
        "",
        "",
        false,
        false,
    ),
    (
        "CraftingBlueprintRecord",
        "",
        "",
        0,
        "",
        "BP Viper Rifle",
        "crafting/blueprints/weapons",
        "",
        "",
        false,
        false,
    ),
    (
        "SCItemManufacturer",
        "",
        "",
        0,
        "",
        "Alpha Synthetics Maker",
        "scitemmanufacturer",
        "",
        "",
        false,
        false,
    ),
    (
        "FactionReputation",
        "",
        "",
        0,
        "",
        "Courier Guild",
        "reputation/factions",
        "",
        "",
        false,
        false,
    ),
    (
        "LoadoutKit",
        "",
        "",
        0,
        "",
        "Guard Kit",
        "loadoutkits/guards",
        "",
        "",
        false,
        false,
    ),
    (
        "ShopFranchise",
        "",
        "",
        0,
        "",
        "Rifle Store",
        "franchises",
        "",
        "",
        false,
        false,
    ),
    (
        "BuildingBlocks_Canvas",
        "",
        "",
        0,
        "",
        "Hud Canvas",
        "ui/hud",
        "",
        "",
        false,
        false,
    ),
    (
        "AmmoParams",
        "",
        "",
        0,
        "",
        "Ember Rounds",
        "ammoparams/vehicle",
        "",
        "",
        false,
        false,
    ),
];

pub fn catalogue_guid(n: usize) -> String {
    format!("54000000-0000-4000-8000-{n:012x}")
}

/// The record name of catalogue row `n`: `SYN_Alpha_Kestrel_PU_AI_CIV`.
pub fn catalogue_name(n: usize) -> String {
    format!("SYN_{}", CATALOGUE[n].5.replace([' ', '-'], "_"))
}

/// Synthetic headline stats of the catalogue (the real backend reads them from GameData).
pub fn catalogue_stats() -> HashMap<String, Vec<Stat>> {
    let st = |key: &str, label: &str, value: f64, unit: Option<&str>| Stat {
        key: key.into(),
        label: label.into(),
        value,
        unit: unit.map(str::to_string),
    };
    CATALOGUE
        .iter()
        .enumerate()
        .filter_map(|(n, r)| {
            let k = n as f64;
            let v = match r.1 {
                "NOITEM_Vehicle" => vec![
                    st("size", "Size", r.3 as f64, None),
                    st("crew", "Crew", (r.3 as f64).max(1.0), None),
                    st("cargo", "Cargo", r.3 as f64 * 16.0, Some("SCU")),
                    st("scm", "SCM", 240.0 - r.3 as f64 * 30.0, Some("m/s")),
                ],
                "WeaponGun" | "WeaponPersonal" => vec![
                    st("dps", "DPS", 120.0 + k * 11.5, None),
                    st("range", "Range", 1500.0 + k * 50.0, Some("m")),
                ],
                "Cooler" | "PowerPlant" | "Shield" | "QuantumDrive" => vec![
                    st("power", "Power", 10.0 + k, None),
                    st("health", "Health", 500.0 + k * 25.0, Some("HP")),
                ],
                "Missile" => vec![st("damage", "Damage", 1000.0 + k * 100.0, None)],
                _ => return None,
            };
            Some((catalogue_guid(n), v))
        })
        .collect()
}

/// Archive files the catalogue's models are (for "Open owning record"). The first ships use
/// the mock archive's own `syn_ship_00n.skin` files, so Files can open them.
pub fn catalogue_geometry(n: usize) -> Option<String> {
    let r = CATALOGUE.get(n)?;
    (r.0 == E && r.9).then(|| {
        if n < 6 {
            format!("Data\\Objects\\Spaceships\\Ships\\SYN\\syn_ship_{n:03}\\syn_ship_{n:03}.skin")
        } else {
            format!(
                "Data\\Objects\\syn\\{}.cga",
                catalogue_name(n).to_lowercase()
            )
        }
    })
}

fn catalogue_facts(r: &CatRow) -> Option<Facts> {
    let some = |s: &str| (!s.is_empty()).then(|| s.to_string());
    (r.0 == E).then(|| Facts {
        item_type: some(r.1),
        sub_type: some(r.2),
        size: (r.3 > 0).then_some(r.3),
        manufacturer: some(r.4),
        display: some(r.5),
        grade: some(r.7),
        class: some(r.8),
        model: r.9,
        sounds: r.10,
    })
}

/// Records the mock marks against another channel (7.17): every third catalogue record
/// changed, every seventh added, always the same so both mocks agree.
pub fn catalogue_marks() -> Vec<(String, Change)> {
    (0..CATALOGUE.len())
        .filter_map(|n| {
            let c = if n % 7 == 3 {
                Change::Added
            } else if n % 3 == 1 {
                Change::Changed
            } else {
                return None;
            };
            Some((catalogue_guid(n), c))
        })
        .collect()
}

/// `manufacturer_Name<CODE>` of the mock's makers.
pub fn maker_name(code: &str) -> Option<String> {
    MAKERS
        .iter()
        .find(|x| x.0.eq_ignore_ascii_case(code))
        .map(|x| x.1.to_string())
}

fn build_core() -> MockCore {
    let mut entries = Vec::new();
    let mut values = Vec::new();
    let ship_name = |n: u32| format!("SYN_Ship_{n:03}");
    let comp_name = |n: u32| format!("SYN_{}_{n:03}", KINDS[(n % 4) as usize]);
    for n in 0..SHIPS {
        entries.push(Entry {
            guid: ship_guid(n),
            name: ship_name(n),
            struct_type: "SynShip".into(),
            path: ship_path(n),
            main: true,
            tag: None,
        });
        let comps: Vec<Value> = (0..3)
            .map(|k| {
                let c = (n * 5 + k) % COMPONENTS;
                reference(
                    component_guid(c),
                    comp_name(c),
                    "SynComponent",
                    component_path(c),
                )
            })
            .collect();
        let mut v = json!({
            "_Type_": "SynShip",
            "DisplayName": format!("Synthetic hull {n}"),
            "Description": ship_key(n),
            "Crew": n % 4 + 1,
            "Mass": 10_000 + n * 250,
            "Speed": { "Scm": 150 + n, "Max": 900 + n * 7, "Boost": { "Forward": 1.5, "Back": 0.75 } },
            "Components": comps,
        });
        if n == HUGE_SHIP {
            v["Loadout"] = Value::Array(
                (0..HUGE_PORTS)
                    .map(|i| {
                        json!({
                            "Port": format!("hardpoint_syn_{i:05}"),
                            "Size": i % 5 + 1,
                            "Note": "synthetic loadout padding ".repeat(16),
                        })
                    })
                    .collect(),
            );
        }
        values.push(json!({
            "_RecordName_": ship_name(n),
            "_RecordId_": ship_guid(n),
            "_RecordValue_": v,
        }));
    }
    for n in 0..COMPONENTS {
        entries.push(Entry {
            guid: component_guid(n),
            name: comp_name(n),
            struct_type: "SynComponent".into(),
            path: component_path(n),
            main: true,
            tag: None,
        });
        let s = n % SHIPS;
        values.push(json!({
            "_RecordName_": comp_name(n),
            "_RecordId_": component_guid(n),
            "_RecordValue_": {
                "_Type_": "SynComponent",
                "Name": component_key(n),
                "Size": n % 4 + 1,
                "Grade": (["A", "B", "C", "D"][(n % 4) as usize]),
                "Health": 500 + n * 3,
                "Ship": reference(ship_guid(s), ship_name(s), "SynShip", ship_path(s)),
            },
        }));
    }
    for n in 0..TAGS {
        // One file: only the last record is a main record.
        entries.push(Entry {
            guid: tag_guid(n),
            name: format!("SynTag{n:02}"),
            struct_type: "SynTag".into(),
            path: TAG_PATH.into(),
            main: n == TAGS - 1,
            tag: None,
        });
        values.push(json!({
            "_RecordName_": format!("SynTag{n:02}"),
            "_RecordId_": tag_guid(n),
            "_RecordValue_": { "_Type_": "SynTag", "Label": format!("tag {n}") },
        }));
    }
    let mut facts: Vec<Option<Facts>> = vec![None; entries.len()];
    for (n, r) in CATALOGUE.iter().enumerate() {
        let name = catalogue_name(n);
        let path = format!("libs/foundry/records/{}/{}.xml", r.6, name.to_lowercase());
        entries.push(Entry {
            guid: catalogue_guid(n),
            name: format!("{}.{name}", r.0),
            struct_type: r.0.into(),
            path,
            main: true,
            tag: None,
        });
        let body = if r.0 == E {
            json!({
                "_Type_": E,
                "Components": [{
                    "_Type_": "SAttachableComponentParams",
                    "AttachDef": {
                        "_Type_": "SItemDefinition",
                        "Type": r.1, "SubType": r.2, "Size": r.3, "Grade": r.7,
                        "Manufacturer": r.4,
                        "Localization": { "Name": r.5 },
                    },
                }],
            })
        } else {
            json!({ "_Type_": r.0, "displayName": r.5 })
        };
        values.push(json!({
            "_RecordName_": format!("{}.{name}", r.0),
            "_RecordId_": catalogue_guid(n),
            "_RecordValue_": body,
        }));
        facts.push(catalogue_facts(r));
    }
    let index = ExplorerIndex::new(entries);
    let sparse: Vec<(u32, Facts)> = facts
        .iter()
        .enumerate()
        .filter_map(|(i, f)| f.clone().map(|f| (i as u32, f)))
        .collect();
    let sections = SectionIndex::new(&index, &sparse, &maker_name);
    MockCore {
        index,
        values,
        facts,
        sections,
    }
}

pub fn core() -> &'static MockCore {
    static CORE: OnceLock<MockCore> = OnceLock::new();
    CORE.get_or_init(build_core)
}

/// Synthetic manufacturer names (`manufacturer_Name<code>`) of the mock's codes.
pub const MAKERS: [(&str, &str); 3] = [
    ("SYNA", "Alpha Synthetics"),
    ("SYNB", "Beta Works"),
    ("SYNC", "Gamma Test Yards"),
];

/// The records the mock's labels know (7.14): every synthetic record, components with their
/// `Name` key, ships with their plain `DisplayName`, tags with their label.
pub fn known() -> Vec<KnownRecord> {
    let c = core();
    c.index
        .entries()
        .iter()
        .zip(&c.values)
        .map(|(e, v)| {
            let tag = e.struct_type == "SynTag";
            let field = match e.struct_type.as_str() {
                "SynTag" => "/Label",
                // Ships carry a plain display name; their key is a description.
                "SynShip" => "/DisplayName",
                "SynComponent" => "/Name",
                // The item catalogue (7.17): the entity's own name, else `displayName`.
                E => "/Components/0/AttachDef/Localization/Name",
                _ => "/displayName",
            };
            KnownRecord {
                guid: Some(e.guid.clone()),
                name: e.name.clone(),
                kind: if tag {
                    LabelKind::Tag
                } else {
                    LabelKind::Record
                },
                display: v["_RecordValue_"]
                    .pointer(field)
                    .and_then(Value::as_str)
                    .map(str::to_string),
            }
        })
        .collect()
}

/// Synthetic strings by lower-case key: (stock, result).
fn texts() -> &'static HashMap<String, (String, String)> {
    static T: OnceLock<HashMap<String, (String, String)>> = OnceLock::new();
    T.get_or_init(|| {
        super::mock::synthetic_strings()
            .into_iter()
            .map(|r| (r.key.to_lowercase(), (r.original, r.result)))
            .collect()
    })
}

/// Enhanced text of the keys the known records name, where it differs from stock (lower-case
/// key -> text); the TS mock gets the same map.
pub fn enhanced() -> &'static HashMap<String, String> {
    static E: OnceLock<HashMap<String, String>> = OnceLock::new();
    E.get_or_init(|| {
        known()
            .iter()
            .filter_map(|r| {
                let k = r.display.as_deref()?.trim_start_matches('@').to_lowercase();
                let (o, res) = texts().get(&k)?;
                (o != res).then(|| (k, res.clone()))
            })
            .collect()
    })
}

/// The mock's lookup: known records, synthetic stock text and manufacturer names, enhanced
/// text. `bare` for dataset rows.
pub fn lookup(bare: bool) -> MapLookup {
    let mut m = MapLookup::new();
    for r in known() {
        m.add(r);
    }
    m.with_stock(Arc::new(|k: &str| {
        let lk = k.to_lowercase();
        if let Some(code) = lk.strip_prefix("manufacturer_name") {
            return MAKERS
                .iter()
                .find(|x| x.0.eq_ignore_ascii_case(code))
                .map(|x| x.1.to_string());
        }
        texts().get(&lk).map(|x| x.0.clone())
    }))
    .with_enhanced(Arc::new(|k: &str| {
        enhanced().get(&k.to_lowercase()).cloned()
    }))
    .with_bare_keys(bare)
}

/// Synthetic GameData-like dataset of a channel's build. PTU differs from LIVE (changed,
/// added and removed rows); HOTFIX equals LIVE except one value.
pub fn build_value(channel: &str) -> Value {
    let ptu = channel == "PTU";
    let hot = channel == "HOTFIX";
    let components: Vec<Value> = (0..24u32)
        .filter(|n| !(ptu && *n == 5))
        .map(|n| {
            let mut health = 500 + n * 3;
            if ptu && n % 4 == 1 {
                health += 25;
            }
            if hot && n == 2 {
                health += 1;
            }
            json!({
                "name": format!("Sample {} {n:02}", ["Cooler", "Shield", "Power Plant", "Quantum Drive"][(n % 4) as usize]),
                "recordName": format!("SYN_{}_{n:03}", KINDS[(n % 4) as usize]),
                "kind": KINDS[(n % 4) as usize],
                "size": n % 4 + 1,
                "grade": (["A", "B", "C", "D"][(n % 4) as usize]),
                "manufacturer": (["SYNA", "SYNB", "SYNC"][(n % 3) as usize]),
                "health": health,
                "loc": { "name": format!("item_NameSYN_Cooler_S{}_{:05}", (n * 8 + 1) % 4 + 1, n * 8 + 1) },
            })
        })
        .chain(ptu.then(|| json!({
            "name": "Sample Cooler 99",
            "recordName": "SYN_cooler_099",
            "kind": "cooler",
            "size": 2,
            "grade": "A",
            "manufacturer": "SYNA",
            "health": 777,
            "loc": { "name": null },
        })))
        .collect();
    let ships: Vec<Value> = (0..12u32)
        .map(|n| {
            json!({
                "name": format!("Synthetic hull {n}"),
                "recordName": format!("SYN_Ship_{n:03}"),
                "crew": n % 4 + 1,
                "scm": if ptu && n == 3 { 200 } else { 150 + n },
                "roles": ["Test", if n % 2 == 0 { "Fighter" } else { "Cargo" }],
            })
        })
        .collect();
    // Weapons look like the real GameData table: type > subType > size, maker, path. PTU
    // drops one, adds one and changes two.
    let weapons: Vec<Value> = (0..10u32)
        .filter(|n| !(ptu && *n == 7))
        .chain(ptu.then_some(10))
        .map(|n| {
            let mining = n % 5 == 4;
            json!({
                "name": format!("Test Repeater {n}"),
                "className": format!("SYN_Laser_S{}_{n:02}", n % 6 + 1),
                "path": format!(
                    "entities/scitem/ships/weapons/{}syn_laser_s{}_{n:02}",
                    if mining { "mining/" } else { "" },
                    n % 6 + 1
                ),
                "type": if mining { "WeaponMining" } else { "WeaponGun" },
                "subType": if n % 3 == 2 { "Rocket" } else { "Gun" },
                "size": n % 3 + 1,
                "manufacturer": (["SYNA", "SYNB"][(n % 2) as usize]),
                "dps": 100.25 + f64::from(n) * 12.5 + if ptu && (n == 0 || n == 4) { 10.0 } else { 0.0 },
                "ammo": { "speed": 1800 + n * 10, "range": 2000 },
            })
        })
        .collect();
    json!({
        "_schema": 1,
        "_build": { "channel": channel, "version": "4.10.0-mock" },
        "components": components,
        "ships": ships,
        "weapons": weapons,
    })
}

/// The sample folder's three files (shapes of the sc-gamedata files, synthetic values).
pub fn folder_files() -> Vec<(&'static str, Value)> {
    let weapons = json!({
        "_source": "synthetic",
        "_extracted": "2026-11-14T12:00:00Z",
        "_build": "4.10.0-mock",
        "shipWeapons": (0..30u32).map(|n| json!({
            "name": format!("Test Repeater {n}"),
            "recordName": format!("SYN_Laser_S{}_{n:02}", n % 6 + 1),
            "id": ship_guid(n % SHIPS),
            "size": n % 6 + 1,
            "type": (["Gun", "Cannon", "Repeater"][(n % 3) as usize]),
            "dps": 100.25 + f64::from(n) * 12.5,
            "modes": [{ "name": "Auto", "rpm": 600 + n }],
        })).collect::<Vec<_>>(),
        "missiles": (0..8u32).map(|n| json!({
            "name": format!("Sample Missile {n}"),
            "recordName": format!("SYN_Missile_S{}_{n:02}", n % 3 + 1),
            "size": n % 3 + 1,
            "damage": 1000 + n * 250,
        })).collect::<Vec<_>>(),
        "summary": { "shipWeapons": 30, "missiles": 8 },
    });
    let ships = json!({
        "_source": "synthetic",
        "_extracted": "2026-11-14T12:00:00Z",
        "_build": "4.10.0-mock",
        "vehicles": (0..12u32).map(|n| json!({
            "name": format!("Synthetic hull {n}"),
            "className": format!("SYN_Ship_{n:03}"),
            "manufacturer": (["Alpha Synthetics", "Beta Works", "Gamma Test Yards"][(n % 3) as usize]),
            "crew": n % 4 + 1,
            "cargo": n * 8,
        })).collect::<Vec<_>>(),
        "byManufacturer": { "Alpha Synthetics": 4, "Beta Works": 4, "Gamma Test Yards": 4 },
        "summary": { "vehicles": 12 },
    });
    let strings: serde_json::Map<String, Value> = (0..40u32)
        .map(|n| {
            let i = n * 8;
            (
                format!("vehicle_DescSYN_Ship_{i:05}"),
                Value::String(format!(
                    "Synthetic hull {i}. A test ship description.\\nSecond line."
                )),
            )
        })
        .collect();
    let strings = json!({
        "_source": "synthetic",
        "_extracted": "2026-11-14T12:00:00Z",
        "_build": "4.10.0-mock",
        "language": "english",
        "strings": strings,
        "summary": { "strings": 40 },
    });
    vec![
        ("game-ships.json", ships),
        ("game-strings-english.json", strings),
        ("game-weapons.json", weapons),
    ]
}

fn to_dataset(file: &str, v: Value) -> Dataset {
    let bytes = serde_json::to_string(&v)
        .map(|s| s.len() as u64)
        .unwrap_or(0);
    dataset::infer(file, bytes, v)
}

/// Summary, per-table results and both sides' datasets of the last comparison.
type MockDiff = (
    DevDiffSummary,
    Vec<TableDiff>,
    Vec<(String, Arc<Dataset>)>,
    Vec<(String, Arc<Dataset>)>,
);

/// The mock's Developer mode state.
#[derive(Default)]
pub struct DevMock {
    /// The channel whose explorer is open (`dev_open`).
    open: Option<(String, String)>,
    folder: bool,
    diff: Option<MockDiff>,
    /// The channel the explorer is compared with (7.17).
    compared: Option<DevCompare>,
}

fn not_found(m: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::NotFound, m)
}

impl MockBackend {
    fn dev_on(&self, r: &ChannelRef) -> CmdResult<()> {
        if !self.dev_mode_on() {
            return Err(disabled());
        }
        if !self.has_channel(r) {
            return Err(not_found(format!(
                "No channel {} in install {}",
                r.channel, r.install
            )));
        }
        Ok(())
    }

    /// Requests start the explorer when it is not open, as the real worker does.
    fn explorer(&self, r: &ChannelRef) -> CmdResult<&'static MockCore> {
        self.dev_on(r)?;
        self.dev_state().open = Some((r.install.clone(), r.channel.clone()));
        Ok(core())
    }

    fn status(&self, r: &ChannelRef) -> DevStatus {
        let open = self.dev_state().open.clone() == Some((r.install.clone(), r.channel.clone()));
        let c = core();
        DevStatus {
            state: if open {
                WorkerState::Ready
            } else {
                WorkerState::Stopped
            },
            channel: Some(r.channel.clone()),
            build: Some("4.10.0-mock".into()),
            records: if open { c.index.len() as u32 } else { 0 },
            main: if open { c.index.main_count() as u32 } else { 0 },
            types: if open {
                c.index.types().len() as u32
            } else {
                0
            },
            started: open.then(|| "2026-11-14T21:30:05Z".into()),
            start_ms: open.then_some(840),
            memory: open.then_some(412 * 1024 * 1024),
            error: None,
            sections: open.then(|| SectionStats {
                build_ms: 210,
                scan_ms: 95,
                bytes: 11 * 1024 * 1024,
                entities: c.facts.iter().flatten().count() as u32,
                models: c.facts.iter().flatten().filter(|f| f.model).count() as u32,
                sounds: c.facts.iter().flatten().filter(|f| f.sounds).count() as u32,
                verified: true,
            }),
            compared: if open {
                self.dev_state().compared.clone()
            } else {
                None
            },
        }
    }

    fn sources_of(&self, r: &ChannelRef) -> Vec<DevSource> {
        let mut chans = self.channel_names(&r.install);
        chans.sort_by_key(|c| *c != r.channel);
        let mut out: Vec<DevSource> = chans
            .iter()
            .map(|c| {
                let d = to_dataset(GAMEDATA, build_value(c));
                DevSource {
                    id: format!("build:{c}"),
                    kind: SourceKind::Build,
                    label: format!("{c} build"),
                    build: Some("4.10.0-mock".into()),
                    state: SourceState::Ready,
                    detail: None,
                    datasets: vec![DevDatasetSummary {
                        file: GAMEDATA.into(),
                        bytes: d.info.bytes,
                        tables: d.tables.len() as u32,
                        rows: d.tables.iter().map(|t| t.rows.len() as u32).sum(),
                    }],
                }
            })
            .collect();
        if self.dev_state().folder {
            out.push(DevSource {
                id: "folder:0".into(),
                kind: SourceKind::Folder,
                label: FOLDER_LABEL.into(),
                build: None,
                state: SourceState::Ready,
                detail: Some(FOLDER_PATH.into()),
                datasets: folder_files()
                    .into_iter()
                    .map(|(f, v)| {
                        let d = to_dataset(f, v);
                        DevDatasetSummary {
                            file: f.into(),
                            bytes: d.info.bytes,
                            tables: d.tables.len() as u32,
                            rows: d.tables.iter().map(|t| t.rows.len() as u32).sum(),
                        }
                    })
                    .collect(),
            });
        }
        out.push(DevSource {
            id: format!("dump:{}", r.channel),
            kind: SourceKind::Dump,
            label: "Published dump".into(),
            build: None,
            state: SourceState::Off,
            detail: Some(if self.dumps_on() {
                "Follow-up: not in this build".into()
            } else {
                "Off (Settings > Network)".into()
            }),
            datasets: vec![],
        });
        out
    }

    fn datasets_of(&self, r: &ChannelRef, source: &str) -> CmdResult<Vec<(String, Arc<Dataset>)>> {
        if let Some(c) = source.strip_prefix("build:") {
            if !self.channel_names(&r.install).iter().any(|x| x == c) {
                return Err(not_found(format!("no source {source}")));
            }
            return Ok(vec![(
                GAMEDATA.into(),
                Arc::new(to_dataset(GAMEDATA, build_value(c))),
            )]);
        }
        if source == "folder:0" && self.dev_state().folder {
            return Ok(folder_files()
                .into_iter()
                .map(|(f, v)| (f.to_string(), Arc::new(to_dataset(f, v))))
                .collect());
        }
        if source.starts_with("dump:") {
            return Err(dump_refusal(false));
        }
        Err(not_found(format!("no source {source}")))
    }

    fn dataset_of(&self, r: &ChannelRef, source: &str, name: &str) -> CmdResult<Arc<Dataset>> {
        self.dev_on(r)?;
        self.datasets_of(r, source)?
            .into_iter()
            .find(|d| d.0 == name)
            .map(|d| d.1)
            .ok_or_else(|| not_found(format!("no dataset {name}")))
    }

    fn progress_job(
        &self,
        label: &str,
        steps: u32,
        channel: &str,
        progress: Progress<'_>,
    ) -> JobProgress {
        let job = self.next_job_id();
        let p = JobProgress {
            job,
            kind: JobKind::Dev,
            channel: Some(channel.into()),
            phase: JobPhase::Running,
            step: 1,
            steps,
            label: label.into(),
            cancellable: false,
        };
        progress(p.clone());
        p
    }

    fn wait(&self) {
        let d = self.job_delay();
        if !d.is_zero() {
            std::thread::sleep(d);
        }
    }
}

fn identity_options(a: Option<&dataset::Table>, b: Option<&dataset::Table>) -> Vec<String> {
    let cols = |t: Option<&dataset::Table>| -> Vec<String> {
        t.map(|t| {
            t.columns
                .iter()
                .filter(|c| c.kind.scalar())
                .map(|c| c.name.clone())
                .collect()
        })
        .unwrap_or_default()
    };
    let (ca, cb) = (cols(a), cols(b));
    match (a, b) {
        (Some(_), Some(_)) => ca.into_iter().filter(|c| cb.contains(c)).collect(),
        (Some(_), None) => ca,
        _ => cb,
    }
}

impl DevBackend for MockBackend {
    fn dev_status(&self, req: ChannelRef) -> CmdResult<DevStatus> {
        self.dev_on(&req)?;
        Ok(self.status(&req))
    }

    fn dev_open(&self, req: ChannelRef, progress: Progress<'_>) -> CmdResult<DevStatus> {
        self.dev_on(&req)?;
        let mut p = self.progress_job("Opening the game data", 2, &req.channel, progress);
        self.wait();
        p.step = 2;
        p.label = "Indexing records".into();
        progress(p.clone());
        self.wait();
        p.phase = JobPhase::Done;
        p.label = "Explorer ready".into();
        progress(p);
        self.explorer(&req)?;
        Ok(self.status(&req))
    }

    fn dev_close(&self, req: ChannelRef) -> CmdResult<DevStatus> {
        self.dev_on(&req)?;
        self.dev_state().open = None;
        Ok(self.status(&req))
    }

    fn dev_types(&self, req: ChannelRef) -> CmdResult<Vec<TypeCount>> {
        Ok(self.explorer(&req)?.index.types())
    }

    fn dev_tree(&self, req: DevTreeRequest) -> CmdResult<TreePage> {
        let c = self.explorer(&req.target)?;
        if req.root == TreeRoot::Section {
            return Ok(c.sections.tree(
                &c.index,
                &req.node,
                req.offset as usize,
                (req.limit as usize).min(MAX_PAGE),
                req.main_only,
                req.player_only,
            ));
        }
        Ok(c.index.tree(
            req.root,
            &req.node,
            req.offset as usize,
            (req.limit as usize).min(MAX_PAGE),
            req.main_only,
        ))
    }

    fn dev_list(&self, req: DevListRequest) -> CmdResult<ListPage> {
        let c = self.explorer(&req.target)?;
        let mut q = req.query;
        q.limit = q.limit.min(MAX_PAGE as u32);
        let marked = self.dev_state().compared.is_some();
        let mut page = if marked {
            let mut si = c.sections.clone();
            si.set_changes(
                catalogue_marks()
                    .into_iter()
                    .filter_map(|(g, ch)| c.index.by_guid(&g).map(|i| (i, ch))),
            );
            si.list(&c.index, &q)
        } else {
            c.sections.list(&c.index, &q)
        };
        let l = lookup(false);
        let stats = catalogue_stats();
        for it in &mut page.items {
            if it.detail.is_none()
                && let Some(r) = readable::Lookup::by_guid(&l, &it.guid)
            {
                let rd = readable::record_label(&l, &r);
                if rd.detail.is_some() {
                    it.detail = rd.detail;
                    it.label = rd.text;
                }
            }
            if let Some(s) = stats.get(&it.guid) {
                it.stats = s.clone();
            }
        }
        Ok(page)
    }

    fn dev_owner(&self, req: DevOwnerRequest) -> CmdResult<Vec<SearchHit>> {
        let c = self.explorer(&req.target)?;
        let want = req.path.replace('/', "\\").to_lowercase();
        Ok((0..CATALOGUE.len())
            .filter(|n| catalogue_geometry(*n).is_some_and(|g| g.to_lowercase() == want))
            .filter_map(|n| c.index.by_guid(&catalogue_guid(n)))
            .filter_map(|i| c.index.entry(i))
            .map(|e| SearchHit {
                guid: e.guid.clone(),
                name: e.name.clone(),
                struct_type: e.struct_type.clone(),
                path: e.path.clone(),
                main: e.main,
            })
            .collect())
    }

    fn dev_compare(&self, req: DevCompareRequest) -> CmdResult<Option<DevCompare>> {
        self.explorer(&req.target)?;
        if !req.other.as_deref().is_some_and(|o| {
            o != req.target.channel
                && self.has_channel(&ChannelRef {
                    install: req.target.install.clone(),
                    channel: o.to_string(),
                })
        }) {
            self.dev_state().compared = None;
            return Ok(None);
        }
        let marks = catalogue_marks();
        let added = marks.iter().filter(|m| m.1 == Change::Added).count() as u32;
        let out = DevCompare {
            other: req.other.unwrap_or_default(),
            added,
            changed: marks.len() as u32 - added,
            removed: 2,
            marked: marks.len() as u32,
            ms: 12,
        };
        self.dev_state().compared = Some(out.clone());
        Ok(Some(out))
    }

    fn dev_search(&self, req: DevSearchRequest) -> CmdResult<SearchPage> {
        let c = self.explorer(&req.target)?;
        Ok(c.index.search(&req.query, req.limit as usize))
    }

    fn dev_record(&self, req: DevRecordRequest) -> CmdResult<RecordView> {
        let c = self.explorer(&req.target)?;
        let i = c
            .index
            .find(&req.what)
            .ok_or_else(|| not_found(format!("no record {:?}", req.what)))?;
        let v = &c.values[i];
        let (json, truncated) =
            record::view_json(v, &req.pointer, req.depth.map(|d| d as usize)).map_err(not_found)?;
        let node = v.pointer(&req.pointer).unwrap_or(v);
        let loc = record::loc_keys(node)
            .into_iter()
            .filter_map(|k| {
                let text = self.stock_text(k.trim_start_matches('@'))?;
                Some(LocText { key: k, text })
            })
            .collect();
        let sent: Value = serde_json::from_str(&json).unwrap_or(Value::Null);
        let labels = readable::labels_in(&lookup(false), &sent);
        let e = c.index.entry(i).expect("found");
        Ok(RecordView {
            header: RecordHeader {
                guid: e.guid.clone(),
                name: e.name.clone(),
                struct_type: e.struct_type.clone(),
                path: e.path.clone(),
                tag: e.tag.clone(),
                main: e.main,
                bytes: serde_json::to_string(v)
                    .map(|s| s.len() as u64)
                    .unwrap_or(0),
            },
            json,
            pointer: req.pointer,
            truncated,
            loc,
            labels,
        })
    }

    /// No native dialog in the mock: always "cancelled", nothing written.
    fn dev_export_record(&self, req: DevExportRecordRequest) -> CmdResult<DevExportResult> {
        let c = self.explorer(&req.target)?;
        c.index
            .find(&req.what)
            .ok_or_else(|| not_found(format!("no record {:?}", req.what)))?;
        Ok(DevExportResult {
            cancelled: true,
            path: None,
            records: 0,
            bytes: 0,
        })
    }

    fn dev_export_type(
        &self,
        req: DevExportTypeRequest,
        _progress: Progress<'_>,
    ) -> CmdResult<DevExportResult> {
        let c = self.explorer(&req.target)?;
        if c.index.of_type(&req.type_name, req.main_only).is_empty() {
            return Err(not_found(format!("no records of type {}", req.type_name)));
        }
        Ok(DevExportResult {
            cancelled: true,
            path: None,
            records: 0,
            bytes: 0,
        })
    }

    fn dev_sources(&self, req: ChannelRef) -> CmdResult<Vec<DevSource>> {
        self.dev_on(&req)?;
        Ok(self.sources_of(&req))
    }

    /// The mock's "folder dialog" always picks the synthetic sample folder.
    fn dev_add_source(&self, req: ChannelRef) -> CmdResult<Vec<DevSource>> {
        self.dev_on(&req)?;
        self.dev_state().folder = true;
        Ok(self.sources_of(&req))
    }

    fn dev_remove_source(&self, req: DevSourceRef) -> CmdResult<Vec<DevSource>> {
        self.dev_on(&req.target)?;
        if req.source != "folder:0" || !self.dev_state().folder {
            return Err(not_found(format!("no source {}", req.source)));
        }
        let mut s = self.dev_state();
        s.folder = false;
        s.diff = None;
        drop(s);
        Ok(self.sources_of(&req.target))
    }

    fn dev_download_dump(
        &self,
        req: ChannelRef,
        _progress: Progress<'_>,
    ) -> CmdResult<Vec<DevSource>> {
        self.dev_on(&req)?;
        Err(dump_refusal(self.dumps_on()))
    }

    fn dev_dataset(&self, req: DevDatasetRef) -> CmdResult<DatasetInfo> {
        Ok(self
            .dataset_of(&req.target, &req.source, &req.dataset)?
            .info
            .clone())
    }

    fn dev_table(&self, req: DevTableRef) -> CmdResult<TableInfo> {
        let d = self.dataset_of(&req.target, &req.source, &req.dataset)?;
        d.table(&req.table)
            .map(|t| t.info())
            .ok_or_else(|| not_found(format!("no table {}", req.table)))
    }

    fn dev_rows(&self, req: DevRowsRequest) -> CmdResult<RowsPage> {
        let d = self.dataset_of(&req.target, &req.source, &req.dataset)?;
        let t = d
            .table(&req.table)
            .ok_or_else(|| not_found(format!("no table {}", req.table)))?;
        Ok(dataset::query_rows(
            t,
            &RowsQuery {
                columns: req.columns,
                search: req.search,
                filters: req.filters,
                sort: req.sort,
                offset: req.offset,
                limit: req.limit,
                group: req.group,
            },
            MAX_PAGE,
            Some(&lookup(true)),
        ))
    }

    fn dev_groups(&self, req: DevGroupsRequest) -> CmdResult<GroupPage> {
        let d = self.dataset_of(&req.target, &req.source, &req.dataset)?;
        let t = d
            .table(&req.table)
            .ok_or_else(|| not_found(format!("no table {}", req.table)))?;
        Ok(starorg_core::dev::group::group_rows(
            t,
            &starorg_core::dev::group::GroupQuery {
                by: req.by,
                open: req.open,
                columns: req.columns,
                search: req.search,
                filters: req.filters,
            },
            Some(&lookup(true)),
        ))
    }

    fn dev_row(&self, req: DevRowRequest) -> CmdResult<DevRowDetail> {
        let d = self.dataset_of(&req.target, &req.source, &req.dataset)?;
        let t = d
            .table(&req.table)
            .ok_or_else(|| not_found(format!("no table {}", req.table)))?;
        let row = t
            .rows
            .get(req.row as usize)
            .ok_or_else(|| not_found(format!("no row {}", req.row)))?;
        let (json, truncated) = record::view_json(row, "", None).map_err(not_found)?;
        let loc = row_loc(row, &|k| self.stock_text(k));
        let open = self.dev_state().open.is_some();
        let ix = &core().index;
        let links = if open {
            explorer_links(row, &|s| {
                ix.by_guid(s)
                    .or_else(|| ix.find_name(s))
                    .and_then(|i| ix.entry(i))
                    .map(|e| e.guid.clone())
            })
        } else {
            vec![]
        };
        Ok(DevRowDetail {
            row: req.row,
            json,
            truncated,
            loc,
            links,
            labels: readable::labels_in(&lookup(true), row),
        })
    }

    fn dev_diff(&self, req: DevDiffRequest, progress: Progress<'_>) -> CmdResult<DevDiffSummary> {
        self.dev_on(&req.target)?;
        let mut p = self.progress_job("Reading datasets", 2, &req.target.channel, progress);
        let a = self.datasets_of(&req.target, &req.a)?;
        let b = self.datasets_of(&req.target, &req.b)?;
        self.wait();
        p.step = 2;
        p.label = "Comparing".into();
        progress(p.clone());
        let ra: Vec<(String, &Dataset)> = a.iter().map(|x| (x.0.clone(), &*x.1)).collect();
        let rb: Vec<(String, &Dataset)> = b.iter().map(|x| (x.0.clone(), &*x.1)).collect();
        let tables = diff::diff_sources(&ra, &rb, &|_, _| {});
        let summary = DevDiffSummary {
            a: req.a,
            b: req.b,
            tables: tables.iter().map(|t| t.summary.clone()).collect(),
            ms: 12,
        };
        self.dev_state().diff = Some((summary.clone(), tables, a, b));
        p.phase = JobPhase::Done;
        p.label = "Compared".into();
        progress(p);
        Ok(summary)
    }

    fn dev_diff_summary(&self, req: ChannelRef) -> CmdResult<DevDiffSummary> {
        self.dev_on(&req)?;
        self.dev_state()
            .diff
            .as_ref()
            .map(|d| d.0.clone())
            .ok_or_else(|| not_found("No comparison yet: pick A and B."))
    }

    fn dev_diff_records(&self, req: DevDiffRecordsRequest) -> CmdResult<DevDiffRecords> {
        self.dev_on(&req.target)?;
        let mut g = self.dev_state();
        let (_, tables, a, b) = g
            .diff
            .as_mut()
            .ok_or_else(|| not_found("No comparison yet: pick A and B."))?;
        let find = |side: &[(String, Arc<Dataset>)]| {
            side.iter()
                .find(|x| x.0 == req.dataset)
                .and_then(|x| x.1.table(&req.table).cloned())
        };
        let (ta, tb) = (find(a), find(b));
        let pos = tables
            .iter()
            .position(|t| t.summary.dataset == req.dataset && t.summary.table == req.table)
            .ok_or_else(|| not_found(format!("no table {}", req.table)))?;
        if req.identity.is_some() && tables[pos].summary.identity != req.identity {
            tables[pos] = diff::diff_table(
                &req.dataset,
                &req.table,
                ta.as_ref(),
                tb.as_ref(),
                req.identity.as_deref(),
            );
        }
        let in_group = diff_group_filter(&req, a, b);
        let l = lookup(true);
        let id = tables[pos].summary.identity.clone();
        Ok(page_records(
            &tables[pos],
            &req,
            identity_options(ta.as_ref(), tb.as_ref()),
            &in_group,
            &|r| record_title(r, id.as_deref(), ta.as_ref(), tb.as_ref(), &l, &l),
        ))
    }

    fn dev_diff_groups(&self, req: DevDiffGroupsRequest) -> CmdResult<GroupPage> {
        self.dev_on(&req.target)?;
        let g = self.dev_state();
        let (_, tables, a, b) = g
            .diff
            .as_ref()
            .ok_or_else(|| not_found("No comparison yet: pick A and B."))?;
        let l = lookup(true);
        Ok(diff_tree(tables, a, b, &req, &|c, v| {
            starorg_core::dev::group::lookup_label(&l, c, v)
        }))
    }

    fn dev_diff_record(&self, req: DevDiffRecordRequest) -> CmdResult<DevFieldDiff> {
        self.dev_on(&req.target)?;
        let g = self.dev_state();
        let (_, tables, a, b) = g
            .diff
            .as_ref()
            .ok_or_else(|| not_found("No comparison yet: pick A and B."))?;
        let td = tables
            .iter()
            .find(|t| t.summary.dataset == req.dataset && t.summary.table == req.table)
            .ok_or_else(|| not_found(format!("no table {}", req.table)))?;
        let rec = td
            .records
            .iter()
            .find(|r| r.id == req.id)
            .ok_or_else(|| not_found(format!("no record {}", req.id)))?;
        let row = |side: &[(String, Arc<Dataset>)], i: Option<u32>| -> Value {
            i.and_then(|i| {
                side.iter()
                    .find(|x| x.0 == req.dataset)
                    .and_then(|x| x.1.table(&req.table))
                    .and_then(|t| t.rows.get(i as usize).cloned())
            })
            .unwrap_or_else(|| Value::Object(Default::default()))
        };
        let l = lookup(true);
        let (ra, rb) = (row(a, rec.a), row(b, rec.b));
        let mut changes = diff::field_changes(&ra, &rb);
        diff::annotate_changes(&mut changes, &l, &l);
        diff::name_positions(&mut changes, &ra, &rb, &l, &l);
        let label = record_title(
            rec,
            td.summary.identity.as_deref(),
            side_table(a, &req.dataset, &req.table),
            side_table(b, &req.dataset, &req.table),
            &l,
            &l,
        );
        Ok(DevFieldDiff {
            id: rec.id.clone(),
            label,
            state: rec.state,
            changes,
        })
    }
}

/// What the TS mock needs to behave like this one (`mock-dev.gen.json`): the index entries,
/// the record values (the huge ship's loadout left out and rebuilt in TS by the same rule),
/// the build datasets and the sample folder, raw.
pub fn gen_file() -> Value {
    let c = core();
    let values: Vec<Value> = c
        .values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            if c.index
                .entry(i)
                .is_some_and(|e| e.guid == ship_guid(HUGE_SHIP))
            {
                let mut v = v.clone();
                v["_RecordValue_"]
                    .as_object_mut()
                    .expect("object")
                    .shift_remove("Loadout");
                v
            } else {
                v.clone()
            }
        })
        .collect();
    json!({
        "entries": c.index.entries(),
        "values": values,
        "huge": { "guid": ship_guid(HUGE_SHIP), "ports": HUGE_PORTS },
        "builds": (["LIVE", "PTU", "HOTFIX"].iter().map(|c| (c.to_string(), build_value(c))).collect::<serde_json::Map<_, _>>()),
        "known": known(),
        "facts": c.facts,
        // The section table stays in Rust: the TS mock gets each record's section.
        "sections": {
            "defs": (starorg_core::dev::sections::rules::SECTIONS.iter().map(|d| json!({ "id": d.id, "label": d.label, "levels": d.levels.iter().map(|l| l.id()).collect::<Vec<_>>() })).collect::<Vec<_>>()),
            "labels": starorg_core::dev::sections::rules::VALUE_LABELS,
            "records": ((0..c.index.len()).map(|i| json!([c.sections.section_id(i), c.sections.is_variant(i)])).collect::<Vec<_>>()),
        },
        "catalogue": { "stats": catalogue_stats().into_iter().collect::<std::collections::BTreeMap<_, _>>(), "marks": catalogue_marks(), "geometry": ((0..CATALOGUE.len()).filter_map(|n| catalogue_geometry(n).map(|g| json!([g, catalogue_guid(n)]))).collect::<Vec<_>>()) },
        "makers": (MAKERS.iter().map(|(c, n)| (c.to_string(), Value::from(*n))).collect::<serde_json::Map<_, _>>()),
        "enhanced": (enhanced().iter().map(|(k, v)| (k.clone(), Value::from(v.clone()))).collect::<std::collections::BTreeMap<_, _>>()),
        "folder": { "label": FOLDER_LABEL, "path": FOLDER_PATH, "files": (folder_files().into_iter().map(|(f, v)| json!({ "file": f, "value": v })).collect::<Vec<_>>()) },
    })
}

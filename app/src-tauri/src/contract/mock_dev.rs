//! Developer mode in the mock backend (addendum 7.7): a synthetic 300-record DataCore (three
//! types, cross references, one 1.5 MiB record), synthetic datasets per channel build and a
//! sample folder of three files, two published LIVE builds to download for Diff, all
//! OS-independent. The explorer, rows and diff go through
//! the same core functions as the real backend. `mock-dev.gen.json` (written by the contract
//! tests) carries this data to the TS mock (`app/src/lib/contract/mock-dev.ts`).

use super::Progress;
use super::dev::*;
use super::mock::MockBackend;
use super::types::*;
use serde_json::{Value, json};
use starorg_core::dev::combat;
use starorg_core::dev::dataset::{self, Dataset, RowsQuery};
use starorg_core::dev::diff::{self, TableDiff};
use starorg_core::dev::explorer::{Entry, ExplorerIndex, MAX_PAGE};
use starorg_core::dev::readable::{self, KnownRecord, LabelKind, MapLookup};
use starorg_core::dev::record;
use starorg_core::dev::sections::{Facts, SectionIndex};
use starorg_core::model::GameData;
use std::collections::{BTreeMap, HashMap};
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

/// The mock's published LIVE builds (tag, title, branch, published), newest first.
pub const DUMPS: [(&str, &str, &str, &str); 2] = [
    (
        "4.10.200.58746",
        "4.10.0 LIVE (4.10.200.58746)",
        "sc-alpha-4.10.0",
        "2026-11-12T18:20:43Z",
    ),
    (
        "4.10.197.2878",
        "4.10.2 LIVE (4.10.197.2878)",
        "sc-alpha-4.10.2",
        "2026-11-01T14:36:39Z",
    ),
];

/// Synthetic datasets of a mock LIVE build. The newer build drops two weapons, adds two,
/// changes the damage of every fifth and adds a ship, so a diff has all three states.
pub fn dump_files(tag: &str) -> Vec<(&'static str, Value)> {
    let new = tag == DUMPS[0].0;
    let range = if new { 2..26u32 } else { 0..24u32 };
    let weapons = json!({
        "_source": "synthetic",
        "_extracted": if new { "2026-11-12T18:19:26Z" } else { "2026-11-01T14:35:00Z" },
        "_build": tag,
        "shipWeapons": range.clone().map(|n| json!({
            "name": format!("Published Repeater {n}"),
            "recordName": format!("PUB_Laser_S{}_{n:02}", n % 4 + 1),
            "size": n % 4 + 1,
            // Never a whole number: serde_json and JSON.stringify write those differently.
            "dps": 150.5 + f64::from(n) * 10.0 + if new && n % 5 == 0 { 7.25 } else { 0.0 },
        })).collect::<Vec<_>>(),
        "summary": { "shipWeapons": range.len() },
    });
    let hulls = if new { 6u32 } else { 5 };
    let ships = json!({
        "_source": "synthetic",
        "_extracted": if new { "2026-11-12T18:19:26Z" } else { "2026-11-01T14:35:00Z" },
        "_build": tag,
        "vehicles": (0..hulls).map(|n| json!({
            "name": format!("Published hull {n}"),
            "className": format!("PUB_Ship_{n:03}"),
            "crew": n % 3 + 1,
            "cargo": n * 16 + if new && n == 1 { 8 } else { 0 },
        })).collect::<Vec<_>>(),
        "summary": { "vehicles": hulls },
    });
    vec![("game-ships.json", ships), ("game-weapons.json", weapons)]
}

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

/// The Sources card of a record by name: the catalogue's guns and coolers are sold in two
/// shops and turn up in loot containers, and the Sadaryx and Aphorite records of [`MOCK_ORES`] are mined ores. Both
/// mocks serve these through [`catalogue_sources`].
fn mock_sources(name: &str) -> RecordSources {
    let short = name.rsplit('.').next().unwrap_or(name);
    let n = (0..CATALOGUE.len()).find(|&n| catalogue_name(n).eq_ignore_ascii_case(short));
    let loot = n
        .filter(|&n| matches!(CATALOGUE[n].1, "WeaponGun" | "Cooler"))
        .map(|n| {
            let gun = CATALOGUE[n].1 == "WeaponGun";
            let (contents, table) = if gun {
                ("Weapons", "v3loottable_ship_weapons")
            } else {
                ("Components", "v3loottable_components")
            };
            let row = |area: &str, place: Option<&str>, size: &str, rarity: &str, odds: f64| {
                LootContainer {
                    area: area.into(),
                    place: place.map(str::to_string),
                    spawn_point: Some(format!("{contents} {size} {rarity}")),
                    kind: "loot".into(),
                    container: format!("{size} Container"),
                    contents: Some(contents.into()),
                    size: Some(size.into()),
                    rarity: Some(rarity.into()),
                    slot_share: 1.0,
                    odds_per_box: odds,
                    per_roll: Some(odds / 4.0),
                    table: Some(table.into()),
                    configs: 3,
                }
            };
            LootSources {
                name: CATALOGUE[n].5.into(),
                can_loot: Some(true),
                reason: None,
                rarity: Some(if gun { "Rare" } else { "Uncommon" }.into()),
                containers: vec![
                    row(
                        "v3slotpreset rockcracker",
                        Some("Rock Cracker, Nyx"),
                        "Large",
                        "Rare",
                        0.21,
                    ),
                    row(
                        "asteroidbases: asteroidbase dungeon 001",
                        None,
                        "Medium",
                        "Rare",
                        0.084,
                    ),
                    row(
                        "caves: loot caves occupied rock pyro",
                        None,
                        "Small",
                        "Uncommon",
                        0.0123,
                    ),
                ],
                container_total: 3,
                tables: vec![LootTableOdds {
                    table: "V3LootTable_Actor_LargeAmmo_RockCracker_Boss".into(),
                    per_roll: 0.0042,
                }],
                table_total: 1,
            }
        });
    let buy = n.map(|n| {
        let sold = matches!(CATALOGUE[n].1, "WeaponGun" | "Cooler");
        let offer =
            |terminal: &str, location: &str, price: f64, updated: &str, stale: bool| ShopOffer {
                terminal: terminal.into(),
                location: location.into(),
                price_buy: price,
                updated: Some(updated.into()),
                stale,
                not_on_live: false,
            };
        let offers = if sold {
            vec![
                offer(
                    "Dumper's GrimHEX",
                    "Grim HEX, Yela, Crusader, Stanton",
                    6900.0,
                    "2026-09-01",
                    true,
                ),
                offer(
                    "Guns Ruin",
                    "Ruin Station, Terminus, Pyro",
                    7029.0,
                    "2026-11-14",
                    false,
                ),
            ]
        } else {
            Vec::new()
        };
        BuySources {
            loaded: true,
            folder: Some("~/.cache/starorg-codex/mcp/uex".into()),
            total: offers.len(),
            offers,
        }
    });
    let lower = short.to_lowercase();
    let mined = if lower.contains("sadaryx") {
        vec![MinedReport {
            ore: "minableelement_fps_sadaryx".into(),
            name: "Sadaryx".into(),
            rocks: vec!["MineableRock_FPS_Sadaryx".into()],
            belts: Vec::new(),
            spots: vec![MinedSpot {
                area: "v3slotpreset rockcracker".into(),
                place: Some("Rock Cracker, Nyx".into()),
                configs: vec!["Sadaryx minable".into()],
                rock: "MineableRock_FPS_Sadaryx".into(),
                slot_share: 1.0,
            }],
        }]
    } else if lower.contains("aphorite") {
        let belt = |location: &str, system: &str, body: &str, share: f64| MinedBelt {
            location: location.into(),
            system: Some(system.into()),
            body: Some(body.into()),
            rock: "MineableRock_FPS_Aphorite".into(),
            kind: Some("hand".into()),
            tier: None,
            spawn_share: Some(share),
            min_percent: Some(50.0),
            max_percent: Some(100.0),
        };
        vec![MinedReport {
            ore: "minableelement_fps_aphorite".into(),
            name: "Aphorite".into(),
            rocks: vec!["MineableRock_FPS_Aphorite".into()],
            belts: vec![
                belt("Stanton3a", "Stanton", "Lyria", 15.275),
                belt("Pyro1", "Pyro", "Pyro I", 9.5),
            ],
            spots: vec![MinedSpot {
                area: "caves: loot caves occupied rock".into(),
                place: None,
                configs: vec!["FPS mineables".into()],
                rock: "MineableRock_FPS_Aphorite".into(),
                slot_share: 0.25,
            }],
        }]
    } else {
        Vec::new()
    };
    let quality = mined
        .iter()
        .map(|m| {
            let range = |mean: f64| QualityRange {
                min: Some(201.0),
                max: Some(1000.0),
                mean: Some(mean),
                stddev: Some(298.0),
            };
            let step = |start: f64, end: f64, shown: f64, share: f64| QualityStep {
                start,
                end,
                shown,
                share: Some(share),
            };
            OreQuality {
                resource: format!("ResourceType.{}", m.name),
                name: m.name.clone(),
                tier: Some("FPSMineable".into()),
                range: range(201.0),
                overrides: if m.name == "Aphorite" {
                    vec![QualityOverride {
                        location: "Pyro System".into(),
                        range: range(209.0),
                    }]
                } else {
                    Vec::new()
                },
                steps: vec![
                    step(0.0, 399.0, 348.0, 0.499),
                    step(400.0, 599.0, 523.0, 0.326),
                    step(600.0, 799.0, 717.0, 0.137),
                    step(800.0, 1000.0, 916.0, 0.038),
                ],
            }
        })
        .collect();
    // Kiosks buying the ore: Aphorite sells, nobody buys Sadaryx (an empty, loaded card).
    let sell = mined.first().map(|m| {
        let offer = |terminal: &str, location: &str, price: f64, stock: f64| SellOffer {
            terminal: terminal.into(),
            location: location.into(),
            price_sell: price,
            scu_stock: Some(stock),
            updated: Some("2026-11-20".into()),
            stale: false,
        };
        let offers = if m.name == "Aphorite" {
            vec![
                offer(
                    "HUR-L2",
                    "HUR-L2 Faithful Dream Station, Stanton",
                    9612.0,
                    4.0,
                ),
                offer("Rod's Fuel", "Rod's Fuel 'N Supplies, Pyro", 9180.0, 952.0),
                offer("TDD Area 18", "Area 18, ArcCorp, Stanton", 9000.0, 124.0),
            ]
        } else {
            Vec::new()
        };
        SellSources {
            loaded: true,
            folder: Some("~/.cache/starorg-codex/mcp/uex".into()),
            resource: format!("ResourceType.{}", m.name),
            name: m.name.clone(),
            total: if offers.is_empty() { 0 } else { 7 },
            offers,
            fetched: Some(super::mock::MOCK_UEX_FETCHED.into()),
        }
    });
    RecordSources {
        buy,
        sell,
        loot,
        mined,
        quality,
    }
}

/// Ore records the mock knows by name, beside the catalogue (Sources card only).
pub const MOCK_ORES: &[&str] = &[
    "ResourceType.Sadaryx",
    "MineableElement.MinableElement_FPS_Sadaryx",
    "EntityClassDefinition.MineableRock_FPS_Sadaryx",
    "ResourceType.Aphorite",
    "EntityClassDefinition.MineableRock_FPS_Aphorite",
];

/// [`mock_sources`] by lower-case short record name, for every catalogue row and
/// [`MOCK_ORES`] record with a card: the TypeScript mock looks names up here exactly.
pub fn catalogue_sources() -> &'static BTreeMap<String, RecordSources> {
    static S: OnceLock<BTreeMap<String, RecordSources>> = OnceLock::new();
    S.get_or_init(|| {
        (0..CATALOGUE.len())
            .map(catalogue_name)
            .chain(MOCK_ORES.iter().map(|s| (*s).to_string()))
            .map(|k| {
                let v = mock_sources(&k);
                (short_key(&k), v)
            })
            .filter(|(_, v)| v.loot.is_some() || !v.mined.is_empty())
            .collect()
    })
}

/// The lower-case short name a record name is looked up by (`resourcetype.sadaryx` ->
/// `sadaryx`).
pub fn short_key(name: &str) -> String {
    name.rsplit('.')
        .next()
        .unwrap_or(name)
        .trim()
        .to_lowercase()
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

/// The catalogue row of a display name.
pub fn catalogue_row(display: &str) -> usize {
    CATALOGUE
        .iter()
        .position(|r| r.5 == display)
        .expect("catalogue row")
}

/// Synthetic GameData of the catalogue's guns, missiles and armed ships, for the combat card
/// (the real backend reads the build's). Numbers are made up; no CIG data.
pub fn catalogue_gamedata() -> GameData {
    use starorg_core::model::{
        Damage, FireKind, FireMode, ItemIdentity, Loadout, LoadoutItem, Missile, Ship, Totals,
        Weapon, WeaponAmmo, WeaponClass,
    };
    let ident = |n: usize| ItemIdentity {
        record_name: format!("{E}.{}", catalogue_name(n)),
        id: catalogue_guid(n),
        size: Some(CATALOGUE[n].3),
        manufacturer: Some(CATALOGUE[n].4.into()),
        ..ItemIdentity::default()
    };
    let mode =
        |kind: FireKind, name: &str, rate: f64, shots: Option<f64>, sustained: Option<f64>| {
            FireMode {
                name: name.into(),
                kind,
                fire_rate: Some(rate),
                shot_count: shots,
                sustained_fire_rate: sustained,
                heat_per_shot: Some(rate / 100.0),
                ..FireMode::default()
            }
        };
    /// A synthetic gun of the catalogue.
    struct Gun {
        display: &'static str,
        class: WeaponClass,
        per_shot: f64,
        energy: bool,
        speed: f64,
        range: f64,
        magazine: f64,
        power: Option<f64>,
        modes: Vec<FireMode>,
    }
    let guns = [
        Gun {
            display: "Ember Repeater S1",
            class: WeaponClass::Ship,
            per_shot: 12.0,
            energy: true,
            speed: 1800.0,
            range: 2700.0,
            magazine: 0.0,
            power: Some(2.0),
            modes: vec![mode(FireKind::Rapid, "Rapid", 900.0, None, None)],
        },
        Gun {
            display: "Ember Repeater S2",
            class: WeaponClass::Ship,
            per_shot: 18.0,
            energy: true,
            speed: 1750.0,
            range: 2600.0,
            magazine: 0.0,
            power: Some(3.0),
            modes: vec![mode(FireKind::Rapid, "Rapid", 800.0, None, None)],
        },
        Gun {
            display: "Torrent Cannon S3",
            class: WeaponClass::Ship,
            per_shot: 140.0,
            energy: false,
            speed: 1200.0,
            range: 3600.0,
            magazine: 120.0,
            power: Some(5.0),
            modes: vec![
                mode(FireKind::Single, "Single", 120.0, None, None),
                mode(FireKind::Burst, "Burst", 300.0, Some(3.0), Some(180.0)),
            ],
        },
        Gun {
            display: "Breaker Scattergun S4",
            class: WeaponClass::Ship,
            per_shot: 30.0,
            energy: false,
            speed: 900.0,
            range: 1100.0,
            magazine: 60.0,
            power: Some(6.0),
            modes: vec![mode(FireKind::Single, "Single", 90.0, None, None)],
        },
        Gun {
            display: "Viper Rifle",
            class: WeaponClass::Fps,
            per_shot: 25.0,
            energy: false,
            speed: 650.0,
            range: 260.0,
            magazine: 30.0,
            power: None,
            modes: vec![
                mode(FireKind::Single, "Single", 300.0, None, None),
                mode(FireKind::Rapid, "Rapid", 600.0, None, None),
            ],
        },
    ];
    let weapons = guns
        .into_iter()
        .map(|g| {
            let mut modes = g.modes;
            for m in &mut modes {
                m.finish(Some(g.per_shot));
            }
            let mut damage = Damage::default();
            damage.set(if g.energy { "energy" } else { "physical" }, g.per_shot);
            let mut w = Weapon {
                item: ident(catalogue_row(g.display)),
                class: g.class,
                power: g.power,
                em: g.power.map(|p| p * 40.0),
                ammo: Some(WeaponAmmo {
                    speed: Some(g.speed),
                    range: Some(g.range),
                    damage: Some(damage),
                    damage_per_shot: Some(g.per_shot),
                    capacity: Some(g.magazine),
                    ..WeaponAmmo::default()
                }),
                modes,
                ..Weapon::default()
            };
            w.derive_headline();
            w
        })
        .collect::<Vec<_>>();
    let missile = |d: &str, damage: f64, range: f64, time: f64| Missile {
        item: ident(catalogue_row(d)),
        damage_total: Some(damage),
        damage_type: Some("physical".into()),
        tracking_signal: Some("Infrared".into()),
        lock_time: Some(time),
        lock_range_max: Some(range),
        speed: Some(800.0),
        ..Missile::default()
    };
    let missiles = vec![
        missile("Wasp Missile", 1800.0, 5000.0, 2.0),
        missile("Hammer Torpedo", 9000.0, 12000.0, 6.0),
    ];
    let gun = |d: &str, count: f64, mount: Option<&str>| {
        let n = catalogue_row(d);
        LoadoutItem {
            item: catalogue_name(n),
            size: Some(CATALOGUE[n].3 as f64),
            count: (count != 1.0).then_some(count),
            item_type: Some("WeaponGun".into()),
            mount: mount.map(str::to_string),
            dps: weapons
                .iter()
                .find(|w| w.item.id == catalogue_guid(n))
                .and_then(|w| w.dps),
            ..LoadoutItem::default()
        }
    };
    let rack = catalogue_name(catalogue_row("Quiver Missile Rack"));
    let msl = |d: &str, count: f64| {
        let n = catalogue_row(d);
        LoadoutItem {
            item: catalogue_name(n),
            size: Some(CATALOGUE[n].3 as f64),
            count: (count != 1.0).then_some(count),
            rack: Some(rack.clone()),
            damage: missiles
                .iter()
                .find(|m| m.item.id == catalogue_guid(n))
                .and_then(|m| m.damage_total),
            ..LoadoutItem::default()
        }
    };
    let ship = |d: &str, weapons: Vec<LoadoutItem>, missiles: Vec<LoadoutItem>| {
        let n = catalogue_row(d);
        let dps: f64 = weapons
            .iter()
            .map(|w| w.dps.unwrap_or(0.0) * w.count())
            .sum();
        let guns: f64 = weapons.iter().map(LoadoutItem::count).sum();
        let mcount: f64 = missiles.iter().map(LoadoutItem::count).sum();
        let mdmg: f64 = missiles
            .iter()
            .map(|m| m.damage.unwrap_or(0.0) * m.count())
            .sum();
        Ship {
            record_name: format!("{E}.{}", catalogue_name(n)),
            id: catalogue_guid(n),
            totals: Some(Totals {
                weapon_dps: Some(dps),
                weapons: Some(guns),
                missiles: (mcount > 0.0).then_some(mcount),
                missile_damage: (mdmg > 0.0).then_some(mdmg),
                ..Totals::default()
            }),
            loadout: Some(Loadout {
                weapons,
                missiles,
                ..Loadout::default()
            }),
            ..Ship::default()
        }
    };
    let ships = vec![
        ship(
            "Alpha Sparrow",
            vec![gun("Ember Repeater S1", 2.0, Some("SYN_Gimbal_Mount_S1"))],
            vec![msl("Wasp Missile", 4.0)],
        ),
        ship(
            "Alpha Kestrel",
            vec![
                gun("Ember Repeater S2", 2.0, Some("SYN_Gimbal_Mount_S2")),
                gun("Torrent Cannon S3", 1.0, Some("SYN_Nose_Turret")),
            ],
            vec![msl("Wasp Missile", 2.0), msl("Hammer Torpedo", 1.0)],
        ),
        ship(
            "Gamma Carrier",
            vec![gun("Breaker Scattergun S4", 4.0, Some("SYN_Turret_Remote"))],
            vec![],
        ),
    ];
    GameData {
        weapons,
        missiles,
        ships,
        ..GameData::default()
    }
}

/// The combat cards of the catalogue by lower-case GUID (both mocks serve these).
pub fn catalogue_combat() -> &'static BTreeMap<String, Combat> {
    static C: OnceLock<BTreeMap<String, Combat>> = OnceLock::new();
    C.get_or_init(|| {
        let gd = catalogue_gamedata();
        let l = lookup(false);
        (0..CATALOGUE.len())
            .filter_map(|n| {
                let g = catalogue_guid(n);
                combat::combat_of(&gd, &l, &g).map(|c| (g, c))
            })
            .collect()
    })
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

/// What catalogue row `r` reads as in game: an AI copy reads as the ship it copies (`Alpha
/// Kestrel`; the ` PU AI CIV` suffix is the record name's only), so the finder folds them.
fn catalogue_display(r: &CatRow) -> &'static str {
    r.5.strip_suffix(" PU AI CIV").unwrap_or(r.5)
}

fn catalogue_facts(r: &CatRow) -> Option<Facts> {
    let some = |s: &str| (!s.is_empty()).then(|| s.to_string());
    (r.0 == E).then(|| Facts {
        item_type: some(r.1),
        sub_type: some(r.2),
        size: (r.3 > 0).then_some(r.3),
        manufacturer: some(r.4),
        display: some(catalogue_display(r)),
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
                        "Localization": { "Name": catalogue_display(r) },
                    },
                }],
            })
        } else {
            json!({ "_Type_": r.0, "displayName": catalogue_display(r) })
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
    /// Tags of the published LIVE builds "downloaded".
    dumps: Vec<String>,
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
        let cached = self.dev_state().dumps.clone();
        for (tag, title, _, _) in DUMPS.iter().filter(|d| cached.iter().any(|c| c == d.0)) {
            out.push(DevSource {
                id: format!("dump:{tag}"),
                kind: SourceKind::Dump,
                label: dump_label(tag),
                build: Some(tag.to_string()),
                state: SourceState::Ready,
                detail: Some(format!("{title} from github.com/voidput/sc-gamedata-dumps")),
                datasets: dump_files(tag)
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
        if let Some(tag) = dump_tag(source)
            && self.dev_state().dumps.iter().any(|c| c == tag)
        {
            return Ok(dump_files(tag)
                .into_iter()
                .map(|(f, v)| (f.to_string(), Arc::new(to_dataset(f, v))))
                .collect());
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
            return Ok(c.sections.tree_with(
                &c.index,
                &req.node,
                req.offset as usize,
                (req.limit as usize).min(MAX_PAGE),
                TreeOpts {
                    main_only: req.main_only,
                    player_only: req.player_only,
                    fold_variants: req.fold_variants,
                },
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
    fn dev_record_combat(&self, req: DevRecordCombatRequest) -> CmdResult<Option<Combat>> {
        self.dev_on(&req.target)?;
        Ok(catalogue_combat()
            .get(&req.guid.trim().to_lowercase())
            .cloned())
    }

    fn dev_record_sources(&self, req: DevRecordSourcesRequest) -> CmdResult<RecordSources> {
        self.dev_on(&req.target)?;
        Ok(catalogue_sources()
            .get(&short_key(&req.name))
            .cloned()
            .unwrap_or_default())
    }

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
        let mut s = self.dev_state();
        if let Some(tag) = dump_tag(&req.source)
            && s.dumps.iter().any(|c| c == tag)
        {
            s.dumps.retain(|c| c != tag);
        } else if req.source == "folder:0" && s.folder {
            s.folder = false;
        } else {
            return Err(not_found(format!("no source {}", req.source)));
        }
        s.diff = None;
        drop(s);
        Ok(self.sources_of(&req.target))
    }

    fn dev_dump_releases(&self, req: ChannelRef) -> CmdResult<DumpReleases> {
        self.dev_on(&req)?;
        let on = self.dumps_on();
        let cached = self.dev_state().dumps.clone();
        let releases = DUMPS
            .iter()
            .map(|(tag, title, branch, published)| DumpRelease {
                tag: tag.to_string(),
                title: title.to_string(),
                channel: "LIVE".into(),
                branch: Some(branch.to_string()),
                published: Some(published.to_string()),
                cached: cached.iter().any(|c| c == tag),
            })
            .filter(|r| on || r.cached)
            .collect();
        Ok(DumpReleases {
            releases,
            error: None,
        })
    }

    fn dev_refresh_dump_releases(&self, req: ChannelRef) -> CmdResult<DumpReleases> {
        // The mock's list is fixed: nothing remembered to forget.
        self.dev_dump_releases(req)
    }

    fn dev_download_dump(
        &self,
        req: DevDumpRequest,
        progress: Progress<'_>,
    ) -> CmdResult<Vec<DevSource>> {
        self.dev_on(&req.target)?;
        if !self.dumps_on() {
            return Err(dumps_off());
        }
        if !starorg_core::source::dump::valid_component(&req.tag) {
            return Err(bad_dump_tag(&req.tag));
        }
        if !DUMPS.iter().any(|d| d.0 == req.tag) {
            return Err(dump_not_published(
                &req.tag,
                &format!("no published build {}", req.tag),
            ));
        }
        // The real backend's sequence: one "Asking" step, one per file, then "Checking".
        let label = dump_label(&req.tag);
        let files = dump_files(&req.tag);
        let n = files.len();
        let mut p = self.progress_job(
            &format!("Asking GitHub for {label}"),
            1,
            &req.target.channel,
            progress,
        );
        p.steps = n as u32 + 1;
        for (i, (f, _)) in files.iter().enumerate() {
            self.wait();
            p.step = i as u32 + 1;
            p.label = format!("Downloading {f} ({} of {n})", i + 1);
            progress(p.clone());
        }
        p.step = n as u32 + 1;
        p.label = format!("Checking {label}");
        progress(p.clone());
        {
            let mut s = self.dev_state();
            if !s.dumps.contains(&req.tag) {
                s.dumps.push(req.tag.clone());
            }
            s.diff = None;
        }
        p.phase = JobPhase::Done;
        p.label = format!("Downloaded {label}");
        progress(p);
        Ok(self.sources_of(&req.target))
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
        let l = lookup(true);
        Ok(DevRowDetail {
            row: req.row,
            json,
            truncated,
            loc,
            links,
            labels: readable::labels_in(&l, row),
            row_links: row_links(&d, &req.table, req.row, &l),
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
            &|r| record_name(r, ta.as_ref(), tb.as_ref()),
        ))
    }

    fn dev_diff_search(&self, req: DevDiffSearchRequest) -> CmdResult<DevDiffSearch> {
        self.dev_on(&req.target)?;
        let g = self.dev_state();
        let (_, tables, a, b) = g
            .diff
            .as_ref()
            .ok_or_else(|| not_found("No comparison yet: pick A and B."))?;
        let l = lookup(true);
        Ok(search_tables(tables, a, b, &req, &l, &l))
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
        let (summary, tables, a, b) = g
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
        let row_links =
            diff_row_links(tables, summary, a, b, &req.dataset, &req.table, rec, &l, &l);
        Ok(DevFieldDiff {
            id: rec.id.clone(),
            label,
            state: rec.state,
            changes,
            row_links,
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
        "catalogue": { "stats": catalogue_stats().into_iter().collect::<std::collections::BTreeMap<_, _>>(), "combat": catalogue_combat(), "sources": catalogue_sources(), "marks": catalogue_marks(), "geometry": ((0..CATALOGUE.len()).filter_map(|n| catalogue_geometry(n).map(|g| json!([g, catalogue_guid(n)]))).collect::<Vec<_>>()) },
        "makers": (MAKERS.iter().map(|(c, n)| (c.to_string(), Value::from(*n))).collect::<serde_json::Map<_, _>>()),
        "enhanced": (enhanced().iter().map(|(k, v)| (k.clone(), Value::from(v.clone()))).collect::<std::collections::BTreeMap<_, _>>()),
        "folder": { "label": FOLDER_LABEL, "path": FOLDER_PATH, "files": (folder_files().into_iter().map(|(f, v)| json!({ "file": f, "value": v })).collect::<Vec<_>>()) },
        "dumps": (DUMPS.iter().map(|(tag, title, branch, published)| json!({ "tag": tag, "title": title, "branch": branch, "published": published, "files": (dump_files(tag).into_iter().map(|(f, v)| json!({ "file": f, "value": v })).collect::<Vec<_>>()) })).collect::<Vec<_>>()),
    })
}

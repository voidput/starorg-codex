//! The mock's Loot screen data: synthetic loot tables and areas over the Developer mode
//! catalogue's items ([`super::mock_dev::CATALOGUE`], so an item links to a record the mock
//! Explorer can open) plus generated armour sets and rations, run through the real index
//! code ([`starorg_mcp::loot::LootScreen`]). No CIG data. `mock-loot.gen.json` (written by
//! the contract tests) carries the same answers for the TS mock.

use super::mock_dev::{CATALOGUE, catalogue_guid, catalogue_name};
use super::types::*;
use serde_json::{Value, json};
use starorg_mcp::loot::LootScreen;
use starorg_mcp::loot::input::{
    Datasets, ItemTags, LootArea, LootData, SpawnConfig, SpawnLoot, SpawnSlot, build_loot_index,
};
use std::sync::OnceLock;

const CAN: &str = "LootGeneration/CanGenerateAsLoot";
const COMMON: &str = "LootGeneration/LootRarity/Common";
const RARE: &str = "LootGeneration/LootRarity/Rare";
/// The build the mock's lists say they are of.
pub const MOCK_LOOT_BUILD: &str = "LIVE 4.10.200.58746";

/// The mock pool tag of a catalogue type, `None` for types that are no loot.
fn pool(ty: &str) -> Option<&'static str> {
    Some(match ty {
        "WeaponPersonal" => "Mock/FPSWeapon",
        "WeaponAttachment" => "Mock/Attachment",
        "WeaponGun" | "Cooler" | "Shield" | "PowerPlant" | "QuantumDrive" => "Mock/ShipPart",
        "FPS_Consumable" | "Food" | "Drink" => "Mock/Consumable",
        _ => return None,
    })
}

fn tags(v: &[&str]) -> Vec<String> {
    v.iter().map(|t| t.to_string()).collect()
}

const PIECES: &[(&str, &str)] = &[
    ("helmet", "Char_Armor_Helmet"),
    ("core", "Char_Armor_Torso"),
    ("arms", "Char_Armor_Arms"),
    ("legs", "Char_Armor_Legs"),
    ("backpack", "Char_Armor_Backpack"),
    ("undersuit", "Char_Armor_Undersuit"),
];
const COLOURS: &[&str] = &["ash", "crimson", "desert", "forest", "night", "storm"];
const RATIONS: &[&str] = &["berry", "chili", "citrus", "mint", "smoked", "vanilla"];

fn title(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

/// Every item the mock's loot can hold: catalogue loot types, 36 armour pieces (one with a
/// placeholder name, shown by its readable class name) and six rations.
pub fn mock_loot_items() -> Vec<ItemTags> {
    let mut out: Vec<ItemTags> = (0..CATALOGUE.len())
        .filter_map(|n| {
            let r = &CATALOGUE[n];
            let p = pool(r.1)?;
            Some(ItemTags {
                name: r.5.into(),
                class_name: catalogue_name(n),
                id: catalogue_guid(n),
                ty: Some(r.1.into()),
                tags: tags(&[CAN, if n % 3 == 0 { RARE } else { COMMON }, p]),
                ..Default::default()
            })
        })
        .collect();
    for (pi, (piece, ty)) in PIECES.iter().enumerate() {
        for (ci, colour) in COLOURS.iter().enumerate() {
            let class = format!("syn_armour_{piece}_{colour}");
            let name = if (pi, ci) == (3, 5) {
                class.clone()
            } else {
                format!("Pathfinder {} ({})", title(piece), title(colour))
            };
            out.push(ItemTags {
                name,
                class_name: class.clone(),
                id: format!("55000000-0000-4000-8000-{:012x}", pi * 16 + ci),
                ty: Some((*ty).into()),
                tags: tags(&[
                    CAN,
                    if ci == 4 { RARE } else { COMMON },
                    "Mock/Armour",
                    "Manufacturer/Synthetic",
                ]),
                ..Default::default()
            });
        }
    }
    for (i, flavour) in RATIONS.iter().enumerate() {
        out.push(ItemTags {
            name: format!("Ration Bar ({})", title(flavour)),
            class_name: format!("syn_food_ration_{flavour}"),
            id: format!("56000000-0000-4000-8000-{i:012x}"),
            ty: Some("Food".into()),
            tags: tags(&[CAN, COMMON, "Mock/Consumable"]),
            ..Default::default()
        });
    }
    out
}

fn loot_data() -> LootData {
    let entry = |tag: &str, weight: f64| json!({"name": tag, "tag": tag, "weight": weight});
    let rarity = json!([{"groupName": "Rarity", "entries": [{"tag": COMMON, "weight": 4}, {"tag": RARE, "weight": 1}]}]);
    let arch = |name: &str, entries: Vec<Value>| json!({"name": name, "primaryOrGroup": {"entries": entries}, "secondaryOrGroups": rarity});
    let table = |name: &str, archetype: &str| json!({"name": name, "version": "v1", "lootArchetypes": [{"archetype": archetype, "weight": 1}]});
    serde_json::from_value(json!({
        "tables": [
            table("Mock_Crate_Weapons", "Mock_Arch_Weapons"),
            table("Mock_Crate_Armour", "Mock_Arch_Armour"),
            table("Mock_Crate_Mixed", "Mock_Arch_Mixed"),
            table("Mock_Crate_Ship", "Mock_Arch_Ship"),
        ],
        "archetypes": [
            arch("Mock_Arch_Weapons", vec![entry("Mock/FPSWeapon", 3.0), entry("Mock/Attachment", 1.0)]),
            arch("Mock_Arch_Armour", vec![entry("Mock/Armour", 1.0)]),
            arch("Mock_Arch_Mixed", vec![entry("Mock/FPSWeapon", 1.0), entry("Mock/Armour", 2.0), entry("Mock/Consumable", 2.0)]),
            arch("Mock_Arch_Ship", vec![entry("Mock/ShipPart", 1.0)]),
        ],
        "filters": [],
        "secondaryChoices": []
    }))
    .expect("mock loot data")
}

/// A crate slot: `what` is its LootContents tag, `size` its LootSize.
fn crate_slot(table: &str, weight: f64, rolls: f64) -> SpawnSlot {
    SpawnSlot {
        entity: format!("syn_{}", table.to_lowercase()),
        entity_name: Some("Supply crate".into()),
        weight,
        loot: Some(SpawnLoot {
            table: table.into(),
            version: "v1".into(),
            max_results: Some(rolls),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn config(name: &str, what: &str, size: &str, fill: f64, slots: Vec<SpawnSlot>) -> SpawnConfig {
    SpawnConfig {
        name: Some(name.into()),
        spawn_tags: tags(&[
            &format!("LootGeneration/LootContents/{what}"),
            &format!("LootGeneration/LootSize/{size}"),
        ]),
        fill_chance: (fill < 1.0).then_some(fill),
        slots,
    }
}

fn mock_areas(items: &[ItemTags]) -> Vec<LootArea> {
    let class_of = |ty: &str| {
        items
            .iter()
            .find(|i| i.ty.as_deref() == Some(ty))
            .map(|i| i.class_name.clone())
    };
    let rack = |weight: f64| SpawnSlot {
        entity: "syn_weapon_rack".into(),
        entity_name: Some("weapon_rack_1_synthetic".into()),
        weight,
        contains: class_of("WeaponPersonal").map(|c| vec![c]),
        ..Default::default()
    };
    let loose = |weight: f64| SpawnSlot {
        entity: "syn_food_ration_berry".into(),
        weight,
        ..Default::default()
    };
    let area = |name: &str, configs: Vec<SpawnConfig>| LootArea {
        name: name.into(),
        configs,
    };
    let weapons = |size: &str, fill: f64| {
        config(
            "Weapons",
            "Weapons",
            size,
            fill,
            vec![crate_slot("Mock_Crate_Weapons", 1.0, 2.0)],
        )
    };
    let armour = |size: &str| {
        config(
            "Armour",
            "Armour",
            size,
            1.0,
            vec![crate_slot("Mock_Crate_Armour", 1.0, 1.0)],
        )
    };
    let mixed = |size: &str, fill: f64| {
        config(
            "Generic",
            "Generic",
            size,
            fill,
            vec![crate_slot("Mock_Crate_Mixed", 1.0, 3.0)],
        )
    };
    let ship = |size: &str| {
        config(
            "Ship parts",
            "ShipComponent",
            size,
            1.0,
            vec![crate_slot("Mock_Crate_Ship", 1.0, 1.0)],
        )
    };
    vec![
        area(
            "caves/loot_caves_occupied_rock",
            vec![
                weapons("Large", 1.0),
                armour("Small"),
                config(
                    "Loose",
                    "Loose",
                    "Small",
                    1.0,
                    vec![loose(1.0), crate_slot("Mock_Crate_Mixed", 3.0, 1.0)],
                ),
            ],
        ),
        area("caves/loot_caves_unoccupied", vec![mixed("Medium", 0.6)]),
        area(
            "lootgeneration_slotpreset_derelictoutpost",
            vec![
                config(
                    "Racks",
                    "Weapons",
                    "Medium",
                    1.0,
                    vec![rack(1.0), crate_slot("Mock_Crate_Weapons", 2.0, 1.0)],
                ),
                mixed("Small", 1.0),
            ],
        ),
        area(
            "v3slotpreset_distributioncentres_highsecurity",
            vec![weapons("Large", 1.0), armour("Large")],
        ),
        area(
            "v3slotpreset_distributioncentres_lowsecurity",
            vec![mixed("Medium", 1.0)],
        ),
        area(
            "lootgeneration_slotpreset_junksites",
            vec![mixed("Small", 0.5)],
        ),
        area(
            "lootgeneration_slotpreset_ugfs",
            vec![weapons("Medium", 0.8)],
        ),
        area(
            "v3slotpreset_asteroidbase",
            vec![ship("S2"), mixed("Large", 1.0)],
        ),
        area("v3slotpreset_spacederelict_large", vec![ship("S3")]),
        area(
            "lootgeneration_slotpreset_kareah",
            vec![config(
                "Racks",
                "Weapons",
                "Large",
                1.0,
                vec![rack(3.0), crate_slot("Mock_Crate_Weapons", 1.0, 2.0)],
            )],
        ),
        area(
            "contestedzone_vault_executive",
            vec![weapons("XLarge", 1.0), armour("Large")],
        ),
        area("v3slotpreset_loot_generic", vec![mixed("Medium", 1.0)]),
        // A test preset: not listed.
        area("test_slotpreset_caves", vec![weapons("Large", 1.0)]),
    ]
}

/// The mock's Loot screen, built once.
pub fn mock_loot_screen() -> &'static LootScreen {
    static S: OnceLock<LootScreen> = OnceLock::new();
    S.get_or_init(|| {
        let items = mock_loot_items();
        let d = Datasets {
            areas: Some(mock_areas(&items)),
            items,
            loot: Some(loot_data()),
            ..Default::default()
        };
        let index = build_loot_index(d.loot.as_ref().expect("loot"));
        LootScreen::build(&d, Some(&index), Some(MOCK_LOOT_BUILD.into()))
    })
}

/// `loot_list` with the channel's game data read (`ready`) or not (`missing`).
pub fn mock_loot_list(ready: bool, view: LootView) -> LootList {
    if ready {
        mock_loot_screen().list(view)
    } else {
        starorg_mcp::loot::missing_list(view)
    }
}

pub fn mock_loot_detail(ready: bool, view: LootView, id: &str) -> LootDetail {
    if ready {
        mock_loot_screen().detail(view, id)
    } else {
        starorg_mcp::loot::missing_detail(view)
    }
}

/// What the TS mock serves (`mock-loot.gen.json`): both lists and every detail of each.
pub fn gen_file() -> Value {
    let s = mock_loot_screen();
    let details = |view: LootView| -> Vec<LootDetail> {
        s.list(view)
            .rows
            .iter()
            .map(|r| s.detail(view, &r.id))
            .collect()
    };
    json!({
        "areas": s.list(LootView::Areas),
        "items": s.list(LootView::Items),
        "area_details": details(LootView::Areas),
        "item_details": details(LootView::Items),
    })
}

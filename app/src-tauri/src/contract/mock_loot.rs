//! The mock's Loot screen data: synthetic loot tables and areas over the Developer mode
//! catalogue's items ([`super::mock_dev::CATALOGUE`], so an item links to a record the mock
//! Explorer can open) plus generated armour sets and rations, run through the real index
//! code ([`starorg_mcp::loot::LootScreen`]), with a few actor tables so the areas list has
//! NPC bodies, and synthetic NPC loadouts (worn gear). No CIG data. `mock-loot.gen.json` (written by
//! the contract tests) carries the same answers for the TS mock.

use super::mock_dev::{CATALOGUE, catalogue_guid, catalogue_name};
use super::types::*;
use serde_json::{Value, json};
use starorg_mcp::loot::input::{
    Datasets, GearItem, ItemTags, LootArea, LootData, NpcActor, NpcLoadouts, SpawnConfig,
    SpawnLoot, SpawnSlot, build_loot_index,
};
use starorg_mcp::loot::{DetailQuery, LootScreen};
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
    let v3slot = |name: &str, weight: f64, tag: &str| json!({"name": name, "weight": weight, "archetype": {"entries": [{"name": name, "weight": 1, "selector": {"tags": {"positiveTags": [tag]}}}]}});
    let actor = |name: &str, slots: Vec<Value>| json!({"name": name, "version": "v3", "lootArchetypes": slots});
    serde_json::from_value(json!({
        "tables": [
            table("Mock_Crate_Weapons", "Mock_Arch_Weapons"),
            table("Mock_Crate_Armour", "Mock_Arch_Armour"),
            table("Mock_Crate_Mixed", "Mock_Arch_Mixed"),
            table("Mock_Crate_Ship", "Mock_Arch_Ship"),
            // NPC bodies (actor tables): a v1 pilot whose rations slot is always filled, V3
            // ground NPCs, a council-scrip variant (a priority slot naming its item by
            // class, there half the time), a named boss, and a template that is no NPC.
            {"name": "LootTable_Actor_AcePilot", "version": "v1", "lootArchetypes": [
                {"archetype": "LootArchetype_Actor_AcePilot_Rations", "weight": 1,
                 "numberOfResultsConstraints": {"minResults": 1, "maxResults": 1}},
                {"archetype": "Mock_Arch_Weapons", "weight": 0.5,
                 "numberOfResultsConstraints": {"minResults": 0, "maxResults": 1}}]},
            actor("V3LootTable_Actor_Grunt", vec![v3slot("Ammo", 10.0, "Mock/Attachment"), v3slot("Rations", 1.0, "Mock/Consumable")]),
            actor("V3LootTable_Actor_Soldier", vec![v3slot("Ammo", 10.0, "Mock/Attachment"), v3slot("Sidearm", 2.0, "Mock/FPSWeapon")]),
            actor("V3LootTable_Actor_Soldier_CouncilScrip", vec![
                v3slot("Ammo", 10.0, "Mock/Attachment"),
                v3slot("Sidearm", 2.0, "Mock/FPSWeapon"),
                json!({"name": "Unique item", "weight": 1, "archetype": {"entries": [
                    {"name": "Mint ration", "weight": 1, "selector": {"entityClasses": {"entityClasses": ["syn_food_ration_mint"]}}}]},
                    "optionalData": [{"priorityCount": 1}, {"choiceLimit": 1}, {"chanceToExist": 0.5}]}),
            ]),
            actor("V3LootTable_Actor_SOO_MendoRen", vec![v3slot("Ammo", 10.0, "Mock/Attachment"), v3slot("Armour", 1.0, "Mock/Armour")]),
            actor("V3LootTable_Actor_TEMPLATE", vec![v3slot("Ammo", 1.0, "Mock/Attachment")]),
        ],
        "archetypes": [
            arch("LootArchetype_Actor_AcePilot_Rations", vec![entry("Mock/Consumable", 1.0)]),
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
        // Two occupied caves: an `Occupied` level in the tree.
        area(
            "caves/loot_caves_occupied_sand_pyro",
            vec![weapons("Medium", 0.7), armour("Medium")],
        ),
        // A wing with and without a security level: a `Cargo wing` level.
        area(
            "distributioncentre_multiconfig_cargowing",
            vec![mixed("Large", 1.0)],
        ),
        area(
            "distributioncentre_multiconfig_cargowing_high",
            vec![weapons("Medium", 1.0), mixed("Small", 0.5)],
        ),
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
        // The same place in another preset family: `General (multi)` beside `(classic)`.
        area(
            "lootgeneration_multislotpreset_kareah",
            vec![armour("Medium"), mixed("Small", 1.0)],
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
        let gear = mock_gear(&items);
        let d = Datasets {
            areas: Some(mock_areas(&items)),
            items,
            loot: Some(loot_data()),
            ..Default::default()
        };
        let index = build_loot_index(d.loot.as_ref().expect("loot"));
        LootScreen::build_with_gear(&d, Some(&index), Some(&gear), Some(MOCK_LOOT_BUILD.into()))
    })
}

/// The mock's NPC loadouts (`game-npc-loadouts`): the ace pilot (its body-loot table), two
/// criminal grunts with the same gear (male and female: one row) and a gated sidearm, a
/// Nine Tails soldier wearing a jacket item-tags lacks, two civilians, and a test actor.
fn mock_gear(items: &[ItemTags]) -> NpcLoadouts {
    let gun = items
        .iter()
        .find(|i| i.ty.as_deref() == Some("WeaponPersonal"))
        .map_or_else(|| "syn_rifle".to_string(), |i| i.class_name.clone());
    let it = |item: &str, slot: &str, chance: f64, requires: &[&str]| GearItem {
        item: item.into(),
        name: None,
        slot: slot.into(),
        chance,
        requires: requires.iter().map(|r| r.to_string()).collect(),
    };
    let piece = |p: &str, c: &str| format!("syn_armour_{p}_{c}");
    let actor =
        |class: &str, name: &str, faction: &str, table: Option<&str>, items: Vec<GearItem>| {
            NpcActor {
                class: class.into(),
                name: name.into(),
                faction: Some(faction.into()),
                loot_table: table.map(str::to_string),
                lootable: true,
                wear: Some(Wear { min: 0.0, max: 0.5 }),
                items,
            }
        };
    let grunt = || {
        vec![
            it(&piece("undersuit", "ash"), "Undersuit", 1.0, &[]),
            it(&piece("helmet", "ash"), "Helmet", 0.5, &[]),
            it(&piece("helmet", "storm"), "Helmet", 0.5, &[]),
            it(&piece("core", "ash"), "Torso", 1.0, &[]),
            it(&gun, "Primary weapon", 1.0, &[]),
            it(
                "syn_food_ration_chili",
                "Carried",
                0.25,
                &["AI/NpcClass/Heavy"],
            ),
        ]
    };
    let mut jacket = it("syn_nt_jacket_01", "Clothing", 1.0, &[]);
    jacket.name = Some("Nine Tails Jacket".into());
    let civilian = |n: &str, colour: &str| {
        actor(
            &format!("NPC_Archetypes-Male-Human-Civilians-Levski-Civilian_Levski_{n}"),
            &format!("Male civilians levski {n}"),
            "genericcivilians",
            None,
            vec![it(&piece("legs", colour), "Legs", 1.0, &[])],
        )
    };
    NpcLoadouts {
        actors: vec![
            actor(
                "PU_Human_Enemy_GroundCombat_NPC_GenericCriminal_AcePilot",
                "Generic criminal ace pilot",
                "genericcriminal",
                Some("loottable_actor_acepilot"),
                vec![
                    it(&piece("undersuit", "night"), "Undersuit", 1.0, &[]),
                    it(&piece("helmet", "night"), "Helmet", 1.0, &[]),
                    it(&piece("core", "night"), "Torso", 1.0, &[]),
                    it(&gun, "Primary weapon", 1.0, &[]),
                ],
            ),
            actor(
                "PU_Human-Criminal-Grunt-Female-Grunt_01",
                "Criminal grunt female 01",
                "genericcriminal",
                Some("v3loottable_actor_grunt"),
                grunt(),
            ),
            actor(
                "PU_Human-Criminal-Grunt-Male-Grunt_01",
                "Criminal grunt male 01",
                "genericcriminal",
                Some("v3loottable_actor_grunt"),
                grunt(),
            ),
            actor(
                "PU_Human-NineTails-Grunt-Male-Grunt_02",
                "Nine tails grunt male 02",
                "ninetails",
                None,
                vec![
                    jacket,
                    it(&piece("arms", "forest"), "Arms", 1.0, &[]),
                    it(&piece("legs", "forest"), "Legs", 1.0, &[]),
                ],
            ),
            civilian("01", "desert"),
            civilian("02", "crimson"),
            actor(
                "FeatureTest_Human_Male-Grunt",
                "Feature test grunt",
                "genericcriminal",
                None,
                grunt(),
            ),
        ],
        ..Default::default()
    }
}

/// `loot_list` with the channel's game data read (`ready`) or not (`missing`).
pub fn mock_loot_list(ready: bool, view: LootView) -> LootList {
    if ready {
        mock_loot_screen().list(view)
    } else {
        starorg_mcp::loot::missing_list(view)
    }
}

pub fn mock_loot_detail(ready: bool, view: LootView, id: &str, q: &DetailQuery) -> LootDetail {
    if ready {
        mock_loot_screen().detail(view, id, q)
    } else {
        starorg_mcp::loot::missing_detail(view)
    }
}

/// What the TS mock serves (`mock-loot.gen.json`): both lists and every detail of each,
/// ungrouped with every row (the TS mock groups, filters and caps them as
/// `LootScreen::detail` does).
pub fn gen_file() -> Value {
    let s = mock_loot_screen();
    let flat = DetailQuery {
        group_by: Some(LootGroupBy::None),
        ..DetailQuery::default()
    };
    let details = |view: LootView| -> Vec<LootDetail> {
        s.list(view)
            .rows
            .iter()
            .map(|r| s.detail(view, &r.id, &flat))
            .collect()
    };
    json!({
        "areas": s.list(LootView::Areas),
        "items": s.list(LootView::Items),
        "area_details": details(LootView::Areas),
        "item_details": details(LootView::Items),
    })
}

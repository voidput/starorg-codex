use super::*;
use crate::ini::BOM;
use crate::model::{Component, Contract, ReputationStanding, Ship, ShipLoc, Weapon};

fn stock(keys: &[&str]) -> StockIni {
    let mut b = BOM.to_vec();
    for k in keys {
        b.extend_from_slice(format!("{k}=text of {k}\r\n").as_bytes());
    }
    b.extend_from_slice(b"manufacturer_NameAEGS=Aegis Dynamics\r\n");
    StockIni::parse(b)
}

fn item(
    record: &str,
    ty: &str,
    name_key: &str,
    mfr: Option<&str>,
    size: Option<i64>,
) -> ItemIdentity {
    ItemIdentity {
        record_name: format!("EntityClassDefinition.{record}"),
        item_type: Some(ty.into()),
        size,
        manufacturer: mfr.map(str::to_string),
        loc: crate::model::LocKeys {
            name: Some(format!("@{name_key}")),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn game_data() -> GameData {
    let mut gd = GameData::default();
    gd.components.push(Component {
        item: item(
            "AEGS_Cooler_S2",
            "Cooler",
            "item_NameAEGS_Cooler_S2",
            Some("AEGS"),
            Some(2),
        ),
        category: "coolers".into(),
        ..Default::default()
    });
    gd.components.push(Component {
        item: item("XNAA_Fuel", "Unheard", "item_NameXNAA_Fuel", None, None),
        category: "fuelTanks".into(),
        ..Default::default()
    });
    gd.weapons.push(Weapon {
        item: item(
            "BEHR_Rifle",
            "WeaponPersonal",
            "item_NameBEHR_Rifle",
            Some("BEHR"),
            Some(1),
        ),
        class: WeaponClass::Fps,
        ..Default::default()
    });
    gd.ships.push(Ship {
        record_name: "EntityClassDefinition.AEGS_Gladius".into(),
        path: "entities/spaceships/aegs/aegs_gladius".into(),
        loc: ShipLoc {
            name: Some("@vehicle_NameAEGS_Gladius".into()),
            ..Default::default()
        },
        manufacturer: Some("aegs".into()),
        ..Default::default()
    });
    gd.contracts.push(Contract {
        generator: "g".into(),
        debug_name: "d".into(),
        title_key: Some("@Bounty_Title_01".into()),
        ..Default::default()
    });
    gd.reputation_standings.push(ReputationStanding {
        name: "n".into(),
        record_name: "SReputationStandingParams.n".into(),
        display_name_key: Some("@RepStanding_Friendly".into()),
        ..Default::default()
    });
    gd
}

fn entity(ty: &str, folder: &str) -> RefSource {
    RefSource {
        struct_type: "EntityClassDefinition".into(),
        item_type: Some(ty.into()),
        folder: folder.into(),
        ..Default::default()
    }
}

fn plain(struct_type: &str, folder: &str) -> RefSource {
    RefSource {
        struct_type: struct_type.into(),
        folder: folder.into(),
        ..Default::default()
    }
}

const KEYS: &[&str] = &[
    "item_NameAEGS_Cooler_S2",
    "item_NameXNAA_Fuel",
    "item_NameBEHR_Rifle",
    "item_DescBEHR_Rifle",
    "vehicle_NameAEGS_Gladius",
    "Bounty_Title_01",
    "RepStanding_Friendly",
    "item_NameShared,P",
    "hud_Speed",
    "Pyro5_Station",
    "Stanton2b_Moon",
    "human_Greeting_01",
    "item_NameUnknownThing",
    "zzz_mystery",
    "ui_canvas_text",
    "Paint_Blue",
    "item_NameFood_Burrito",
];

fn refs() -> KeyRefs {
    let mut b = KeyRefsBuilder::new();
    // Used by a UI canvas three times and by an item once: the item wins (rank).
    b.add(
        plain("BuildingBlocks_Canvas", "ui/hud"),
        ["@item_NameShared", "@item_NameShared", "@item_NameShared"],
    );
    b.add(
        RefSource {
            manufacturer: Some("aegs".into()),
            size: Some(3),
            ..entity("Shield", "entities/scitem/ships")
        },
        ["@item_NameShared"],
    );
    b.add(
        plain("BuildingBlocks_Canvas", "ui/hud/visor"),
        ["@ui_canvas_text", "@LOC_EMPTY", "@"],
    );
    b.add(
        entity("Brand_New_Type", "entities/scitem"),
        ["@item_NameUnknownThing"],
    );
    b.add(
        entity("FPS_Consumable", "entities/scitem"),
        ["@item_NameFood_Burrito"],
    );
    b.finish()
}

#[test]
fn every_key_gets_exactly_one_leaf_and_counts_sum() {
    let s = stock(KEYS);
    let idx = assign(&s, &game_data(), Some(&refs()));
    assert_eq!(idx.rows.len(), s.key_count());
    assert!(idx.exact);
    for row in 0..idx.rows.len() {
        let g = idx.group(row);
        assert!(!has_children(g.id), "{} is not a leaf", g.id);
    }
}

#[test]
fn rules_in_order() {
    let s = stock(KEYS);
    let idx = assign(&s, &game_data(), Some(&refs()));
    let group = |k: &str| {
        let row = s.entries().position(|(_, key, _)| key == k).unwrap();
        idx.group(row).id
    };
    // Rule 1: GameData links, including the category fallback for an unnamed type.
    assert_eq!(group("item_NameAEGS_Cooler_S2"), "components/coolers");
    assert_eq!(group("item_NameXNAA_Fuel"), "components/fuel");
    assert_eq!(group("item_NameBEHR_Rifle"), "fps-weapons/weapons");
    assert_eq!(group("vehicle_NameAEGS_Gladius"), "ships/ships");
    assert_eq!(group("Bounty_Title_01"), "missions/contracts");
    assert_eq!(group("RepStanding_Friendly"), "reputation");
    // Rule 2: item beats the UI canvas although the canvas has more references.
    assert_eq!(group("item_NameShared,P"), "components/shields");
    assert_eq!(group("ui_canvas_text"), "interface/hud");
    assert_eq!(group("item_NameUnknownThing"), UNKNOWN_ITEM);
    assert_eq!(group("item_NameFood_Burrito"), "consumables/other");
    // Rule 3: namesake (desc key of the rifle), prefixes, numbered stems.
    assert_eq!(group("item_DescBEHR_Rifle"), "fps-weapons/weapons");
    assert_eq!(group("hud_Speed"), "interface/hud");
    assert_eq!(group("Pyro5_Station"), "locations/star-map");
    assert_eq!(group("Stanton2b_Moon"), "locations/star-map");
    assert_eq!(group("human_Greeting_01"), "dialogue/voice");
    assert_eq!(group("Paint_Blue"), "ships/paints");
    // Rule 4.
    assert_eq!(group("zzz_mystery"), OTHER);
    assert_eq!(idx.unknown_item_types.get("Brand_New_Type"), Some(&1));
}

#[test]
fn facets_and_also_used_by() {
    let s = stock(KEYS);
    let idx = assign(&s, &game_data(), Some(&refs()));
    let row = |k: &str| s.entries().position(|(_, key, _)| key == k).unwrap();
    let r = row("item_NameShared,P");
    assert_eq!(idx.manufacturer(r).unwrap().label, "Aegis Dynamics");
    assert_eq!(idx.size(r), Some(3));
    assert_eq!(idx.also_used_by(r), vec!["HUD"]);
    // BEHR has no stock display name: no manufacturer facet.
    let r = row("item_NameBEHR_Rifle");
    assert_eq!(idx.manufacturer(r), None);
    assert_eq!(idx.size(r), Some(1));
    let r = row("vehicle_NameAEGS_Gladius");
    assert_eq!(idx.manufacturer(r).unwrap().code, "AEGS");
    assert_eq!(idx.manufacturer(row("hud_Speed")), None);
    assert_eq!(idx.size(row("hud_Speed")), None);
    let labels: Vec<&str> = idx.manufacturers.iter().map(|m| m.label.as_str()).collect();
    assert_eq!(labels, ["Aegis Dynamics"]);
}

#[test]
fn without_keyrefs_is_approximate() {
    let s = stock(KEYS);
    let idx = assign(&s, &game_data(), None);
    assert!(!idx.exact);
    let row = s
        .entries()
        .position(|(_, k, _)| k == "ui_canvas_text")
        .unwrap();
    assert_eq!(idx.group(row).id, "interface/menus");
}

#[test]
fn hierarchy_is_well_formed() {
    let mut seen = std::collections::HashSet::new();
    for (i, g) in HIERARCHY.iter().enumerate() {
        assert!(seen.insert(g.id), "duplicate id {}", g.id);
        if let Some(p) = g.parent {
            let pi = HIERARCHY
                .iter()
                .position(|x| x.id == p)
                .expect("parent exists");
            assert!(pi < i, "{} listed before its parent", g.id);
            assert!(HIERARCHY[pi].parent.is_none(), "two levels only");
            assert!(g.id.starts_with(&format!("{p}/")), "{} under {p}", g.id);
        }
    }
    let leaves =
        |t: &'static [(&'static str, &'static str)]| t.iter().map(|(_, g)| *g).collect::<Vec<_>>();
    let all = leaves(rules::ITEM_TYPES)
        .into_iter()
        .chain(leaves(rules::ITEM_TYPE_PREFIXES))
        .chain(leaves(rules::KEY_PREFIXES))
        .chain(leaves(rules::CANVAS_PATHS))
        .chain(rules::STRUCT_TYPES.iter().map(|(_, g, _)| *g))
        .chain(rules::STRUCT_PREFIXES.iter().map(|(_, g, _)| *g))
        .chain([UNKNOWN_ITEM, OTHER]);
    for leaf in all {
        assert!(index_of(leaf).is_some(), "{leaf} is not in the hierarchy");
        assert!(!has_children(leaf), "{leaf} is not a leaf");
    }
}

/// The item types the addendum names map to their groups (8.3, 8.8).
#[test]
fn named_item_types() {
    for (ty, leaf) in [
        ("Cooler", "components/coolers"),
        ("PowerPlant", "components/power-plants"),
        ("Shield", "components/shields"),
        ("QuantumDrive", "components/quantum-drives"),
        ("Radar", "components/radars"),
        ("MainThruster", "components/thrusters"),
        ("ManneuverThruster", "components/thrusters"),
        ("FuelTank", "components/fuel"),
        ("FuelIntake", "components/fuel"),
        ("QuantumFuelTank", "components/fuel"),
        ("Armor", "components/armor"),
        ("Turret", "components/turrets"),
        ("MissileLauncher", "components/missile-racks"),
        ("WeaponGun", "ship-weapons/guns"),
        ("WeaponDefensive", "ship-weapons/countermeasures"),
        ("Missile", "missiles"),
        ("Bomb", "missiles"),
        ("WeaponPersonal", "fps-weapons/weapons"),
        ("WeaponAttachment", "fps-weapons/attachments"),
        ("Char_Armor_Helmet", "armor/helmets"),
        ("Char_Armor_Something", "armor/other"),
        ("Char_Clothing_Feet", "clothing/feet"),
        ("Food", "consumables/food"),
        ("Drink", "consumables/drink"),
        ("Cargo", "cargo/cargo"),
        ("Usable", "items/usables"),
        ("Misc", "items/misc"),
        ("NOITEM_Player", "items/misc"),
        ("Paints", "ships/paints"),
        ("NOITEM_Vehicle", "ships/ships"),
    ] {
        assert_eq!(rules::find_item_type(ty), Some(leaf), "{ty}");
    }
    assert_eq!(
        entity_leaf("NOITEM_Vehicle", None, "entities/groundvehicles/x"),
        Some("ships/ground")
    );
    assert_eq!(
        entity_leaf("FPS_Consumable", Some("MedPack"), ""),
        Some("consumables/medical")
    );
}

#[test]
fn builder_caps_and_orders_candidates() {
    let mut b = KeyRefsBuilder::new();
    for i in 0..6 {
        let keys: Vec<&str> = std::iter::repeat_n("@k", i + 1).collect();
        b.add(plain(&format!("T{i}"), ""), keys);
    }
    // Same source twice is one source.
    b.add(plain("T0", ""), ["@other"]);
    let r = b.finish();
    assert_eq!(r.sources.len(), 6);
    let k = &r.keys["k"];
    assert_eq!(k.len(), MAX_CANDIDATES);
    assert_eq!(k[0], [5, 6]);
    assert_eq!(r.keys["other"], vec![[0, 1]]);
    let json = serde_json::to_string(&r).unwrap();
    assert_eq!(serde_json::from_str::<KeyRefs>(&json).unwrap(), r);
}

#[test]
fn ids_are_stable() {
    // The UI remembers selections by id; renaming one is a breaking change.
    let ids: Vec<&str> = HIERARCHY
        .iter()
        .filter(|g| g.parent.is_none())
        .map(|g| g.id)
        .collect();
    assert_eq!(
        ids,
        [
            "ships",
            "components",
            "ship-weapons",
            "missiles",
            "fps-weapons",
            "armor",
            "clothing",
            "consumables",
            "cargo",
            "items",
            "missions",
            "blueprints",
            "reputation",
            "locations",
            "manufacturers",
            "dialogue",
            "interface",
            "game-modes",
            "other"
        ]
    );
}

/// Real data, read only: a worker output folder (`extract-worker --keyrefs --out DIR`) in
/// `STARORG_GROUPS_DIR`. Prints the distribution; run with `--ignored --nocapture`.
#[test]
#[ignore = "needs a real extraction in STARORG_GROUPS_DIR"]
fn real_extraction_distribution() {
    let dir = std::path::PathBuf::from(
        std::env::var("STARORG_GROUPS_DIR").expect("set STARORG_GROUPS_DIR"),
    );
    let stock = StockIni::parse(std::fs::read(dir.join("base.ini")).unwrap());
    let gd: GameData =
        serde_json::from_slice(&std::fs::read(dir.join("gamedata.json")).unwrap()).unwrap();
    let kr: KeyRefs =
        serde_json::from_slice(&std::fs::read(dir.join("keyrefs.json")).unwrap()).unwrap();
    let t = std::time::Instant::now();
    let idx = assign(&stock, &gd, Some(&kr));
    let took = t.elapsed();
    let approx = assign(&stock, &gd, None);
    let mut counts: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for row in 0..idx.rows.len() {
        counts.entry(idx.group(row).id).or_default().0 += 1;
        counts.entry(approx.group(row).id).or_default().1 += 1;
    }
    let total = idx.rows.len();
    let referenced = stock
        .entries()
        .filter(|(_, k, _)| kr.keys.contains_key(&fold_key(k)))
        .count();
    eprintln!(
        "keys {total}, referenced {referenced}, assign {took:?}, manufacturers {}",
        idx.manufacturers.len()
    );
    for g in HIERARCHY {
        if let Some((e, a)) = counts.get(g.id) {
            eprintln!("{:<34} {:>6} exact {:>6} approximate", g.id, e, a);
        }
    }
    eprintln!("unknown item types: {:?}", idx.unknown_item_types);
    let other = counts.get(OTHER).map(|c| c.0).unwrap_or(0);
    assert!(referenced >= 55_000, "{referenced} referenced keys");
    assert!(other * 10 < total, "Other holds {other} of {total}");
}

use super::*;
use crate::blueprints::{OwnedEntry, SOURCE_LOG, SOURCE_SMART_CITIZEN, Source};
use crate::groups::KeyRefsBuilder;
use crate::groups::RefSource;
use crate::model::{Component, Weapon};

/// Synthetic build shaped like LIVE 4.10 (paths, categories, name keys); no CIG text.
fn bp(rec: &str, id: &str, path: &str, cat: &str, produces: &str) -> Blueprint {
    Blueprint {
        record_name: format!("CraftingBlueprintRecord.{rec}"),
        id: id.into(),
        path: format!("crafting/blueprints/crafting/{path}/{}", rec.to_lowercase()),
        category: Some(cat.into()),
        produces: Some(produces.into()),
        produces_name_key: Some(format!("@item_Name{produces}")),
        tiers: Vec::new(),
    }
}

fn ident(rec: &str, ty: &str, size: i64, grade: i64, mfr: &str) -> ItemIdentity {
    ItemIdentity {
        record_name: format!("EntityClassDefinition.{rec}"),
        item_type: Some(ty.into()),
        size: Some(size),
        grade: Some(grade),
        manufacturer: Some(mfr.into()),
        ..Default::default()
    }
}

fn game() -> (GameData, StockIni) {
    let mut gd = GameData::default();
    gd.components.push(Component {
        item: ident("SYN_Cooler_S2", "Cooler", 2, 3, "SYNA"),
        category: "coolers".into(),
        ..Default::default()
    });
    gd.weapons.push(Weapon {
        item: ident("SYN_LaserCannon_S3", "WeaponGun", 3, 1, "synb"),
        ..Default::default()
    });
    gd.weapons.push(Weapon {
        item: ident("SYN_Rifle_01", "WeaponPersonal", 3, 1, "SYNB"),
        ..Default::default()
    });
    gd.blueprints = vec![
        bp(
            "BP_CRAFT_SYN_Cooler_S2",
            "00000000-0000-0000-0000-00000000000a",
            "vehiclegear/cooler",
            "VehicleComponentS2",
            "SYN_Cooler_S2",
        ),
        bp(
            "BP_CRAFT_SYN_LaserCannon_S3",
            "00000000-0000-0000-0000-00000000000b",
            "vehiclegear/weapons/laser/cannon",
            "VehicleWeaponsS3",
            "SYN_LaserCannon_S3",
        ),
        bp(
            "BP_CRAFT_SYN_Rifle_01",
            "00000000-0000-0000-0000-00000000000c",
            "fpsgear/weapons/rifle",
            "FPSWeapons",
            "SYN_Rifle_01",
        ),
        bp(
            "BP_CRAFT_syn_heavy_armor_arms_01",
            "00000000-0000-0000-0000-00000000000d",
            "fpsgear/armour/combat/heavy",
            "FPSArmours",
            "syn_heavy_armor_arms_01",
        ),
        bp(
            "BP_CRAFT_syn_undersuit_01",
            "00000000-0000-0000-0000-00000000000e",
            "fpsgear/armour/undersuit",
            "FPSArmours",
            "syn_undersuit_01",
        ),
        bp(
            "BP_CRAFT_SYN_Ammo_01",
            "00000000-0000-0000-0000-00000000000f",
            "fpsgear/ammo/ballistic",
            "FPSWeapons",
            "SYN_Ammo_01",
        ),
        bp(
            "BP_CRAFT_SYN_Crate",
            "00000000-0000-0000-0000-000000000010",
            "missionitems",
            "MissionItem",
            "SYN_Crate",
        ),
        // Two blueprints of one name (a variant): matched by name, neither is marked.
        bp(
            "BP_CRAFT_syn_light_armor_core_01",
            "00000000-0000-0000-0000-000000000011",
            "fpsgear/armour/stealth/light",
            "FPSArmours",
            "syn_light_armor_core_01",
        ),
        bp(
            "BP_CRAFT_syn_light_armor_core_02",
            "00000000-0000-0000-0000-000000000012",
            "fpsgear/armour/stealth/light",
            "FPSArmours",
            "syn_light_armor_core_02",
        ),
        // No stock name: left out.
        bp(
            "BP_CRAFT_SYN_Unnamed",
            "00000000-0000-0000-0000-000000000013",
            "vehiclegear/shield",
            "VehicleComponentS1",
            "SYN_Unnamed",
        ),
    ];
    let ini = "\u{FEFF}item_NameSYN_Cooler_S2=Synthetic Cooler\n\
item_NameSYN_LaserCannon_S3=Synthetic Cannon\n\
item_NameSYN_Rifle_01=Test Rifle\n\
item_Namesyn_heavy_armor_arms_01=Sample Arms Heavy\n\
item_Namesyn_undersuit_01=Sample Undersuit\n\
item_NameSYN_Ammo_01=Test Magazine\n\
item_NameSYN_Crate=Sample Crate\n\
item_Namesyn_light_armor_core_01=Sample Core\n\
item_Namesyn_light_armor_core_02=Sample Core\n\
manufacturer_NameSYNA=Synthetic Aerospace\n\
manufacturer_NameSYNB=Sample Arms Co\n\
manufacturer_NameSYNC=Test Outfitters\n";
    (gd, StockIni::parse(ini.as_bytes().to_vec()))
}

fn by_record<'a>(c: &'a [CatalogItem], rec: &str) -> &'a CatalogItem {
    c.iter().find(|i| i.record == rec).unwrap()
}

#[test]
fn catalog_classifies_from_paths_and_items() {
    let (gd, stock) = game();
    let c = catalog(&gd, &stock, None);
    assert_eq!(c.len(), 9, "the unnamed blueprint is left out");
    let cooler = by_record(&c, "BP_CRAFT_SYN_Cooler_S2");
    assert_eq!(cooler.kind, Kind::ShipComponent);
    assert_eq!(cooler.type_label.as_deref(), Some("Cooler"));
    assert_eq!((cooler.size, cooler.grade.as_deref()), (Some(2), Some("C")));
    assert_eq!(cooler.manufacturer.as_deref(), Some("SYNA"));
    assert_eq!(
        cooler.manufacturer_name.as_deref(),
        Some("Synthetic Aerospace")
    );
    let cannon = by_record(&c, "BP_CRAFT_SYN_LaserCannon_S3");
    assert_eq!(cannon.kind, Kind::ShipWeapon);
    assert_eq!(cannon.type_label.as_deref(), Some("Laser cannon"));
    assert_eq!((cannon.size, cannon.grade.as_deref()), (Some(3), None));
    assert_eq!(
        cannon.manufacturer.as_deref(),
        Some("SYNB"),
        "codes are upper-cased"
    );
    let rifle = by_record(&c, "BP_CRAFT_SYN_Rifle_01");
    assert_eq!(
        (rifle.kind, rifle.type_label.as_deref(), rifle.size),
        (Kind::FpsWeapon, Some("Rifle"), Some(3))
    );
    let arms = by_record(&c, "BP_CRAFT_syn_heavy_armor_arms_01");
    assert_eq!(arms.kind, Kind::Armor);
    assert_eq!(
        (arms.type_label.as_deref(), arms.class.as_deref()),
        (Some("Arms"), Some("Heavy"))
    );
    assert_eq!(
        (arms.size, arms.manufacturer.as_deref()),
        (None, None),
        "no item record, no keyrefs"
    );
    let suit = by_record(&c, "BP_CRAFT_syn_undersuit_01");
    assert_eq!(suit.type_label.as_deref(), Some("Undersuit"));
    let ammo = by_record(&c, "BP_CRAFT_SYN_Ammo_01");
    assert_eq!(
        (ammo.kind, ammo.type_label.as_deref()),
        (Kind::FpsAmmo, Some("Ballistic ammo"))
    );
    assert_eq!(by_record(&c, "BP_CRAFT_SYN_Crate").kind, Kind::MissionItem);
}

#[test]
fn groups_add_armor_maker_and_slot() {
    let (gd, stock) = game();
    let mut b = KeyRefsBuilder::new();
    b.add(
        RefSource {
            struct_type: "EntityClassDefinition".into(),
            item_type: Some("Char_Armor_Arms".into()),
            sub_type: Some("Heavy".into()),
            size: Some(1),
            manufacturer: Some("SYNC".into()),
            folder: "entities/scitem/characters".into(),
        },
        ["@item_Namesyn_heavy_armor_arms_01"],
    );
    let refs = b.finish();
    let groups = crate::groups::assign(&stock, &gd, Some(&refs));
    let c = catalog(&gd, &stock, Some(&groups));
    let arms = by_record(&c, "BP_CRAFT_syn_heavy_armor_arms_01");
    assert_eq!(arms.manufacturer.as_deref(), Some("SYNC"));
    assert_eq!(arms.manufacturer_name.as_deref(), Some("Test Outfitters"));
    assert_eq!(arms.type_label.as_deref(), Some("Arms"));
}

#[test]
fn classify_falls_back_to_category() {
    let mut b = bp("BP_X", "x", "odd/place", "VehicleWeaponsS4", "X");
    b.path.clear();
    let k = classify(&b, None, None);
    assert_eq!(k.kind, Kind::ShipWeapon);
    // Record words name what `$templates` paths do not.
    for (rec, path, cat, ty) in [
        (
            "BP_CRAFT_x_crossbow_ballistic_01",
            "fpsgear/weapons/$templates",
            "FPSWeapons",
            "Crossbow",
        ),
        (
            "BP_CRAFT_x_hmg_ballistic_01",
            "fpsgear/weapons/$templates",
            "FPSWeapons",
            "HMG",
        ),
        (
            "BP_CRAFT_Cargo_ShipMining_Pod_X",
            "vehiclegear",
            "VehicleComponentS0",
            "Ore pod",
        ),
        (
            "BP_CRAFT_Mining_Modules_Active_X",
            "vehiclegear",
            "VehicleComponentS0",
            "Mining module",
        ),
        (
            "BP_CRAFT_X_TachyonCannon_S1",
            "vehiclegear/weapons/$templates/ballistic",
            "VehicleWeaponsS1",
            "Tachyon cannon",
        ),
    ] {
        let b = bp(rec, "x", path, cat, "X");
        assert_eq!(classify(&b, None, None).ty.as_deref(), Some(ty), "{rec}");
    }
    assert_eq!(category_size("VehicleWeaponsS4"), Some(4));
    assert_eq!(category_size("FPSWeapons"), None);
    assert_eq!(category_size("MissionItem"), None);
    b.category = Some("Unknown".into());
    assert_eq!(classify(&b, None, None).kind, Kind::Other);
}

fn entry(name: &str, ids: &[&str], ambiguous: bool, kind: &str, time: &str) -> OwnedEntry {
    OwnedEntry {
        name: name.into(),
        ids: ids.iter().map(|s| s.to_string()).collect(),
        ambiguous,
        favorite: false,
        sources: vec![Source {
            kind: kind.into(),
            time: Some(time.into()),
            file: None,
        }],
    }
}

fn set(geid: &str, entries: Vec<OwnedEntry>) -> OwnedSet {
    let mut s = OwnedSet::new(Some(geid));
    for e in entries {
        s.entries.insert(normalize(&e.name), e);
    }
    s
}

fn chars(list: &[(&str, &str, &str)]) -> Characters {
    let mut c = Characters::default();
    for (g, n, t) in list {
        c.seen(g, None, n, Some(t));
    }
    c
}

#[test]
fn build_merges_channels_and_keeps_ownership_scoped() {
    let (gd, stock) = game();
    let live_cat = catalog(&gd, &stock, None);
    // PTU's build lacks the crate and has one blueprint LIVE does not.
    let mut ptu_gd = gd.clone();
    ptu_gd
        .blueprints
        .retain(|b| !b.record_name.ends_with("SYN_Crate"));
    let ptu_cat = catalog(&ptu_gd, &stock, None);
    let live = ChannelInput {
        channel: "LIVE".into(),
        catalog: Some(live_cat),
        characters: chars(&[
            ("111", "Pilot_A", "2026-11-14T10:00:00Z"),
            ("222", "Alt_B", "2026-11-01T10:00:00Z"),
        ]),
        sets: vec![
            set(
                "111",
                vec![
                    // Log entries carry names only.
                    entry(
                        "Synthetic Cooler",
                        &[],
                        false,
                        SOURCE_LOG,
                        "2026-11-10T00:00:00Z",
                    ),
                    entry(
                        "Test Rifle",
                        &["00000000-0000-0000-0000-00000000000C"],
                        false,
                        SOURCE_SMART_CITIZEN,
                        "2026-11-01T00:00:00Z",
                    ),
                    // One name, two blueprints: an own row, marks neither.
                    entry(
                        "Sample Core",
                        &[],
                        false,
                        SOURCE_LOG,
                        "2026-11-02T00:00:00Z",
                    ),
                    // Import matched several: stays an ambiguous row.
                    entry(
                        "Synthetic Cannon",
                        &["a", "b"],
                        true,
                        SOURCE_SMART_CITIZEN,
                        "2026-11-03T00:00:00Z",
                    ),
                    // In no build.
                    entry(
                        "Retired Thing",
                        &[],
                        false,
                        SOURCE_LOG,
                        "2026-10-01T00:00:00Z",
                    ),
                ],
            ),
            set(
                "222",
                vec![entry(
                    "Test Rifle",
                    &[],
                    false,
                    SOURCE_LOG,
                    "2026-11-05T00:00:00Z",
                )],
            ),
            // A set file without its character id: counted as skipped, not shown.
            OwnedSet {
                geid: None,
                ..set(
                    "x",
                    vec![entry(
                        "Test Rifle",
                        &[],
                        false,
                        SOURCE_LOG,
                        "2026-11-05T00:00:00Z",
                    )],
                )
            },
        ],
        unattributed: Some({
            let mut u = set(
                "x",
                vec![entry(
                    "Sample Crate",
                    &[],
                    false,
                    SOURCE_LOG,
                    "2026-10-02T00:00:00Z",
                )],
            );
            u.geid = None;
            u
        }),
        unreadable: 0,
    };
    let ptu = ChannelInput {
        channel: "PTU".into(),
        catalog: Some(ptu_cat),
        // Same geid as LIVE (PTU copy), newer name.
        characters: chars(&[("111", "Pilot_A2", "2026-11-15T10:00:00Z")]),
        sets: vec![set(
            "111",
            vec![entry(
                "Test Rifle",
                &[],
                false,
                "starorg:Org",
                "2026-11-12T00:00:00Z",
            )],
        )],
        unattributed: None,
        unreadable: 0,
    };
    let hotfix = ChannelInput {
        channel: "HOTFIX".into(),
        catalog: None,
        ..Default::default()
    };
    let f = build(&[live, ptu, hotfix]);
    assert_eq!(
        f.channels
            .iter()
            .map(|c| (
                c.channel.as_str(),
                c.catalog,
                c.characters,
                c.owned,
                c.skipped
            ))
            .collect::<Vec<_>>(),
        vec![
            ("LIVE", Some(9), 2, 7, 1),
            ("PTU", Some(8), 1, 1, 0),
            ("HOTFIX", None, 0, 0, 0)
        ]
    );
    // Two characters, then the unattributed set as a last pseudo-character.
    assert_eq!(f.characters.len(), 3);
    assert_eq!(f.characters[2].id, UNATTRIBUTED);
    let crate_row = f.rows.iter().find(|r| r.name == "Sample Crate").unwrap();
    assert_eq!(crate_row.owners[0].character, UNATTRIBUTED);
    let a = f.characters.iter().find(|c| c.id == "111").unwrap();
    assert_eq!(
        (a.name.as_str(), a.channels.clone()),
        ("Pilot_A2", vec!["LIVE".to_string(), "PTU".into()])
    );

    let row = |name: &str| f.rows.iter().filter(|r| r.name == name).collect::<Vec<_>>();
    let rifle = row("Test Rifle");
    assert_eq!(rifle.len(), 1, "one row over channels");
    let rifle = rifle[0];
    assert_eq!(rifle.key, "00000000-0000-0000-0000-00000000000c");
    assert_eq!(rifle.channels, vec!["LIVE", "PTU"]);
    let owners: Vec<(&str, &str, &str)> = rifle
        .owners
        .iter()
        .map(|o| {
            (
                o.channel.as_str(),
                o.character.as_str(),
                o.sources[0].as_str(),
            )
        })
        .collect();
    assert_eq!(
        owners,
        vec![
            ("LIVE", "111", SOURCE_SMART_CITIZEN),
            ("LIVE", "222", SOURCE_LOG),
            ("PTU", "111", "starorg:Org")
        ]
    );
    assert_eq!(row("Sample Crate")[0].channels, vec!["LIVE"]);
    // Name-only match.
    assert_eq!(row("Synthetic Cooler")[0].owners.len(), 1);
    // A log name two blueprints have: a name row, ambiguous, listing both candidates and
    // what they agree on; each candidate shows the owner as "maybe".
    let core = row("Sample Core");
    assert_eq!(core.len(), 3);
    let name_row = core.iter().find(|r| r.key.starts_with("name:")).unwrap();
    assert_eq!(name_row.owners.len(), 1);
    assert!(
        name_row.owners[0].ambiguous,
        "a name several blueprints have"
    );
    assert_eq!(name_row.candidates.len(), 2);
    assert_eq!(
        (
            name_row.kind,
            name_row.type_label.as_deref(),
            name_row.class.as_deref()
        ),
        (Kind::Armor, Some("Torso"), Some("Light"))
    );
    assert_eq!(name_row.channels, vec!["LIVE", "PTU"]);
    for c in core.iter().filter(|r| r.record.is_some()) {
        assert!(c.owners.is_empty());
        assert_eq!(c.maybe.len(), 1);
        assert_eq!(c.maybe[0].character, "111");
    }
    let cannon = row("Synthetic Cannon");
    assert_eq!(cannon.len(), 2);
    assert!(
        cannon
            .iter()
            .any(|r| r.key == "name:synthetic cannon" && r.owners[0].ambiguous)
    );
    // Its import ids are unknown here; the one blueprint of that name is the candidate.
    let cannon_bp = cannon.iter().find(|r| r.record.is_some()).unwrap();
    assert!(cannon_bp.owners.is_empty());
    assert_eq!(cannon_bp.maybe.len(), 1);
    let retired = row("Retired Thing");
    assert_eq!(
        (
            retired[0].kind,
            retired[0].channels.len(),
            retired[0].candidates.len()
        ),
        (Kind::Other, 0, 0)
    );
    // Sorted by name.
    let names: Vec<String> = f.rows.iter().map(|r| r.name.to_lowercase()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
}

#[test]
fn two_entries_on_one_blueprint_merge_into_one_owner() {
    let (gd, stock) = game();
    let mut s = set(
        "111",
        vec![entry(
            "Test Rifle",
            &[],
            false,
            SOURCE_LOG,
            "2026-11-05T00:00:00Z",
        )],
    );
    // An import under another (tagged) name, matched by id.
    s.entries.insert(
        "rifle alias".into(),
        entry(
            "Rifle Alias",
            &["00000000-0000-0000-0000-00000000000c"],
            false,
            SOURCE_SMART_CITIZEN,
            "2026-11-01T00:00:00Z",
        ),
    );
    let f = build(&[ChannelInput {
        channel: "LIVE".into(),
        catalog: Some(catalog(&gd, &stock, None)),
        characters: chars(&[("111", "Pilot_A", "2026-11-14T10:00:00Z")]),
        sets: vec![s],
        unattributed: None,
        unreadable: 0,
    }]);
    let r = f.rows.iter().find(|r| r.name == "Test Rifle").unwrap();
    assert_eq!(r.owners.len(), 1);
    assert_eq!(
        r.owners[0].sources,
        vec![SOURCE_SMART_CITIZEN.to_string(), SOURCE_LOG.into()]
    );
    assert_eq!(
        r.owners[0].acquired.as_deref(),
        Some("2026-11-01T00:00:00Z")
    );
}

/// Real build, read only: `STARORG_REAL_CACHE=<cache entry dir> cargo test -p starorg-core
/// real_catalog -- --ignored --nocapture`. Prints counts only.
#[test]
#[ignore]
fn real_catalog() {
    let Ok(dir) = std::env::var("STARORG_REAL_CACHE") else {
        return;
    };
    let dir = std::path::Path::new(&dir);
    let gd: GameData =
        serde_json::from_slice(&std::fs::read(dir.join("gamedata.json")).unwrap()).unwrap();
    let stock = StockIni::parse(std::fs::read(dir.join("base.ini")).unwrap());
    let refs: Option<crate::groups::KeyRefs> = std::fs::read(dir.join("keyrefs.json"))
        .ok()
        .map(|b| serde_json::from_slice(&b).unwrap());
    let groups = crate::groups::assign(&stock, &gd, refs.as_ref());
    let t = std::time::Instant::now();
    let c = catalog(&gd, &stock, Some(&groups));
    println!(
        "{} named of {} blueprints in {:?}",
        c.len(),
        gd.blueprints.len(),
        t.elapsed()
    );
    let mut by: BTreeMap<(Kind, Option<String>), usize> = BTreeMap::new();
    for i in &c {
        *by.entry((i.kind, i.type_label.clone())).or_default() += 1;
    }
    for (k, n) in by {
        println!("{n:5} {:?} {:?}", k.0, k.1);
    }
    let count = |f: &dyn Fn(&CatalogItem) -> bool| c.iter().filter(|i| f(i)).count();
    println!(
        "manufacturer {} (named {}), size {}, grade {}, class {}",
        count(&|i| i.manufacturer.is_some()),
        count(&|i| i.manufacturer_name.is_some()),
        count(&|i| i.size.is_some()),
        count(&|i| i.grade.is_some()),
        count(&|i| i.class.is_some()),
    );
    // Owned sets of the same channel (`STARORG_REAL_DATA=<channel data dir>`), counts only.
    let Ok(data) = std::env::var("STARORG_REAL_DATA") else {
        return;
    };
    let data = std::path::Path::new(&data);
    let characters = crate::blueprints::load_characters(data).unwrap();
    let sets = characters
        .characters
        .keys()
        .map(|g| crate::blueprints::load_set(data, g).unwrap())
        .collect();
    let f = build(&[ChannelInput {
        channel: "LIVE".into(),
        catalog: Some(c),
        characters,
        sets,
        unattributed: crate::blueprints::load_unattributed(data).ok(),
        unreadable: 0,
    }]);
    let owned: Vec<&Row> = f.rows.iter().filter(|r| !r.owners.is_empty()).collect();
    println!(
        "characters {}, owned rows {} (in catalog {}, by name only {}), max owners {}",
        f.characters.len(),
        owned.len(),
        owned.iter().filter(|r| !r.channels.is_empty()).count(),
        owned.iter().filter(|r| r.channels.is_empty()).count(),
        owned.iter().map(|r| r.owners.len()).max().unwrap_or(0),
    );
    let names: Vec<&Row> = f.rows.iter().filter(|r| r.record.is_none()).collect();
    println!(
        "name rows {} (with candidates {}, kinds {:?}), maybe-owned blueprints {}, skipped {}",
        names.len(),
        names.iter().filter(|r| !r.candidates.is_empty()).count(),
        names.iter().map(|r| r.kind).collect::<Vec<_>>(),
        f.rows.iter().filter(|r| !r.maybe.is_empty()).count(),
        f.channels[0].skipped,
    );
}

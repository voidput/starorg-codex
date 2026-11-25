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
        catalog: Some(Arc::new(live_cat)),
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
        catalog: Some(Arc::new(ptu_cat)),
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
        catalog: Some(Arc::new(catalog(&gd, &stock, None))),
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
        serde_json::from_slice(&crate::gz::read_either(&dir.join("gamedata.json")).unwrap())
            .unwrap();
    let stock = StockIni::parse(crate::gz::read_either(&dir.join("base.ini")).unwrap());
    let refs: Option<crate::groups::KeyRefs> = crate::gz::read_either(&dir.join("keyrefs.json"))
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
        catalog: Some(Arc::new(c)),
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

/// A merge seen on one channel (old geid -> kept geid) joins the character up on every
/// channel in All channels: one character, its owners under the kept geid.
#[test]
fn merged_geids_join_up_across_channels() {
    let who = |g: &str, name: &str| crate::blueprints::Character {
        account_id: Some("77".into()),
        name: name.into(),
        last_seen: Some(format!(
            "2026-0{}-01T00:00:00Z",
            if g == "2002" { 3 } else { 1 }
        )),
    };
    let mut live = Characters::default();
    live.characters
        .insert("2002".into(), who("2002", "Test_Pilot"));
    live.aliases.insert("1001".into(), "2002".into());
    let mut ptu = Characters::default();
    ptu.characters
        .insert("1001".into(), who("1001", "Test_Pilot"));
    let mut old = OwnedSet::new(Some("1001"));
    old.add(
        "Synthetic Thing",
        Source {
            kind: SOURCE_LOG.into(),
            time: None,
            file: None,
        },
    );
    let f = build(&[
        ChannelInput {
            channel: "LIVE".into(),
            characters: live,
            ..Default::default()
        },
        ChannelInput {
            channel: "PTU".into(),
            characters: ptu,
            sets: vec![old],
            ..Default::default()
        },
    ]);
    let ids: Vec<&str> = f.characters.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["2002"]);
    assert_eq!(f.characters[0].channels, ["LIVE", "PTU"]);
    let row = f.rows.iter().find(|r| r.name == "Synthetic Thing").unwrap();
    assert_eq!(row.owners[0].character, "2002");
    assert_eq!(row.owners[0].channel, "PTU");
}

fn reward(pool: &str, chance: Option<f64>) -> crate::model::BlueprintReward {
    crate::model::BlueprintReward {
        pool: pool.into(),
        chance,
    }
}

fn pool(name: &str, list: &[(&str, Option<f64>)]) -> crate::model::BlueprintPool {
    crate::model::BlueprintPool {
        name: name.into(),
        blueprints: list
            .iter()
            .map(|(b, w)| crate::model::BlueprintPoolEntry {
                blueprint: (*b).into(),
                weight: *w,
            })
            .collect(),
    }
}

fn contract(
    debug: &str,
    title: Option<&str>,
    rewards: Vec<crate::model::BlueprintReward>,
) -> Contract {
    Contract {
        generator: "SYN_Generator".into(),
        debug_name: debug.into(),
        title_key: title.map(|t| format!("@{t}")),
        blueprint_rewards: rewards,
        ..Default::default()
    }
}

fn rep(faction: &str, name_key: Option<&str>) -> crate::model::ReputationReward {
    crate::model::ReputationReward {
        faction: Some(faction.into()),
        faction_name_key: name_key.map(str::to_string),
        amount: 100.0,
        on_success: true,
        on_failure: false,
    }
}

/// The game of [`game`] with contracts: a bounty whose two variants both award a weighted
/// pool (paying two factions), a salvage job awarded by chance (pool without weights), a
/// courier run only one of whose two variants awards a pool, a crate run whose pool holds
/// one blueprint, an untitled contract and an unreleased one.
fn game_with_missions() -> (GameData, StockIni) {
    let (mut gd, stock) = game();
    let mut bounty_a = contract(
        "SYN_Bounty_Easy",
        Some("syn_bounty"),
        vec![reward("BP_POOL_A", Some(1.0))],
    );
    bounty_a.reputation.push(rep(
        "factionreputation_synthetic_cds",
        Some("@syn_faction_cds"),
    ));
    bounty_a.systems = vec!["Stanton".into()];
    let mut bounty_b = contract(
        "SYN_Bounty_Hard",
        Some("syn_bounty"),
        vec![reward("BP_POOL_A", None)],
    );
    bounty_b
        .reputation
        .push(rep("factionreputation_lawful_other_guild", None));
    bounty_b.systems = vec!["Pyro".into()];
    let mut salvage = contract(
        "SYN_Salvage",
        Some("syn_salvage"),
        vec![reward("BP_POOL_B", Some(0.25))],
    );
    salvage.chain = true;
    salvage.once_only = true;
    salvage.min_standing = Some("syn_trusted".into());
    let courier_a = contract(
        "SYN_Courier_A",
        Some("syn_courier"),
        vec![reward("bp_pool_b", Some(1.0))],
    );
    let courier_b = contract("SYN_Courier_B", Some("syn_courier"), Vec::new());
    let crate_run = contract(
        "SYN_Crate_Run",
        Some("syn_crate"),
        vec![reward("BP_POOL_C", Some(1.0))],
    );
    let untitled = contract("SYN_Untitled", None, vec![reward("BP_POOL_A", Some(1.0))]);
    let mut unreleased = contract(
        "SYN_Test_Only",
        Some("syn_test"),
        vec![reward("BP_POOL_A", Some(1.0))],
    );
    unreleased.not_for_release = true;
    gd.contracts = vec![
        bounty_a, bounty_b, salvage, courier_a, courier_b, crate_run, untitled, unreleased,
    ];
    gd.blueprint_pools = vec![
        // The rifle twice: one entry, both weights.
        pool(
            "BP_POOL_A",
            &[
                ("BP_CRAFT_SYN_Rifle_01", Some(1.0)),
                ("BP_CRAFT_SYN_Cooler_S2", Some(3.0)),
                ("BP_CRAFT_SYN_Rifle_01", Some(1.0)),
            ],
        ),
        pool(
            "BP_POOL_B",
            &[
                ("BP_CRAFT_SYN_Rifle_01", None),
                ("BP_CRAFT_SYN_Ammo_01", None),
                ("BP_CRAFT_SYN_Cooler_S2", None),
            ],
        ),
        pool("BP_POOL_C", &[("BP_CRAFT_SYN_Crate", Some(1.0))]),
    ];
    gd.reputation_standings = vec![crate::model::ReputationStanding {
        name: "syn_trusted".into(),
        record_name: "SReputationStandingParams.syn_trusted".into(),
        display_name_key: Some("@syn_standing_trusted".into()),
        min_reputation: Some(500.0),
        ..Default::default()
    }];
    let mut ini = stock.as_bytes().to_vec();
    ini.extend_from_slice(
        b"syn_bounty=Eliminate the <EM4>Sample</EM4> Boss\n\
syn_salvage=Recover cargo near ~mission(Location|Address)\n\
syn_courier=Sample Courier Run\n\
syn_crate=Sample Crate Run\n\
syn_test=Unreleased Test\n\
syn_standing_trusted=Trusted\n\
syn_faction_cds=Synthetic Salvage Corp\n",
    );
    (gd, StockIni::parse(ini))
}

#[test]
fn earn_lists_the_missions_that_award_a_blueprint() {
    let (gd, stock) = game_with_missions();
    let c = catalog(&gd, &stock, None);
    let rifle = by_record(&c, "BP_CRAFT_SYN_Rifle_01").earn.clone().unwrap();
    let titles: Vec<Option<&str>> = rifle.iter().map(|e| e.mission.title.as_deref()).collect();
    // Guaranteed pools first (titled before untitled), then by title; the unreleased one is
    // left out.
    assert_eq!(
        titles,
        [
            Some("Eliminate the Sample Boss"),
            None,
            Some("Recover cargo near [Location]"),
            Some("Sample Courier Run"),
        ]
    );
    let bounty = &rifle[0];
    // The pool is sure, the rifle is one draw of two: not "guaranteed" for the rifle.
    assert_eq!(
        (bounty.odds.as_str(), bounty.award),
        ("Guaranteed", Award::GuaranteedPool)
    );
    assert_eq!((bounty.variants, bounty.mission.contracts), (2, 2));
    assert_eq!(
        (bounty.pool.as_str(), bounty.pick.as_deref()),
        ("2", Some("40%")),
        "duplicate weights add up"
    );
    assert_eq!(bounty.mission.record, "SYN_Bounty_Easy (+1 more)");
    assert_eq!(bounty.mission.generator, "SYN_Generator");
    // Every faction the title pays: the game's name when the record has one.
    assert_eq!(
        bounty.mission.factions,
        [
            "factionreputation_synthetic_cds",
            "factionreputation_lawful_other_guild"
        ]
    );
    assert_eq!(
        bounty.mission.reputation,
        ["Synthetic Salvage Corp", "Other Guild"]
    );
    assert_eq!(bounty.mission.systems, ["Pyro", "Stanton"]);
    assert!(bounty.mission.kinds.is_empty());
    assert_eq!(rifle[1].mission.record, "SYN_Untitled");
    let salvage = &rifle[2];
    assert_eq!(
        (salvage.odds.as_str(), salvage.award),
        ("25%", Award::Chance)
    );
    assert_eq!(
        (salvage.pool.as_str(), salvage.pick.as_deref()),
        ("3", None),
        "no weights, no pick share"
    );
    assert_eq!(salvage.mission.kinds, ["Chain", "Once only"]);
    assert_eq!(salvage.mission.requires.as_deref(), Some("Trusted"));
    assert!(salvage.mission.systems.is_empty(), "no locality: not known");
    assert!(salvage.mission.factions.is_empty());
    // Only one of the two courier variants awards a pool (pool names fold case).
    let courier = &rifle[3];
    assert_eq!(
        (
            courier.odds.as_str(),
            courier.award,
            courier.variants,
            courier.mission.contracts
        ),
        ("Some variants only", Award::Chance, 1, 2)
    );
    // A sure pool of one: the blueprint itself is guaranteed.
    let crate_earn = by_record(&c, "BP_CRAFT_SYN_Crate").earn.clone().unwrap();
    assert_eq!(
        (crate_earn[0].award, crate_earn[0].pool.as_str()),
        (Award::Guaranteed, "1")
    );
    let cooler = by_record(&c, "BP_CRAFT_SYN_Cooler_S2")
        .earn
        .clone()
        .unwrap();
    assert_eq!(cooler.len(), 4);
    assert_eq!(cooler[0].pick.as_deref(), Some("60%"));
    assert_eq!(
        by_record(&c, "BP_CRAFT_SYN_LaserCannon_S3").earn,
        Some(Vec::new())
    );
    // Contracts not read: not known, rather than "no missions".
    let (plain, stock) = game();
    assert!(
        catalog(&plain, &stock, None)
            .iter()
            .all(|i| i.earn.is_none())
    );
}

#[test]
fn finder_rows_merge_every_channels_missions() {
    let (gd, stock) = game_with_missions();
    // LIVE's contracts were read but award nothing; HOTFIX has PTU's; RETAIL is not read.
    let (mut live, live_stock) = game();
    live.contracts = vec![contract("SYN_Nothing", None, Vec::new())];
    let input = |channel: &str, cat: Vec<CatalogItem>| ChannelInput {
        channel: channel.into(),
        catalog: Some(Arc::new(cat)),
        ..Default::default()
    };
    let (plain, plain_stock) = game();
    let mut ptu = input("PTU", catalog(&gd, &stock, None));
    ptu.characters = chars(&[("1", "Pilot", "2026-01-01T00:00:00Z")]);
    ptu.sets = vec![set(
        "1",
        vec![entry(
            "Not A Blueprint",
            &[],
            false,
            SOURCE_LOG,
            "2026-01-01T00:00:00Z",
        )],
    )];
    let f = build(&[
        input("LIVE", catalog(&live, &live_stock, None)),
        ptu,
        input("HOTFIX", catalog(&gd, &stock, None)),
        input("RETAIL", catalog(&plain, &plain_stock, None)),
    ]);
    let row = |n: &str| f.rows.iter().find(|r| r.name == n).unwrap();
    let rifle = row("Test Rifle");
    // LIVE's empty list does not hide the other channels' missions; each says where it is.
    assert_eq!(rifle.earn_read, ["LIVE", "PTU", "HOTFIX"]);
    assert_eq!(rifle.earn.len(), 4);
    assert!(rifle.earn.iter().all(|e| e.channels == ["PTU", "HOTFIX"]));
    assert_eq!(f.missions.len(), 5, "each mission once over the channels");
    let mut sorted = f.missions.clone();
    sorted.sort();
    assert_eq!(f.missions, sorted);
    assert_eq!(
        f.missions[rifle.earn[0].mission as usize].title.as_deref(),
        Some("Eliminate the Sample Boss")
    );
    let cooler = row("Synthetic Cooler");
    assert_eq!(
        cooler.earn[0].mission, rifle.earn[0].mission,
        "shared, not copied"
    );
    let name_row = row("Not A Blueprint");
    assert!(
        name_row.earn.is_empty() && name_row.earn_read.is_empty(),
        "a name row: not known"
    );
}

#[test]
fn readable_titles_and_faction_names() {
    assert_eq!(
        readable_title("Go to ~mission(Location|Address) <EM4>now</EM4>  "),
        "Go to [Location] now"
    );
    assert_eq!(readable_title("50% ~ off <b"), "50% ~ off <b");
    assert_eq!(faction_label("factionreputation_lawful_ftl"), "FTL");
    assert_eq!(
        faction_label("factionreputation_unlawful_red_wind"),
        "Red Wind"
    );
    assert_eq!(faction_label("FactionReputation_Lawful"), "Lawful");
    assert_eq!(faction_label("hockrow_agency"), "Hockrow Agency");
}

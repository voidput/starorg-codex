//! `GameData` extraction against small synthetic record fixtures (golden), and against the
//! real game (ignored by default).
//!
//! Fixtures are hand-written StarBreaker-export-shaped records with made-up names and
//! filler strings: no CIG prose.
//!
//! Regenerate the golden snapshot with `UPDATE_GOLDEN=1 cargo test -p starorg-data --test
//! gamedata` and review the diff.

use starorg_core::ini::StockIni;
use starorg_core::model::{FireKind, GameData, KeySource, WeaponClass};
use starorg_data::gamedata::store::FixtureStore;
use starorg_data::gamedata::{Loc, drift, extract};
use std::path::{Path, PathBuf};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fixture_gamedata() -> (GameData, Vec<starorg_core::source::Warning>) {
    let records: Vec<serde_json::Value> =
        serde_json::from_slice(&std::fs::read(fixtures().join("records.json")).unwrap()).unwrap();
    let store = FixtureStore::from_envelopes(records).unwrap();
    let ini = StockIni::parse(std::fs::read(fixtures().join("strings.ini")).unwrap());
    let x = extract(&store, &Loc::new(&ini), None);
    (x.game_data, x.warnings)
}

#[test]
fn golden_snapshot() {
    let (gd, _) = fixture_gamedata();
    let got = serde_json::to_string_pretty(&gd).unwrap() + "\n";
    let path = fixtures().join("gamedata.golden.json");
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &got).unwrap();
    }
    let want = std::fs::read_to_string(&path).expect("golden file; run with UPDATE_GOLDEN=1");
    assert_eq!(
        got, want,
        "GameData changed; review and regenerate the golden file"
    );
}

#[test]
fn ship_weapon_modes_ammo_and_capacitor() {
    let (gd, _) = fixture_gamedata();
    let w = gd.weapon("TEST_Repeater_S3").unwrap();
    assert_eq!(w.class, WeaponClass::Ship);
    assert_eq!(w.item.item_type.as_deref(), Some("WeaponGun"));
    assert_eq!(w.item.manufacturer.as_deref(), Some("TEST"));
    assert_eq!(
        w.item.loc.desc.as_deref(),
        Some("@item_DescTEST_Repeater_S3")
    );
    // First DamageInfo with any non-zero part: 40.5 energy + 4.5 distortion.
    let ammo = w.ammo.as_ref().unwrap();
    assert_eq!(ammo.damage_per_shot, Some(45.0));
    assert_eq!(ammo.range, Some(4000.0));
    assert_eq!(w.damage_type.as_deref(), Some("energy"));
    assert_eq!(w.modes.len(), 2);
    assert_eq!(w.modes[0].kind, FireKind::Rapid);
    assert_eq!(w.modes[0].dps, Some(450.0));
    // Burst 3 at 900 RPM + 0.25 s cooldown sustains 400 RPM -> 300 DPS.
    assert_eq!(w.modes[1].sustained_fire_rate, Some(400.0));
    assert_eq!(w.modes[1].dps, Some(300.0));
    assert_eq!(w.dps, Some(450.0), "headline is the highest-DPS mode");
    // Capacitor weapon: magazine 0, so the capacitor's shots stand in.
    assert_eq!(w.ammo_capacity, Some(90.0));
    assert_eq!(w.power, Some(2.0));
    assert_eq!(w.em, Some(41.0));
    assert_eq!(w.health, Some(550.0));
    let falloff = &ammo.falloff.as_ref().unwrap()["energy"];
    assert_eq!(
        (falloff.min_distance, falloff.per_meter, falloff.floor),
        (50.0, 0.2, 20.0)
    );
}

#[test]
fn fps_weapon_ammo_comes_through_the_magazine() {
    let (gd, _) = fixture_gamedata();
    let w = gd.weapon("TEST_Rifle").unwrap();
    assert_eq!(w.class, WeaponClass::Fps);
    let ammo = w.ammo.as_ref().unwrap();
    assert_eq!(ammo.magazine.as_deref(), Some("TEST_Rifle_Mag"));
    assert_eq!(ammo.capacity, Some(30.0));
    assert_eq!(
        w.fire_rate,
        Some(900.0),
        "300/900 RPM rifle: headline is 900"
    );
    assert_eq!(w.headline().unwrap().label(), "Auto");
    assert_eq!(w.mass, Some(1.3));
}

#[test]
fn components_missile_and_rack() {
    let (gd, _) = fixture_gamedata();
    let s = gd.component("TEST_Shield_S1").unwrap();
    assert_eq!(s.category, "shields");
    assert_eq!(s.stats["MaxShieldHealth"], 7480.0);
    assert_eq!(s.distortion_max, Some(2450.0));
    let r = s.detail.resistance.as_ref().unwrap();
    assert_eq!(r["physical"].max, 0.25);
    assert_eq!(
        gd.component("TEST_Power_S1").unwrap().stats["powerOutput"],
        20.0
    );
    let rack = gd.component("TEST_Rack_S3").unwrap();
    assert_eq!(rack.category, "missileRacks");
    assert_eq!(rack.stats["missileCount"], 2.0);
    assert_eq!(rack.stats["missileSize"], 2.0);
    assert_eq!(gd.component("TEST_Grid").unwrap().stats["scu"], 16.0);
    let m = gd.missile("TEST_Missile_S2").unwrap();
    assert_eq!(m.tracking_signal.as_deref(), Some("Infrared"));
    assert_eq!(m.damage_total, Some(1700.0));
    assert_eq!(m.speed, Some(1250.0));
    assert!(!m.is_bomb);
    // The paint variant shares the namesake's keys and has no own name.
    let black = gd.weapon("TEST_Repeater_S3_Black").unwrap();
    assert_eq!(
        black.item.loc.name.as_deref(),
        Some("@item_NameTEST_Repeater_S3")
    );
    assert!(!black.item.is_template);
}

#[test]
fn ship_loadout_flight_and_armor() {
    let (gd, _) = fixture_gamedata();
    let ship = gd.ship("TEST_Ship").unwrap();
    assert_eq!(ship.loc.desc.as_deref(), Some("@vehicle_DescTEST_Ship"));
    assert!(!ship.is_template);
    let f = ship.flight.as_ref().unwrap();
    // x = pitch, y = roll, z = yaw from maxAngularVelocity, never speedProfile.
    assert_eq!(
        (f.pitch, f.roll, f.yaw),
        (Some(38.0), Some(110.0), Some(32.0))
    );
    let lo = ship.loadout.as_ref().unwrap();
    assert_eq!(lo.weapons.len(), 1, "{:?}", lo.weapons);
    assert_eq!(lo.weapons[0].count(), 2.0, "gun under a gun is skipped");
    assert_eq!(lo.missiles[0].count(), 2.0);
    assert_eq!(lo.missiles[0].rack.as_deref(), Some("TEST_Rack_S3"));
    assert_eq!(lo.missile_racks, Some(1.0));
    assert_eq!(lo.cargo_grids[0].item, "TEST_Grid");
    let t = ship.totals.as_ref().unwrap();
    assert_eq!(t.weapon_dps, Some(900.0));
    assert_eq!(t.shield_health, Some(7480.0));
    assert_eq!(t.cargo_scu, Some(16.0));
    assert_eq!(t.missile_damage, Some(3400.0));
    let a = ship.armor.as_ref().unwrap();
    assert_eq!(a.health, Some(3300.0));
    assert_eq!(a.damage_multiplier.as_ref().unwrap()["physical"], 0.75);
    let bb = ship.bounding_box.as_ref().unwrap();
    assert_eq!((bb["x"], bb["y"], bb["z"]), (16.5, 21.0, 5.5));
}

#[test]
fn contracts_blueprints_reputation() {
    let (gd, _) = fixture_gamedata();
    assert_eq!(
        gd.contracts.len(),
        3,
        "two here, one of the hauling generator"
    );
    let easy = gd
        .contracts
        .iter()
        .find(|c| c.debug_name == "TEST_Contract_Easy")
        .unwrap();
    assert_eq!(easy.title_key.as_deref(), Some("@TEST_title"));
    assert_eq!(easy.title_from, Some(KeySource::Contract));
    assert_eq!(easy.description_key.as_deref(), Some("@TEST_shared_desc"));
    assert_eq!(easy.description_from, Some(KeySource::Generator));
    assert!(easy.once_only && easy.chain);
    assert_eq!(easy.reputation_gain(), Some(150.0));
    assert_eq!(easy.failure_penalty(), Some(-1000.0));
    assert_eq!(easy.blueprint_rewards[0].pool, "BP_MISSIONREWARD_TEST");
    assert_eq!(easy.blueprint_rewards[0].chance, Some(1.0));
    assert_eq!(easy.item_rewards[0].amount, 3.0);
    assert_eq!(
        easy.item_rewards[0].name_key.as_deref(),
        Some("@item_NameTEST_Armor_Arms")
    );
    assert_eq!(
        easy.min_standing.as_deref(),
        Some("ReputationStanding_TEST_Rank2")
    );
    assert_eq!(easy.difficulty.as_deref(), Some("general"));
    let p2 = gd
        .contracts
        .iter()
        .find(|c| c.debug_name == "TEST_Contract_Phase2")
        .unwrap();
    assert_eq!(p2.title_key.as_deref(), Some("@TEST_P2_title"));
    assert_eq!(p2.title_from, Some(KeySource::Template));
    assert_eq!(p2.description_key.as_deref(), Some("@TEST_shared_desc"));
    assert_eq!(
        gd.blueprints.len(),
        1,
        "dismantle record without tiers is skipped"
    );
    let bp = &gd.blueprints[0];
    assert_eq!(bp.produces.as_deref(), Some("TEST_Armor_Arms"));
    assert_eq!(
        bp.produces_name_key.as_deref(),
        Some("@item_NameTEST_Armor_Arms")
    );
    assert_eq!(bp.category.as_deref(), Some("FPSArmours"));
    assert_eq!(bp.tiers[0].craft_time_seconds, Some(90.0));
    assert_eq!(
        gd.blueprint_pools[0].blueprints[0].blueprint,
        "BP_CRAFT_TEST_Armor_Arms"
    );
    let st = &gd.reputation_standings[0];
    assert_eq!(st.min_reputation, Some(2500.0));
    assert_eq!(
        st.display_name_key.as_deref(),
        Some("@RepStanding_TEST_Rank2")
    );
}

#[test]
fn fixture_has_no_drift_but_empty_categories_do() {
    let (gd, warnings) = fixture_gamedata();
    // The fixture lacks some categories on purpose (coolers, radars, quantum drives).
    assert!(
        warnings.iter().all(|w| w.kind == drift::KIND),
        "{warnings:?}"
    );
    let text: Vec<_> = warnings.iter().map(|w| w.detail.as_str()).collect();
    assert!(
        text.iter()
            .any(|t| t.starts_with("coolers: query returned nothing"))
    );
    assert!(!text.iter().any(|t| t.starts_with("ships:")), "{text:?}");
    assert!(
        !text.iter().any(|t| t.starts_with("contracts:")),
        "{text:?}"
    );
    assert_eq!(gd.counts().ships, 1);
}

// ---------------------------------------------------------------------------------------
// Real data (ignored by default; needs a game install). Reading only.

fn real_channel(name: &str) -> Option<PathBuf> {
    let root = std::env::var_os("STARORG_GAME_DIR").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(
            "/mnt/games/star-citizen/drive_c/Program Files/Roberts Space Industries/StarCitizen",
        )
    });
    let p = root.join(name).join("Data.p4k");
    p.is_file().then_some(p)
}

fn peak_rss_kb() -> Option<u64> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    s.lines()
        .find(|l| l.starts_with("VmHWM:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

fn real_extract(channel: &str) -> Option<starorg_data::extract::Extraction> {
    let p4k = real_channel(channel)?;
    let t = std::time::Instant::now();
    let x = starorg_data::extract::extract(&p4k).unwrap();
    eprintln!(
        "{channel}: extraction {:.2} s, peak RSS {} MB, {:?}",
        t.elapsed().as_secs_f64(),
        peak_rss_kb().unwrap_or(0) / 1024,
        x.game_data.counts()
    );
    Some(x)
}

/// Spec 14 (M2): zero schema drift on LIVE; reference values of spec 13.
#[test]
#[ignore = "needs the real LIVE Data.p4k"]
fn real_live_has_no_drift_and_reference_values() {
    let Some(x) = real_extract("LIVE") else {
        return;
    };
    let drift: Vec<_> = x.warnings.iter().filter(|w| w.0 == drift::KIND).collect();
    assert!(drift.is_empty(), "{drift:?}");
    let gd = &x.game_data;
    let prowler = gd.ship("ESPR_Prowler").unwrap().flight.clone().unwrap();
    assert_eq!(
        (prowler.pitch, prowler.roll, prowler.yaw),
        (Some(38.0), Some(110.0), Some(32.0))
    );
    let behr = gd.weapon("BEHR_BallisticGatling_S5").unwrap();
    assert_eq!(behr.alpha, Some(102.5));
    assert_eq!(behr.fire_rate, Some(900.0));
    assert_eq!(behr.dps, Some(1537.5));
    assert_eq!(behr.range, Some(4003.0));
    let gladius = gd.ship("AEGS_Gladius").unwrap();
    assert!(!gladius.loadout.as_ref().unwrap().weapons.is_empty());
    assert!(gd.ship("ANVL_Carrack").unwrap().loadout.is_some());
    // Bengal S6 reuses the APAR S4 keys: the namesake test case of spec 4.5.
    let bengal = gd.weapon("Bengal_BallisticGatling_S6").unwrap();
    assert_eq!(
        bengal.item.loc.name.as_deref(),
        Some("@item_NameAPAR_BallisticGatling_S4")
    );
}

#[test]
#[ignore = "needs the real HOTFIX Data.p4k"]
fn real_hotfix_extracts() {
    let Some(x) = real_extract("HOTFIX") else {
        return;
    };
    for w in &x.warnings {
        eprintln!("HOTFIX warning: {}: {}", w.0, w.1);
    }
    assert!(x.game_data.counts().ships > 0);
}

#[test]
#[ignore = "needs the real PTU Data.p4k"]
fn real_ptu_extracts() {
    let Some(x) = real_extract("PTU") else { return };
    for w in &x.warnings {
        eprintln!("PTU warning: {}: {}", w.0, w.1);
    }
    assert!(x.game_data.counts().ships > 0);
}

/// Addendum 7.15: the real LIVE weapons table grouped by type > subType > size in the
/// Datasets tree. Counts are checked against a plain count of the rows, children sum to their
/// parent, and picking a node filters the rows to its count. Read only.
#[test]
#[ignore = "needs the real LIVE Data.p4k"]
fn real_live_weapons_group_by_type_subtype_size() {
    use starorg_core::dev::dataset::{RowsQuery, infer, query_rows};
    use starorg_core::dev::group::{GroupPick, GroupQuery, group_rows, presets, row_path};
    use starorg_core::dev::readable::{Lookup, MapLookup};
    use std::collections::BTreeMap;
    let Some(x) = real_extract("LIVE") else {
        return;
    };
    let v = serde_json::to_value(&x.game_data).unwrap();
    let d = infer("gamedata.json", 0, v);
    // Labels as the app reads them (addendum 7.14).
    let lookup = MapLookup::from_gamedata(&x.game_data);
    let l: Option<&dyn Lookup> = Some(&lookup);
    let t = d.table("weapons").expect("weapons table");
    let chain = presets(t).into_iter().next().expect("a default preset");
    assert_eq!(chain.label, "type › subType › size");
    let by = chain.levels;
    // Plain counts by key path prefix.
    let mut want: BTreeMap<Vec<Option<String>>, u32> = BTreeMap::new();
    for r in &t.rows {
        let p = row_path(r, &by);
        for k in 1..=p.len() {
            *want.entry(p[..k].to_vec()).or_default() += 1;
        }
    }
    // Open every node: the whole tree in one reply.
    let open: Vec<String> = want
        .keys()
        .map(|k| serde_json::to_string(k).unwrap())
        .collect();
    let q = GroupQuery {
        by: by.clone(),
        open,
        ..Default::default()
    };
    let page = group_rows(t, &q, l);
    assert_eq!(
        (page.total, page.all),
        (t.rows.len() as u32, t.rows.len() as u32)
    );
    assert_eq!(page.nodes.len(), want.len(), "one node per key path");
    let mut by_id = BTreeMap::new();
    for n in &page.nodes {
        let k: Vec<Option<String>> = serde_json::from_str(&n.id).unwrap();
        assert_eq!(Some(&n.count), want.get(&k), "{}", n.id);
        by_id.insert(n.id.clone(), n.count);
    }
    for n in &page.nodes {
        if n.children > 0 {
            let kids: u32 = page
                .nodes
                .iter()
                .filter(|c| c.parent.as_deref() == Some(n.id.as_str()))
                .map(|c| c.count)
                .sum();
            assert_eq!(kids, n.count, "children of {}", n.id);
        }
        let rows = query_rows(
            t,
            &RowsQuery {
                limit: 1,
                group: Some(GroupPick {
                    by: by.clone(),
                    node: n.id.clone(),
                }),
                ..Default::default()
            },
            1,
            l,
        );
        assert_eq!(rows.total, n.count, "rows of {}", n.id);
    }
    let top: u32 = page
        .nodes
        .iter()
        .filter(|n| n.depth == 1)
        .map(|n| n.count)
        .sum();
    assert_eq!(top, t.rows.len() as u32);
    for n in page.nodes.iter().filter(|n| n.depth <= 2) {
        eprintln!(
            "{}{} {}",
            "  ".repeat(n.depth as usize - 1),
            n.label,
            n.count
        );
    }
    eprintln!("{} weapons, {} nodes", t.rows.len(), page.nodes.len());
}

#[test]
fn quest_items_from_spawns_and_hauling_orders() {
    use starorg_core::model::QuestRef;
    let (gd, _) = fixture_gamedata();
    let names: Vec<&str> = gd.quest_items.iter().map(|q| q.short_name()).collect();
    assert_eq!(names, ["TEST_Datapad_A", "TEST_QuestChip"]);
    let chip = &gd.quest_items[1];
    assert_eq!(chip.item_type.as_deref(), Some("RemovableChip"));
    assert_eq!(chip.sub_type, None, "UNDEFINED is left out");
    assert_eq!(chip.loc.desc.as_deref(), Some("@item_DescTEST_QuestChip"));
    assert_eq!(chip.desc_shared_with, ["TEST_QuestChip_debug"]);
    let uses: Vec<_> = chip
        .uses
        .iter()
        .map(|u| {
            (
                u.kind,
                u.source.as_str(),
                u.mission.as_str(),
                u.title_key.as_deref(),
                u.systems.clone(),
            )
        })
        .collect();
    assert_eq!(
        uses,
        [
            (
                QuestRef::Spawned,
                "MissionBrokerEntry",
                "TEST_Mission",
                Some("@TEST_mission_title"),
                vec!["Stanton".to_string()]
            ),
            (
                QuestRef::Hauled,
                "TEST_HaulGenerator",
                "TEST_Contract_Haul",
                Some("@TEST_haul_title"),
                vec!["Pyro".to_string()]
            ),
        ]
    );
    assert_eq!(gd.quest_items[0].desc_shared_with, ["TEST_Datapad_B"]);
    // Contracts of the hauling generator are contracts like any other.
    assert!(
        gd.contracts
            .iter()
            .any(|c| c.debug_name == "TEST_Contract_Haul")
    );
}

#[test]
fn name_keys_of_entities() {
    let (gd, _) = fixture_gamedata();
    let n = gd
        .item_names
        .iter()
        .find(|n| n.class == "test_repeater_s3_black")
        .expect("paint variant listed");
    assert_eq!(n.name, "@item_NameTEST_Repeater_S3");
    assert_eq!(n.desc.as_deref(), Some("@item_DescTEST_Repeater_S3"));
    assert_eq!(n.kind, "WeaponGun");
    // Sorted by class, lower case, real `@` references only.
    assert!(gd.item_names.windows(2).all(|w| w[0].class <= w[1].class));
    assert!(
        gd.item_names
            .iter()
            .all(|n| n.class == n.class.to_ascii_lowercase() && n.name.starts_with('@'))
    );
}

#[test]
fn description_keys_of_entities_and_resources() {
    let (gd, warnings) = fixture_gamedata();
    // Every entity with a real AttachDef description, by lower-case class.
    assert_eq!(
        gd.item_descs.get("test_repeater_s3").map(String::as_str),
        Some("@item_DescTEST_Repeater_S3")
    );
    // A paint variant keeps its own (shared) reference; the namesake rule decides later.
    assert_eq!(
        gd.item_descs
            .get("test_repeater_s3_black")
            .map(String::as_str),
        Some("@item_DescTEST_Repeater_S3")
    );
    assert!(gd.item_descs.keys().all(|k| *k == k.to_ascii_lowercase()));
    assert!(gd.item_descs.values().all(|v| v.starts_with('@')));
    // Resources: nested groups walked, nulls and placeholders skipped.
    assert_eq!(
        gd.resource_descs.get("test_gem").map(String::as_str),
        Some("@items_commodities_test_gem_desc")
    );
    assert_eq!(
        gd.resource_descs.get("raw_test_ore").map(String::as_str),
        Some("@items_commodities_test_ore_raw_desc")
    );
    assert!(!gd.resource_descs.contains_key("test_blank"));
    assert!(
        !warnings
            .iter()
            .any(|w| w.detail.contains("description keys")),
        "{warnings:?}"
    );
}

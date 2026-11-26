//! Golden and targeted tests for the enhancement generators (spec 5, 6, 13).
//!
//! Fixtures are synthetic (`tests/fixtures/enhance/`, made by `make_fixtures.py`): made-up
//! records and filler prose, no CIG text. The expected strings below are written from the
//! spec's format table and examples, not copied from output.
//!
//! Regenerate the golden file with `UPDATE_GOLDEN=1 cargo test -p starorg-core --test enhance`.

use starorg_core::enhance::check::check_file_with;
use starorg_core::enhance::owned::{NoneOwned, OwnedList};
use starorg_core::enhance::profile::Profile;
use starorg_core::enhance::{Category, Generated, Reason, generate, render};
use starorg_core::ini::StockIni;
use starorg_core::model::GameData;
use std::path::Path;

fn fixture() -> (GameData, StockIni) {
    let d = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/enhance");
    let gd: GameData =
        serde_json::from_str(&std::fs::read_to_string(d.join("gamedata.json")).unwrap()).unwrap();
    let stock = StockIni::parse(std::fs::read(d.join("strings.ini")).unwrap());
    (gd, stock)
}

fn owned() -> OwnedList {
    OwnedList::from_names(["[P-S3] Filler rifle"])
}

fn run(p: &Profile) -> (Generated, StockIni) {
    let (gd, stock) = fixture();
    let g = generate(&gd, &stock, p, &owned());
    (g, stock)
}

/// Final value of `key` after rendering.
fn value(g: &Generated, stock: &StockIni, key: &str) -> Option<String> {
    let line = stock.find(key).ok()?;
    let e = g.edits.get(&line)?;
    Some(e.value(stock.value_at(line).unwrap()))
}

fn added(g: &Generated, stock: &StockIni, key: &str) -> String {
    let line = stock.find(key).unwrap();
    g.edits
        .get(&line)
        .map(|e| e.generated_part())
        .unwrap_or_default()
}

fn block(lines: &[&str]) -> String {
    format!("\\n\\n--- STATS ---\\n{}", lines.join("\\n"))
}

#[test]
fn golden_snapshot() {
    let (g, stock) = run(&Profile::default());
    let mut text = String::new();
    for e in g.edits.values() {
        text.push_str(&format!(
            "[{}] {} <- {}\n{}={}\n\n",
            e.category.name(),
            e.key,
            e.record.as_deref().unwrap_or("-"),
            e.key,
            e.value(stock.value_at(e.line).unwrap())
        ));
    }
    for a in g.added.values() {
        text.push_str(&format!(
            "[{}] {} <- {} (added, {})\n{}={}\n\n",
            a.category.name(),
            a.key,
            a.record.as_deref().unwrap_or("-"),
            a.rule,
            a.key,
            a.value
        ));
    }
    text.push_str(&serde_json::to_string_pretty(&g.coverage).unwrap());
    text.push('\n');
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/enhance/golden.txt");
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &text).unwrap();
    }
    let want = std::fs::read_to_string(&path).expect("golden file; run with UPDATE_GOLDEN=1");
    assert_eq!(
        text, want,
        "golden output changed; review and UPDATE_GOLDEN=1"
    );
}

#[test]
fn component_blocks_follow_the_spec_examples() {
    let (g, stock) = run(&Profile::default());
    assert_eq!(
        added(&g, &stock, "item_DescSHLD_ZETA_S01_Wall"),
        block(&[
            "Shield HP: 7,480  |  Regen: 1,346 HP/s",
            "Delay: 8.5s when down  |  4.2s when damaged",
            "Resist: Phys 0–25%  |  Energy 0–10%  |  Dist 0–90%",
            "Power draw: 2 PU/s",
            "Signatures: EM 500  |  IR 0",
            "Component HP: 300  |  Max distortion: 2,450",
        ])
    );
    assert_eq!(
        added(&g, &stock, "item_DescQDRV_ZETA_S01_Hop"),
        block(&[
            "Speed: 301 Mm/s  |  Spool: 8s  |  Cooldown: 16.5s",
            "Fuel: 0.0054 per Gm",
            "Accel: stage 1 6,936,600  |  stage 2 10,494,000",
            "Calibration: 1,000/s  |  Needs 5,000–10,000",
            "Power draw: 2 PU/s",
            "Signatures: EM 15,000  |  IR 0",
            "Component HP: 170  |  Max distortion: 2,600",
        ])
    );
    // Zero regen and zero damaged delay are left out; a draw below 1 keeps a decimal.
    assert_eq!(
        added(&g, &stock, "item_DescSHLD_ZETA_S02_Dome"),
        block(&[
            "Shield HP: 12,000",
            "Delay: 10s when down",
            "Power draw: 0.4 PU/s"
        ])
    );
    assert!(added(&g, &stock, "item_DescPOWR_ZETA_S01_Glow").contains("Output: 20 PU/s"));
    assert!(added(&g, &stock, "item_DescCOOL_ZETA_S01_Chill").contains("Cooling: 46 CR/s"));
    // Radar: the detection line is left out (labels not confirmed).
    let radar = added(&g, &stock, "item_DescRADR_ZETA_S01_Ear");
    assert!(radar.contains("Ping cooldown: 2.5s") && !radar.contains("Detects"));
    assert_eq!(
        added(&g, &stock, "item_DescMRCK_ZETA_S03_Quad"),
        block(&["Slots: 4× S3"])
    );
    // Common-only category with two lines; one with a single line gets nothing.
    assert_eq!(
        added(&g, &stock, "item_DescLFSP_ZETA_S01_Air"),
        block(&["Power draw: 1 PU/s", "Component HP: 100"])
    );
    assert_eq!(added(&g, &stock, "item_DescTHRS_ZETA_Main"), "");
    // Trailing `\n` and spaces of the stock prose are trimmed before the block.
    let v = value(&g, &stock, "item_DescSHLD_ZETA_S01_Wall").unwrap();
    assert!(v.contains("made-up part.\\n\\n--- STATS ---"), "{v}");
}

#[test]
fn weapon_blocks_and_reference_values() {
    let (g, stock) = run(&Profile::default());
    // Spec 13: 102.5 × 900 RPM = 1,537.5 DPS; 1,251 m/s × 3.2 s = 4,003 m.
    assert_eq!(
        added(&g, &stock, "item_DescZETA_BallisticGatling_S5"),
        block(&[
            "Damage: 102.5 Phys  |  DPS: 1,537.5",
            "Fire rate: 900 RPM  |  Mode: Auto",
            "Velocity: 1,251 m/s  |  Range: 4 km",
            "Ammo: 5,004",
            "Power draw: 0.1 PU/s",
            "Signatures: EM 36  |  IR 0",
            "Component HP: 1,650",
        ])
    );
    // Spec 5.4 example: capacitor weapon, sequence of single shots labelled Semi.
    assert_eq!(
        added(&g, &stock, "item_DescZETA_LaserCannon_S2"),
        block(&[
            "Damage: 225.4 Energy  |  DPS: 375.7",
            "Fire rate: 100 RPM  |  Mode: Semi",
            "Velocity: 1,184 m/s  |  Range: 3 km",
            "Capacitor: 90 per shot  |  Regen: 1,125/s after 1.4s",
            "Power draw: 2 PU/s",
            "Signatures: EM 41  |  IR 0",
            "Component HP: 550",
        ])
    );
    assert!(
        added(&g, &stock, "item_DescZETA_Scatter_S3")
            .contains("Damage: 49.5 ×8 Energy  |  DPS: 264")
    );
    assert!(
        added(&g, &stock, "item_DescZETA_Mixed_S1")
            .contains("Damage: 120 Phys + 30 Energy  |  DPS: 1,500")
    );
    // Tools: no damage, fire rate, velocity or ammo lines.
    let tractor = added(&g, &stock, "item_DescZETA_TractorBeam_S1");
    assert_eq!(
        tractor,
        block(&[
            "Power draw: 3 PU/s",
            "Signatures: EM 200  |  IR 0",
            "Component HP: 500"
        ])
    );
    let mining = added(&g, &stock, "item_DescZETA_MiningLaser_S1");
    assert!(
        mining
            .contains("Modifiers: Instability +35%  |  Resistance +25%  |  Optimal window +40%\\n")
    );
    assert!(!mining.contains("DPS") && !mining.contains("Velocity"));
}

#[test]
fn fps_weapon_blocks() {
    let (g, stock) = run(&Profile::default());
    assert_eq!(
        added(&g, &stock, "item_Desczeta_pistol_01"),
        block(&[
            "Damage: 60 Phys  |  DPS: 180",
            "Fire rate: 180 RPM  |  Mode: Semi",
            "Velocity: 550 m/s  |  Range: 1.1 km",
            "Full damage to 55 m  |  then -0.20 per m  |  Min: 35",
            "Magazine: 6  |  Weight: 1.3 kg",
        ])
    );
    // Spec 13: a 300/900 RPM rifle headlines at 900 RPM, never Semi.
    let rifle = added(&g, &stock, "item_Desczeta_rifle_01");
    assert!(
        rifle.contains("Fire rate: 900 RPM  |  Mode: Auto / Semi / Burst 3"),
        "{rifle}"
    );
    // No falloff line when the per-metre drop is zero.
    assert!(!added(&g, &stock, "item_Desczeta_shotgun_01").contains("Full damage"));
    assert!(
        added(&g, &stock, "item_Desczeta_smg_energy_01")
            .contains("DPS: 275 Energy\\nMode: Beam\\nRange: 25 m")
    );
    // FPS tools get no block.
    assert_eq!(added(&g, &stock, "item_Desczeta_medgun_01"), "");
}

#[test]
fn missile_blocks() {
    let (g, stock) = run(&Profile::default());
    assert_eq!(
        added(&g, &stock, "item_DescMISL_S02_IR_ZETA_Spark"),
        block(&[
            "Damage: 1,700 Phys  |  Blast radius: 3 m",
            "Tracking: Infrared  |  Lock: 1.5s",
            "Lock range: 1.8–10 km",
            "Speed: 1,250 m/s  |  Lifetime: 30s",
            "Component HP: 25",
        ])
    );
    assert!(added(&g, &stock, "item_DescMISL_S03_EM_ZETA_Bolt").starts_with(
        "\\n\\n--- STATS ---\\nDamage: 27,000 (12,500 Phys + 14,500 Energy)  |  Blast radius: 10 m\\nTracking: Electromagnetic  |  Lock: 2s\\nLock range: 50 m–15 km"
    ));
    let bomb = added(&g, &stock, "item_DescBOMB_S03_ZETA_Drop");
    assert!(
        bomb.contains("Damage: 27,000 Phys  |  Blast radius: 40 m")
            && !bomb.contains("Tracking")
            && !bomb.contains("Lock")
    );
}

#[test]
fn ship_blocks() {
    let (g, stock) = run(&Profile::default());
    assert_eq!(
        added(&g, &stock, "vehicle_DescZETA_Gladiator"),
        block(&[
            "Role: Light Fighter  |  Career: Combat  |  Crew: 1",
            "Size: 21 × 16.5 × 5.5 m",
            "SCM: 226 m/s  |  Max: 1,193 m/s",
            "Boost: 520 m/s fwd  |  268 m/s back",
            "Pitch: 71°/s  |  Yaw: 63°/s  |  Roll: 195°/s",
            "Guns: 2× S3, 1× S2  |  Missiles: 4× S3",
            "Shields: 2× S1  |  Power: 1× S1  |  Coolers: 2× S1  |  QD: 1× S1",
            "Armor HP: 3,300",
            "Damage taken: Phys 0.75x  |  Energy 0.60x  |  Dist 1.00x  |  Therm 1.00x",
            "Cargo: 12 SCU  |  H2 fuel: 28 SCU  |  QT fuel: 1.2 SCU",
        ])
    );
    // Spec 13 reference: the Prowler controller gives pitch 38, roll 110, yaw 32.
    let prowl = added(&g, &stock, "vehicle_DescZETA_Prowl");
    assert!(prowl.contains("Pitch: 38°/s  |  Yaw: 32°/s  |  Roll: 110°/s"));
    // Tractor beams are not guns.
    assert!(prowl.contains("Guns: 2× S5\\n"), "{prowl}");
    let hauler = added(&g, &stock, "vehicle_DescZETA_Hauler");
    assert!(
        hauler.contains("Guns: 8× S4\\nShields: 2× S3\\n") && hauler.contains("Cargo: 456 SCU")
    );
    // Unresolvable role is left out; size-0 items keep their size.
    let buggy = added(&g, &stock, "vehicle_DescZETA_Buggy");
    assert!(buggy.contains("Career: Combat  |  Crew: 1") && !buggy.contains("Role:"));
    assert!(buggy.contains("Shields: 1× S0"));
    assert_eq!(added(&g, &stock, "vehicle_DescZETA_Bare"), "");
    // The AI copy that shares the key is skipped: the player hull's numbers are written.
    assert!(added(&g, &stock, "vehicle_DescZETA_Scout").contains("SCM: 250 m/s"));
}

#[test]
fn name_tags() {
    let (g, stock) = run(&Profile::default());
    let v = |k| value(&g, &stock, k);
    assert_eq!(
        v("item_NameSHLD_ZETA_S01_Wall").as_deref(),
        Some("[MIL-S1-B] Filler Wall")
    );
    assert_eq!(
        v("item_NameSHLD_ZETA_S02_Dome").as_deref(),
        Some("[STH-S2-C] Filler Dome")
    );
    assert_eq!(
        v("item_NameRADR_ZETA_S01_Ear").as_deref(),
        Some("[CMP-S1-D] Filler Ear")
    );
    // No class line in the description: the element is dropped.
    assert_eq!(
        v("item_NameCOOL_ZETA_S01_Chill").as_deref(),
        Some("[S1-A] Filler Chill")
    );
    assert_eq!(
        v("item_NameMRCK_ZETA_S03_Quad").as_deref(),
        Some("[S3] Filler Quad")
    );
    assert_eq!(v("item_NameLFSP_ZETA_S01_Air"), None);
    assert_eq!(
        v("item_NameZETA_BallisticGatling_S5").as_deref(),
        Some("[P-S5] Filler BallisticGatling")
    );
    assert_eq!(
        v("item_NameZETA_LaserCannon_S2").as_deref(),
        Some("[E-S2] Filler LaserCannon")
    );
    assert_eq!(
        v("item_NameZETA_RocketPod_S1").as_deref(),
        Some("[S1] Filler RocketPod")
    );
    assert_eq!(v("item_NameZETA_TractorBeam_S1"), None);
    assert_eq!(
        v("item_NameMISL_S02_IR_ZETA_Spark").as_deref(),
        Some("[IR-S2] Filler Spark")
    );
    assert_eq!(
        v("item_NameMISL_S01_CS_ZETA_Dart").as_deref(),
        Some("[CS-S1] Filler Dart")
    );
    assert_eq!(
        v("item_NameBOMB_S03_ZETA_Drop").as_deref(),
        Some("[B-S3] Filler Drop")
    );
    // FPS weapons and ships get no tag in v1.
    assert_eq!(v("item_Namezeta_pistol_01"), None);
    assert_eq!(v("vehicle_NameZETA_Gladiator"), None);

    // Tool tags and suffix placement are profile switches.
    let mut p = Profile::default();
    p.tags.tool_size_tag = true;
    p.tags.missiles.placement = "suffix".into();
    let (g, stock) = run(&p);
    assert_eq!(
        value(&g, &stock, "item_NameZETA_TractorBeam_S1").as_deref(),
        Some("[S1] Filler TractorBeam")
    );
    assert_eq!(
        value(&g, &stock, "item_NameMISL_S02_IR_ZETA_Spark").as_deref(),
        Some("Filler Spark [IR-S2]")
    );
}

/// Spec 4.5/13: the Bengal S6 gatling reuses the APAR S4 keys; with no patch the namesake
/// rule gives both keys to APAR, so the APAR text shows APAR stats.
#[test]
fn bengal_and_apar_under_the_namesake_rule() {
    let (g, stock) = run(&Profile::default());
    let desc = added(&g, &stock, "item_DescAPAR_BallisticGatling_S4");
    assert!(
        desc.contains("DPS: 1,266") && !desc.contains("3,333"),
        "{desc}"
    );
    assert_eq!(
        value(&g, &stock, "item_NameAPAR_BallisticGatling_S4").as_deref(),
        Some("[P-S4] Filler BallisticGatling")
    );
    assert!(
        g.edits
            .values()
            .all(|e| e.record.as_deref() != Some("Bengal_BallisticGatling_S6"))
    );
}

#[test]
fn shared_keys_same_text_and_contested() {
    let (g, stock) = run(&Profile::default());
    // Two paint variants with equal stats: written once.
    assert!(added(&g, &stock, "item_DescCOOL_ZETA_S02_Frost").contains("Cooling: 60 CR/s"));
    // Two records with different stats and no namesake: left as stock, reported.
    assert_eq!(added(&g, &stock, "item_DescPOWR_ZETA_Twin"), "");
    let c = g.coverage.get(Category::Components);
    assert_eq!(c.left_for(Reason::ContendersDiffer), 1);
    assert!(
        g.coverage
            .contested
            .iter()
            .any(|x| x.key == "item_DescPOWR_ZETA_Twin"
                && x.records == ["POWR_ZETA_S02_TwinA", "POWR_ZETA_S02_TwinB"])
    );
    // A missile whose name key is not in stock gets no entry (its name is unresolved, so
    // it counts as a placeholder).
    assert_eq!(
        g.coverage
            .get(Category::MissileTags)
            .left_for(Reason::Placeholder),
        1
    );
    assert!(
        g.edits
            .values()
            .all(|e| e.record.as_deref() != Some("MISL_S01_IR_ZETA_Ghost"))
    );
}

#[test]
fn mission_titles() {
    let (g, stock) = run(&Profile::default());
    let v = |k| value(&g, &stock, k);
    // Trailing `\n` and spaces trimmed before the tags.
    assert_eq!(
        v("zeta_haul_title").as_deref(),
        Some("Filler Haul <EM4>[BP]</EM4> <EM4>[150 Rep]</EM4>")
    );
    assert_eq!(
        v("zeta_bounty_title").as_deref(),
        Some("Filler Bounty <EM4>[BP?]</EM4> <EM4>[100–300 Rep]</EM4>")
    );
    assert_eq!(
        v("zeta_patrol_title").as_deref(),
        Some("Filler Patrol <EM4>[8,000 Rep]</EM4>")
    );
    // Format keys and work-in-progress contracts are skipped.
    assert_eq!(v("zeta_format_title"), None);
    assert_eq!(v("zeta_wip_title"), None);
    assert_eq!(
        g.coverage
            .get(Category::MissionTitles)
            .left_for(Reason::FormatKey),
        1
    );
    // Plain tags when the EM option is off.
    let mut p = Profile::default();
    p.mission.title_em = false;
    p.mission.title_reputation = false;
    let (g, stock) = run(&p);
    assert_eq!(
        value(&g, &stock, "zeta_haul_title").as_deref(),
        Some("Filler Haul [BP]")
    );
    assert_eq!(value(&g, &stock, "zeta_patrol_title"), None);
}

#[test]
fn mission_descriptions() {
    let (g, stock) = run(&Profile::default());
    assert_eq!(
        added(&g, &stock, "zeta_haul_desc"),
        [
            "\\n\\n<EM3>BLUEPRINTS</EM3>",
            "- Filler pistol",
            "- Filler rifle <EM4>[Owned]</EM4>",
            "- [MIL-S1-B] Filler Wall",
            "",
            "<EM3>ITEM REWARDS</EM3>",
            "- Filler Scrip ×3",
            "",
            "<EM3>MISSION DETAILS</EM3>",
            "<EM4>Blueprint reward:</EM4> Guaranteed",
            "<EM4>Reputation:</EM4> 150",
            "<EM4>Failure:</EM4> -1,000",
            "<EM4>Requires:</EM4> Filler Junior",
            "<EM4>Type:</EM4> Chain, Once only",
        ]
        .join("\\n")
    );
    // Variants: one sub-header per pool, labelled by difficulty; part of the group has no
    // pool; the chance never puts a letter after `%`.
    let bounty = added(&g, &stock, "zeta_bounty_desc");
    assert!(
        bounty.starts_with("\\n\\n<EM3>BLUEPRINTS</EM3>\\nNot every offer with this title awards one\\n<EM4>[Easy]</EM4>\\n- Filler pistol\\n"),
        "{bounty}"
    );
    assert!(
        bounty.contains("<EM4>[Hard]</EM4>\\n- Filler lmg\\n- [E-S3] Filler Scatter\\n"),
        "{bounty}"
    );
    assert!(bounty.contains("<EM4>Blueprint reward:</EM4> Some variants only\\n<EM4>Reputation:</EM4> 100–300\\n<EM4>Requires:</EM4> Filler Senior"));
    // A key used as a title never gets a body; LOC_EMPTY is skipped.
    assert!(!added(&g, &stock, "zeta_patrol_title").contains("MISSION DETAILS"));
    assert_eq!(
        added(&g, &stock, "zeta_patrol_desc"),
        "\\n\\n<EM3>MISSION DETAILS</EM3>\\n<EM4>Reputation:</EM4> 8,000"
    );
    // Open tail: still appended, and counted.
    assert!(!added(&g, &stock, "zeta_open_desc").is_empty());
    assert_eq!(g.coverage.get(Category::MissionDescriptions).open_tail, 1);
    // CIG's stray close stays; our text is appended.
    assert!(
        value(&g, &stock, "zeta_stray_desc")
            .unwrap()
            .starts_with("Filler prose with a stray </EM4> close\\n\\n<EM3>MISSION DETAILS")
    );
    // No owned set: no markers.
    let (gd, stock) = fixture();
    let g = generate(&gd, &stock, &Profile::default(), &NoneOwned);
    assert!(!added(&g, &stock, "zeta_haul_desc").contains("[Owned]"));
}

#[test]
fn chances_print_percent_at_line_end() {
    let (mut gd, stock) = fixture();
    for c in gd.contracts.iter_mut() {
        if c.debug_name == "Zeta_Bounty_Free" {
            c.blueprint_rewards = vec![starorg_core::model::BlueprintReward {
                pool: "BP_POOL_B".into(),
                chance: Some(0.5),
            }];
        }
    }
    let g = generate(&gd, &stock, &Profile::default(), &NoneOwned);
    let b = added(&g, &stock, "zeta_bounty_desc");
    // The variants differ and the difficulty tells them apart: one line each, so every `%`
    // still ends its line.
    assert!(
        b.contains("<EM4>Blueprint reward (Easy):</EM4> 25%\\n"),
        "{b}"
    );
    assert!(
        b.contains("<EM4>Blueprint reward (Hard):</EM4> 50%\\n"),
        "{b}"
    );
    // The note under BLUEPRINTS says the same, with the `%` at the end of its line.
    assert!(
        b.starts_with("\\n\\n<EM3>BLUEPRINTS</EM3>\\nAwarded by chance: 25–50%\\n"),
        "{b}"
    );
}

#[test]
fn render_keeps_the_file_shape() {
    let (g, stock) = run(&Profile::everything());
    let bytes = render(&stock, &g).unwrap();
    assert_eq!(check_file_with(&stock, &bytes, g.added.len()), Ok(()));
    let text = std::str::from_utf8(&bytes).unwrap();
    assert!(text.starts_with('\u{feff}'));
    assert!(text.contains("\r\nui_unrelated,P=Unrelated filler\r\n"));
    // Missing names: new keys after the last stock line, stock lines untouched.
    assert!(
        text.ends_with(
            "\r\nitem_Name_zeta_suit_01_01_02=Filler Suit\r\nitem_NameZetaCrate_S4=Zeta Crate S4\r\n"
        ),
        "{}",
        &text[text.len() - 200..]
    );
    // Branding is on in Everything.
    assert!(
        text.contains("Frontend_PU_Version=Filler version\\nText enhanced with StarOrg Codex v")
    );
    assert_eq!(g.warnings, vec![]);
}

#[test]
fn profile_switches() {
    // Nothing generated: byte-identical.
    let (g, stock) = run(&Profile::off());
    assert!(g.edits.is_empty());
    assert_eq!(render(&stock, &g).unwrap(), stock.as_bytes());
    // Minimal: name tags only.
    let (g, _) = run(&Profile::minimal());
    assert!(g.edits.values().all(|e| matches!(
        e.category,
        Category::ComponentTags | Category::ShipWeaponTags | Category::MissileTags
    )));
    assert!(g.count(Category::ComponentTags) > 0);
    // Stats above prose, a line switched off, another header.
    let p = Profile {
        stats_above: true,
        header: "STATS".into(),
        lines_off: ["components.signatures".to_string()].into(),
        ..Profile::default()
    };
    let (g, stock) = run(&p);
    let v = value(&g, &stock, "item_DescPOWR_ZETA_S01_Glow").unwrap();
    assert!(
        v.starts_with(
            "STATS\\nOutput: 20 PU/s\\nComponent HP: 140\\n\\n------\\n\\nItem Type: Filler"
        ),
        "{v}"
    );
}

#[test]
fn coverage_thresholds_and_baseline() {
    let (g, _) = run(&Profile::default());
    let base = g.coverage.baseline();
    assert!(base.contested.is_empty());
    assert!(g.coverage.baseline_failures(&base).is_empty());
    let mut low = base.clone();
    low.categories.get_mut(&Category::Ships).unwrap().written += 10;
    assert_eq!(g.coverage.baseline_failures(&low).len(), 1);
    // The fixture contains a contested key on purpose: components miss the 5% ceiling.
    let fails = g.coverage.threshold_failures();
    assert!(
        fails
            .iter()
            .any(|f| f.starts_with("components: contenders-differ")),
        "{fails:?}"
    );
}

#[test]
fn colour_settings_on_markers_titles_and_lists() {
    let (gd, stock) = fixture();
    let mut p = Profile::default();
    p.mission.owned_em = 4;
    p.mission.missing_marker = true;
    p.mission.missing_em = 2;
    p.mission.variant_em = 5;
    p.mission.reward_em = 1;
    p.mission.header_em = 0;
    p.mission.label_em = 3;
    p.mission.title_rep_em = 0;
    let g = generate(&gd, &stock, &p, &owned());
    let haul = added(&g, &stock, "zeta_haul_desc");
    assert!(
        haul.starts_with(
            "\\n\\nBLUEPRINTS\\n- Filler pistol <EM2>[Missing]</EM2>\\n- Filler rifle <EM4>[Owned]</EM4>\\n- [MIL-S1-B] Filler Wall <EM2>[Missing]</EM2>\\n\\nITEM REWARDS\\n- <EM1>Filler Scrip</EM1> ×3\\n\\nMISSION DETAILS\\n<EM3>Blueprint reward:</EM3> Guaranteed"
        ),
        "{haul}"
    );
    let bounty = added(&g, &stock, "zeta_bounty_desc");
    assert!(bounty.contains("\\n<EM5>[Easy]</EM5>\\n"), "{bounty}");
    assert_eq!(
        value(&g, &stock, "zeta_haul_title").as_deref(),
        Some("Filler Haul <EM4>[BP]</EM4> [150 Rep]")
    );
    // Whole owned line in the marker's colour, the marker not wrapped twice.
    p.mission.owned_line = true;
    let g = generate(&gd, &stock, &p, &owned());
    assert!(
        added(&g, &stock, "zeta_haul_desc").contains("\\n- <EM4>Filler rifle [Owned]</EM4>\\n"),
        "{}",
        added(&g, &stock, "zeta_haul_desc")
    );
    // No owned set: no [Missing] on every line.
    let g = generate(&gd, &stock, &p, &NoneOwned);
    let haul = added(&g, &stock, "zeta_haul_desc");
    assert!(
        !haul.contains("[Missing]") && !haul.contains("[Owned]"),
        "{haul}"
    );
    // `[Owned]` off: neither marker colour shows.
    p.mission.owned_marker = false;
    let g = generate(&gd, &stock, &p, &owned());
    assert!(!added(&g, &stock, "zeta_haul_desc").contains("[Owned]"));
}

#[test]
fn bp_owned_title_tag() {
    let (gd, stock) = fixture();
    let all = OwnedList::from_names(["Filler pistol", "Filler rifle", "Filler Wall"]);
    // Off by default: owning everything changes nothing in the title.
    let g = generate(&gd, &stock, &Profile::default(), &all);
    assert_eq!(
        value(&g, &stock, "zeta_haul_title").as_deref(),
        Some("Filler Haul <EM4>[BP]</EM4> <EM4>[150 Rep]</EM4>")
    );
    let mut p = Profile::default();
    p.mission.title_owned = true;
    let g = generate(&gd, &stock, &p, &all);
    assert_eq!(
        value(&g, &stock, "zeta_haul_title").as_deref(),
        Some("Filler Haul <EM4>[BP Owned]</EM4> <EM4>[150 Rep]</EM4>")
    );
    // Something still to get: the usual tag.
    let g = generate(&gd, &stock, &p, &owned());
    assert_eq!(
        value(&g, &stock, "zeta_haul_title").as_deref(),
        Some("Filler Haul <EM4>[BP]</EM4> <EM4>[150 Rep]</EM4>")
    );
    let g = generate(&gd, &stock, &p, &NoneOwned);
    assert!(
        value(&g, &stock, "zeta_bounty_title")
            .unwrap()
            .contains("[BP?]")
    );
    // Its colour is the blueprint tag's; plain when title colours are off.
    p.mission.title_bp_em = 2;
    let g = generate(&gd, &stock, &p, &all);
    assert!(
        value(&g, &stock, "zeta_haul_title")
            .unwrap()
            .contains(" <EM2>[BP Owned]</EM2> ")
    );
    p.mission.title_em = false;
    let g = generate(&gd, &stock, &p, &all);
    assert_eq!(
        value(&g, &stock, "zeta_haul_title").as_deref(),
        Some("Filler Haul [BP Owned] [150 Rep]")
    );
}

#[test]
fn colour_test_line() {
    let (g, stock) = run(&Profile::default());
    assert_eq!(value(&g, &stock, "mG_ContractsMgr_Select"), None);
    assert_eq!(g.count(Category::ColourTest), 0);
    let p = Profile {
        colour_test: true,
        ..Profile::default()
    };
    let (g, stock) = run(&p);
    assert_eq!(
        value(&g, &stock, "mG_ContractsMgr_Select").as_deref(),
        Some(
            "Filler select prompt. Colour test: <EM1>EM1</EM1> <EM2>EM2</EM2> <EM3>EM3</EM3> <EM4>EM4</EM4> <EM5>EM5</EM5>"
        )
    );
    assert_eq!(g.count(Category::ColourTest), 1);
    // One string only, and the rest of the output is as without it.
    let (plain, _) = run(&Profile::default());
    assert_eq!(g.edits.len(), plain.edits.len() + 1);
    let bytes = render(&stock, &g).unwrap();
    assert_eq!(check_file_with(&stock, &bytes, g.added.len()), Ok(()));
}

#[test]
fn every_colour_setting_writes_valid_markup() {
    use starorg_core::enhance::check::tokens;
    let (gd, stock) = fixture();
    let all = OwnedList::from_names(["Filler pistol", "Filler rifle", "Filler Wall"]);
    for n in 0..=5u8 {
        for line in [false, true] {
            let mut p = Profile::everything();
            p.colour_test = true;
            let m = &mut p.mission;
            m.title_owned = true;
            m.owned_line = line;
            m.missing_marker = true;
            for e in [
                &mut m.header_em,
                &mut m.label_em,
                &mut m.title_bp_em,
                &mut m.title_rep_em,
                &mut m.owned_em,
                &mut m.missing_em,
                &mut m.variant_em,
                &mut m.reward_em,
            ] {
                *e = n;
            }
            for owned in [
                &all as &dyn starorg_core::enhance::owned::OwnedBlueprints,
                &owned(),
            ] {
                let g = generate(&gd, &stock, &p, owned);
                for e in g.edits.values() {
                    let part = e.generated_part();
                    assert!(tokens(&part).em_balanced(), "{n}: {part}");
                }
                for (c, cov) in &g.coverage.categories {
                    assert_eq!(cov.left_for(Reason::DroppedByValidation), 0, "{n} {c:?}");
                }
                let bytes = render(&stock, &g).unwrap();
                assert_eq!(check_file_with(&stock, &bytes, g.added.len()), Ok(()));
            }
        }
    }
}

#[test]
fn same_named_blueprints_are_marked_only_when_all_agree() {
    let (gd, stock) = fixture();
    let ids = |ids: &[&str]| {
        let entries: serde_json::Map<String, serde_json::Value> = ids
            .iter()
            .map(|id| {
                (
                    id.to_string(),
                    serde_json::json!({"name": "-", "ids": [id]}),
                )
            })
            .collect();
        let mut o =
            OwnedList::from_json(&serde_json::json!({ "entries": entries }).to_string()).unwrap();
        o.names.insert("filler pistol".into());
        o
    };
    let mut p = Profile::default();
    p.mission.title_owned = true;
    p.mission.missing_marker = true;
    let body = |g: &Generated| added(g, &stock, "zeta_twin_desc");
    // Core owned, helmet not: one "Filler Twin" bullet with neither marker, and no
    // [BP Owned] in the title.
    let g = generate(&gd, &stock, &p, &ids(&["bp-bp_craft_zeta_twin_core"]));
    assert!(
        body(&g).contains("\\n- Filler pistol <EM4>[Owned]</EM4>\\n- Filler Twin\\n\\n"),
        "{}",
        body(&g)
    );
    assert_eq!(
        value(&g, &stock, "zeta_twin_title").as_deref(),
        Some("Filler Twin Job <EM4>[BP]</EM4> <EM4>[100 Rep]</EM4>")
    );
    // Both owned: marked, and the title says so.
    let g = generate(
        &gd,
        &stock,
        &p,
        &ids(&["bp-bp_craft_zeta_twin_core", "bp-bp_craft_zeta_twin_helmet"]),
    );
    assert!(
        body(&g).contains("\\n- Filler Twin <EM4>[Owned]</EM4>"),
        "{}",
        body(&g)
    );
    assert!(
        value(&g, &stock, "zeta_twin_title")
            .unwrap()
            .contains("[BP Owned]")
    );
    // Neither owned: [Missing].
    let g = generate(&gd, &stock, &p, &ids(&[]));
    assert!(
        body(&g).contains("\\n- Filler Twin [Missing]"),
        "{}",
        body(&g)
    );
    // Only the shared name is listed: cannot tell which, so no marker either way.
    let g = generate(&gd, &stock, &p, &OwnedList::from_names(["Filler Twin"]));
    assert!(
        body(&g).contains("\\n- Filler pistol [Missing]\\n- Filler Twin\\n\\n"),
        "{}",
        body(&g)
    );
    assert!(
        !value(&g, &stock, "zeta_twin_title")
            .unwrap()
            .contains("Owned")
    );
}

#[test]
fn quest_hints_on_and_off() {
    let (g, stock) = run(&Profile::default());
    assert_eq!(
        added(&g, &stock, "item_DescZETA_Chip"),
        "\\n\\n<EM4>Used in:</EM4> Filler Heist · Stanton",
        "runtime titles and unreleased uses are left out; EM tags of the title are dropped"
    );
    assert_eq!(
        added(&g, &stock, "item_DescZETA_Part_1_a"),
        "\\n\\n<EM4>Used in:</EM4>\\n- Filler Haul · Stanton\\n- Filler Patrol · Pyro, Stanton",
        "a variant sharing the key is fine"
    );
    for key in [
        "item_DescZETA_Ore",
        "item_DescZETA_Trinket",
        "item_DescZETA_Datapad",
    ] {
        assert_eq!(added(&g, &stock, key), "", "{key}");
    }
    assert_eq!(g.count(Category::QuestHints), 2);
    assert!(
        g.edits
            .values()
            .all(|e| !e.key.to_ascii_uppercase().starts_with("LOC_")),
        "never a LOC_* key"
    );

    let p = Profile {
        quest_hints: false,
        ..Profile::default()
    };
    let (off, _) = run(&p);
    assert_eq!(off.count(Category::QuestHints), 0);
    assert_eq!(off.edits.len(), g.edits.len() - 2);
    assert!(!Profile::minimal().quest_hints);
    let bytes = render(&stock, &g).unwrap();
    assert_eq!(check_file_with(&stock, &bytes, g.added.len()), Ok(()));
}

/// Mission rewards read item names through the name chain (spec missing-names): a reward
/// whose stock name is a placeholder shows the generated name, and a curated one when the
/// patch set has it; the placeholder line itself is renamed.
#[test]
fn rewards_use_the_name_chain() {
    let (mut gd, stock) = fixture();
    let text = String::from_utf8(stock.as_bytes().to_vec()).unwrap();
    let stock = StockIni::parse(
        text.replace(
            "item_NameZETA_Scrip=Filler Scrip",
            "item_NameZETA_Scrip=<= PLACEHOLDER =>",
        )
        .into_bytes(),
    );
    gd.item_names.push(starorg_core::model::ItemName {
        class: "ZETA_Scrip".into(),
        name: "@item_NameZETA_Scrip".into(),
        desc: None,
        kind: "Misc".into(),
    });
    let p = Profile::default();
    let rewards = |g: &Generated| {
        let v = value(g, &stock, "zeta_haul_desc").unwrap();
        v.split("ITEM REWARDS")
            .nth(1)
            .unwrap_or_default()
            .to_string()
    };
    let g = generate(&gd, &stock, &p, &owned());
    assert!(rewards(&g).contains("ZETA Scrip"), "{}", rewards(&g));
    assert_eq!(
        value(&g, &stock, "item_NameZETA_Scrip").as_deref(),
        Some("ZETA Scrip")
    );
    let curated = [("item_namezeta_scrip".to_string(), "Zeta Credit".to_string())]
        .into_iter()
        .collect();
    let g = starorg_core::enhance::generate_named(&gd, &stock, &p, &owned(), None, None, &curated);
    assert!(rewards(&g).contains("Zeta Credit"), "{}", rewards(&g));
    // Its own line keeps the generated name: the patch replaces it in merge.
    assert_eq!(
        value(&g, &stock, "item_NameZETA_Scrip").as_deref(),
        Some("ZETA Scrip")
    );
}

/// Contracts shaped like the LIVE salvage groups (one description key over several
/// variants), with filler keys and names.
mod variants {
    use super::*;
    use starorg_core::model::{Contract, ReputationReward};

    fn contract(name: &str, desc: &str, rep: f64) -> Contract {
        Contract {
            generator: "Gen_Var".into(),
            debug_name: name.into(),
            id: format!("c-{name}"),
            title_key: Some("@var_title".into()),
            description_key: Some(format!("@{desc}")),
            reputation: vec![ReputationReward {
                faction: Some("f_var".into()),
                faction_name_key: Some("@var_faction".into()),
                amount: rep,
                on_success: true,
                on_failure: false,
            }],
            ..Default::default()
        }
    }

    /// The fixture plus `contracts` and stock lines for their keys.
    fn run_with(contracts: Vec<Contract>) -> (Generated, StockIni) {
        let (mut gd, _) = fixture();
        gd.contracts.extend(contracts);
        let d = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/enhance");
        let mut bytes = std::fs::read(d.join("strings.ini")).unwrap();
        for l in [
            "var_title=Filler Salvage",
            "var_faction=Filler Holdings",
            "var_shared=Filler shared prose.",
            "var_hard=Filler hard prose.",
            "var_none=Filler none prose.",
        ] {
            bytes.extend_from_slice(format!("{l}\r\n").as_bytes());
        }
        let stock = StockIni::parse(bytes);
        let g = generate(&gd, &stock, &Profile::default(), &NoneOwned);
        (g, stock)
    }

    #[test]
    fn shared_key_lists_reputation_per_difficulty_and_the_systems() {
        let mut cs = Vec::new();
        for sys in ["Nyx", "PyroRegionA", "Stanton1"] {
            for (d, rep) in [("VeryEasy", 100.0), ("Easy", 100.0), ("Medium", 150.0)] {
                cs.push(contract(
                    &format!("Var_{sys}_Lawful_Salvage_{d}"),
                    "var_shared",
                    rep,
                ));
            }
            cs.push(contract(
                &format!("Var_{sys}_Lawful_Salvage_Hard"),
                "var_hard",
                200.0,
            ));
        }
        let (g, stock) = run_with(cs);
        let b = added(&g, &stock, "var_shared");
        assert!(
            b.contains(
                "<EM4>Reputation (Filler Holdings):</EM4> 100 (Very Easy, Easy) · 150 (Medium)\\n"
            ),
            "{b}"
        );
        assert!(
            b.contains("<EM4>Systems:</EM4> Stanton, Pyro, Nyx\\n"),
            "{b}"
        );
        assert!(
            b.ends_with(
                "<EM4>Shared text:</EM4> shown on 9 contract variants (Very Easy, Easy, Medium)"
            ),
            "{b}"
        );
        // The Hard split has its own key: one value, no per-variant list.
        let h = added(&g, &stock, "var_hard");
        assert!(
            h.contains("<EM4>Reputation (Filler Holdings):</EM4> 200\\n"),
            "{h}"
        );
        assert!(!h.contains(" · "), "{h}");
        // The title stays short: the range over every variant.
        assert_eq!(added(&g, &stock, "var_title"), " <EM4>[100–200 Rep]</EM4>");
    }

    #[test]
    fn single_letter_difficulty_and_rank_labels() {
        let (g, stock) = run_with(vec![
            contract("Var_Industrial_Levski_E_001", "var_shared", 100.0),
            contract("Var_Industrial_Levski_M_001", "var_shared", 100.0),
            contract("Var_Industrial_Levski_H_001", "var_shared", 150.0),
            contract("Var_ShipSalvage_Nyx_Rank0_RMCSmall", "var_hard", 100.0),
            contract("Var_ShipSalvage_Nyx_Rank1_RMCMedium", "var_hard", 100.0),
            contract("Var_ShipSalvage_Nyx_Rank2_RMCLarge", "var_hard", 150.0),
        ]);
        let b = added(&g, &stock, "var_shared");
        assert!(
            b.contains("</EM4> 100 (Easy, Medium) · 150 (Hard)\\n"),
            "{b}"
        );
        let h = added(&g, &stock, "var_hard");
        assert!(
            h.contains("</EM4> 100 (Rank 0, Rank 1) · 150 (Rank 2)\\n"),
            "{h}"
        );
    }

    #[test]
    fn system_only_difference_and_no_separating_label() {
        let mut a = contract("Var_Nyx_Thing", "var_shared", 100.0);
        a.systems = vec!["Nyx".into()];
        let mut b = contract("Var_Pyro_Thing", "var_shared", 150.0);
        b.systems = vec!["Pyro".into()];
        let (g, stock) = run_with(vec![
            a,
            b,
            contract("Var_Alpha", "var_none", 100.0),
            contract("Var_Beta", "var_none", 300.0),
        ]);
        let s = added(&g, &stock, "var_shared");
        assert!(s.contains("</EM4> 150 (Pyro) · 100 (Nyx)\\n"), "{s}");
        assert!(s.contains("<EM4>Systems:</EM4> Pyro, Nyx\\n"), "{s}");
        // Told apart by system, which has its own line: no list after the count.
        assert!(
            s.ends_with("<EM4>Shared text:</EM4> shown on 2 contract variants"),
            "{s}"
        );
        // Nothing tells these apart: keep the range, never a made-up label.
        let n = added(&g, &stock, "var_none");
        assert!(
            n.contains("<EM4>Reputation (Filler Holdings):</EM4> 100–300\\n"),
            "{n}"
        );
        assert!(n.ends_with("shown on 2 contract variants"), "{n}");
    }

    #[test]
    fn requires_and_failure_per_variant() {
        let mut e = contract("Var_Bounty_Easy", "var_shared", 100.0);
        e.min_standing = Some("Standing_Jr".into());
        let mut h = contract("Var_Bounty_Hard", "var_shared", 300.0);
        h.min_standing = Some("Standing_Sr".into());
        h.reputation.push(ReputationReward {
            faction: Some("f_var".into()),
            faction_name_key: Some("@var_faction".into()),
            amount: -500.0,
            on_success: false,
            on_failure: true,
        });
        let (g, stock) = run_with(vec![e, h]);
        let b = added(&g, &stock, "var_shared");
        assert!(b.contains("</EM4> 100 (Easy) · 300 (Hard)\\n"), "{b}");
        assert!(
            b.contains("<EM4>Failure:</EM4> none (Easy) · -500 (Hard)\\n"),
            "{b}"
        );
        assert!(
            b.contains("<EM4>Requires:</EM4> Filler Junior (Easy) · Filler Senior (Hard)\\n"),
            "{b}"
        );
    }
}

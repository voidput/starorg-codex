//! Golden and targeted tests for the enhancement generators (spec 5, 6, 13).
//!
//! Fixtures are synthetic (`tests/fixtures/enhance/`, made by `make_fixtures.py`): made-up
//! records and filler prose, no CIG text. The expected strings below are written from the
//! spec's format table and examples, not copied from output.
//!
//! Regenerate the golden file with `UPDATE_GOLDEN=1 cargo test -p starorg-core --test enhance`.

use starorg_core::enhance::check::check_file;
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
            "- Filler rifle [Owned]",
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
        bounty.starts_with("\\n\\n<EM3>BLUEPRINTS</EM3>\\n<EM4>[Easy]</EM4>\\n- Filler pistol\\n"),
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
    assert!(b.contains("<EM4>Blueprint reward:</EM4> 25–50%\\n"), "{b}");
}

#[test]
fn render_keeps_the_file_shape() {
    let (g, stock) = run(&Profile::everything());
    let bytes = render(&stock, &g).unwrap();
    assert_eq!(check_file(&stock, &bytes), Ok(()));
    let text = std::str::from_utf8(&bytes).unwrap();
    assert!(text.starts_with('\u{feff}'));
    assert!(text.contains("\r\nui_unrelated,P=Unrelated filler\r\n"));
    // Branding is on in Everything.
    assert!(
        text.contains("Frontend_PU_Version=Filler version\\nText enhanced with StarOrg Strings v")
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

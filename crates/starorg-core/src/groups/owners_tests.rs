use super::*;
use crate::ini::BOM;
use crate::model::{Component, Contract, LocKeys, ReputationStanding, Ship, ShipLoc, Weapon};

fn stock(lines: &[(&str, &str)]) -> StockIni {
    let mut b = BOM.to_vec();
    for (k, v) in lines {
        b.extend_from_slice(format!("{k}={v}\r\n").as_bytes());
    }
    StockIni::parse(b)
}

fn item(record: &str, name: &str, desc: Option<&str>, mfr: Option<&str>) -> ItemIdentity {
    ItemIdentity {
        record_name: format!("EntityClassDefinition.{record}"),
        manufacturer: mfr.map(str::to_string),
        loc: LocKeys {
            name: Some(format!("@{name}")),
            desc: desc.map(|d| format!("@{d}")),
            ..Default::default()
        },
        ..Default::default()
    }
}

const LINES: &[(&str, &str)] = &[
    ("manufacturer_NameAEGS", "Aegis Dynamics"),
    (
        "manufacturer_DescAEGS",
        "Makers of the Gladius and other ships.",
    ),
    ("item_NameCOOL_AEGS_S01_Bracer", "Bracer"),
    (
        "item_DescCOOL_AEGS_S01_Bracer",
        "Item Type: Cooler\\nSize: 1",
    ),
    ("item_NameBEHR_Rifle", "P4-AR Rifle"),
    ("item_DescShared", "A shared description."),
    ("vehicle_NameAEGS_Gladius", "Aegis Gladius"),
    (
        "vehicle_DescAEGS_Gladius",
        "Manufacturer: Aegis Dynamics\\nA fighter.",
    ),
    ("vehicle_RoleFighter", "Light Fighter"),
    ("Bounty_Title_01", "Bounty: ~mission(TargetName)"),
    ("Bounty_Desc_01", "Find and kill the target."),
    ("RepStanding_Friendly", "Friendly"),
    ("item_Name_fta_helmet_01", "Venture Helmet"),
    ("item_Desc_fta_helmet_01", "Item Type: Light Armor"),
    ("item_Descfta_helmet_01,P", "Same stem, flag suffix"),
    ("Pyro1_L2_03", "PYR1 L2-C"),
    ("Pyro1_L2_03_desc", "A gas pocket."),
    ("mg_miles_drug_title_0003", "Boxman job"),
    ("mg_miles_drug_desc_0003", "Run a crate."),
    ("item_DescNOPE_Orphan", "Nobody names me."),
    ("hud_law_TrespassArea", "Leave Area"),
    ("item_NameTemplate", "<= PLACEHOLDER =>"),
    ("LOC_PLACEHOLDER", "Placeholder"),
];

fn game_data() -> GameData {
    let mut gd = GameData::default();
    gd.components.push(Component {
        item: item(
            "COOL_AEGS_S01_Bracer",
            "item_NameCOOL_AEGS_S01_Bracer",
            Some("item_DescCOOL_AEGS_S01_Bracer"),
            Some("AEGS"),
        ),
        category: "coolers".into(),
        ..Default::default()
    });
    // An AI copy with the same name and maker: one owner, not two.
    gd.components.push(Component {
        item: item(
            "COOL_AEGS_S01_Bracer_AI",
            "item_NameCOOL_AEGS_S01_Bracer",
            Some("item_DescCOOL_AEGS_S01_Bracer"),
            Some("AEGS"),
        ),
        category: "coolers".into(),
        ..Default::default()
    });
    // Two different records share one description.
    gd.weapons.push(Weapon {
        item: item(
            "BEHR_Rifle",
            "item_NameBEHR_Rifle",
            Some("item_DescShared"),
            Some("BEHR"),
        ),
        class: WeaponClass::Fps,
        ..Default::default()
    });
    gd.components.push(Component {
        item: item(
            "COOL_AEGS_S01_Bracer",
            "item_NameCOOL_AEGS_S01_Bracer",
            Some("item_DescShared"),
            Some("AEGS"),
        ),
        category: "coolers".into(),
        ..Default::default()
    });
    // An engine filler key is owned by nobody.
    gd.components.push(Component {
        item: item(
            "COOL_Filler",
            "LOC_PLACEHOLDER",
            Some("LOC_PLACEHOLDER"),
            None,
        ),
        category: "coolers".into(),
        ..Default::default()
    });
    // A template is not an owner.
    gd.components.push(Component {
        item: ItemIdentity {
            is_template: true,
            ..item("Template", "item_NameTemplate", None, None)
        },
        category: "coolers".into(),
        ..Default::default()
    });
    gd.ships.push(Ship {
        record_name: "EntityClassDefinition.AEGS_Gladius".into(),
        path: "entities/spaceships/aegs/aegs_gladius".into(),
        loc: ShipLoc {
            name: Some("@vehicle_NameAEGS_Gladius".into()),
            desc: Some("@vehicle_DescAEGS_Gladius".into()),
            role: Some("@vehicle_RoleFighter".into()),
            ..Default::default()
        },
        manufacturer: Some("aegs".into()),
        ..Default::default()
    });
    gd.contracts.push(Contract {
        generator: "g".into(),
        debug_name: "bounty_01".into(),
        title_key: Some("@Bounty_Title_01".into()),
        description_key: Some("@Bounty_Desc_01".into()),
        ..Default::default()
    });
    gd.reputation_standings.push(ReputationStanding {
        name: "friendly".into(),
        record_name: "SReputationStandingParams.friendly".into(),
        display_name_key: Some("@RepStanding_Friendly".into()),
        ..Default::default()
    });
    gd
}

fn index() -> (StockIni, OwnerIndex) {
    let s = stock(LINES);
    let idx = OwnerIndex::build(&s, &game_data());
    (s, idx)
}

fn row(s: &StockIni, key: &str) -> usize {
    s.entries()
        .position(|(_, k, _)| k == key)
        .unwrap_or_else(|| panic!("no {key}"))
}

fn primary<'a>(s: &StockIni, idx: &'a OwnerIndex, key: &str) -> Option<Owner<'a>> {
    idx.primary(row(s, key))
}

#[test]
fn records_name_their_keys_with_the_field() {
    let (s, idx) = index();
    assert_eq!(idx.rows(), LINES.len());
    let o = primary(&s, &idx, "item_DescCOOL_AEGS_S01_Bracer").unwrap();
    assert_eq!(
        (o.name, o.field, o.kind, o.detail),
        (
            "Bracer",
            Field::Description,
            OwnerKind::Component,
            Some("Aegis Dynamics")
        )
    );
    // The AI copy collapsed into the same owner.
    assert_eq!(idx.count(row(&s, "item_DescCOOL_AEGS_S01_Bracer")), 1);
    let o = primary(&s, &idx, "vehicle_NameAEGS_Gladius").unwrap();
    assert_eq!(
        (o.name, o.field, o.kind),
        ("Aegis Gladius", Field::Name, OwnerKind::Ship)
    );
    assert_eq!(o.detail, Some("Aegis Dynamics"));
    let o = primary(&s, &idx, "vehicle_RoleFighter").unwrap();
    assert_eq!((o.name, o.field), ("Aegis Gladius", Field::Role));
    let o = primary(&s, &idx, "Bounty_Desc_01").unwrap();
    assert_eq!(
        (o.name, o.field, o.kind),
        (
            "Bounty: ~mission(TargetName)",
            Field::Description,
            OwnerKind::Mission
        )
    );
    let o = primary(&s, &idx, "RepStanding_Friendly").unwrap();
    assert_eq!((o.name, o.kind), ("Friendly", OwnerKind::Reputation));
    let o = primary(&s, &idx, "manufacturer_DescAEGS").unwrap();
    assert_eq!(
        (o.name, o.field, o.kind),
        (
            "Aegis Dynamics",
            Field::Description,
            OwnerKind::Manufacturer
        )
    );
    // Templates own nothing; the placeholder text names nothing either.
    assert!(primary(&s, &idx, "item_NameTemplate").is_none());
    assert!(primary(&s, &idx, "LOC_PLACEHOLDER").is_none());
}

#[test]
fn shared_keys_list_every_owner_namesake_first() {
    let (s, idx) = index();
    let all: Vec<(&str, OwnerKind)> = idx
        .owners(row(&s, "item_DescShared"))
        .map(|o| (o.name, o.kind))
        .collect();
    assert_eq!(
        all,
        vec![
            ("Bracer", OwnerKind::Component),
            ("P4-AR Rifle", OwnerKind::FpsWeapon)
        ]
    );
    // The namesake record goes first even when its kind sorts later.
    let mut gd = game_data();
    gd.weapons[0].item.loc.desc = Some("@item_DescBEHR_Rifle".into());
    gd.components[0].item.loc.desc = Some("@item_DescBEHR_Rifle".into());
    let s2 = stock(&[
        ("item_DescBEHR_Rifle", "x"),
        ("item_NameBEHR_Rifle", "P4-AR Rifle"),
        ("item_NameCOOL_AEGS_S01_Bracer", "Bracer"),
    ]);
    let idx2 = OwnerIndex::build(&s2, &gd);
    let first = idx2.primary(0).unwrap();
    assert_eq!(first.name, "P4-AR Rifle");
    assert_eq!(idx2.count(0), 2);
}

#[test]
fn key_shapes_name_keys_without_a_record() {
    let (s, idx) = index();
    let o = primary(&s, &idx, "item_Desc_fta_helmet_01").unwrap();
    assert_eq!(
        (o.name, o.field, o.kind),
        ("Venture Helmet", Field::Description, OwnerKind::Named)
    );
    let o = primary(&s, &idx, "item_Descfta_helmet_01,P").unwrap();
    assert_eq!(o.name, "Venture Helmet");
    let o = primary(&s, &idx, "item_Name_fta_helmet_01").unwrap();
    assert_eq!((o.name, o.field), ("Venture Helmet", Field::Name));
    let o = primary(&s, &idx, "Pyro1_L2_03_desc").unwrap();
    assert_eq!((o.name, o.field), ("PYR1 L2-C", Field::Description));
    let o = primary(&s, &idx, "Pyro1_L2_03").unwrap();
    assert_eq!((o.name, o.field), ("PYR1 L2-C", Field::Name));
    let o = primary(&s, &idx, "mg_miles_drug_desc_0003").unwrap();
    assert_eq!((o.name, o.field), ("Boxman job", Field::Description));
    let o = primary(&s, &idx, "mg_miles_drug_title_0003").unwrap();
    assert_eq!((o.name, o.field), ("Boxman job", Field::Title));
}

#[test]
fn unknown_keys_have_no_owner_and_a_pretty_fallback() {
    let (s, idx) = index();
    assert!(primary(&s, &idx, "item_DescNOPE_Orphan").is_none());
    assert!(primary(&s, &idx, "hud_law_TrespassArea").is_none());
    assert_eq!(idx.count(row(&s, "hud_law_TrespassArea")), 0);
    assert_eq!(
        pretty_key("item_DescCOOL_AEGS_S01_Bracer"),
        "item desc · COOL_AEGS_S01_Bracer"
    );
    assert_eq!(
        pretty_key("hud_law_TrespassArea"),
        "hud · law trespass area"
    );
    assert_eq!(
        pretty_key("item_Name_987_jacket_03"),
        "item name · 987 jacket 03"
    );
    assert_eq!(
        pretty_key("item_desc_FLBL_ANVL_Pisces"),
        "item desc · FLBL_ANVL_Pisces"
    );
    assert_eq!(
        pretty_key("ui_CIDynamicZoomAbsDesc"),
        "ui · CI dynamic zoom abs desc"
    );
    assert_eq!(pretty_key("item_Nameless_thing"), "item · nameless thing");
    assert_eq!(pretty_key("Pyro2"), "pyro2");
    assert_eq!(
        pretty_key("@area_name_int_transit,P"),
        "area name · int transit"
    );
    assert_eq!(pretty_key("vehicle_Name"), "vehicle name");
}

#[test]
fn out_of_range_rows_are_empty() {
    let (_, idx) = index();
    assert!(idx.primary(10_000).is_none());
    assert_eq!(idx.owners(10_000).count(), 0);
    assert!(idx.heap_bytes() > 0);
    assert!(OwnerIndex::default().primary(0).is_none());
}

/// Real data, read only: a cache entry folder (`base.ini` + `gamedata.json`) in
/// `STARORG_OWNERS_DIR`. Prints time, size and samples; run with `--ignored --nocapture`.
#[test]
#[ignore = "needs a real cache entry in STARORG_OWNERS_DIR"]
fn real_cache_entry() {
    let dir = std::path::PathBuf::from(
        std::env::var("STARORG_OWNERS_DIR").expect("set STARORG_OWNERS_DIR"),
    );
    let stock = StockIni::parse(std::fs::read(dir.join("base.ini")).unwrap());
    let gd: GameData =
        serde_json::from_slice(&std::fs::read(dir.join("gamedata.json")).unwrap()).unwrap();
    let t = std::time::Instant::now();
    let idx = OwnerIndex::build(&stock, &gd);
    let took = t.elapsed();
    let rows = idx.rows();
    let owned = (0..rows).filter(|r| idx.count(*r) > 0).count();
    let shared = (0..rows).filter(|r| idx.count(*r) > 1).count();
    let records = (0..rows)
        .filter(|r| idx.primary(*r).is_some_and(|o| o.kind != OwnerKind::Named))
        .count();
    eprintln!(
        "keys {rows}, with owner {owned} ({records} from records), shared {shared}, distinct owners {}, build {took:?}, heap {} KiB",
        idx.records(),
        idx.heap_bytes() / 1024
    );
    for key in [
        "item_DescCOOL_AEGS_S01_Bracer",
        "vehicle_NameAEGS_Gladius",
        "vehicle_DescAEGS_Gladius",
        "manufacturer_NameAEGS",
        "Pyro1_L2_03_desc",
        "item_Desc_fta_medium_helmet_01_04_10",
        "hud_law_TrespassAreaRevokedAuthorisation",
    ] {
        let r = stock.entries().position(|(_, k, _)| k == key);
        let o = r.and_then(|r| idx.primary(r));
        eprintln!(
            "{key}: {:?} / {} owners / {}",
            o.map(|o| (o.name, o.field, o.kind, o.detail)),
            r.map(|r| idx.count(r)).unwrap_or(0),
            pretty_key(key)
        );
    }
    if let Ok(out) = std::env::var("STARORG_OWNERS_DUMP") {
        let mut tsv = String::new();
        for (row, (_, key, _)) in stock.entries().enumerate() {
            let o = idx.primary(row);
            tsv.push_str(&format!(
                "{key}\t{}\t{}\t{}\t{}\n",
                o.map(|o| o.name).unwrap_or(""),
                o.map(|o| o.field.label()).unwrap_or(""),
                o.map(|o| o.kind.label()).unwrap_or(""),
                idx.count(row)
            ));
        }
        std::fs::write(out, tsv).unwrap();
    }
    assert!(owned * 3 > rows / 2, "{owned} of {rows} keys have an owner");
}

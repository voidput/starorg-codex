//! Fleet import and ship names end to end (addendum §6): synthetic catalogue from records +
//! stock, the generator through validation and merge, override precedence, the stale hash.
//! Synthetic data only.

use starorg_core::enhance::owned::NoneOwned;
use starorg_core::enhance::profile::Profile;
use starorg_core::enhance::{Category, check, generate, generate_with, render};
use starorg_core::fleet::store::{ShipEntry, ShipNames, Style};
use starorg_core::fleet::{Catalog, import};
use starorg_core::ini::StockIni;
use starorg_core::merge::{self, Source};
use starorg_core::model::{GameData, Ship, ShipLoc};
use starorg_core::patch::PatchSet;
use starorg_core::state::Override;

const STOCK: &str = "\u{feff}; synthetic\r\nvehicle_NameSYN_Alpha=Synthetic Alpha\r\nvehicle_NameSYN_Alpha_short=Alpha\r\nvehicle_NameSYN_Bravo,P=Synthetic Bravo\r\nvehicle_NameSYN_Bravo_short=Bravo\r\nvehicle_NameSYN_Charlie=Synthetic Charlie \r\nvehicle_DescSYN_Alpha=A synthetic ship.\r\nvehicle_NameSYN_Fmt=Ship %s\r\nui_Other=Other\r\n";

fn ship(record: &str, key: &str) -> Ship {
    Ship {
        record_name: format!("EntityClassDefinition.{record}"),
        loc: ShipLoc {
            name: Some(format!("@{key}")),
            ..Default::default()
        },
        manufacturer: Some("syn".into()),
        movement_class: Some("Spaceship".into()),
        ..Default::default()
    }
}

fn data() -> (GameData, StockIni) {
    let gd = GameData {
        ships: vec![
            ship("SYN_Alpha", "vehicle_NameSYN_Alpha"),
            ship("SYN_Alpha_PU_AI", "vehicle_NameSYN_Alpha"),
            ship("SYN_Bravo", "vehicle_NameSYN_Bravo"),
            ship("SYN_Charlie", "vehicle_NameSYN_Charlie"),
            ship("SYN_Fmt", "vehicle_NameSYN_Fmt"),
        ],
        ..GameData::default()
    };
    (gd, StockIni::parse(STOCK.as_bytes().to_vec()))
}

fn entry(hull: &str, name: Option<&str>) -> ShipEntry {
    ShipEntry {
        hull: hull.into(),
        group: None,
        name: name.map(str::to_string),
        short: None,
        extra: Default::default(),
    }
}

fn settings() -> ShipNames {
    ShipNames {
        enabled: true,
        ships: vec![
            entry("vehicle_NameSYN_Charlie", None),
            entry("vehicle_namesyn_bravo", Some("Needle")),
            entry("vehicle_NameSYN_Gone", None),
            entry("vehicle_NameSYN_Fmt", None),
        ],
        ..ShipNames::default()
    }
}

fn value(stock: &StockIni, bytes: &[u8], key: &str) -> String {
    let out = StockIni::parse(bytes.to_vec());
    out.value_at(stock.find(key).unwrap()).unwrap().to_string()
}

#[test]
fn generator_prefixes_renames_and_keeps_key_spelling() {
    let (gd, stock) = data();
    let p = Profile::preset("minimal").unwrap();
    let g = generate_with(&gd, &stock, &p, &NoneOwned, Some(&settings()));
    let bytes = render(&stock, &g).expect("whole-file rules pass");
    assert_eq!(
        value(&stock, &bytes, "vehicle_NameSYN_Charlie"),
        "01 Synthetic Charlie ",
        "stock untrimmed"
    );
    assert_eq!(value(&stock, &bytes, "vehicle_NameSYN_Bravo"), "02 Needle");
    assert_eq!(
        value(&stock, &bytes, "vehicle_NameSYN_Bravo_short"),
        "Needle"
    );
    assert_eq!(
        value(&stock, &bytes, "vehicle_NameSYN_Alpha"),
        "Synthetic Alpha",
        "not listed"
    );
    // `,P` stays in the file, CRLF and BOM too.
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(
        text.contains("vehicle_NameSYN_Bravo,P=02 Needle\r\n"),
        "{text}"
    );
    assert!(text.starts_with('\u{feff}'));
    let c = g.coverage.get(Category::ShipNames);
    assert_eq!(c.written, 3);
    assert_eq!(c.left.len(), 1, "the format key is left as stock: {c:?}");
    assert_eq!(value(&stock, &bytes, "vehicle_NameSYN_Fmt"), "Ship %s");
    // The edit keeps its parts: prefix and a whole-value rename.
    let e = g
        .edits
        .values()
        .find(|e| e.key == "vehicle_NameSYN_Bravo,P")
        .unwrap();
    assert_eq!(
        (e.prefix.as_str(), e.replace.as_deref()),
        ("02 ", Some("Needle"))
    );
    assert_eq!(e.generated_part(), "02 Needle");

    // Without settings nothing changes, and the old entry point never writes ship names.
    let plain = generate(&gd, &stock, &p, &NoneOwned);
    assert_eq!(plain.count(Category::ShipNames), 0);
}

#[test]
fn style_none_renames_only_and_symbol_floats() {
    let (gd, stock) = data();
    let p = Profile::preset("minimal").unwrap();
    let mut s = settings();
    s.style = Style::None;
    let g = generate_with(&gd, &stock, &p, &NoneOwned, Some(&s));
    assert_eq!(g.count(Category::ShipNames), 2, "Bravo long + short");
    s.style = Style::Symbol;
    s.symbol = "!".into();
    let g = generate_with(&gd, &stock, &p, &NoneOwned, Some(&s));
    let bytes = render(&stock, &g).unwrap();
    assert_eq!(
        value(&stock, &bytes, "vehicle_NameSYN_Charlie"),
        "! Synthetic Charlie "
    );
}

#[test]
fn renames_are_whole_value_and_checked() {
    assert!(check::check_replaced("Synthetic Alpha", "01 Needle").is_ok());
    assert!(
        check::check_replaced("Ship %s", "01 Needle").is_err(),
        "format key stays stock"
    );
    assert!(check::check_replaced("Synthetic Alpha", "01 <EM4>x").is_err());
}

#[test]
fn an_override_on_the_hull_key_wins() {
    let (gd, stock) = data();
    let p = Profile::preset("minimal").unwrap();
    let g = generate_with(&gd, &stock, &p, &NoneOwned, Some(&settings()));
    let gen_edits: Vec<merge::GenEdit> = g.edits.values().map(merge::GenEdit::from).collect();
    let overrides = vec![Override {
        line: 1,
        key: "vehicle_NameSYN_Bravo".into(),
        value: "My Bravo".into(),
    }];
    let m = merge::merge(
        &stock,
        &gd,
        None,
        &gen_edits,
        &PatchSet::default(),
        &overrides,
    );
    let bytes = stock.render(&m.edits);
    assert_eq!(value(&stock, &bytes, "vehicle_NameSYN_Bravo"), "My Bravo");
    assert_eq!(
        value(&stock, &bytes, "vehicle_NameSYN_Bravo_short"),
        "Needle"
    );
    assert_eq!(
        value(&stock, &bytes, "vehicle_NameSYN_Charlie"),
        "01 Synthetic Charlie "
    );
    let line = stock.find("vehicle_NameSYN_Bravo").unwrap();
    assert_eq!(m.provenance.get(&line), Some(&Source::Override));
    let line = stock.find("vehicle_NameSYN_Charlie").unwrap();
    assert_eq!(
        m.provenance.get(&line),
        Some(&Source::Generated {
            generator: "ship-names".into()
        })
    );
    assert!(starorg_core::validate::check_file(&stock, &bytes).is_ok());
}

#[test]
fn import_matches_the_built_catalogue() {
    let (gd, stock) = data();
    let cat = Catalog::build(&gd, &stock);
    assert_eq!(cat.flyable().count(), 4);
    let p = import::parse(b"2x Synthetic Alpha\nBravo\nSYN Delta\n", None).unwrap();
    let o = import::run(&p, &cat, &Default::default(), false);
    let got: Vec<(&str, u32)> = o
        .report
        .hulls
        .iter()
        .map(|h| (h.hull.as_str(), h.count))
        .collect();
    assert_eq!(
        got,
        [("vehicle_NameSYN_Alpha", 2), ("vehicle_NameSYN_Bravo", 1)]
    );
    assert_eq!(o.report.unmatched[0].name, "SYN Delta");
}

#[test]
fn applied_manifests_without_the_field_read_as_empty() {
    let old = r#"{"branch":null,"game_version":null,"change":null,"version":null,
        "fingerprint":{"change":null,"p4k_size":1,"p4k_mtime_ns":1},"cache_key":"k",
        "profile_hash":"p","patch_set_hash":"s","overrides_hash":"o","app_version":"0",
        "output_sha256":"x","time":"t","backup":null,"warnings":[],"real_target":"/x",
        "user_cfg_changed":false}"#;
    let a: starorg_core::apply::Applied = serde_json::from_str(old).unwrap();
    assert_eq!(a.ship_names_hash, "");
    // Nor `output_version` (0.1.0): reads as the version 0.1.0 generated.
    assert_eq!(a.output_version, 1);
}

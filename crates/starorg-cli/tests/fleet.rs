//! `fleet import|list|match|clear` and ship names through apply/status (addendum §6.7, 6.9)
//! on the synthetic fake install. The ship records come from a small GameData file through
//! the hidden `--test-gamedata` hook. Synthetic names, pledge ids and custom names only.
#![cfg(unix)]

mod common;

use common::*;
use serde_json::{Value, json};
use std::path::PathBuf;

fn ship(record: &str, key: &str) -> Value {
    json!({ "recordName": format!("EntityClassDefinition.{record}"), "id": record,
            "loc": { "name": format!("@{key}") }, "manufacturer": "syn",
            "movementClass": "Spaceship" })
}

/// Stock with three synthetic hulls added, and the GameData that names them.
fn with_ships(f: &Fake) -> PathBuf {
    let mut stock = stock_bytes("Star Citizen Alpha 4.10");
    stock.extend_from_slice(
        b"vehicle_NameSYN_Alpha=Synthetic Alpha\r\nvehicle_NameSYN_Alpha_short=Alpha\r\nvehicle_NameSYN_Bravo,P=Synthetic Bravo\r\nvehicle_NameSYN_Charlie=Synthetic Charlie\r\n",
    );
    std::fs::write(&f.stock, stock).unwrap();
    let gd = json!({ "schema": 2, "build": null, "ships": [
        ship("SYN_Alpha", "vehicle_NameSYN_Alpha"),
        ship("SYN_Alpha_PU_AI", "vehicle_NameSYN_Alpha"),
        ship("SYN_Bravo", "vehicle_NameSYN_Bravo"),
        ship("SYN_Charlie", "vehicle_NameSYN_Charlie"),
    ]});
    let p = f.base.join("gamedata.json");
    std::fs::write(&p, gd.to_string()).unwrap();
    p
}

fn export(f: &Fake) -> PathBuf {
    let rows = json!([
        { "ship_code": "SYN_Alpha", "ship_name": "Needle", "manufacturer_code": "SYN", "manufacturer_name": "Synthetic",
          "name": "Alpha", "lti": true, "warbond": false, "entity_type": "ship", "pledge_id": "90000001",
          "pledge_name": "Synthetic pledge", "pledge_date": "January 01, 2950", "pledge_cost": "$1.00 USD" },
        { "ship_code": "SYN_Alpha", "ship_name": "Alpha", "manufacturer_code": "SYN", "name": "Alpha", "entity_type": "ship" },
        { "ship_code": "SYN_Bravo", "ship_name": "Bravo", "manufacturer_code": "SYN", "name": "Synthetic Bravo", "entity_type": "ship" },
        { "ship_code": "SYN_Zulu", "ship_name": "Zulu", "manufacturer_code": "SYN", "name": "Zulu", "entity_type": "ship" },
        { "name": "Synthetic paint", "entity_type": "skin" }
    ]);
    let p = f.base.join("fleet-export.json");
    std::fs::write(&p, rows.to_string()).unwrap();
    p
}

fn data_root(f: &Fake) -> PathBuf {
    f.base.join("home/data/starorg-codex")
}

fn run_json(f: &Fake, gd: &std::path::Path, args: &[&str]) -> (i32, Value) {
    let mut a = vec!["--test-gamedata", gd.to_str().unwrap()];
    a.extend_from_slice(args);
    f.json(&a)
}

#[test]
fn dry_run_writes_nothing_then_import_lists_and_apply_prefixes() {
    let f = Fake::new();
    let gd = with_ships(&f);
    let file = export(&f);
    f.setup("LIVE");
    let fleet_json = data_root(&f).join("fleet.json");
    let names_toml = data_root(&f).join("ship-names.toml");

    let (code, v) = run_json(
        &f,
        &gd,
        &["fleet", "import", file.to_str().unwrap(), "--dry-run"],
    );
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["dry_run"], true);
    assert_eq!(v["written"], false);
    let r = &v["report"];
    assert_eq!(r["format"], "xplor-json");
    assert_eq!(
        (r["total"].as_u64(), r["skipped"].as_u64()),
        (Some(5), Some(1))
    );
    assert_eq!(r["hulls"][0]["hull"], "vehicle_NameSYN_Alpha");
    assert_eq!(r["hulls"][0]["count"], 2);
    assert_eq!(r["hulls"][0]["names"], json!(["Needle"]));
    assert_eq!(r["hulls"][1]["hull"], "vehicle_NameSYN_Bravo");
    assert_eq!(r["unmatched"][0]["name"], "Zulu");
    assert!(
        !fleet_json.exists() && !names_toml.exists(),
        "dry run wrote a file"
    );

    let (code, v) = run_json(&f, &gd, &["fleet", "import", file.to_str().unwrap()]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["written"], true);
    assert_eq!(v["added_to_order"], 2);
    let stored = std::fs::read_to_string(&fleet_json).unwrap();
    for gone in ["90000001", "pledge", "lti", "January", "USD"] {
        assert!(!stored.contains(gone), "{gone} stored: {stored}");
    }

    // Listed, off by default: apply writes no ship names.
    let (code, v) = run_json(&f, &gd, &["fleet", "list", "--channel", "LIVE"]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["enabled"], false);
    assert_eq!(v["ships"][0]["count"], 2);
    assert_eq!(v["pending"][0]["name"], "Zulu");
    let (code, v) = run_json(&f, &gd, &["apply", "--channel", "LIVE"]);
    assert_eq!(code, 0, "{v}");
    let ini = std::fs::read_to_string(f.ini("LIVE")).unwrap();
    assert!(
        ini.contains("vehicle_NameSYN_Alpha=Synthetic Alpha\r\n"),
        "{ini}"
    );
    let (_, st) = run_json(&f, &gd, &["status", "--channel", "LIVE"]);
    assert_eq!(st["channels"][0]["state"], "applied", "{st}");

    // Turn it on by hand: stale with reason "ship names", then apply writes the prefixes.
    let text = std::fs::read_to_string(&names_toml).unwrap();
    std::fs::write(
        &names_toml,
        text.replace("enabled = false", "enabled = true"),
    )
    .unwrap();
    let (_, st) = run_json(&f, &gd, &["status", "--channel", "LIVE"]);
    assert_eq!(st["channels"][0]["state"], "stale", "{st}");
    assert_eq!(st["channels"][0]["reasons"], json!(["ship names"]));
    let (code, v) = run_json(&f, &gd, &["apply", "--channel", "LIVE"]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["channels"][0]["counts"]["ship_names"], 2, "{v}");
    let ini = std::fs::read_to_string(f.ini("LIVE")).unwrap();
    assert!(
        ini.contains("vehicle_NameSYN_Alpha=01 Synthetic Alpha\r\n"),
        "{ini}"
    );
    assert!(
        ini.contains("vehicle_NameSYN_Bravo,P=02 Synthetic Bravo\r\n"),
        "{ini}"
    );
    assert!(ini.contains("vehicle_NameSYN_Charlie=Synthetic Charlie\r\n"));
    assert!(
        ini.contains("vehicle_NameSYN_Alpha_short=Alpha\r\n"),
        "short key untouched"
    );

    // A rename and an override: the override wins on its key, `list` says so.
    let mut s: toml::Table = std::fs::read_to_string(&names_toml)
        .unwrap()
        .parse()
        .unwrap();
    s["ship"].as_array_mut().unwrap()[1]
        .as_table_mut()
        .unwrap()
        .insert("name".into(), "Needle".into());
    std::fs::write(&names_toml, toml::to_string(&s).unwrap()).unwrap();
    std::fs::write(
        f.data_dir("LIVE").join("overrides.ini"),
        "vehicle_NameSYN_Alpha=My Alpha\n",
    )
    .unwrap();
    let (_, st) = run_json(&f, &gd, &["status", "--channel", "LIVE"]);
    assert_eq!(
        st["channels"][0]["reasons"],
        json!(["overrides", "ship names"])
    );
    let (_, v) = run_json(&f, &gd, &["fleet", "list", "--channel", "LIVE"]);
    assert_eq!(v["ships"][0]["value"], "My Alpha");
    assert_eq!(v["ships"][0]["overridden"], true);
    assert_eq!(v["ships"][1]["value"], "02 Needle");
    run_json(&f, &gd, &["apply", "--channel", "LIVE"]);
    let ini = std::fs::read_to_string(f.ini("LIVE")).unwrap();
    assert!(ini.contains("vehicle_NameSYN_Alpha=My Alpha\r\n"));
    assert!(ini.contains("vehicle_NameSYN_Bravo,P=02 Needle\r\n"));

    // Hand match: the pending row leaves the list.
    let (code, v) = run_json(
        &f,
        &gd,
        &["fleet", "match", "Zulu", "vehicle_NameSYN_Charlie"],
    );
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["added_to_fleet"], "vehicle_NameSYN_Charlie");
    let (_, v) = run_json(&f, &gd, &["fleet", "list"]);
    assert!(v["pending"].as_array().unwrap().is_empty(), "{v}");
    assert_eq!(v["ships"].as_array().unwrap().len(), 3);
    let (code, _) = run_json(&f, &gd, &["fleet", "match", "Zulu", "vehicle_NameNope"]);
    assert_eq!(code, 2);

    // The human form names both files and flags the override.
    let o = f.run(&["--test-gamedata", gd.to_str().unwrap(), "fleet", "list"]);
    let text = String::from_utf8_lossy(&o.stdout);
    assert!(
        text.contains("fleet.json") && text.contains("ship-names.toml"),
        "{text}"
    );
    assert!(text.contains("overridden"), "{text}");
    println!("{text}");

    // Clear forgets the fleet, keeps the order.
    let (code, v) = run_json(&f, &gd, &["fleet", "clear"]);
    assert_eq!(code, 0, "{v}");
    assert!(names_toml.exists());
}

#[test]
fn text_from_stdin_and_refusals() {
    use std::io::Write;
    use std::process::Stdio;
    let f = Fake::new();
    let gd = with_ships(&f);
    let mut c = f.cmd(&[
        "--test-gamedata",
        gd.to_str().unwrap(),
        "fleet",
        "import",
        "-",
        "--dry-run",
        "--json",
    ]);
    let mut child = c
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"# mine\n2x Synthetic Charlie\nAlpha x3\n")
        .unwrap();
    let o = child.wait_with_output().unwrap();
    assert_eq!(o.status.code(), Some(0));
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["report"]["format"], "text");
    assert_eq!(v["report"]["hulls"][0]["count"], 2);
    assert_eq!(v["report"]["hulls"][1]["count"], 3);

    // Not a fleet export: exit 1.
    let bad = f.base.join("bad.json");
    std::fs::write(&bad, "{\"ships\": []}").unwrap();
    let (code, v) = run_json(&f, &gd, &["fleet", "import", bad.to_str().unwrap()]);
    assert_eq!(code, 1, "{v}");
    assert_eq!(v["error"]["kind"], "bad-file");
    // No game data (no ships in this build): exit 1 for a real import, 0 for a dry run.
    let list = f.base.join("list.txt");
    std::fs::write(&list, "Alpha\n").unwrap();
    let (code, _) = f.json(&["fleet", "import", list.to_str().unwrap()]);
    assert_eq!(code, 1);
    let (code, v) = f.json(&["fleet", "import", list.to_str().unwrap(), "--dry-run"]);
    assert_eq!(code, 0);
    assert_eq!(v["report"]["empty_catalog"], true);
    assert!(!data_root(&f).join("fleet.json").exists());
}

//! `hook` and `blueprints` CLI tests (addendum §2-§4) on the Wine-shaped fake tree from
//! `common`. Logs and the Smart Citizen export are synthetic: invented character names and
//! ids, invented blueprint names.
#![cfg(unix)]

mod common;

use common::*;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

const LOGIN_A: &str = "<2026-03-01T10:00:00.000Z> [Notice] <AccountLoginCharacterStatus_Character> Character: createdAt 1 - updatedAt 2 - geid 100001 - accountId 900 - name Test_Pilot - state STATE_CURRENT [Team_GameServices][Login]\n";
const LOGIN_B: &str = "<2026-03-02T10:00:00.000Z> [Notice] <AccountLoginCharacterStatus_Character> Character: createdAt 1 - updatedAt 2 - geid 200002 - accountId 900 - name Alt_Pilot - state STATE_CURRENT [Team_GameServices][Login]\n";
const OTHER: &str = "<2026-03-02T10:01:00.000Z> [Notice] <Something> nickname=\"Stranger_X\" playerGEID=555 state STATE_CURRENT\n";

fn bp(t: &str, name: &str) -> String {
    format!(
        "<{t}> [Notice] <SHUDEvent_OnNotification> Added notification \"Received Blueprint: {name}: \" [7] to queue. New queue size: 1 [Team_CoreGameplayFeatures][Missions][Comms]\n<{t}>    \"Received Blueprint: {name}: \" [7]\n"
    )
}

fn write_logs(f: &Fake) {
    let live = f.lib.join("LIVE");
    std::fs::create_dir_all(live.join("logbackups")).unwrap();
    std::fs::write(
        live.join("logbackups/Game Build(1) 01 Mar 26 (10 00 00).log"),
        format!(
            "<2026-03-01T09:59:00.000Z> Log started\n{}{LOGIN_A}{}{}",
            bp("2026-03-01T09:59:30.000Z", "[H] Early Helmet"),
            bp("2026-03-01T10:05:00.000Z", "[L] Fake Rifle 'Kestrel'"),
            bp("2026-03-01T10:06:00.000Z", "Test Armor Arms"),
        ),
    )
    .unwrap();
    std::fs::write(
        live.join("Game.log"),
        format!(
            "<2026-03-02T09:59:00.000Z> Log started\n{LOGIN_B}{OTHER}{}",
            bp("2026-03-02T10:05:00.000Z", "[M] Alt Only Core"),
        ),
    )
    .unwrap();
}

fn json_of(o: &std::process::Output) -> Value {
    serde_json::from_slice(&o.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: {}\n{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        )
    })
}

#[test]
fn postexit_scans_attributes_and_reapplies() {
    let f = Fake::new();
    write_logs(&f);
    f.setup("LIVE");
    let (code, v) = f.json(&["apply", "--channel", "LIVE"]);
    assert_eq!(code, 0, "{v}");
    let (code, v) = f.json(&["hook", "postexit", "--timeout", "30"]);
    assert_eq!(code, 0, "{v}");
    let live_scan = v["scans"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["channel"] == "LIVE")
        .unwrap();
    assert_eq!(
        live_scan["scan"]["new_blueprints"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(live_scan["scan"]["last_character"], "200002");
    let live = v["channels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["channel"] == "LIVE")
        .unwrap();
    // New owned blueprints for the auto character (newest session = Alt_Pilot) make the
    // channel stale, so the post-exit run re-applies.
    assert_eq!(live["result"], "applied", "{v}");
    assert_eq!(live["owned"]["mode"], "auto");
    assert_eq!(live["owned"]["characters"][0], "200002");
    assert_eq!(live["owned"]["count"], 1);
    let data = f.data_dir("LIVE").join("blueprints");
    let a: Value =
        serde_json::from_slice(&std::fs::read(data.join("100001.json")).unwrap()).unwrap();
    let names: Vec<&str> = a["entries"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(names, vec!["fake rifle 'kestrel'", "test armor arms"]);
    let un: Value =
        serde_json::from_slice(&std::fs::read(data.join("unattributed.json")).unwrap()).unwrap();
    assert!(un["entries"].get("early helmet").is_some());
    let chars = std::fs::read_to_string(data.join("characters.json")).unwrap();
    assert!(!chars.contains("Stranger_X") && !chars.contains("555"));

    // Second run: nothing new, nothing re-applied.
    let (code, v) = f.json(&["hook", "postexit", "--timeout", "30"]);
    assert_eq!(code, 0);
    let live = v["channels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["channel"] == "LIVE")
        .unwrap();
    assert_eq!(live["result"], "up-to-date", "{v}");

    // Switching the active character to the main makes LIVE stale again.
    let o = f.run(&["blueprints", "use", "--channel", "LIVE", "test_pilot"]);
    assert_eq!(o.status.code(), Some(0), "{o:?}");
    let (_, v) = f.json(&["status", "--channel", "LIVE"]);
    assert_eq!(v["channels"][0]["state"], "stale", "{v}");
    assert_eq!(v["channels"][0]["reasons"][0], "owned blueprints");
    let (code, v) = f.json(&["hook", "prelaunch"]);
    assert_eq!(code, 0);
    let live = v["channels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["channel"] == "LIVE")
        .unwrap();
    assert_eq!(live["result"], "applied");
    assert_eq!(live["owned"]["count"], 2);
    // Characters list (local output) and an unknown character.
    let (code, v) = f.json(&["blueprints", "characters", "--channel", "LIVE"]);
    assert_eq!(code, 0);
    assert_eq!(v["characters"].as_array().unwrap().len(), 2);
    assert_eq!(v["unattributed"], 1);
    let o = f.run(&["blueprints", "use", "--channel", "LIVE", "Nobody"]);
    assert_eq!(o.status.code(), Some(2));
    let (code, v) = f.json(&[
        "apply",
        "--channel",
        "LIVE",
        "--character",
        "any",
        "--dry-run",
    ]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["channels"][0]["owned"]["count"], 3);

    // A one-off pick is kept: status stays applied and the hook does not flip it back.
    let (code, v) = f.json(&["apply", "--channel", "LIVE", "--character", "any"]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["channels"][0]["owned"]["count"], 3);
    assert_eq!(f.status_of("LIVE"), "applied");
    let (code, v) = f.json(&["hook", "prelaunch"]);
    assert_eq!(code, 0);
    let live = v["channels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["channel"] == "LIVE")
        .unwrap();
    assert_eq!(live["result"], "up-to-date", "{v}");
    // `blueprints use` ends it: the channel setting counts again.
    let o = f.run(&["blueprints", "use", "--channel", "LIVE", "test_pilot"]);
    assert_eq!(o.status.code(), Some(0), "{o:?}");
    assert_eq!(f.status_of("LIVE"), "stale");
    let (code, v) = f.json(&["apply", "--channel", "LIVE"]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["channels"][0]["owned"]["count"], 2);
}

#[test]
fn prelaunch_is_non_fatal() {
    let f = Fake::new();
    f.setup("LIVE");
    // Stock file gone: the apply fails, the hook still exits 0 and reports the code.
    std::fs::remove_file(&f.stock).unwrap();
    let o = f.cmd(&["hook", "prelaunch", "--json"]).output().unwrap();
    assert_eq!(o.status.code(), Some(0), "{o:?}");
    let v = json_of(&o);
    assert_eq!(v["exit_code"], 1);
    let skipped: Vec<&str> = v["channels"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["result"] == "needs-setup")
        .map(|c| c["channel"].as_str().unwrap())
        .collect();
    assert_eq!(skipped, vec!["HOTFIX", "PTU"]);
    // No install at all: still 0.
    let o = f
        .cmd_in(&f.base.join("nowhere"), &["hook", "prelaunch"])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(0), "{o:?}");
}

const SC_EXPORT: &str = r#"{"version": 1, "exportedAt": "2026-03-03T12:00:00.000Z", "missions": [],
  "blueprints": [
    {"name": "[CIV-S3-A] Fake Rifle ’Kestrel’", "completed": true, "favorite": true},
    {"name": "[E-S2] [H] New Visor", "completed": true, "favorite": false},
    {"name": "[L] Twin Name", "completed": true, "favorite": false},
    {"name": "Unknown Thing", "completed": true, "favorite": false},
    {"name": "Only Tracked", "completed": false, "favorite": false}
  ]}"#;

const CATALOG: &str = "bp_rifle\tFake Rifle 'Kestrel'\nbp_visor\tNew Visor\nbp_twin_1\tTwin Name\nbp_twin_2\tTwin Name\nbp_tracked\tOnly Tracked\n";

fn import_files(f: &Fake) -> (PathBuf, PathBuf) {
    let e = f.base.join("owned-blueprints.json");
    std::fs::write(&e, SC_EXPORT).unwrap();
    let c = f.base.join("catalog.tsv");
    std::fs::write(&c, CATALOG).unwrap();
    (e, c)
}

#[test]
fn import_smart_citizen_export() {
    let f = Fake::new();
    write_logs(&f);
    f.setup("LIVE");
    let (e, cat) = import_files(&f);
    let e = e.to_str().unwrap();
    let cat = cat.to_str().unwrap();
    let bp_dir = f.data_dir("LIVE").join("blueprints");

    // Dry run: report, write nothing (not even the log scan).
    let (code, v) = f.json(&[
        "blueprints",
        "import",
        e,
        "--character",
        "Test_Pilot",
        "--dry-run",
        "--test-catalog",
        cat,
    ]);
    assert_eq!(code, 0, "{v}");
    let r = &v["report"];
    assert_eq!(r["imported"].as_array().unwrap().len(), 1, "{v}");
    assert_eq!(
        r["known"].as_array().unwrap().len(),
        1,
        "rifle came from the log"
    );
    assert_eq!(r["unmatched"][0], "Unknown Thing");
    assert_eq!(r["ambiguous"][0]["candidates"].as_array().unwrap().len(), 2);
    assert_eq!(r["not_completed"], 1);
    assert_eq!(v["written"], false);
    assert!(!bp_dir.exists(), "dry run wrote {bp_dir:?}");

    // Real import into the main (by geid).
    let (code, v) = f.json(&[
        "blueprints",
        "import",
        e,
        "--character",
        "100001",
        "--test-catalog",
        cat,
    ]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["owned_after"], 4);
    let set: Value =
        serde_json::from_slice(&std::fs::read(bp_dir.join("100001.json")).unwrap()).unwrap();
    let rifle = &set["entries"]["fake rifle 'kestrel'"];
    assert_eq!(rifle["favorite"], true);
    let kinds: Vec<&str> = rifle["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, vec!["log", "import:smart-citizen"]);
    assert_eq!(set["entries"]["twin name"]["ambiguous"], true);
    // The alt is untouched.
    let alt: Value =
        serde_json::from_slice(&std::fs::read(bp_dir.join("200002.json")).unwrap()).unwrap();
    assert_eq!(alt["entries"].as_object().unwrap().len(), 1);

    // Again: all known, nothing deleted.
    let (_, v) = f.json(&[
        "blueprints",
        "import",
        e,
        "--character",
        "100001",
        "--test-catalog",
        cat,
    ]);
    assert_eq!(v["report"]["known"].as_array().unwrap().len(), 3);
    assert_eq!(v["owned_after"], 4);

    // Unknown character: usage error. Unsupported version: refused.
    let o = f.run(&[
        "blueprints",
        "import",
        e,
        "--character",
        "Nobody",
        "--test-catalog",
        cat,
    ]);
    assert_eq!(o.status.code(), Some(2), "{o:?}");
    let v2 = f.base.join("v2.json");
    std::fs::write(&v2, r#"{"version": 2, "blueprints": []}"#).unwrap();
    let o = f.run(&[
        "blueprints",
        "import",
        v2.to_str().unwrap(),
        "--character",
        "100001",
    ]);
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("version 2 is not supported"));

    // No catalog (GameData has no blueprints yet): refused without --dry-run, nothing written.
    let before = std::fs::read(bp_dir.join("100001.json")).unwrap();
    let o = f.run(&["blueprints", "import", e, "--character", "100001"]);
    assert_eq!(o.status.code(), Some(1), "{o:?}");
    assert_eq!(std::fs::read(bp_dir.join("100001.json")).unwrap(), before);
}

const PRE: &str = "#!/usr/bin/env bash\n# user's pre-launch script\nWINEPREFIX=\"${WINEPREFIX:-$HOME/games/sc}\"\necho \"user: prelaunch\"\nexit 0\n";
const POST: &str = "#!/usr/bin/env bash\necho \"user: postexit\"\n";

fn scripts(f: &Fake) -> (PathBuf, PathBuf) {
    // Like the owner's machine: the prefix holds links into a separate repo.
    let repo = f.base.join("sc-setup/prefix");
    std::fs::create_dir_all(&repo).unwrap();
    let pre = repo.join("sc-prelaunch.sh");
    let post = repo.join("sc-postexit.sh");
    std::fs::write(&pre, PRE).unwrap();
    std::fs::write(&post, POST).unwrap();
    for p in [&pre, &post] {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink(p, f.prefix.join(p.file_name().unwrap())).unwrap();
    }
    (pre, post)
}

fn is_link(p: &Path) -> bool {
    std::fs::symlink_metadata(p)
        .unwrap()
        .file_type()
        .is_symlink()
}

#[test]
fn hook_install_status_uninstall() {
    let f = Fake::new();
    let (pre, post) = scripts(&f);
    let prefix = f.prefix.to_str().unwrap();
    let (code, v) = f.json(&["hook", "status", "--prefix", prefix]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["scripts"][0]["result"], "not-installed");
    assert_eq!(v["scripts"][0]["link"], true);

    let (code, v) = f.json(&["hook", "install", "--prefix", prefix, "--bin", BIN]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["scripts"][0]["result"], "installed");
    assert_eq!(v["scripts"][1]["result"], "installed");
    assert!(Path::new(v["scripts"][0]["backup"].as_str().unwrap()).is_file());
    let text = std::fs::read_to_string(&pre).unwrap();
    assert!(text.starts_with("#!/usr/bin/env bash\n# user's pre-launch script\n"));
    assert!(
        text.ends_with("# <<< starorg-strings hook <<<\nexit 0\n"),
        "{text}"
    );
    assert!(text.contains(" hook prelaunch --game-dir "));
    assert!(
        std::fs::read_to_string(&post)
            .unwrap()
            .contains(" hook postexit ")
    );
    assert!(is_link(&f.prefix.join("sc-prelaunch.sh")));
    {
        use std::os::unix::fs::PermissionsExt;
        let m = std::fs::metadata(&pre).unwrap().permissions().mode() & 0o777;
        assert_eq!(m, 0o755);
    }
    let (_, v) = f.json(&["hook", "status", "--prefix", prefix, "--bin", BIN]);
    assert_eq!(v["scripts"][0]["result"], "installed", "{v}");
    let (_, v) = f.json(&["hook", "install", "--prefix", prefix, "--bin", BIN]);
    assert_eq!(v["scripts"][0]["result"], "already-installed");
    // Different options: updated in place, still one block.
    let (_, v) = f.json(&[
        "hook", "install", "--prefix", prefix, "--bin", BIN, "--notify",
    ]);
    assert_eq!(v["scripts"][0]["result"], "updated");
    let text = std::fs::read_to_string(&pre).unwrap();
    assert_eq!(text.matches("# >>> starorg-strings hook >>>").count(), 1);
    assert!(text.contains("--notify || true"));

    let (code, v) = f.json(&["hook", "uninstall", "--prefix", prefix]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(std::fs::read_to_string(&pre).unwrap(), PRE);
    assert_eq!(std::fs::read_to_string(&post).unwrap(), POST);
    let (_, v) = f.json(&["hook", "uninstall", "--prefix", prefix]);
    assert_eq!(v["scripts"][0]["result"], "not-installed");
}

#[test]
fn hook_refuses_game_folders_and_missing_scripts() {
    let f = Fake::new();
    let inside = f.prefix.join("drive_c/sc-prelaunch.sh");
    std::fs::write(&inside, PRE).unwrap();
    let in_game = f.lib.join("LIVE/sc-postexit.sh");
    std::fs::write(&in_game, POST).unwrap();
    let (code, v) = f.json(&[
        "hook",
        "install",
        "--prefix",
        f.prefix.to_str().unwrap(),
        "--prelaunch-script",
        inside.to_str().unwrap(),
        "--postexit-script",
        in_game.to_str().unwrap(),
    ]);
    assert_eq!(code, 1, "{v}");
    assert_eq!(v["scripts"][0]["result"], "refused");
    assert_eq!(v["scripts"][1]["result"], "refused");
    assert_eq!(std::fs::read_to_string(&inside).unwrap(), PRE);
    assert_eq!(std::fs::read_to_string(&in_game).unwrap(), POST);
    // Nothing there: exit 4, nothing created.
    let empty = f.base.join("empty-prefix");
    std::fs::create_dir_all(&empty).unwrap();
    let (code, _) = f.json(&["hook", "install", "--prefix", empty.to_str().unwrap()]);
    assert_eq!(code, 4);
    assert_eq!(std::fs::read_dir(&empty).unwrap().count(), 0);
}

#[test]
fn installed_block_runs_from_the_script() {
    let f = Fake::new();
    let (pre, _) = scripts(&f);
    write_logs(&f);
    f.setup("LIVE");
    let o = f
        .cmd(&[
            "hook",
            "install",
            "--prefix",
            f.prefix.to_str().unwrap(),
            "--bin",
            BIN,
        ])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(0), "{o:?}");
    // The block passes --game-dir but not the hidden stock-file hook, so the apply tries a
    // real extraction of the fake Data.p4k and fails: the script must still go on and exit 0.
    let out = Command::new("bash")
        .arg(&pre)
        .env("WINEPREFIX", &f.prefix)
        .env("HOME", f.base.join("home"))
        .env("XDG_CONFIG_HOME", f.base.join("home/config"))
        .env("XDG_DATA_HOME", f.base.join("home/data"))
        .env("XDG_CACHE_HOME", f.base.join("home/cache"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stdout.contains("user: prelaunch"), "{stdout}");
    assert!(stderr.contains("(non-fatal)"), "{stderr}");
}

#[test]
fn game_log_import_off_reads_no_log_in_scan_or_postexit() {
    let f = Fake::new();
    write_logs(&f);
    f.setup("LIVE");
    let (code, v) = f.json(&["sync", "status"]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["environment"], "production");
    assert_eq!(v["host"], "starorg.tools");
    assert_eq!(v["sync"]["import_game_log"], true);
    assert_eq!(v["sync"]["push_blueprints"], false);

    let (code, v) = f.json(&["sync", "set", "--import-game-log", "off"]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["sync"]["import_game_log"], false);
    let (code, v) = f.json(&["blueprints", "scan", "--channel", "LIVE"]);
    assert_eq!(code, 0, "{v}");
    let text = v.to_string();
    assert!(text.contains("\"disabled\":true"), "{v}");
    let (code, v) = f.json(&["hook", "postexit", "--timeout", "30"]);
    assert_eq!(code, 0, "{v}");
    for s in v["scans"].as_array().unwrap() {
        assert_eq!(s["scan"]["disabled"], true, "{v}");
        assert_eq!(s["scan"]["files_read"], 0, "{v}");
    }
    assert_eq!(v["sync_pending"], false);
    let bp = f.data_dir("LIVE").join("blueprints");
    assert!(!bp.join("characters.json").exists() && !bp.join("scan.json").exists());

    // Back on: the same logs are read.
    f.json(&["sync", "set", "--import-game-log", "on"]);
    let (_, v) = f.json(&["blueprints", "scan", "--channel", "LIVE"]);
    assert!(!v.to_string().contains("\"disabled\":true"), "{v}");
    assert!(bp.join("characters.json").exists());
}

#[test]
fn sync_set_takes_starorg_env_and_postexit_leaves_the_app_a_note() {
    let f = Fake::new();
    f.setup("LIVE");
    let o = f
        .cmd(&["sync", "set", "--auto", "on", "--push", "on", "--json"])
        .env("STARORG_ENV", "dev")
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(0), "{o:?}");
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["environment"], "dev");
    assert_eq!(v["host"], "dev.starorg.tools");
    let o = f
        .cmd(&["sync", "set", "--json"])
        .env("STARORG_ENV", "staging")
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(2), "{o:?}");
    let (code, v) = f.json(&["hook", "postexit", "--timeout", "30"]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["sync_pending"], true);
    let note = std::fs::read_to_string(
        f.base
            .join("home/data/starorg-strings/starorg-sync-pending.txt"),
    )
    .unwrap();
    assert!(note.lines().any(|l| l == "LIVE"), "{note}");
}

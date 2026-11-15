//! Apply tests (spec 13, "Apply tests" and "CLI tests") against a temp tree shaped like an
//! install inside a Wine prefix: `Data/` and `USER.cfg` casing, `USER.cfg` as a symlink
//! shared by LIVE and HOTFIX, a PTU that is not set up. The stock text comes from a small
//! synthetic file through the hidden `--test-stock-ini` hook; no game install is needed.
//!
//! Unix only for now: the tree uses symlinks, which need extra rights on Windows.
#![cfg(unix)]

mod common;

use common::*;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[test]
fn setup_apply_keeps_casing_symlink_and_bytes() {
    let f = Fake::new();
    let cfg_before = std::fs::read(f.shared_cfg()).unwrap();
    f.setup("LIVE");
    let (code, v) = f.json(&["apply", "--channel", "LIVE"]);
    assert_eq!(code, 0, "{v}");
    let r = &v["channels"][0];
    assert_eq!(r["result"], "applied");
    assert_eq!(r["user_cfg"]["changed"], false);
    assert_eq!(r["user_cfg"]["shared_with"][0], "HOTFIX");
    assert!(r["backup"].as_str().unwrap().ends_with("-pre-existing"));
    // No enhancements yet: output is stock, byte for byte; Data/ casing kept (no data/).
    assert_eq!(
        std::fs::read(f.ini("LIVE")).unwrap(),
        stock_bytes("Star Citizen Alpha 4.10")
    );
    let names: Vec<_> = std::fs::read_dir(f.lib.join("LIVE"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert!(!names.iter().any(|n| n == "data"), "{names:?}");
    // USER.cfg still a link, target byte-identical (the line was already right).
    assert!(is_link(&f.lib.join("LIVE/USER.cfg")));
    assert_eq!(std::fs::read(f.shared_cfg()).unwrap(), cfg_before);
    assert_eq!(f.status_of("LIVE"), "applied");
    // Second run with --if-stale is a no-op.
    let (code, v) = f.json(&["apply", "--channel", "LIVE", "--if-stale"]);
    assert_eq!(code, 0);
    assert_eq!(v["channels"][0]["result"], "up-to-date");
}

/// Rewrite `applied.json` of a channel with `f`.
fn edit_applied(f: &Fake, ch: &str, edit: impl FnOnce(&mut serde_json::Map<String, Value>)) {
    let p = f.data_dir(ch).join("applied.json");
    let mut v: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    edit(v.as_object_mut().unwrap());
    std::fs::write(&p, serde_json::to_vec_pretty(&v).unwrap()).unwrap();
}

fn status_json(f: &Fake, ch: &str) -> Value {
    let (_, v) = f.json(&["status", "--channel", ch]);
    v["channels"][0].clone()
}

#[test]
fn app_update_alone_is_not_stale_but_an_output_version_bump_is() {
    let f = Fake::new();
    f.setup("LIVE");
    let (code, v) = f.json(&["apply", "--channel", "LIVE"]);
    assert_eq!(code, 0, "{v}");
    let a: Value =
        serde_json::from_slice(&std::fs::read(f.data_dir("LIVE").join("applied.json")).unwrap())
            .unwrap();
    assert_eq!(a["output_version"], starorg_core::OUTPUT_VERSION);

    // Written by another app version, same generator: still applied.
    edit_applied(&f, "LIVE", |a| {
        a.insert("app_version".into(), "0.0.1".into());
    });
    let st = status_json(&f, "LIVE");
    assert_eq!(st["state"], "applied", "{st}");

    // Written by an older generator: stale, for that reason alone.
    edit_applied(&f, "LIVE", |a| {
        a.insert(
            "output_version".into(),
            (starorg_core::OUTPUT_VERSION - 1).into(),
        );
    });
    let st = status_json(&f, "LIVE");
    assert_eq!(st["state"], "stale", "{st}");
    assert_eq!(st["reasons"], serde_json::json!(["enhancer update"]));
    let (code, v) = f.json(&["apply", "--channel", "LIVE", "--if-stale"]);
    assert_eq!((code, &v["channels"][0]["result"]), (0, &"applied".into()));
    assert_eq!(f.status_of("LIVE"), "applied");
}

#[test]
fn manifest_from_0_1_0_reads_as_applied() {
    let f = Fake::new();
    f.setup("LIVE");
    let (code, v) = f.json(&["apply", "--channel", "LIVE"]);
    assert_eq!(code, 0, "{v}");
    // What 0.1.0 wrote: no output_version, the profile hash of the TOML text.
    let data = f.data_dir("LIVE");
    let profile =
        starorg_core::state::Profile::load_file(&starorg_core::state::Profile::path(&data))
            .unwrap();
    assert_ne!(profile.hash(), profile.legacy_hash());
    edit_applied(&f, "LIVE", |a| {
        a.remove("output_version");
        a.insert("app_version".into(), "0.1.0".into());
        a.insert("profile_hash".into(), profile.legacy_hash().into());
    });
    let st = status_json(&f, "LIVE");
    assert_eq!(st["state"], "applied", "{st}");
    // A real settings change still shows against the old record.
    std::fs::write(
        starorg_core::state::Profile::path(&data),
        "schema = 1\npreset = \"recommended\"\n[patches]\ndisabled = []\n[enhance]\nmissions = false\n",
    )
    .unwrap();
    let st = status_json(&f, "LIVE");
    assert_eq!(st["reasons"], serde_json::json!(["profile"]), "{st}");
}

#[test]
fn overrides_change_values_and_keep_flag_suffixes_and_crlf() {
    let f = Fake::new();
    f.setup("LIVE");
    std::fs::write(
        f.data_dir("LIVE").join("overrides.ini"),
        "; mine\nitem_NameA=*05-Filler Alpha\nITEM_DESCA=Mine ~mission(Location) <EM4>open tail\nfmt_key=Deals %i dmg 5%\nnot_a_key=x\n",
    )
    .unwrap();
    let (code, v) = f.json(&["apply", "--channel", "LIVE"]);
    assert_eq!(
        code, 5,
        "dropped override and unknown key are warnings: {v}"
    );
    let r = &v["channels"][0];
    assert_eq!(r["counts"]["overrides"], 2);
    assert_eq!(r["counts"]["dropped"], 1);
    let out = String::from_utf8(std::fs::read(f.ini("LIVE")).unwrap()).unwrap();
    assert!(out.contains("\r\nitem_NameA=*05-Filler Alpha\r\n"));
    assert!(out.contains("\r\nitem_DescA,P=Mine ~mission(Location) <EM4>open tail\r\n"));
    assert!(
        out.contains("\r\nfmt_key=Deals %i filler\r\n"),
        "bad override dropped"
    );
    let stock = String::from_utf8(stock_bytes("Star Citizen Alpha 4.10")).unwrap();
    assert_eq!(out.matches(",P=").count(), stock.matches(",P=").count());
    assert_eq!(out.matches("\r\n").count(), stock.matches("\r\n").count());
    assert!(out.as_bytes().starts_with(BOM));
    // Changing overrides makes the channel stale.
    std::fs::write(
        f.data_dir("LIVE").join("overrides.ini"),
        "item_NameA=Other\n",
    )
    .unwrap();
    assert_eq!(f.status_of("LIVE"), "stale");
}

#[test]
fn ptu_not_set_up_is_exit_6_and_all_skips_it() {
    let f = Fake::new();
    let (code, v) = f.json(&["apply", "--channel", "PTU"]);
    assert_eq!(code, 6);
    assert_eq!(v["channels"][0]["result"], "needs-setup");
    assert!(!f.lib.join("PTU/Data/Localization").exists());
    assert!(!f.lib.join("PTU/USER.cfg").exists());
    f.setup("LIVE");
    f.setup("HOTFIX");
    let (code, v) = f.json(&["apply", "--all"]);
    assert_eq!(code, 0, "{v}");
    let ch: Vec<(&str, &str)> = v["channels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["channel"].as_str().unwrap(),
                c["result"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        ch,
        vec![
            ("LIVE", "applied"),
            ("HOTFIX", "applied"),
            ("PTU", "needs-setup")
        ]
    );
    assert!(!f.ini("PTU").exists());
}

#[test]
fn missing_line_is_appended_and_remove_keeps_hand_added_lines() {
    let f = Fake::new();
    let before = "r_width = 2560\r\nr_height = 1440\r\n";
    std::fs::write(f.shared_cfg(), before).unwrap();
    f.setup("LIVE");
    f.setup("HOTFIX");
    let (code, v) = f.json(&["apply", "--channel", "LIVE"]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["channels"][0]["user_cfg"]["changed"], true);
    let (code, _) = f.json(&["apply", "--channel", "HOTFIX"]);
    assert_eq!(code, 0);
    let now = std::fs::read_to_string(f.shared_cfg()).unwrap();
    assert_eq!(now, format!("{before}g_language = english\r\n"));
    assert!(is_link(&f.lib.join("LIVE/USER.cfg")) && is_link(&f.lib.join("HOTFIX/USER.cfg")));
    // The user adds a line between apply and remove.
    std::fs::write(f.shared_cfg(), format!("{now}sys_MaxFPS = 144\r\n")).unwrap();
    // Removing LIVE while HOTFIX is applied leaves the shared line.
    let (code, v) = f.json(&["remove", "--channel", "LIVE"]);
    assert_eq!(code, 0, "{v}");
    assert!(!f.ini("LIVE").exists());
    assert!(
        std::fs::read_to_string(f.shared_cfg())
            .unwrap()
            .contains("g_language = english")
    );
    let (code, _) = f.json(&["remove", "--channel", "HOTFIX"]);
    assert_eq!(code, 0);
    assert_eq!(
        std::fs::read_to_string(f.shared_cfg()).unwrap(),
        format!("{before}sys_MaxFPS = 144\r\n")
    );
    assert!(is_link(&f.lib.join("LIVE/USER.cfg")));
    assert_eq!(f.status_of("LIVE"), "not-applied");
}

/// Two channels sharing one USER.cfg apply at the same time: the slower run must re-read
/// the file under the cfg lock, see the line the other run appended, and join its ledger
/// instead of overwriting both the file and the ledger with what it read earlier.
#[test]
fn concurrent_applies_on_a_shared_user_cfg_keep_both_on_the_ledger() {
    let f = Fake::new();
    let before = "r_width = 2560\r\n";
    std::fs::write(f.shared_cfg(), before).unwrap();
    f.setup("LIVE");
    f.setup("HOTFIX");
    // LIVE reads USER.cfg (no language line), then waits before its user.cfg step until the
    // `go` file exists. Signal files, not sleeps, so a slow CI runner cannot reorder the runs.
    let go = f.prefix.join("cfg-go");
    let reached = f.prefix.join("cfg-go.reached");
    let slow = f
        .cmd(&[
            "apply",
            "--channel",
            "LIVE",
            "--json",
            "--test-pause-before-cfg-until",
            go.to_str().unwrap(),
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let t0 = std::time::Instant::now();
    while !reached.exists() {
        assert!(
            t0.elapsed() < std::time::Duration::from_secs(120),
            "LIVE never reached the user.cfg step"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    // Meanwhile HOTFIX appends the line and the user adds one of their own.
    let (code, v) = f.json(&["apply", "--channel", "HOTFIX"]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["channels"][0]["user_cfg"]["changed"], true);
    let mid = std::fs::read_to_string(f.shared_cfg()).unwrap();
    std::fs::write(f.shared_cfg(), format!("{mid}sys_MaxFPS = 144\r\n")).unwrap();
    std::fs::write(&go, b"").unwrap();
    let o = slow.wait_with_output().unwrap();
    assert_eq!(o.status.code(), Some(0), "{o:?}");
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["channels"][0]["user_cfg"]["changed"], false, "{v}");
    assert_eq!(
        std::fs::read_to_string(f.shared_cfg()).unwrap(),
        format!("{before}g_language = english\r\nsys_MaxFPS = 144\r\n"),
        "LIVE must not write the text it read before HOTFIX changed the file"
    );
    // Removing HOTFIX leaves the line LIVE still relies on.
    let (code, v) = f.json(&["remove", "--channel", "HOTFIX"]);
    assert_eq!(code, 0, "{v}");
    assert!(
        std::fs::read_to_string(f.shared_cfg())
            .unwrap()
            .contains("g_language = english"),
        "{v}"
    );
    assert_eq!(f.status_of("LIVE"), "applied");
    let (code, _) = f.json(&["remove", "--channel", "LIVE"]);
    assert_eq!(code, 0);
    assert_eq!(
        std::fs::read_to_string(f.shared_cfg()).unwrap(),
        format!("{before}sys_MaxFPS = 144\r\n")
    );
}

/// A linked global.ini: remove deletes the file behind the link and keeps the link; the
/// next apply writes through the (now dangling) link again instead of failing.
#[test]
fn linked_global_ini_survives_remove_and_reapply() {
    let f = Fake::new();
    f.setup("LIVE");
    let elsewhere = f.base.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let real = elsewhere.join("global.ini");
    std::fs::write(&real, common::stock_bytes("Star Citizen Alpha 4.10")).unwrap();
    let ini = f.ini("LIVE");
    std::fs::create_dir_all(ini.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&real, &ini).unwrap();
    let (code, v) = f.json(&["apply", "--channel", "LIVE"]);
    assert!(code == 0 || code == 5, "{v}");
    assert!(is_link(&ini) && real.exists());
    let (code, v) = f.json(&["remove", "--channel", "LIVE", "--yes"]);
    assert_eq!(code, 0, "{v}");
    assert!(is_link(&ini), "link kept");
    assert!(!real.exists(), "file behind the link deleted");
    assert!(v.to_string().contains("link-kept"), "{v}");
    let (code, v) = f.json(&["apply", "--channel", "LIVE"]);
    assert_eq!(code, 0, "{v}");
    assert!(is_link(&ini) && real.exists());
    assert_eq!(f.status_of("LIVE"), "applied");
}

#[test]
fn foreign_file_is_pinned_and_cross_build_restore_needs_confirmation() {
    let f = Fake::new();
    std::fs::create_dir_all(f.ini("LIVE").parent().unwrap()).unwrap();
    let foreign = stock_bytes("Star Citizen Alpha 4.10 - Smart Citizen v2");
    std::fs::write(f.ini("LIVE"), &foreign).unwrap();
    assert_eq!(f.status_of("LIVE"), "changed-by-another-tool");
    f.setup("LIVE");
    let (code, v) = f.json(&["apply", "--channel", "LIVE"]);
    assert_eq!(code, 0, "{v}");
    let pinned = v["channels"][0]["backup"].as_str().unwrap().to_string();
    let (_, b) = f.json(&["backups", "--channel", "LIVE"]);
    let m = &b["backups"][0];
    assert_eq!(m["id"], pinned);
    assert_eq!(m["pinned"], true);
    assert_eq!(m["manifest"]["file"]["origin"], "foreign");
    assert_eq!(m["manifest"]["file"]["foreign_tool"], "Smart Citizen");
    // The game is patched: newer change number.
    std::fs::write(
        f.lib.join("LIVE/build_manifest.id"),
        manifest("sc-alpha-4.10.0", 13200000, "4.10.300.1"),
    )
    .unwrap();
    let ours = std::fs::read(f.ini("LIVE")).unwrap();
    let (code, v) = f.json(&["restore", "--channel", "LIVE", "--backup", &pinned]);
    assert_eq!(code, 6, "{v}");
    assert_eq!(
        std::fs::read(f.ini("LIVE")).unwrap(),
        ours,
        "nothing changed"
    );
    let (code, v) = f.json(&[
        "restore",
        "--channel",
        "LIVE",
        "--backup",
        &pinned,
        "--allow-build-mismatch",
    ]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(std::fs::read(f.ini("LIVE")).unwrap(), foreign);
    assert_eq!(f.status_of("LIVE"), "changed-by-another-tool");
}

#[test]
fn kill_between_write_and_record_then_status_shows_applied() {
    let f = Fake::new();
    f.setup("LIVE");
    let mut child = f
        .cmd(&[
            "apply",
            "--channel",
            "LIVE",
            "--test-pause-after-write-ms",
            "20000",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut err = child.stderr.take().unwrap();
    let mut buf = Vec::new();
    let mut chunk = [0u8; 256];
    loop {
        let n = std::io::Read::read(&mut err, &mut chunk).unwrap();
        assert!(
            n > 0,
            "apply ended before the pause: {}",
            String::from_utf8_lossy(&buf)
        );
        buf.extend_from_slice(&chunk[..n]);
        if String::from_utf8_lossy(&buf).contains("test pause after write") {
            break;
        }
    }
    child.kill().unwrap(); // SIGKILL
    child.wait().unwrap();
    assert!(f.data_dir("LIVE").join("intent.json").exists());
    assert!(!f.data_dir("LIVE").join("applied.json").exists());
    assert_eq!(f.status_of("LIVE"), "applied");
    assert!(!f.data_dir("LIVE").join("intent.json").exists());
}

#[test]
fn game_running_is_exit_3_and_changes_nothing() {
    let f = Fake::new();
    f.setup("LIVE");
    f.setup("HOTFIX");
    let mut game = Command::new("bash")
        .arg("-c")
        .arg("exec -a 'C:\\Program Files\\Roberts Space Industries\\StarCitizen\\LIVE\\Bin64\\StarCitizen.exe' sleep 30")
        .env("WINEPREFIX", &f.prefix)
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    let (code, v) = f.json(&["apply", "--channel", "LIVE"]);
    let (code_h, _) = f.json(&["apply", "--channel", "HOTFIX"]);
    game.kill().unwrap();
    game.wait().unwrap();
    assert_eq!(code, 3, "{v}");
    assert_eq!(v["channels"][0]["result"], "skipped");
    assert!(!f.ini("LIVE").exists());
    assert_eq!(code_h, 0, "a LIVE game does not block HOTFIX");
    let (code, _) = f.json(&["apply", "--channel", "LIVE"]);
    assert_eq!(code, 0);
}

#[test]
fn other_language_needs_decision_then_switch_and_undo() {
    let f = Fake::new();
    let before = "g_language = german_(germany) ; mine\r\nr_width = 1\r\n";
    std::fs::write(f.shared_cfg(), before).unwrap();
    f.setup("LIVE");
    let (code, v) = f.json(&["apply", "--channel", "LIVE"]);
    assert_eq!(code, 6, "{v}");
    assert_eq!(v["channels"][0]["result"], "needs-decision");
    assert_eq!(std::fs::read_to_string(f.shared_cfg()).unwrap(), before);
    let o = f.run(&[
        "setup",
        "--channel",
        "LIVE",
        "--preset",
        "recommended",
        "--switch-language",
        "--yes",
    ]);
    assert_eq!(o.status.code(), Some(0));
    let (code, _) = f.json(&["apply", "--channel", "LIVE"]);
    assert_eq!(code, 0);
    assert_eq!(
        std::fs::read_to_string(f.shared_cfg()).unwrap(),
        "g_language = english\r\nr_width = 1\r\n"
    );
    let (code, _) = f.json(&["remove", "--channel", "LIVE"]);
    assert_eq!(code, 0);
    assert_eq!(std::fs::read_to_string(f.shared_cfg()).unwrap(), before);
}

#[test]
fn lock_timeout_and_dry_run() {
    let f = Fake::new();
    f.setup("LIVE");
    // Dry run writes nothing.
    let (code, v) = f.json(&["apply", "--channel", "LIVE", "--dry-run"]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["channels"][0]["result"], "dry-run");
    assert!(!f.ini("LIVE").exists());
    // Timeout 0: skipped, nothing changed.
    let (code, v) = f.json(&["apply", "--channel", "LIVE", "--timeout", "0"]);
    assert_eq!(code, 3, "{v}");
    assert!(!f.ini("LIVE").exists());
    // Lock held by another process.
    let data = f.data_dir("LIVE");
    let id = data
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let lock = f
        .base
        .join(format!("home/data/starorg-strings/locks/{id}-LIVE.lock"));
    std::fs::create_dir_all(lock.parent().unwrap()).unwrap();
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock)
        .unwrap();
    file.lock().unwrap();
    let (code, v) = f.json(&["apply", "--channel", "LIVE"]);
    assert_eq!(code, 3, "{v}");
    drop(file);
    let (code, _) = f.json(&["apply", "--channel", "LIVE"]);
    assert_eq!(code, 0, "a leftover lock file never blocks");
}

#[test]
fn moved_install_relinks_and_our_file_is_not_foreign() {
    let f = Fake::new();
    f.setup("LIVE");
    let (code, _) = f.json(&["apply", "--channel", "LIVE"]);
    assert_eq!(code, 0);
    let old_data = f.data_dir("LIVE");
    let moved = f.base.join("moved-prefix");
    std::fs::rename(&f.prefix, &moved).unwrap();
    let lib = moved.join("drive_c/Program Files/Roberts Space Industries/StarCitizen");
    let run = |args: &[&str]| {
        let mut c = f.cmd_in(&lib, args);
        c.arg("--json");
        c.output().unwrap()
    };
    let o = run(&["apply", "--channel", "LIVE", "--if-stale"]);
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(
        o.status.code(),
        Some(0),
        "{v} {}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert_eq!(
        v["channels"][0]["result"], "up-to-date",
        "same id, our own file recognised"
    );
    assert_eq!(
        f.data_dir("LIVE"),
        old_data,
        "profile and backups follow the install"
    );
}

#[test]
fn remove_foreign_needs_yes_and_backs_it_up() {
    let f = Fake::new();
    std::fs::create_dir_all(f.ini("HOTFIX").parent().unwrap()).unwrap();
    std::fs::write(f.ini("HOTFIX"), stock_bytes("x Smart Citizen")).unwrap();
    // Not set up: never removed.
    let (code, _) = f.json(&["remove", "--channel", "HOTFIX", "--yes"]);
    assert_eq!(code, 6);
    assert!(f.ini("HOTFIX").exists());
    f.setup("HOTFIX");
    let (code, _) = f.json(&["remove", "--channel", "HOTFIX"]);
    assert_eq!(code, 6, "foreign file needs --yes");
    let (code, v) = f.json(&["remove", "--channel", "HOTFIX", "--yes"]);
    assert_eq!(code, 0, "{v}");
    assert!(!f.ini("HOTFIX").exists());
    let (_, b) = f.json(&["backups", "--channel", "HOTFIX"]);
    assert_eq!(b["backups"][0]["manifest"]["file"]["origin"], "foreign");
}

#[test]
fn build_refuses_output_inside_the_wine_prefix() {
    let f = Fake::new();
    let out = f.prefix.join("drive_c/users/someone/x.ini");
    let o = f.run(&["build", "--channel", "LIVE", "--out", out.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(2), "{o:?}");
    assert!(String::from_utf8_lossy(&o.stderr).contains("Wine prefix"));
    assert!(!out.exists());
}

#[test]
fn status_doctor_patches_change_nothing() {
    let f = Fake::new();
    std::fs::write(f.lib.join("LIVE/Data/languages.ini"), "").unwrap();
    std::fs::create_dir_all(f.lib.join("LIVE/Industries")).unwrap();
    let snapshot = |p: &Path| -> Vec<(PathBuf, u64)> { walk(p) };
    let before = snapshot(&f.prefix);
    let o = f.run(&["doctor", "--json"]);
    assert_eq!(o.status.code(), Some(0), "{o:?}");
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    let traces = v["installs"][0]["channels"][0]["other_tool_traces"].to_string();
    assert!(
        traces.contains("languages.ini") && traces.contains("Industries"),
        "{traces}"
    );
    assert_eq!(f.run(&["status"]).status.code(), Some(0));
    assert_eq!(
        f.run(&["patches", "--channel", "LIVE"]).status.code(),
        Some(0)
    );
    assert_eq!(snapshot(&f.prefix), before);
}

fn walk(p: &Path) -> Vec<(PathBuf, u64)> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(p) {
        for e in rd.flatten() {
            let path = e.path();
            let meta = std::fs::symlink_metadata(&path).unwrap();
            out.push((path.clone(), meta.len()));
            if meta.is_dir() {
                out.extend(walk(&path));
            }
        }
    }
    out.sort();
    out
}

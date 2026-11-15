//! CLI tests: exit codes and `--json` shape for `channels` and `build` (spec 8, 13).

use std::path::Path;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_starorg-strings");

fn run(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .env_remove("WINEPREFIX")
        .output()
        .unwrap()
}

fn fake_install(root: &Path) {
    for (name, branch, change, version) in [
        ("LIVE", "sc-alpha-4.10.0", "13165946", "4.10.200.58746"),
        (
            "HOTFIX",
            "sc-alpha-4.10.0-hotfix",
            "12572603",
            "1.0.191.55227",
        ),
    ] {
        let d = root.join(name);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("Data.p4k"), b"not a real archive").unwrap();
        std::fs::write(
            d.join("build_manifest.id"),
            format!(
                r#"{{"Data":{{"Branch":"{branch}","RequestedP4ChangeNum":"{change}","Version":"{version}"}}}}"#
            ),
        )
        .unwrap();
    }
}

#[test]
fn channels_json_shape_and_hotfix_game_version() {
    let t = tempfile::tempdir().unwrap();
    fake_install(t.path());
    let o = run(&[
        "channels",
        "--json",
        "--game-dir",
        t.path().to_str().unwrap(),
    ]);
    assert_eq!(o.status.code(), Some(0), "{o:?}");
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["schema"], 1);
    let ch = &v["installs"][0]["channels"];
    assert_eq!(v["installs"][0]["found_by"], "game-dir");
    assert_eq!(ch[0]["channel"], "LIVE");
    assert_eq!(ch[1]["channel"], "HOTFIX");
    assert_eq!(ch[1]["game_version"], "4.10.0");
    assert_eq!(ch[1]["version"], "1.0.191.55227");
    assert_eq!(ch[1]["change"], 12572603);
}

#[test]
fn channels_human_output() {
    let t = tempfile::tempdir().unwrap();
    fake_install(t.path());
    let o = run(&["--game-dir", t.path().to_str().unwrap(), "channels"]);
    assert_eq!(o.status.code(), Some(0));
    let s = String::from_utf8_lossy(&o.stdout);
    assert!(
        s.contains("HOTFIX         sc-alpha-4.10.0-hotfix     4.10.0   12572603"),
        "{s}"
    );
}

#[test]
fn nothing_found_is_exit_4() {
    let t = tempfile::tempdir().unwrap();
    let o = run(&["channels", "--game-dir", t.path().to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(4));
    let o = run(&[
        "channels",
        "--json",
        "--game-dir",
        t.path().to_str().unwrap(),
    ]);
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["error"]["kind"], "not-found");
}

#[test]
fn unknown_channel_is_exit_4() {
    let t = tempfile::tempdir().unwrap();
    fake_install(t.path());
    let out = t.path().join("x.ini");
    let o = run(&[
        "build",
        "--channel",
        "PTU",
        "--out",
        out.to_str().unwrap(),
        "--game-dir",
        t.path().join(".").to_str().unwrap(),
    ]);
    assert_eq!(o.status.code(), Some(4), "{o:?}");
}

#[test]
fn build_refuses_to_write_into_the_install() {
    let t = tempfile::tempdir().unwrap();
    fake_install(t.path());
    let out = t.path().join("LIVE/Data/Localization/english/global.ini");
    let o = run(&[
        "build",
        "--channel",
        "live",
        "--out",
        out.to_str().unwrap(),
        "--game-dir",
        t.path().to_str().unwrap(),
    ]);
    assert_eq!(o.status.code(), Some(2), "{o:?}");
    assert!(
        !t.path().join("LIVE/Data").exists(),
        "nothing created in the game folder"
    );
}

#[test]
fn build_extraction_failure_is_exit_1_and_writes_nothing() {
    let t = tempfile::tempdir().unwrap();
    let game = t.path().join("game");
    fake_install(&game);
    let out = t.path().join("out/x.ini");
    let o = run(&[
        "build",
        "--json",
        "--channel",
        "LIVE",
        "--out",
        out.to_str().unwrap(),
        "--game-dir",
        game.to_str().unwrap(),
    ]);
    assert_eq!(o.status.code(), Some(1), "{o:?}");
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["error"]["kind"], "extraction");
    assert!(!out.exists());
}

#[test]
fn usage_errors_are_exit_2() {
    assert_eq!(run(&["build"]).status.code(), Some(2));
    assert_eq!(run(&["no-such-command"]).status.code(), Some(2));
    assert_eq!(run(&["--help"]).status.code(), Some(0));
}

/// Local integration: `build --channel LIVE` with no enhancements equals the stock
/// `global.ini` inside LIVE's `Data.p4k`, as read by the extraction itself.
/// Run: `cargo test --release -p starorg-cli --test cli -- --ignored`.
#[test]
#[ignore = "needs a game install"]
fn real_build_equals_stock() {
    let t = tempfile::tempdir().unwrap();
    let out = t.path().join("x.ini");
    let mut args = vec![
        "build",
        "--json",
        "--no-enhancements",
        "--channel",
        "LIVE",
        "--out",
        out.to_str().unwrap(),
    ];
    let game_dir = std::env::var("STARORG_GAME_DIR").ok();
    if let Some(g) = &game_dir {
        args.extend(["--game-dir", g.as_str()]);
    }
    let o = run(&args);
    assert_eq!(o.status.code(), Some(0), "{o:?}");
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["counts"]["changed"], 0);
    let bytes = std::fs::read(&out).unwrap();
    assert!(bytes.starts_with(b"\xEF\xBB\xBF"));
    assert_eq!(bytes.len() as u64, v["bytes"].as_u64().unwrap());
    let ch = v["channel"].as_str().unwrap();
    assert_eq!(ch, "LIVE");
    let x = starorg_data::extract::extract(
        &Path::new(game_dir.as_deref().unwrap_or(
            "/mnt/games/star-citizen/drive_c/Program Files/Roberts Space Industries/StarCitizen",
        ))
        .join("LIVE/Data.p4k"),
    )
    .unwrap();
    assert!(bytes == x.base_ini, "build output differs from stock");
}

/// A Wine prefix with an install inside: `<t>/pfx/{system.reg, drive_c/.../StarCitizen/LIVE}`.
fn fake_prefix(t: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let pfx = t.join("pfx");
    let root = pfx.join("drive_c/Program Files/Roberts Space Industries/StarCitizen");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(pfx.join("system.reg"), b"").unwrap();
    fake_install(&root);
    (pfx, root)
}

/// Commands that write output refuse the whole Wine prefix, not just the install root.
#[test]
fn output_inside_wine_prefix_is_refused() {
    let t = tempfile::tempdir().unwrap();
    let (pfx, root) = fake_prefix(t.path());
    let game_dir = root.to_str().unwrap();
    for out in [
        pfx.join("drive_c/users/someone/out.json"),
        pfx.join("out.ini"),
        root.join("LIVE/x.ini"),
    ] {
        let out = out.to_str().unwrap();
        for cmd in ["build", "gamedata"] {
            let o = run(&[
                cmd,
                "--game-dir",
                game_dir,
                "--channel",
                "LIVE",
                "--out",
                out,
            ]);
            assert_eq!(o.status.code(), Some(2), "{cmd} {out}: {o:?}");
            assert!(
                String::from_utf8_lossy(&o.stderr).contains("Wine prefix"),
                "{o:?}"
            );
        }
        let p4k = root.join("LIVE/Data.p4k");
        let o = run(&[
            "extract-worker",
            "--p4k",
            p4k.to_str().unwrap(),
            "--out",
            out,
        ]);
        assert_eq!(o.status.code(), Some(2), "extract-worker {out}: {o:?}");
    }
    assert!(!pfx.join("out.ini").exists());
    assert!(!pfx.join("drive_c/users").exists());
}

/// Extra report files of `build` get the same guard as `--out`.
#[test]
fn build_report_files_inside_wine_prefix_are_refused() {
    let t = tempfile::tempdir().unwrap();
    let (pfx, root) = fake_prefix(t.path());
    let ok_out = t.path().join("x.ini");
    for flag in ["--coverage-out", "--edits-out"] {
        let bad = pfx.join("drive_c/report.json");
        let o = run(&[
            "build",
            "--game-dir",
            root.to_str().unwrap(),
            "--channel",
            "LIVE",
            "--out",
            ok_out.to_str().unwrap(),
            flag,
            bad.to_str().unwrap(),
        ]);
        assert_eq!(o.status.code(), Some(2), "{flag}: {o:?}");
        assert!(!bad.exists());
    }
    assert!(!ok_out.exists());
}

/// Profile and owned-set problems are usage errors, found before anything is read.
#[test]
fn build_profile_errors_are_exit_2() {
    let t = tempfile::tempdir().unwrap();
    fake_install(t.path());
    let out = t.path().join("x.ini");
    let bad_profile = t.path().join("profile.toml");
    std::fs::write(&bad_profile, "[enhance.mission]\nheader_em = 9\n").unwrap();
    let bad_owned = t.path().join("owned.json");
    std::fs::write(&bad_owned, "42").unwrap();
    let base = [
        "build",
        "--game-dir",
        t.path().to_str().unwrap(),
        "--channel",
        "LIVE",
        "--out",
        out.to_str().unwrap(),
    ];
    for extra in [
        vec!["--preset", "loud"],
        vec!["--profile", bad_profile.to_str().unwrap()],
        vec!["--owned-blueprints", bad_owned.to_str().unwrap()],
        vec!["--preset", "minimal", "--no-enhancements"],
    ] {
        let mut args = base.to_vec();
        args.extend(extra.iter().copied());
        let o = run(&args);
        assert_eq!(o.status.code(), Some(2), "{extra:?}: {o:?}");
    }
    // A key from a newer build is ignored with a warning, not a usage error: this build
    // goes on to read the game (which the fake install then fails, exit 1).
    let newer = t.path().join("newer.toml");
    std::fs::write(&newer, "[enhance.mission]\nsome_future_switch = true\n").unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let out2 = elsewhere.path().join("x.ini");
    let mut args = base.to_vec();
    let last = args.len() - 1;
    args[last] = out2.to_str().unwrap();
    args.extend(["--profile", newer.to_str().unwrap()]);
    let o = run(&args);
    assert_eq!(o.status.code(), Some(1), "{o:?}");
    assert!(!out.exists() && !out2.exists());
}

#[test]
fn gamedata_without_network_feature_refuses_published_data() {
    if cfg!(feature = "published-dumps") {
        return;
    }
    let t = tempfile::tempdir().unwrap();
    fake_install(t.path());
    let out = t
        .path()
        .join("..")
        .join(format!("gd-{}.json", std::process::id()));
    let o = run(&[
        "gamedata",
        "--game-dir",
        t.path().to_str().unwrap(),
        "--channel",
        "LIVE",
        "--out",
        out.to_str().unwrap(),
        "--use-published-data",
        "--cache-dir",
        t.path().join("cache").to_str().unwrap(),
    ]);
    assert_eq!(o.status.code(), Some(2), "{o:?}");
    assert!(!out.exists());
}

#[test]
fn version_prints_build_details_without_network() {
    let out = run(&["version", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["name"], "starorg-strings");
    assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(v["features"]["published_dumps"], false);
    assert_eq!(v["starbreaker_commit"].as_str().unwrap().len(), 40);
    let human = run(&["version"]);
    assert_eq!(human.status.code(), Some(0));
    let text = String::from_utf8(human.stdout).unwrap();
    assert!(
        text.starts_with(&format!("starorg-strings {}\n", env!("CARGO_PKG_VERSION"))),
        "{text}"
    );
    assert!(text.contains("/releases"));
    assert!(run(&["version", "--quiet"]).stdout.is_empty());
}

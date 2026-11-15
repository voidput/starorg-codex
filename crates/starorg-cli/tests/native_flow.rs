//! The whole apply flow on a native (non-Wine) install tree, on every OS: this is the
//! Windows apply test (spec 13, "Apply tests (Linux and Windows CI)"). A plain folder with
//! `LIVE` and `PTU`, a real `USER.cfg` file (NTFS installs have no symlinks), and the stock
//! text from the hidden `--test-stock-ini` hook. No Wine prefix, no `HOME` tricks: the app
//! folders go through `%APPDATA%`/`%LOCALAPPDATA%` on Windows and XDG elsewhere, all inside
//! the temp dir.
//!
//! GitHub's Windows runners are elevated, and apply refuses to run elevated by design. The
//! test build sets `STARORG_TEST_ALLOW_ELEVATED=1`, which only a `test-hooks` build honours
//! (never a release binary; `elevated_refusal_is_the_default` below checks the switch is
//! needed).

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_starorg-strings");
const BOM: &[u8] = b"\xEF\xBB\xBF";

fn stock() -> Vec<u8> {
    let mut v = BOM.to_vec();
    v.extend_from_slice(b"; synthetic stock-shaped file, filler text only\r\n");
    v.extend_from_slice(b"Frontend_PU_Version=Star Citizen Alpha 4.10\r\n");
    v.extend_from_slice(b"item_NameA=Filler Alpha\r\n");
    for i in 0..100 {
        v.extend_from_slice(format!("filler_key_{i:03}=Filler value {i}\r\n").as_bytes());
    }
    v
}

struct Native {
    _t: tempfile::TempDir,
    base: PathBuf,
    lib: PathBuf,
    stock: PathBuf,
}

impl Native {
    fn new() -> Self {
        let t = tempfile::tempdir().unwrap();
        let base = starorg_core::fsutil::canonicalize(t.path()).unwrap();
        let lib = base.join("Roberts Space Industries").join("StarCitizen");
        for (ch, change) in [("LIVE", 13165946u64), ("PTU", 13189690)] {
            let d = lib.join(ch);
            std::fs::create_dir_all(d.join("Data")).unwrap();
            std::fs::write(d.join("Data.p4k"), b"fake archive").unwrap();
            std::fs::write(
                d.join("build_manifest.id"),
                format!(
                    r#"{{"Data":{{"Branch":"sc-alpha-4.10.0","RequestedP4ChangeNum":"{change}","Version":"4.10.200.1"}}}}"#
                ),
            )
            .unwrap();
        }
        // The game writes USER.cfg with CRLF; ours has no language line yet.
        std::fs::write(lib.join("LIVE").join("USER.cfg"), "r_width = 2560\r\n").unwrap();
        let stock_path = base.join("stock.ini");
        std::fs::write(&stock_path, stock()).unwrap();
        Self {
            _t: t,
            base,
            lib,
            stock: stock_path,
        }
    }

    fn cmd(&self, args: &[&str], allow_elevated: bool) -> Command {
        let home = self.base.join("home");
        let mut c = Command::new(BIN);
        c.args(args)
            .arg("--game-dir")
            .arg(&self.lib)
            .arg("--test-stock-ini")
            .arg(&self.stock)
            .arg("--json")
            .env_remove("WINEPREFIX")
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", home.join("config"))
            .env("XDG_DATA_HOME", home.join("data"))
            .env("XDG_CACHE_HOME", home.join("cache"))
            .env("APPDATA", home.join("AppData").join("Roaming"))
            .env("LOCALAPPDATA", home.join("AppData").join("Local"));
        if allow_elevated {
            c.env("STARORG_TEST_ALLOW_ELEVATED", "1");
        } else {
            c.env_remove("STARORG_TEST_ALLOW_ELEVATED");
        }
        c
    }

    fn json(&self, args: &[&str]) -> (i32, Value) {
        let o = self.cmd(args, true).output().unwrap();
        let v = serde_json::from_slice(&o.stdout).unwrap_or_else(|e| {
            panic!(
                "{e}: {}\n{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            )
        });
        (o.status.code().unwrap_or(-1), v)
    }

    fn ini(&self, ch: &str) -> PathBuf {
        self.lib
            .join(ch)
            .join("Data")
            .join("Localization")
            .join("english")
            .join("global.ini")
    }

    fn state(&self, ch: &str) -> String {
        let (_, v) = self.json(&["status", "--channel", ch]);
        v["channels"][0]["state"]
            .as_str()
            .unwrap_or("?")
            .to_string()
    }
}

fn read(p: &Path) -> Vec<u8> {
    std::fs::read(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

#[test]
fn setup_apply_status_remove_on_a_native_install() {
    let f = Native::new();
    let cfg = f.lib.join("LIVE").join("USER.cfg");

    let (code, v) = f.json(&[
        "setup",
        "--channel",
        "LIVE",
        "--preset",
        "recommended",
        "--yes",
    ]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(f.state("LIVE"), "not-applied");

    let (code, v) = f.json(&["apply", "--channel", "LIVE"]);
    assert_eq!(code, 0, "{v}");
    let r = &v["channels"][0];
    assert_eq!(r["result"], "applied", "{v}");
    assert_eq!(r["user_cfg"]["changed"], true, "{v}");
    assert_eq!(
        read(&f.ini("LIVE")),
        stock(),
        "no enhancements: stock bytes"
    );
    // USER.cfg is a plain file that stays a plain file, CRLF kept.
    assert!(
        std::fs::symlink_metadata(&cfg)
            .unwrap()
            .file_type()
            .is_file()
    );
    assert_eq!(
        String::from_utf8(read(&cfg)).unwrap(),
        "r_width = 2560\r\ng_language = english\r\n"
    );
    // The Data/ casing is the one on disk (no second data/ folder).
    let names: Vec<String> = std::fs::read_dir(f.lib.join("LIVE"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names
            .iter()
            .filter(|n| n.eq_ignore_ascii_case("data"))
            .count(),
        1,
        "{names:?}"
    );
    assert_eq!(f.state("LIVE"), "applied");
    assert_eq!(f.state("PTU"), "not-applied", "other channels untouched");
    assert!(!f.ini("PTU").exists());

    let (code, v) = f.json(&["apply", "--channel", "LIVE", "--if-stale"]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["channels"][0]["result"], "up-to-date");

    let (code, v) = f.json(&["remove", "--channel", "LIVE"]);
    assert_eq!(code, 0, "{v}");
    assert!(!f.ini("LIVE").exists());
    assert_eq!(String::from_utf8(read(&cfg)).unwrap(), "r_width = 2560\r\n");
    assert_eq!(f.state("LIVE"), "not-applied");
    // The app folders were used, not the game folder.
    let leftovers: Vec<String> = std::fs::read_dir(f.lib.join("LIVE"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("tmp") || n.starts_with('.'))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn foreign_file_is_backed_up_and_restored() {
    let f = Native::new();
    // Another tool's file is already there.
    let mut foreign = BOM.to_vec();
    foreign.extend_from_slice(b"; Smart Citizen\r\nFrontend_PU_Version=x\r\n");
    std::fs::create_dir_all(f.ini("LIVE").parent().unwrap()).unwrap();
    std::fs::write(f.ini("LIVE"), &foreign).unwrap();
    let (code, v) = f.json(&[
        "setup",
        "--channel",
        "LIVE",
        "--preset",
        "recommended",
        "--yes",
    ]);
    assert_eq!(code, 0, "{v}");
    let (code, v) = f.json(&["apply", "--channel", "LIVE"]);
    assert_eq!(code, 0, "{v}");
    let backup = v["channels"][0]["backup"].as_str().unwrap().to_string();
    assert_eq!(read(&f.ini("LIVE")), stock());
    let (code, v) = f.json(&["restore", "--channel", "LIVE", "--backup", &backup]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(read(&f.ini("LIVE")), foreign);
}

/// Without the test switch, an elevated process is refused (on Windows CI this is the real
/// refusal; elsewhere the test only runs as root, which CI never is).
#[test]
fn elevated_refusal_is_the_default() {
    if !starorg_core::fsutil::is_elevated() {
        eprintln!("not elevated: nothing to check here");
        return;
    }
    let f = Native::new();
    let (code, v) = f.json(&[
        "setup",
        "--channel",
        "LIVE",
        "--preset",
        "recommended",
        "--yes",
    ]);
    assert_eq!(code, 0, "{v}");
    let o = f
        .cmd(&["apply", "--channel", "LIVE"], false)
        .output()
        .unwrap();
    assert_ne!(o.status.code(), Some(0), "{o:?}");
    let text = String::from_utf8_lossy(&o.stdout);
    assert!(text.contains("elevated"), "{text}");
    assert!(!f.ini("LIVE").exists());
}

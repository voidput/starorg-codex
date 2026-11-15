//! Shared fake install for the CLI tests: a Wine-shaped temp tree (see apply.rs).
#![cfg(unix)]
#![allow(dead_code)]

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub const BIN: &str = env!("CARGO_BIN_EXE_starorg-codex");
pub const BOM: &[u8] = b"\xEF\xBB\xBF";

pub struct Fake {
    _t: tempfile::TempDir,
    pub base: PathBuf,
    pub prefix: PathBuf,
    pub lib: PathBuf,
    pub stock: PathBuf,
}

pub fn stock_bytes(version_text: &str) -> Vec<u8> {
    let mut v = BOM.to_vec();
    v.extend_from_slice(b"; synthetic stock-shaped file, filler text only\r\n");
    v.extend_from_slice(format!("Frontend_PU_Version={version_text}\r\n").as_bytes());
    v.extend_from_slice(b"item_NameA=Filler Alpha\r\n");
    v.extend_from_slice(b"item_DescA,P=Filler desc ~mission(Location) <EM4>open tail\r\n");
    v.extend_from_slice(b"fmt_key=Deals %i filler\r\n");
    v.extend_from_slice(b"\r\n");
    for i in 0..200 {
        let flag = if i % 7 == 0 { ",P" } else { "" };
        v.extend_from_slice(format!("filler_key_{i:03}{flag}=Filler value {i}\r\n").as_bytes());
    }
    v
}

pub fn manifest(branch: &str, change: u64, version: &str) -> String {
    format!(
        r#"{{"Data":{{"Branch":"{branch}","RequestedP4ChangeNum":"{change}","Version":"{version}"}}}}"#
    )
}

impl Fake {
    pub fn new() -> Self {
        let t = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(t.path()).unwrap();
        let prefix = base.join("prefix");
        std::fs::create_dir_all(prefix.join("drive_c/users/Public")).unwrap();
        std::fs::write(prefix.join("system.reg"), "").unwrap();
        let lib = prefix.join("drive_c/Program Files/Roberts Space Industries/StarCitizen");
        let shared = prefix.join("shared");
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::write(
            shared.join("USER.cfg"),
            "g_language = english\r\nr_width = 2560\r\n",
        )
        .unwrap();
        for (ch, branch, change, version) in [
            ("LIVE", "sc-alpha-4.10.0", 13165946, "4.10.200.58746"),
            (
                "HOTFIX",
                "sc-alpha-4.10.0-hotfix",
                12572603,
                "1.0.191.55227",
            ),
            ("PTU", "sc-alpha-5.0.0-ptu", 13189690, "5.0.0.1"),
        ] {
            let d = lib.join(ch);
            std::fs::create_dir_all(d.join("Data")).unwrap();
            std::fs::write(d.join("Data.p4k"), b"fake archive").unwrap();
            std::fs::write(
                d.join("build_manifest.id"),
                manifest(branch, change, version),
            )
            .unwrap();
            if ch != "PTU" {
                std::os::unix::fs::symlink("../../../../../shared/USER.cfg", d.join("USER.cfg"))
                    .unwrap();
            }
        }
        let stock = base.join("stock.ini");
        std::fs::write(&stock, stock_bytes("Star Citizen Alpha 4.10")).unwrap();
        Self {
            _t: t,
            base,
            prefix,
            lib,
            stock,
        }
    }

    pub fn cmd(&self, args: &[&str]) -> Command {
        self.cmd_in(&self.lib, args)
    }

    pub fn cmd_in(&self, lib: &Path, args: &[&str]) -> Command {
        let mut c = Command::new(BIN);
        c.args(args)
            .arg("--game-dir")
            .arg(lib)
            .arg("--test-stock-ini")
            .arg(&self.stock)
            .env_remove("WINEPREFIX")
            .env_remove("STARORG_ENV")
            .env("HOME", self.base.join("home"))
            .env("XDG_CONFIG_HOME", self.base.join("home/config"))
            .env("XDG_DATA_HOME", self.base.join("home/data"))
            .env("XDG_CACHE_HOME", self.base.join("home/cache"));
        c
    }

    pub fn run(&self, args: &[&str]) -> Output {
        self.cmd(args).output().unwrap()
    }

    pub fn json(&self, args: &[&str]) -> (i32, Value) {
        let mut a = args.to_vec();
        a.push("--json");
        let o = self.run(&a);
        let v = serde_json::from_slice(&o.stdout).unwrap_or_else(|e| {
            panic!(
                "{e}: {}\n{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            )
        });
        (o.status.code().unwrap_or(-1), v)
    }

    pub fn ini(&self, ch: &str) -> PathBuf {
        self.lib
            .join(ch)
            .join("Data/Localization/english/global.ini")
    }

    pub fn shared_cfg(&self) -> PathBuf {
        self.prefix.join("shared/USER.cfg")
    }

    pub fn setup(&self, ch: &str) {
        let o = self.run(&["setup", "--channel", ch, "--preset", "recommended", "--yes"]);
        assert_eq!(o.status.code(), Some(0), "{o:?}");
    }

    pub fn data_dir(&self, ch: &str) -> PathBuf {
        let cfg = std::fs::read_to_string(self.base.join("home/config/starorg-codex/config.toml"))
            .unwrap();
        let t: toml::Table = cfg.parse().unwrap();
        let id = t["installs"][0]["id"].as_str().unwrap().to_string();
        self.base
            .join("home/data/starorg-codex/channels")
            .join(id)
            .join(ch)
    }

    pub fn status_of(&self, ch: &str) -> String {
        let (_, v) = self.json(&["status", "--channel", ch]);
        v["channels"][0]["state"].as_str().unwrap().to_string()
    }
}

pub fn is_link(p: &Path) -> bool {
    std::fs::symlink_metadata(p)
        .unwrap()
        .file_type()
        .is_symlink()
}

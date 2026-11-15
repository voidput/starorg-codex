//! `dev extract|export-model|export-sounds` (addendum 7.16) on a synthetic Data.p4k (the
//! `--test-p4k` hook): files, folders and globs with and without conversion, the game-folder
//! refusal, model and sound errors, and the asset cache (`--cache-dir`, `--no-cache`).

use starorg_data::assets::synthetic;
use starorg_data::starbreaker::synthetic as dcb;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_starorg-strings");

fn run(home: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .env_remove("WINEPREFIX")
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .output()
        .unwrap()
}

fn json(o: &Output) -> serde_json::Value {
    let s = String::from_utf8_lossy(&o.stdout);
    serde_json::from_str(s.lines().last().unwrap_or("null")).unwrap_or(serde_json::Value::Null)
}

fn p4k(dir: &Path) -> PathBuf {
    let p = dir.join("game").join("Data.p4k");
    synthetic::write(&p).unwrap();
    p
}

#[test]
fn extract_files_folders_and_globs() {
    let t = tempfile::tempdir().unwrap();
    let a = p4k(t.path()).display().to_string();
    let out = t.path().join("out");
    let o = run(
        t.path(),
        &[
            "--json",
            "dev",
            "extract",
            "Data/Textures",
            "Data/Scripts/syn_config.xml",
            "--out",
            out.to_str().unwrap(),
            "--convert",
            "--test-p4k",
            &a,
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v = json(&o);
    assert_eq!(v["files"], 2);
    assert_eq!(v["merged"], 1);
    assert!(out.join("Textures/ships/syn_hull_diff.png").exists());
    let g = t.path().join("glob");
    let o = run(
        t.path(),
        &[
            "--json",
            "dev",
            "extract",
            "Data/**/*.dds*",
            "--out",
            g.to_str().unwrap(),
            "--test-p4k",
            &a,
        ],
    );
    assert!(o.status.success());
    assert_eq!(json(&o)["files"], 2);
    assert!(g.join("Data/Textures/ships/syn_hull_diff.dds.1").exists());
    // Nothing matches; into the game folder.
    let o = run(
        t.path(),
        &[
            "dev",
            "extract",
            "Data/Nope/*",
            "--out",
            g.to_str().unwrap(),
            "--test-p4k",
            &a,
        ],
    );
    assert_eq!(o.status.code(), Some(4), "not found");
    let bad = t.path().join("game").join("x");
    let o = run(
        t.path(),
        &[
            "dev",
            "extract",
            "Data",
            "--out",
            bad.to_str().unwrap(),
            "--test-p4k",
            &a,
        ],
    );
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("game folder"));
    assert!(!bad.exists());
}

#[test]
fn models_and_sounds_report_clean_errors_and_use_the_cache() {
    let t = tempfile::tempdir().unwrap();
    let a = p4k(t.path()).display().to_string();
    let cache = t.path().join("my-cache");
    let item = dcb::guid_text(1, 2);
    // Not an entity: a usage error, nothing written.
    let out = t.path().join("m.glb");
    let o = run(
        t.path(),
        &[
            "dev",
            "export-model",
            &item,
            "--preset",
            "print",
            "--out",
            out.to_str().unwrap(),
            "--test-p4k",
            &a,
        ],
    );
    assert_eq!(
        o.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(String::from_utf8_lossy(&o.stderr).contains("not an entity"));
    assert!(!out.exists());
    // A cached model is copied without reading the archive (even a missing one).
    let key = format!("{}-print-lod0", starorg_core::assetcache::safe_name(&item));
    let c = starorg_core::assetcache::AssetCache::new(&cache, 1 << 30);
    c.prepare("LIVE", "test", starorg_core::assetcache::Kind::Models)
        .unwrap();
    std::fs::write(
        c.path(
            "LIVE",
            "test",
            starorg_core::assetcache::Kind::Models,
            &key,
            "glb",
        ),
        b"glTF-cached",
    )
    .unwrap();
    let o = run(
        t.path(),
        &[
            "--json",
            "dev",
            "export-model",
            &item,
            "--preset",
            "print",
            "--out",
            out.to_str().unwrap(),
            "--cache-dir",
            cache.to_str().unwrap(),
            "--test-p4k",
            &a,
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(json(&o)["cached"], true);
    assert_eq!(std::fs::read(&out).unwrap(), b"glTF-cached");
    // --no-cache reads the archive again (and fails: not an entity).
    let o = run(
        t.path(),
        &[
            "--no-cache",
            "dev",
            "export-model",
            &item,
            "--preset",
            "print",
            "--out",
            out.to_str().unwrap(),
            "--cache-dir",
            cache.to_str().unwrap(),
            "--test-p4k",
            &a,
        ],
    );
    assert_eq!(o.status.code(), Some(2));
    // Sounds: a synthetic record has none; the list is empty and nothing fails.
    let snd = t.path().join("snd");
    let o = run(
        t.path(),
        &[
            "--json",
            "dev",
            "export-sounds",
            &item,
            "--list",
            "--out",
            snd.to_str().unwrap(),
            "--test-p4k",
            &a,
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(json(&o)["sounds"]["name"], "SYN_Item_002");
    let o = run(
        t.path(),
        &[
            "--json",
            "dev",
            "export-sounds",
            "nope",
            "--out",
            snd.to_str().unwrap(),
            "--test-p4k",
            &a,
        ],
    );
    assert_eq!(o.status.code(), Some(4));
}

//! `dev extract|export-model|export-sounds` (addendum 7.16) on a synthetic Data.p4k (the
//! `--test-p4k` hook): files, folders and globs with and without conversion, the game-folder
//! refusal, model and sound errors, and the asset cache (`--cache-dir`, `--no-cache`).

use starorg_data::assets::synthetic;
use starorg_data::starbreaker::synthetic as dcb;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_starorg-codex");

fn run(home: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .env_remove("WINEPREFIX")
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("APPDATA", home.join("appdata"))
        .env("LOCALAPPDATA", home.join("localappdata"))
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
    let c = starorg_core::assetcache::AssetCache::new(
        cache.join(starorg_core::assetcache::SUBDIR),
        1 << 30,
    );
    c.prepare(
        "LIVE",
        "000000000000000000000000",
        starorg_core::assetcache::Kind::Models,
    )
    .unwrap();
    std::fs::write(
        c.path(
            "LIVE",
            "000000000000000000000000",
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

#[test]
fn a_cache_folder_in_the_game_folder_or_prefix_is_refused_at_use() {
    let t = tempfile::tempdir().unwrap();
    let pfx = t.path().join("pfx");
    let p = pfx
        .join("drive_c")
        .join("Games")
        .join("LIVE")
        .join("Data.p4k");
    synthetic::write(&p).unwrap();
    let a = p.display().to_string();
    let item = dcb::guid_text(1, 2);
    let out = t.path().join("m.glb");
    // --cache-dir inside the channel folder.
    let inside = p.parent().unwrap().join("cache");
    let o = run(
        t.path(),
        &[
            "dev",
            "export-model",
            &item,
            "--out",
            out.to_str().unwrap(),
            "--cache-dir",
            inside.to_str().unwrap(),
            "--test-p4k",
            &a,
        ],
    );
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("never goes into the game folder"));
    assert!(!inside.exists());
    // The setting pointing into the Wine prefix.
    // On Windows there is no Wine prefix and the config lives under %APPDATA%: point the
    // setting into the channel folder there instead.
    let (cfg, bad) = if cfg!(windows) {
        (
            t.path()
                .join("appdata")
                .join("StarOrg Tools")
                .join("Strings")
                .join("config"),
            p.parent().unwrap().join("x"),
        )
    } else {
        (t.path().join("config").join("starorg-codex"), pfx.join("x"))
    };
    std::fs::create_dir_all(&cfg).unwrap();
    std::fs::write(
        cfg.join("config.toml"),
        format!("[dev]\nasset_cache_dir = {:?}\n", bad.display().to_string()),
    )
    .unwrap();
    let o = run(
        t.path(),
        &[
            "dev",
            "export-sounds",
            &item,
            "--out",
            t.path().join("s").to_str().unwrap(),
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
    assert!(!bad.exists());
}

#[test]
fn a_3mf_needs_the_print_preset() {
    let t = tempfile::tempdir().unwrap();
    let a = p4k(t.path()).display().to_string();
    let item = dcb::guid_text(1, 2);
    let out = t.path().join("m.glb");
    let o = run(
        t.path(),
        &[
            "dev",
            "export-model",
            &item,
            "--3mf",
            "--out",
            out.to_str().unwrap(),
            "--test-p4k",
            &a,
        ],
    );
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("--3mf needs --preset print"));
    assert!(!out.exists() && !out.with_extension("3mf").exists());
}

/// A GLB with one triangle 2 m wide, 1 m up (glTF Y-up), the node named `geo_body`.
fn triangle_glb() -> Vec<u8> {
    let mut bin = Vec::new();
    for v in [[0f32, 1.0, 0.0], [2.0, 1.0, 0.0], [0.0, 2.0, 0.0]] {
        for c in v {
            bin.extend_from_slice(&c.to_le_bytes());
        }
    }
    let json = serde_json::json!({
        "asset": {"version": "2.0"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"name": "geo_body", "mesh": 0}],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
        "buffers": [{"byteLength": bin.len()}],
        "bufferViews": [{"buffer": 0, "byteOffset": 0, "byteLength": 36}],
        "accessors": [{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3",
                       "min": [0.0, 1.0, 0.0], "max": [2.0, 2.0, 0.0]}]
    });
    let mut j = serde_json::to_vec(&json).unwrap();
    while !j.len().is_multiple_of(4) {
        j.push(b' ');
    }
    let total = 12 + 8 + j.len() + 8 + bin.len();
    let mut g = Vec::new();
    g.extend_from_slice(b"glTF");
    g.extend_from_slice(&2u32.to_le_bytes());
    g.extend_from_slice(&(total as u32).to_le_bytes());
    g.extend_from_slice(&(j.len() as u32).to_le_bytes());
    g.extend_from_slice(b"JSON");
    g.extend_from_slice(&j);
    g.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    g.extend_from_slice(b"BIN\0");
    g.extend_from_slice(&bin);
    g
}

#[test]
fn print_options_that_do_not_go_together_are_usage_errors() {
    let t = tempfile::tempdir().unwrap();
    let a = p4k(t.path()).display().to_string();
    let item = dcb::guid_text(1, 2);
    let out = t.path().join("m.glb");
    let out = out.to_str().unwrap();
    let cases: &[(&[&str], &str)] = &[
        (&["--split", "--3mf"], "--split needs --solid"),
        (&["--solid", "--3mf"], "--solid needs --fit-mm"),
        (
            &["--solid", "--fit-mm", "100", "--peg", "5", "--3mf"],
            "--peg needs --split",
        ),
        (
            &["--solid", "0.3", "--fit-mm", "100", "--3mf"],
            "0.1, 0.2 or 0.4",
        ),
        (&["--bed", "200x200", "--3mf"], "--bed \"200x200\""),
        (&["--hollow", "x", "--3mf"], "--hollow \"x\""),
        (&["--fit-mm", "100"], "add --stl or --3mf"),
        (
            &["--fit-mm", "100", "--scale-ratio", "72", "--3mf"],
            "cannot be used with",
        ),
    ];
    for (flags, want) in cases {
        let mut args = vec![
            "dev",
            "export-model",
            &item,
            "--preset",
            "print",
            "--out",
            out,
            "--test-p4k",
            &a,
        ];
        args.extend_from_slice(flags);
        let o = run(t.path(), &args);
        let err = String::from_utf8_lossy(&o.stderr);
        assert_eq!(o.status.code(), Some(2), "{flags:?}: {err}");
        assert!(err.contains(want), "{flags:?}: {err}");
    }
    // A Full export takes none of them.
    let o = run(
        t.path(),
        &[
            "dev",
            "export-model",
            &item,
            "--no-weapons",
            "--out",
            out,
            "--test-p4k",
            &a,
        ],
    );
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("need --preset print"));
    // The help says they are experimental.
    let h = run(t.path(), &["dev", "export-model", "--help"]);
    let help = String::from_utf8_lossy(&h.stdout);
    for flag in [
        "--fit-mm",
        "--scale-ratio",
        "--bed",
        "--shrink-to-bed",
        "--drop-small",
        "--solid",
        "--thicken",
        "--hollow",
        "--peg",
        "--split",
        "--multicolor",
        "--simplify",
        "--no-weapons",
    ] {
        let line = help
            .lines()
            .skip_while(|l| !l.trim_start().starts_with(flag))
            .take(2)
            .collect::<String>();
        assert!(line.contains("(experimental)"), "{flag}: {line}");
    }
}

#[test]
fn print_glb_writes_the_stl_and_3mf_of_a_glb_through_the_pipeline() {
    let t = tempfile::tempdir().unwrap();
    let glb = t.path().join("ship.glb");
    std::fs::write(&glb, triangle_glb()).unwrap();
    let dir = t.path().join("out");
    let o = run(
        t.path(),
        &[
            "--json",
            "dev",
            "print-glb",
            glb.to_str().unwrap(),
            "--out",
            dir.to_str().unwrap(),
            "--fit-mm",
            "100",
        ],
    );
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let j = json(&o);
    let kinds: Vec<&str> = j["meshes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["stl", "3mf"]);
    assert!(dir.join("ship.stl").exists() && dir.join("ship.3mf").exists());
    // Scaled to 100 mm and sitting on the bed: z from 0 (it was 1 m up).
    let stl = std::fs::read(dir.join("ship.stl")).unwrap();
    let f = |o: usize| f32::from_le_bytes(stl[o..o + 4].try_into().unwrap());
    let zs = [f(84 + 12 + 8), f(84 + 24 + 8), f(84 + 36 + 8)];
    assert_eq!(zs.iter().copied().fold(f32::INFINITY, f32::min), 0.0);
    // 100 mm wide, centred on the bed.
    let xs = [f(84 + 12), f(84 + 24), f(84 + 36)];
    let (lo, hi) = xs
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), &x| {
            (a.min(x), b.max(x))
        });
    assert!(
        (hi - lo - 100.0).abs() < 1e-3 && (lo + hi).abs() < 1e-3,
        "{xs:?}"
    );
    // Only the 3MF when asked; --no-weapons is for export-model.
    let only = t.path().join("only");
    let o = run(
        t.path(),
        &[
            "dev",
            "print-glb",
            glb.to_str().unwrap(),
            "--out",
            only.to_str().unwrap(),
            "--3mf",
        ],
    );
    assert_eq!(o.status.code(), Some(0));
    assert!(only.join("ship.3mf").exists() && !only.join("ship.stl").exists());
    let o = run(
        t.path(),
        &[
            "dev",
            "print-glb",
            glb.to_str().unwrap(),
            "--out",
            only.to_str().unwrap(),
            "--no-weapons",
        ],
    );
    assert_eq!(o.status.code(), Some(2));
    let o = run(
        t.path(),
        &[
            "dev",
            "print-glb",
            "nope.glb",
            "--out",
            only.to_str().unwrap(),
        ],
    );
    assert_eq!(
        o.status.code(),
        Some(4),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
}

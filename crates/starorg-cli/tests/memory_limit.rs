//! `--memory-limit` / `STARORG_MEMORY_LIMIT_GB` of `dev` and `mcp` (addendum 7.18): bad
//! values are usage errors, a model export estimated above the limit is refused before the
//! archive is read, and `0` lifts the limit.

use starorg_data::assets::synthetic;
use starorg_data::starbreaker::synthetic as dcb;
use std::path::Path;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_starorg-codex");

fn run(home: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut c = Command::new(BIN);
    c.args(args)
        .env_remove("WINEPREFIX")
        .env_remove("STARORG_MEMORY_LIMIT_GB")
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("APPDATA", home.join("appdata"))
        .env("LOCALAPPDATA", home.join("localappdata"));
    for (k, v) in env {
        c.env(k, v);
    }
    c.output().unwrap()
}

fn export(home: &Path, a: &str, out: &Path, extra: &[&str], env: &[(&str, &str)]) -> Output {
    let item = dcb::guid_text(1, 2);
    let mut args = vec!["--json", "dev", "export-model", &item, "--preset", "full"];
    let out = out.to_str().unwrap();
    args.extend(["--out", out, "--no-cache", "--test-p4k", a]);
    args.extend(extra);
    run(home, &args, env)
}

#[test]
fn a_model_over_the_limit_is_refused_up_front() {
    let t = tempfile::tempdir().unwrap();
    let a = t.path().join("game").join("Data.p4k");
    synthetic::write(&a).unwrap();
    let a = a.display().to_string();
    let out = t.path().join("m.glb");
    // Full export (about 6.5 GB) against a 1.5 GB limit: refused, nothing read or written.
    for (extra, env) in [
        (vec!["--memory-limit", "1.5"], vec![]),
        (vec![], vec![("STARORG_MEMORY_LIMIT_GB", "1.5")]),
    ] {
        let o = export(t.path(), &a, &out, &extra, &env);
        let err = String::from_utf8_lossy(&o.stderr);
        assert_eq!(o.status.code(), Some(1), "{err}");
        assert!(err.contains("more than the 1.5 GB memory limit"), "{err}");
        // Says exactly how to raise it: Full at LOD 1 (about 6.5 GB) needs 7.5 GB.
        assert!(err.contains("--memory-limit 7.5"), "{err}");
        let v: serde_json::Value =
            serde_json::from_str(String::from_utf8_lossy(&o.stdout).lines().last().unwrap())
                .unwrap();
        assert_eq!(v["error"]["kind"], "memory-limit");
        assert!(!out.exists());
    }
    // No limit: it gets as far as the archive (a synthetic item is not an entity: exit 2).
    let o = export(t.path(), &a, &out, &["--memory-limit", "0"], &[]);
    assert_eq!(
        o.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(String::from_utf8_lossy(&o.stderr).contains("not an entity"));
}

#[test]
fn bad_values_are_usage_errors() {
    let t = tempfile::tempdir().unwrap();
    let o = run(t.path(), &["dev", "types", "--memory-limit", "lots"], &[]);
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("not a number of GB"));
    let o = run(t.path(), &["mcp", "--memory-limit", "-3"], &[]);
    assert_eq!(o.status.code(), Some(2));
    // An empty STARORG_MEMORY_LIMIT_GB= is unset, not a usage error.
    let o = run(
        t.path(),
        &["dev", "types", "--game-dir", "/nonexistent"],
        &[("STARORG_MEMORY_LIMIT_GB", "")],
    );
    assert_ne!(
        o.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(!String::from_utf8_lossy(&o.stderr).contains("not a number of GB"));
}

/// Read-only check on a real archive (`STARORG_REAL_P4K=<Data.p4k>`, output under
/// `STARORG_REAL_OUT`): with a 1.5 GB limit `game-ships` (about 4.1 GB) is refused before a
/// worker starts and `game-armor` (about 0.9 GB) is extracted under the cap.
#[test]
fn real_archive_heavy_refused_light_extracted() {
    let (Ok(p4k), Ok(out)) = (
        std::env::var("STARORG_REAL_P4K"),
        std::env::var("STARORG_REAL_OUT"),
    ) else {
        eprintln!("skipped: set STARORG_REAL_P4K and STARORG_REAL_OUT");
        return;
    };
    use starorg_mcp::worker::{Extractor, ProcessExtractor};
    let limit = 3 * starorg_core::memlimit::GIB / 2;
    let x = ProcessExtractor::new(BIN.into()).with_limit(std::sync::Arc::new(move || Some(limit)));
    let (p4k, out) = (Path::new(&p4k), Path::new(&out));
    let started = std::time::Instant::now();
    let e = x.extract(p4k, "game-ships", out).unwrap_err();
    eprintln!("game-ships: {e} ({} ms)", started.elapsed().as_millis());
    assert!(e.contains("more than the 1.5 GB memory limit"), "{e}");
    let m = x.extract(p4k, "game-armor", out).unwrap();
    eprintln!("game-armor: {m:?}");
    assert!(m.peak_rss_bytes.is_none_or(|p| p < limit));
}

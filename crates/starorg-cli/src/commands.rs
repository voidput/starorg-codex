//! `channels` and `build`.

use crate::{GlobalOpts, exit};
use serde_json::json;
use starorg_core::discovery::{
    Channel, Discovery, DiscoveryEnv, DiscoveryRequest, Install, discover,
};
use starorg_core::enhance::{
    self, Category,
    owned::{NoneOwned, OwnedBlueprints, OwnedList},
    profile::Profile,
};
use starorg_core::fleet::store::ShipNames;
use starorg_core::ini::StockIni;
use starorg_core::source::local::LocalExtraction;
use starorg_core::source::{Budget, GameSource, SourceError};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

/// JSON output schema version (fields are only ever added within a major version).
pub(crate) const JSON_SCHEMA: u32 = 1;

pub(crate) fn run_discovery(g: &GlobalOpts) -> Discovery {
    let req = DiscoveryRequest {
        game_dir: g.game_dir.clone(),
        prefix: g.prefix.clone(),
    };
    discover(&req, &DiscoveryEnv::from_process())
}

pub(crate) fn fail(g: &GlobalOpts, code: u8, kind: &str, message: &str) -> ExitCode {
    if g.json {
        println!(
            "{}",
            json!({ "schema": JSON_SCHEMA, "error": { "kind": kind, "message": message } })
        );
    }
    eprintln!("starorg-codex: {message}");
    ExitCode::from(code)
}

fn channel_json(c: &Channel) -> serde_json::Value {
    let b = c.build.as_ref();
    json!({
        "channel": c.name,
        "dir": c.dir,
        "branch": b.map(|b| b.branch.as_str()),
        "game_version": b.and_then(|b| b.game_version).map(|v| v.to_string()),
        "change": b.and_then(|b| b.change),
        "version": b.map(|b| b.version.as_str()),
        "build_error": c.build_error,
    })
}

fn install_json(i: &Install) -> serde_json::Value {
    json!({
        "root": i.root,
        "prefix": i.prefix,
        "found_by": i.found_by,
        "channels": i.channels.iter().map(channel_json).collect::<Vec<_>>(),
    })
}

pub fn channels(g: &GlobalOpts) -> ExitCode {
    let d = run_discovery(g);
    if d.installs.is_empty() {
        return fail(
            g,
            exit::NOT_FOUND,
            "not-found",
            "no Star Citizen install found (try --game-dir or --prefix)",
        );
    }
    if g.json {
        let installs: Vec<_> = d.installs.iter().map(install_json).collect();
        println!("{}", json!({ "schema": JSON_SCHEMA, "installs": installs }));
        return ExitCode::from(exit::OK);
    }
    if g.quiet {
        return ExitCode::from(exit::OK);
    }
    for (n, i) in d.installs.iter().enumerate() {
        let default = if n == 0 { " (default)" } else { "" };
        println!("Install{default}: {}", i.root.display());
        print!("  found by: {}", i.found_by.describe());
        match &i.prefix {
            Some(p) => println!(", Wine prefix {}", p.display()),
            None => println!(),
        }
        println!(
            "  {:<14} {:<26} {:<8} {:<10} VERSION",
            "CHANNEL", "BRANCH", "GAME", "CHANGE"
        );
        for c in &i.channels {
            match &c.build {
                Some(b) => println!(
                    "  {:<14} {:<26} {:<8} {:<10} {}",
                    c.name,
                    b.branch,
                    b.game_version
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "unknown".into()),
                    b.change
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "unknown".into()),
                    b.version
                ),
                None => println!(
                    "  {:<14} (no build info: {})",
                    c.name,
                    c.build_error.as_deref().unwrap_or("unknown")
                ),
            }
        }
    }
    ExitCode::from(exit::OK)
}

/// Canonical form of a path that may not exist yet: canonicalise the deepest existing
/// ancestor and append the rest.
pub(crate) fn canonical_lenient(p: &Path) -> PathBuf {
    starorg_core::fsutil::canonical_lenient(p)
}

/// Why an output path must not be written, if it must not: inside the install root, inside
/// the install's Wine prefix (found or derived from a `drive_c` component), or inside any
/// Wine prefix above the output path itself.
pub(crate) fn forbidden_output(out_abs: &Path, install: &Install) -> Option<String> {
    if let Some(hit) = install
        .protected_roots()
        .into_iter()
        .find(|r| out_abs.starts_with(r))
    {
        return Some(if out_abs.starts_with(&install.root) {
            "inside the game install".into()
        } else {
            format!("inside the Wine prefix {}", hit.display())
        });
    }
    starorg_core::discovery::enclosing_wine_prefix(out_abs)
        .map(|p| format!("inside the Wine prefix {}", p.display()))
}

/// Exit with a usage error when `out_abs` lies inside the install or any Wine prefix
/// (commands that write output never write there).
pub(crate) fn refuse_protected(
    g: &GlobalOpts,
    out_abs: &Path,
    install: &Install,
) -> Option<ExitCode> {
    let why = forbidden_output(out_abs, install)?;
    Some(fail(
        g,
        exit::USAGE,
        "usage",
        &format!(
            "--out {} is {why}; output is never written into the game install or a Wine prefix",
            out_abs.display()
        ),
    ))
}

/// Write `bytes` to `path` atomically: temp file in the same directory, fsync, rename.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty());
    if let Some(d) = dir {
        std::fs::create_dir_all(d)?;
    }
    let name = path
        .file_name()
        .ok_or_else(|| std::io::Error::other("output path has no file name"))?;
    let tmp = path.with_file_name(format!(
        ".{}.tmp-{}",
        name.to_string_lossy(),
        std::process::id()
    ));
    let result = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

pub(crate) fn worker_exe() -> std::io::Result<PathBuf> {
    std::env::current_exe()
}

pub struct BuildArgs<'a> {
    pub channel: &'a str,
    pub out: &'a Path,
    pub timeout: Option<u64>,
    pub preset: Option<String>,
    pub profile: Option<PathBuf>,
    pub no_enhancements: bool,
    pub owned_blueprints: Option<PathBuf>,
    pub coverage_out: Option<PathBuf>,
    pub edits_out: Option<PathBuf>,
    pub ship_names: Option<PathBuf>,
}

/// The profile, its name for reports, and warnings for `[enhance]` keys this build does
/// not know (ignored).
type LoadedProfile = (Profile, String, Vec<starorg_core::source::Warning>);

fn load_profile(a: &BuildArgs) -> Result<LoadedProfile, String> {
    if a.no_enhancements {
        return Ok((Profile::off(), "none".into(), Vec::new()));
    }
    if let Some(path) = &a.profile {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let (p, unknown) = Profile::from_toml_checked(&text).map_err(|e| e.to_string())?;
        let w = unknown
            .iter()
            .map(|k| starorg_core::apply::unknown_key_warning(k))
            .collect();
        return Ok((p, path.display().to_string(), w));
    }
    let name = a.preset.as_deref().unwrap_or("recommended");
    Profile::preset(name)
        .map(|p| (p, name.to_string(), Vec::new()))
        .map_err(|e| e.to_string())
}

/// `--ship-names FILE`: validated settings (`None` when not given or off) and the
/// validation warnings (dropped entries), reported with the build's own warnings.
type LoadedShipNames = (Option<ShipNames>, Vec<starorg_core::source::Warning>);

fn load_ship_names(a: &BuildArgs) -> Result<LoadedShipNames, String> {
    let Some(path) = &a.ship_names else {
        return Ok((None, Vec::new()));
    };
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let s = ShipNames::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let (s, warnings, invalid) = s.validated();
    if invalid {
        let why: Vec<&str> = warnings.iter().map(|w| w.detail.as_str()).collect();
        return Err(format!(
            "{}: invalid ship-names table ({})",
            path.display(),
            why.join("; ")
        ));
    }
    Ok((s.enabled.then_some(s), warnings))
}

fn load_owned(a: &BuildArgs) -> Result<Option<OwnedList>, String> {
    let Some(path) = &a.owned_blueprints else {
        return Ok(None);
    };
    let meta =
        std::fs::metadata(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if meta.len() > 16 << 20 {
        return Err(format!("{} is larger than 16 MiB", path.display()));
    }
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    OwnedList::from_json(&text)
        .map(Some)
        .map_err(|e| format!("{}: {e}", path.display()))
}

pub fn build(g: &GlobalOpts, a: &BuildArgs) -> ExitCode {
    let started = Instant::now();
    let (profile, profile_name, profile_warnings) = match load_profile(a) {
        Ok(p) => p,
        Err(e) => return fail(g, exit::USAGE, "usage", &e),
    };
    let owned = match load_owned(a) {
        Ok(o) => o,
        Err(e) => return fail(g, exit::USAGE, "usage", &e),
    };
    let (ship_names, ship_name_warnings) = match load_ship_names(a) {
        Ok(s) => s,
        Err(e) => return fail(g, exit::USAGE, "usage", &e),
    };
    let d = run_discovery(g);
    let Some(install) = d.default_install() else {
        return fail(
            g,
            exit::NOT_FOUND,
            "not-found",
            "no Star Citizen install found (try --game-dir or --prefix)",
        );
    };
    let channel = a.channel;
    let Some(ch) = install.channel(channel) else {
        let names: Vec<_> = install.channels.iter().map(|c| c.name.as_str()).collect();
        return fail(
            g,
            exit::NOT_FOUND,
            "not-found",
            &format!(
                "channel {channel} not found in {} (have: {})",
                install.root.display(),
                names.join(", ")
            ),
        );
    };

    // `build` writes only the given files, never into the game or its Wine prefix.
    let out = a.out;
    let out_abs = canonical_lenient(out);
    if let Some(code) = refuse_protected(g, &out_abs, install) {
        return code;
    }
    for extra in [&a.coverage_out, &a.edits_out].into_iter().flatten() {
        if let Some(code) = refuse_protected(g, &canonical_lenient(extra), install) {
            return code;
        }
    }

    let worker = match worker_exe() {
        Ok(w) => w,
        Err(e) => {
            return fail(
                g,
                exit::ERROR,
                "error",
                &format!("cannot find own binary: {e}"),
            );
        }
    };
    let budget = match a.timeout {
        Some(s) => Budget::with_timeout(Duration::from_secs(s)),
        None => Budget::unlimited(),
    };
    if !g.quiet && !g.json {
        eprintln!("Extracting {} from {} ...", ch.name, ch.p4k.display());
    }
    let mut local = LocalExtraction::new(worker);
    if a.no_enhancements {
        local = local.ini_only();
    }
    let loaded = match local.load(ch, &budget) {
        Ok(l) => l,
        Err(SourceError::Timeout(t)) => {
            return fail(
                g,
                exit::SKIPPED,
                "timeout",
                &format!(
                    "out of time after {:.1} s; nothing written",
                    t.as_secs_f64()
                ),
            );
        }
        Err(e) => return fail(g, exit::ERROR, "extraction", &e.to_string()),
    };

    let stock = StockIni::parse(loaded.base_ini);
    let gen_started = Instant::now();
    let owned_dyn: &dyn OwnedBlueprints = match &owned {
        Some(o) => o,
        None => &NoneOwned,
    };
    let generated = enhance::generate_with(
        &loaded.game_data,
        &stock,
        &profile,
        owned_dyn,
        ship_names.as_ref(),
        None,
    );
    let bytes = match enhance::render(&stock, &generated) {
        Ok(b) => b,
        Err(e) => {
            return fail(
                g,
                exit::ERROR,
                "writer",
                &format!("whole-file validation failed, nothing written: {e}"),
            );
        }
    };
    let gen_ms = gen_started.elapsed().as_millis() as u64;
    if generated.edits.is_empty() && bytes != stock.as_bytes() {
        return fail(
            g,
            exit::ERROR,
            "writer",
            "writer bug: unchanged render differs from stock",
        );
    }
    if let Err(e) = write_atomic(out, &bytes) {
        return fail(
            g,
            exit::ERROR,
            "io",
            &format!("cannot write {}: {e}", out.display()),
        );
    }
    let coverage = &generated.coverage;
    let edits: Vec<&enhance::GenEdit> = generated.edits.values().collect();
    for (path, text) in [
        (&a.coverage_out, serde_json::to_vec_pretty(coverage)),
        (&a.edits_out, serde_json::to_vec_pretty(&edits)),
    ] {
        let Some(c) = path else { continue };
        if let Err(e) = write_atomic(c, &text.unwrap_or_default()) {
            return fail(
                g,
                exit::ERROR,
                "io",
                &format!("cannot write {}: {e}", c.display()),
            );
        }
    }

    let mut warnings = loaded.warnings.clone();
    warnings.extend(profile_warnings);
    warnings.extend(ship_name_warnings);
    warnings.extend(generated.warnings.iter().cloned());
    let dropped = warnings.iter().filter(|w| w.kind == "dropped-edit").count();
    let code = if warnings.is_empty() {
        exit::OK
    } else {
        exit::WARNINGS
    };
    let counts: serde_json::Map<String, serde_json::Value> = Category::ALL
        .iter()
        .map(|c| (c.name().replace('-', "_"), json!(generated.count(*c))))
        .collect();
    let thresholds = coverage.threshold_failures();
    let b = ch.build.as_ref();
    if g.json {
        let mut counts = counts;
        counts.insert("changed".into(), json!(generated.edits.len()));
        counts.insert("dropped".into(), json!(dropped));
        counts.insert(
            "owned_blueprints".into(),
            json!(owned.as_ref().map(|o| o.len()).unwrap_or(0)),
        );
        println!(
            "{}",
            json!({
                "schema": JSON_SCHEMA,
                "channel": ch.name,
                "result": "built",
                "build": {
                    "branch": b.map(|b| b.branch.as_str()),
                    "game_version": b.and_then(|b| b.game_version).map(|v| v.to_string()),
                    "change": b.and_then(|b| b.change),
                    "version": b.map(|b| b.version.as_str()),
                },
                "profile": { "name": profile_name, "hash": profile.hash() },
                "out": out_abs,
                "bytes": bytes.len(),
                "keys": stock.key_count(),
                "counts": counts,
                "coverage": coverage,
                "coverage_threshold_failures": thresholds,
                "origin": loaded.origin,
                "warnings": warnings,
                "duration_ms": started.elapsed().as_millis() as u64,
                "generate_ms": gen_ms,
            })
        );
    } else if !g.quiet {
        for w in &warnings {
            eprintln!("warning: {}: {}", w.kind, w.detail);
        }
        println!(
            "Wrote {} ({} bytes, {} keys, {} changed) for {} {} in {:.1} s (profile {profile_name})",
            out_abs.display(),
            bytes.len(),
            stock.key_count(),
            generated.edits.len(),
            ch.name,
            b.map(|b| b.version.as_str()).unwrap_or("unknown build"),
            started.elapsed().as_secs_f64()
        );
        if !generated.edits.is_empty() {
            println!(
                "  {:<22} {:>9} {:>8} {:>8}  LEFT AS STOCK",
                "CATEGORY", "CANDIDATE", "WRITTEN", "SHARE"
            );
            for (c, v) in &coverage.categories {
                let left: Vec<String> = v
                    .left
                    .iter()
                    .map(|(r, n)| format!("{} {n}", r.name()))
                    .collect();
                println!(
                    "  {:<22} {:>9} {:>8} {:>7.1}%  {}",
                    c.name(),
                    v.candidates,
                    v.written,
                    v.written_share() * 100.0,
                    left.join(", ")
                );
            }
            for t in &thresholds {
                println!("  below M2 threshold: {t}");
            }
        }
    }
    ExitCode::from(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_lenient_handles_missing_tail() {
        let t = tempfile::tempdir().unwrap();
        let real = starorg_core::fsutil::canonicalize(t.path()).unwrap();
        assert_eq!(
            canonical_lenient(&t.path().join("a/b/c.ini")),
            real.join("a/b/c.ini")
        );
    }

    #[test]
    fn write_atomic_creates_parent_and_leaves_no_temp() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("sub/x.ini");
        write_atomic(&p, b"abc").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"abc");
        let names: Vec<_> = std::fs::read_dir(t.path().join("sub"))
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec!["x.ini"]);
    }

    #[test]
    fn ship_names_file_keeps_its_validation_warnings() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("names.toml");
        std::fs::write(
            &p,
            "enabled = true\nstyle = \"numeric\"\n\n[[ship]]\nhull = \"vehicle_NameSYN_Alpha\"\n\n[[ship]]\nhull = \"vehicle_NameSYN_Bravo\"\nname = \"Yacht 100% <b>\"\n",
        )
        .unwrap();
        let out = t.path().join("x.ini");
        let a = BuildArgs {
            channel: "LIVE",
            out: &out,
            timeout: None,
            preset: None,
            profile: None,
            no_enhancements: false,
            owned_blueprints: None,
            coverage_out: None,
            edits_out: None,
            ship_names: Some(p),
        };
        let (s, w) = load_ship_names(&a).unwrap();
        assert!(s.is_some());
        assert!(
            w.iter()
                .any(|w| w.kind == "ship-names" && w.detail.contains('%')),
            "{w:?}"
        );
    }
}

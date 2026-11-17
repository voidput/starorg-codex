//! `dev extract`, `dev export-model`, `dev export-sounds` (addendum 7.16): game files, models
//! and sounds from the command line. Run in this process (the memory goes back when it exits),
//! take `locks/extract.lock` while Data.p4k is read, never write into the install or a Wine
//! prefix. Models and decoded sounds go through the asset cache (`--cache-dir`, else the
//! `[dev] asset_cache_dir` setting, else `<app cache>/assets`), so a second run of the same
//! export does not read the archive; `--no-cache` skips it.

use crate::commands::{JSON_SCHEMA, canonical_lenient, fail, refuse_protected, run_discovery};
use crate::{DevCmd, GlobalOpts, exit};
use serde_json::json;
use starorg_core::assetcache::{self, AssetCache, Kind};
use starorg_core::memlimit;
use starorg_data::assets::audio::{self, SoundRef};
use starorg_data::assets::files;
use starorg_data::assets::model::{self, ModelOptions, ModelPreset};
use starorg_data::assets::wemcodec;
use starorg_data::assets::{AssetArchive, AssetError};
use starorg_data::assetworker;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

/// The channel to read: its archive, the install root (outputs never go there), the cache
/// key of its build, and the lock while the archive is open.
struct Target {
    p4k: PathBuf,
    root: Option<PathBuf>,
    channel: String,
    build: String,
}

fn target(g: &GlobalOpts, channel: &str, test_p4k: Option<PathBuf>) -> Result<Target, ExitCode> {
    if let Some(p) = test_p4k {
        return Ok(Target {
            root: p.parent().map(Path::to_path_buf),
            p4k: p,
            channel: channel.to_string(),
            build: TEST_BUILD.into(),
        });
    }
    let d = run_discovery(g);
    let Some(install) = d.default_install() else {
        return Err(fail(
            g,
            exit::NOT_FOUND,
            "not-found",
            "no Star Citizen install found (try --game-dir or --prefix)",
        ));
    };
    let Some(ch) = install.channel(channel) else {
        return Err(fail(
            g,
            exit::NOT_FOUND,
            "not-found",
            &format!("channel {channel} not found in {}", install.root.display()),
        ));
    };
    let fp = ch.fingerprint().map_err(|e| {
        fail(
            g,
            exit::ERROR,
            "error",
            &format!("{}: {e}", ch.p4k.display()),
        )
    })?;
    let build =
        starorg_core::cache::KeyInputs::local(&fp, starorg_data::starbreaker::STARBREAKER_COMMIT)
            .key();
    Ok(Target {
        p4k: ch.p4k.clone(),
        root: Some(install.root.clone()),
        channel: ch.name.clone(),
        build,
    })
}

/// Refuse outputs inside the install or a Wine prefix (as `dev export-type`).
fn check_out(g: &GlobalOpts, t: &Target, out: &Path, testing: bool) -> Result<PathBuf, ExitCode> {
    let out_abs = canonical_lenient(out);
    if !testing {
        let d = run_discovery(g);
        if let Some(install) = d.default_install()
            && Some(&install.root) == t.root.as_ref()
            && let Some(code) = refuse_protected(g, &out_abs, install)
        {
            return Err(code);
        }
        if starorg_core::discovery::enclosing_wine_prefix(&out_abs).is_some() {
            return Err(fail(
                g,
                exit::USAGE,
                "usage",
                &format!("--out {} is inside a Wine prefix", out_abs.display()),
            ));
        }
    }
    if let Some(r) = &t.root
        && out_abs.starts_with(canonical_lenient(r))
    {
        return Err(fail(
            g,
            exit::USAGE,
            "usage",
            &format!(
                "--out {} is inside the game folder; output is never written there",
                out_abs.display()
            ),
        ));
    }
    Ok(out_abs)
}

/// Take the extract lock and open the archive index.
fn open(
    g: &GlobalOpts,
    t: &Target,
    testing: bool,
) -> Result<(AssetArchive, Option<starorg_core::lock::Lock>), ExitCode> {
    let lock = if testing {
        None
    } else {
        let Some(dirs) = starorg_core::appdirs::AppDirs::from_env() else {
            return Err(fail(g, exit::ERROR, "error", "cannot find the app folders"));
        };
        match starorg_core::lock::try_lock(&dirs.extract_lock()) {
            Ok(l) => Some(l),
            Err(starorg_core::lock::LockError::Held(_)) => {
                return Err(fail(
                    g,
                    exit::SKIPPED,
                    "busy",
                    "another StarOrg Codex run is reading game data; try again",
                ));
            }
            Err(e) => return Err(fail(g, exit::ERROR, "error", &e.to_string())),
        }
    };
    let a = AssetArchive::open(&t.p4k).map_err(|e| asset_fail(g, &e))?;
    Ok((a, lock))
}

fn asset_fail(g: &GlobalOpts, e: &AssetError) -> ExitCode {
    let (code, kind) = match e.code.as_str() {
        "not-found" => (exit::NOT_FOUND, "not-found"),
        "invalid" => (exit::USAGE, "usage"),
        "busy" => (exit::SKIPPED, "busy"),
        _ => (exit::ERROR, "error"),
    };
    fail(g, code, kind, &e.message)
}

/// The build key of a `--test-p4k` archive.
pub const TEST_BUILD: &str = "000000000000000000000000";

/// The asset cache (none with `--no-cache`), refused when its folder (`--cache-dir` or the
/// setting) lies inside the install or a Wine prefix. Creates nothing.
fn cache(g: &GlobalOpts, t: &Target, dir: Option<&Path>) -> Result<Option<AssetCache>, ExitCode> {
    if g.no_cache {
        return Ok(None);
    }
    let Some(dirs) = starorg_core::appdirs::AppDirs::from_env() else {
        return Ok(None);
    };
    let cfg = starorg_core::config::Config::load(&dirs).unwrap_or_default();
    let protected = assetcache::protected_roots(&t.root.iter().cloned().collect::<Vec<_>>());
    assetcache::resolve_checked(&dirs, &cfg, dir, &protected)
        .map(Some)
        .map_err(|e| fail(g, exit::USAGE, "usage", &e))
}

/// Progress on stderr, at most every 2 s (none with --quiet or --json).
struct Ticker {
    on: bool,
    last: std::sync::Mutex<Instant>,
}

impl Ticker {
    fn new(g: &GlobalOpts) -> Self {
        Self {
            on: !g.quiet && !g.json,
            last: std::sync::Mutex::new(Instant::now()),
        }
    }
    fn tell(&self, what: &str, f: f32) {
        if !self.on {
            return;
        }
        let mut l = self.last.lock().unwrap_or_else(|p| p.into_inner());
        if l.elapsed() >= Duration::from_secs(2) {
            *l = Instant::now();
            eprintln!("starorg-codex: {what} {:.0} %", f * 100.0);
        }
    }
}

pub fn run(g: &GlobalOpts, cmd: DevCmd) -> ExitCode {
    match cmd {
        DevCmd::Extract {
            paths,
            out,
            convert,
            channel,
            test_p4k,
        } => extract(g, &paths, &out, convert, &channel, test_p4k.get()),
        DevCmd::ExportModel {
            what,
            out,
            preset,
            lod,
            stl,
            paint,
            layers,
            cache_dir,
            channel,
            test_p4k,
        } => {
            let preset = if preset == "print" {
                ModelPreset::PrintShell
            } else {
                ModelPreset::Full
            };
            let o = ModelOptions {
                preset,
                lod: lod.unwrap_or(preset.default_lod()).min(model::MAX_LOD),
                stl,
                paint,
                layers,
            };
            if preset == ModelPreset::PrintShell && (o.paint.is_some() || layers) {
                return fail(
                    g,
                    exit::USAGE,
                    "usage",
                    "--paint and --layers need --preset full (a print shell has no materials)",
                );
            }
            export_model(
                g,
                &what,
                &out,
                o,
                cache_dir.as_deref(),
                &channel,
                test_p4k.get(),
            )
        }
        DevCmd::ExportSounds {
            what,
            out,
            wem,
            list,
            cache_dir,
            channel,
            test_p4k,
        } => export_sounds(
            g,
            &what,
            &out,
            wem,
            list,
            cache_dir.as_deref(),
            &channel,
            test_p4k.get(),
        ),
        DevCmd::ModelPaints {
            what,
            channel,
            test_p4k,
        } => model_paints(g, &what, &channel, test_p4k.get()),
        _ => ExitCode::from(exit::USAGE),
    }
}

fn model_paints(g: &GlobalOpts, what: &str, channel: &str, test_p4k: Option<PathBuf>) -> ExitCode {
    let testing = test_p4k.is_some();
    let t = match target(g, channel, test_p4k) {
        Ok(t) => t,
        Err(c) => return c,
    };
    let (a, lock) = match open(g, &t, testing) {
        Ok(x) => x,
        Err(c) => return c,
    };
    let r = model::list_paints(&a, what);
    drop(lock);
    match r {
        Ok(p) => {
            if g.json {
                println!("{}", json!({ "schema": JSON_SCHEMA, "paints": p }));
            } else if !g.quiet {
                if p.paints.is_empty() {
                    println!("No paints for {what}.");
                }
                for x in &p.paints {
                    let fitted = if p
                        .equipped
                        .as_deref()
                        .is_some_and(|e| e.eq_ignore_ascii_case(&x.tag))
                    {
                        "  (fitted)"
                    } else {
                        ""
                    };
                    println!(
                        "{}\t{}\t{}{fitted}",
                        x.tag,
                        x.name.as_deref().unwrap_or("-"),
                        x.items.join(", ")
                    );
                }
            }
            ExitCode::from(exit::OK)
        }
        Err(e) => asset_fail(g, &e),
    }
}

fn extract(
    g: &GlobalOpts,
    paths: &[String],
    out: &Path,
    convert: bool,
    channel: &str,
    test_p4k: Option<PathBuf>,
) -> ExitCode {
    let testing = test_p4k.is_some();
    let t = match target(g, channel, test_p4k) {
        Ok(t) => t,
        Err(c) => return c,
    };
    let out_abs = match check_out(g, &t, out, testing) {
        Ok(p) => p,
        Err(c) => return c,
    };
    let (a, _lock) = match open(g, &t, testing) {
        Ok(x) => x,
        Err(c) => return c,
    };
    let mut plan = Vec::new();
    for p in paths {
        let r = if files::is_glob(p) {
            files::plan_glob(&a, p)
        } else {
            files::plan_extract(&a, std::slice::from_ref(p))
        };
        match r {
            Ok(mut x) => plan.append(&mut x),
            Err(e) => return asset_fail(g, &e),
        }
    }
    plan.sort_by_key(|i| i.entry);
    plan.dedup_by_key(|i| i.entry);
    let tick = Ticker::new(g);
    let r = files::extract_all(&a, &plan, &out_abs, convert, &mut |k, n| {
        tick.tell("extract", k as f32 / n.max(1) as f32);
        true
    });
    match r {
        Ok(d) => {
            if g.json {
                println!(
                    "{}",
                    json!({ "schema": JSON_SCHEMA, "out": d.out_dir, "files": d.files, "bytes": d.bytes,
                            "converted": d.converted, "merged": d.merged, "failures": d.failures, "failed": d.failed })
                );
            } else if !g.quiet {
                eprintln!(
                    "starorg-codex: {} files ({} bytes, {} converted) written to {}",
                    d.files, d.bytes, d.converted, d.out_dir
                );
                for f in &d.failed {
                    eprintln!("  failed: {f}");
                }
            }
            if d.failures > 0 && d.files == 0 {
                ExitCode::from(exit::ERROR)
            } else {
                ExitCode::from(exit::OK)
            }
        }
        Err(e) => asset_fail(g, &e),
    }
}

fn copy_out(src: &Path, out: &Path) -> std::io::Result<u64> {
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::copy(src, out)
}

fn export_model(
    g: &GlobalOpts,
    what: &str,
    out: &Path,
    o: ModelOptions,
    cache_dir: Option<&Path>,
    channel: &str,
    test_p4k: Option<PathBuf>,
) -> ExitCode {
    let testing = test_p4k.is_some();
    let t = match target(g, channel, test_p4k) {
        Ok(t) => t,
        Err(c) => return c,
    };
    let out_abs = match check_out(g, &t, out, testing) {
        Ok(p) => p,
        Err(c) => return c,
    };
    let c = match cache(g, &t, cache_dir) {
        Ok(c) => c,
        Err(code) => return code,
    };
    // A LOD the mesh lacks is the cached one below it (the LODs an earlier export recorded).
    let mut o = o;
    if let Some(c) = &c {
        o.lod = model::effective_lod(o.lod, &model::cached_lods(c, &t.channel, &t.build, what));
    }
    let key = o.cache_key(what);
    // A cached GLB: no archive read at all (an STL is made from it here).
    if let Some(hit) = c
        .as_ref()
        .and_then(|c| c.get(&t.channel, &t.build, Kind::Models, &key, "glb"))
    {
        let r = copy_out(&hit, &out_abs).and_then(|n| {
            let stl = if o.stl {
                let glb = std::fs::read(&hit)?;
                let s = out_abs.with_extension("stl");
                starorg_data::assets::stl::glb_file_to_stl(&glb, &s)
                    .map(|x| Some((s, x)))
                    .map_err(|e| std::io::Error::other(e.message))?
            } else {
                None
            };
            Ok((n, stl))
        });
        return match r {
            Ok((n, stl)) => {
                report_model(
                    g,
                    &out_abs,
                    n,
                    stl.as_ref().map(|s| (&s.0, s.1.0)),
                    true,
                    None,
                );
                ExitCode::from(exit::OK)
            }
            Err(e) => fail(g, exit::ERROR, "error", &e.to_string()),
        };
    }
    // The memory limit (addendum 7.18): an export estimated above it is refused up front;
    // more than the free memory is a warning (the limit still applies).
    let need = model::memory_need(&o, what);
    match memlimit::check(
        need,
        crate::mem_opts::current(),
        memlimit::meminfo().available,
    ) {
        memlimit::Fit::OverLimit { need, limit } => {
            return fail(
                g,
                exit::ERROR,
                "memory-limit",
                &memlimit::over_limit_message("Exporting this model", need, limit),
            );
        }
        memlimit::Fit::LowFree { need, free } if !g.quiet && !g.json => {
            eprintln!(
                "starorg-codex: {}",
                memlimit::low_free_message("This export", need, free)
            );
        }
        _ if o.preset == ModelPreset::Full && !g.quiet && !g.json => {
            eprintln!("starorg-codex: {}", model::FULL_MEMORY_NOTE);
        }
        _ => {}
    }
    let (a, lock) = match open(g, &t, testing) {
        Ok(x) => x,
        Err(c) => return c,
    };
    let tick = Ticker::new(g);
    let r = model::export_model(&a, what, &o, &out_abs, &|f, _| {
        tick.tell("export", f);
        true
    });
    drop(lock);
    match r {
        Ok(d) => {
            // Cached under the LOD it has (LOD 4 asked of a mesh with 0-3 is LOD 3).
            let key = ModelOptions {
                lod: d.lod,
                ..o.clone()
            }
            .cache_key(what);
            if let Some(c) = &c {
                let _ = c.put_copy(&t.channel, &t.build, Kind::Models, &key, "glb", &out_abs);
                let _ = model::cache_lods(c, &t.channel, &t.build, what, &d.lods);
            }
            let stl = d.stl_path.as_ref().map(PathBuf::from);
            report_model(
                g,
                &out_abs,
                d.bytes,
                stl.as_ref().zip(d.stl_path.as_ref().and(d.triangles)),
                false,
                Some(&d),
            );
            ExitCode::from(exit::OK)
        }
        Err(e) => asset_fail(g, &e),
    }
}

fn report_model(
    g: &GlobalOpts,
    out: &Path,
    bytes: u64,
    stl: Option<(&PathBuf, u64)>,
    cached: bool,
    done: Option<&model::ModelDone>,
) {
    if g.json {
        println!(
            "{}",
            json!({ "schema": JSON_SCHEMA, "out": out, "bytes": bytes, "cached": cached,
                    "stl": stl.map(|s| s.0),
                    "triangles": done.and_then(|d| d.triangles).or(stl.map(|s| s.1)),
                    "lod": done.map(|d| d.lod), "lods": done.map(|d| &d.lods),
                    "peak_rss": done.and_then(|d| d.peak_rss), "ms": done.map_or(0, |d| d.ms) })
        );
    } else if !g.quiet {
        let detail = done
            .map(|d| {
                let lods = match d.lods.as_slice() {
                    [] => String::new(),
                    l => format!(
                        " (the mesh has LOD 0-{})",
                        l.iter().max().copied().unwrap_or(0)
                    ),
                };
                let tris = d
                    .triangles
                    .map(|t| format!(", {t} triangles"))
                    .unwrap_or_default();
                format!(", LOD {}{lods}{tris}", d.lod)
            })
            .unwrap_or_default();
        eprintln!(
            "starorg-codex: model ({} bytes{}{detail}) written to {}",
            bytes,
            if cached { ", from the cache" } else { "" },
            out.display()
        );
        if let Some((p, n)) = stl {
            eprintln!(
                "starorg-codex: STL ({n} triangles) written to {}",
                p.display()
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn export_sounds(
    g: &GlobalOpts,
    what: &str,
    out: &Path,
    wem: bool,
    list: bool,
    cache_dir: Option<&Path>,
    channel: &str,
    test_p4k: Option<PathBuf>,
) -> ExitCode {
    let testing = test_p4k.is_some();
    let t = match target(g, channel, test_p4k) {
        Ok(t) => t,
        Err(c) => return c,
    };
    let out_abs = match check_out(g, &t, out, testing) {
        Ok(p) => p,
        Err(c) => return c,
    };
    let (a, _lock) = match open(g, &t, testing) {
        Ok(x) => x,
        Err(c) => return c,
    };
    let rs = match audio::record_sounds(&a, what) {
        Ok(r) => r,
        Err(e) => return asset_fail(g, &e),
    };
    let sounds: Vec<(String, SoundRef)> = rs
        .triggers
        .iter()
        .flat_map(|t| {
            t.sounds
                .iter()
                .filter(|s| s.playable)
                .map(|s| (t.name.clone(), s.sound.clone()))
        })
        .collect();
    if list {
        // Each playable sound's codec, from its header (and the file it becomes).
        let idx = match a.audio() {
            Ok(i) => i,
            Err(e) => return asset_fail(g, &e),
        };
        let refs: Vec<&SoundRef> = sounds.iter().map(|(_, s)| s).collect();
        let codecs = idx.media_codecs(&a, &refs);
        let codec_of = |s: &SoundRef| codecs.get(&s.key()).copied().flatten();
        let as_file = |s: &SoundRef| match codec_of(s) {
            Some(c) => format!(
                "{c} -> .{}",
                c.format().map(wemcodec::Format::ext).unwrap_or("wem")
            ),
            None => "?".into(),
        };
        if g.json {
            let by_key: serde_json::Map<String, serde_json::Value> = sounds
                .iter()
                .map(|(_, s)| {
                    let c = codec_of(s);
                    (
                        s.key(),
                        json!({ "codec": c.map(|c| c.to_string()),
                                "ext": c.map(|c| c.format().map(wemcodec::Format::ext).unwrap_or("wem")) }),
                    )
                })
                .collect();
            println!(
                "{}",
                json!({ "schema": JSON_SCHEMA, "sounds": rs, "codecs": by_key })
            );
        } else {
            for tr in &rs.triggers {
                println!(
                    "{}  ({} sounds, from {}{})",
                    tr.name,
                    tr.sounds.len(),
                    tr.from,
                    tr.bank
                        .as_deref()
                        .map(|b| format!(", {b}"))
                        .unwrap_or_default()
                );
                for s in &tr.sounds {
                    println!(
                        "    {:<12} {:<12} {}",
                        s.label,
                        format!("{:?}", s.sound.source),
                        if s.playable {
                            as_file(&s.sound)
                        } else {
                            "unavailable".into()
                        }
                    );
                }
            }
        }
        return ExitCode::from(exit::OK);
    }
    // Decoded sounds come from the cache when there.
    let c = match cache(g, &t, cache_dir) {
        Ok(c) => c,
        Err(code) => return code,
    };
    let mut files_n = 0u32;
    let mut bytes = 0u64;
    let mut formats = std::collections::BTreeMap::<String, u32>::new();
    let mut todo = Vec::new();
    for (trigger, s) in &sounds {
        // A cached converted file (or a WEM kept as WEM); with --wem every sound is read again.
        let hit = (!wem)
            .then(|| {
                c.as_ref()
                    .and_then(|c| assetworker::cached_sound(c, &t.channel, &t.build, s))
            })
            .flatten();
        match hit {
            Some((p, ext)) => {
                match copy_out(&p, &out_abs.join(audio::file_name(trigger, s, ext))) {
                    Ok(n) => {
                        files_n += 1;
                        bytes += n;
                        *formats.entry(ext.to_string()).or_default() += 1;
                    }
                    Err(_) => todo.push((trigger.clone(), s.clone())),
                }
            }
            None => todo.push((trigger.clone(), s.clone())),
        }
    }
    let tick = Ticker::new(g);
    let r = assetworker::export_sounds(&a, &todo, &out_abs, wem, &mut |k, n| {
        tick.tell("sounds", k as f32 / n.max(1) as f32);
        true
    });
    match r {
        Ok(d) => {
            if let Some(c) = &c {
                assetworker::cache_written(c, &t.channel, &t.build, &d);
            }
            let files_n = files_n + d.files;
            let bytes = bytes + d.bytes;
            for (e, n) in &d.formats {
                *formats.entry(e.clone()).or_default() += n;
            }
            if g.json {
                println!(
                    "{}",
                    json!({ "schema": JSON_SCHEMA, "record": rs.name, "triggers": rs.triggers.len(),
                            "files": files_n, "bytes": bytes, "out": out_abs, "formats": formats,
                            "kept": d.kept, "failures": d.failures, "failed": d.failed })
                );
            } else if !g.quiet {
                eprintln!(
                    "starorg-codex: {} sound files ({} bytes; {}) of {} ({} triggers) written to {}",
                    files_n,
                    bytes,
                    assetworker::SoundsDone::formats_line(&formats),
                    rs.name,
                    rs.triggers.len(),
                    out_abs.display()
                );
                for k in &d.kept {
                    eprintln!("  kept as .wem: {k}");
                }
                for f in &d.failed {
                    eprintln!("  failed: {f}");
                }
            }
            ExitCode::from(exit::OK)
        }
        Err(e) => asset_fail(g, &e),
    }
}

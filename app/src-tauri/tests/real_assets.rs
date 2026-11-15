//! Developer mode assets on the real backend (addendum 7.16): the real `asset-worker`
//! process (this package's binary) on a synthetic Data.p4k in a fake install, native dialogs
//! replaced by fixed answers, the asset cache in a temp folder. Checks that nothing starts
//! before a Files / Sounds / Model request, that cache hits need no worker, and the cache
//! folder setting (validation, switch, move, clear).

use starorg_codex_app::contract::Backend;
use starorg_codex_app::contract::assets::*;
use starorg_codex_app::contract::dev::WorkerState;
use starorg_codex_app::contract::real::{
    AssetSpawner, DialogSlot, Dialogs, FileFilter, RealBackend, RealConfig, SourceFactory,
};
use starorg_codex_app::contract::types::*;
use starorg_core::appdirs::AppDirs;
use starorg_core::apply::Ctx;
use starorg_core::assetcache::{self, AssetCache, Kind};
use starorg_core::discovery::{Channel, DiscoveryEnv, DiscoveryRequest};
use starorg_core::source::GameSource;
use starorg_data::assets::synthetic;
use starorg_data::starbreaker::synthetic as dcb;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};

struct NoSource;
impl SourceFactory for NoSource {
    fn source(&self, _: &AppDirs) -> Result<Box<dyn GameSource>, String> {
        Err("no extraction in these tests".into())
    }
}

#[derive(Default)]
struct Answers {
    folder: Option<PathBuf>,
    save: Option<PathBuf>,
}

struct FixedDialogs(Arc<Mutex<Answers>>);
impl Dialogs for FixedDialogs {
    fn pick_folder(&self, _: &str) -> Option<PathBuf> {
        self.0.lock().unwrap().folder.clone()
    }
    fn pick_file(&self, _: &str, _: FileFilter) -> Option<PathBuf> {
        None
    }
    fn save_file(&self, _: &str, _: &str, _: FileFilter) -> Option<PathBuf> {
        self.0.lock().unwrap().save.clone()
    }
}

/// The app binary as `asset-worker --p4k <the channel's archive>`, counting starts.
struct Counting(Arc<Mutex<u32>>);
impl AssetSpawner for Counting {
    fn command(&self, ch: &Channel) -> Result<Command, String> {
        *self.0.lock().unwrap() += 1;
        let mut c = Command::new(env!("CARGO_BIN_EXE_starorg-codex-app"));
        c.arg("asset-worker").arg("--p4k").arg(&ch.p4k);
        Ok(c)
    }
}

struct Fake {
    _t: tempfile::TempDir,
    base: PathBuf,
    game: PathBuf,
    backend: RealBackend,
    answers: Arc<Mutex<Answers>>,
    starts: Arc<Mutex<u32>>,
}

fn fake() -> Fake {
    let t = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(t.path()).unwrap();
    let lib = base.join("games/StarCitizen");
    let d = lib.join("LIVE");
    synthetic::write(&d.join("Data.p4k")).unwrap();
    std::fs::write(
        d.join("build_manifest.id"),
        r#"{"Data":{"Branch":"sc-alpha-4.10.0","RequestedP4ChangeNum":"13165946","Version":"4.10.200.58746"}}"#,
    )
    .unwrap();
    let dirs = AppDirs::under(&base.join("app"));
    let ctx = Ctx::new(dirs, "0.0.0-test", "test");
    let answers = Arc::new(Mutex::new(Answers::default()));
    let slot = DialogSlot::default();
    slot.set(FixedDialogs(answers.clone()));
    let starts = Arc::new(Mutex::new(0));
    let backend = RealBackend::new(RealConfig {
        ctx,
        discovery: DiscoveryRequest {
            game_dir: Some(lib.clone()),
            prefix: None,
        },
        env: DiscoveryEnv::from_process(),
        source: Arc::new(NoSource),
        dialogs: slot,
    })
    .with_asset_spawner(Arc::new(Counting(starts.clone())));
    Fake {
        _t: t,
        base,
        game: lib,
        backend,
        answers,
        starts,
    }
}

impl Fake {
    fn live(&self) -> ChannelRef {
        ChannelRef {
            install: self.backend.overview().unwrap().installs[0].id.clone(),
            channel: "LIVE".into(),
        }
    }
    fn dev_mode(&self, on: bool) {
        let mut s = self.backend.get_settings().unwrap();
        s.advanced.developer_mode = on;
        self.backend.save_settings(s).unwrap();
    }
    fn starts(&self) -> u32 {
        *self.starts.lock().unwrap()
    }
    fn cache(&self) -> AssetCache {
        let s = self.backend.asset_cache_status().unwrap();
        AssetCache::new(s.path, s.max_bytes)
    }
}

fn noop(_: JobProgress) {}

#[test]
fn off_by_default_and_nothing_starts_before_a_request() {
    let f = fake();
    let live = f.live();
    let e = f.backend.asset_status(live.clone()).unwrap_err();
    assert_eq!(e.code, ErrorCode::DevDisabled);
    let e = f
        .backend
        .asset_list(AssetListRequest {
            target: live.clone(),
            dir: String::new(),
            offset: 0,
            limit: 10,
        })
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::DevDisabled);
    f.dev_mode(true);
    // Status, the cache row and the explorer never start the asset worker.
    assert_eq!(
        f.backend.asset_status(live.clone()).unwrap().state,
        WorkerState::Stopped
    );
    let c = f.backend.asset_cache_status().unwrap();
    assert!(c.is_default && c.files == 0);
    assert_eq!(f.starts(), 0);
}

#[test]
fn files_previews_and_sounds_with_the_cache() {
    let f = fake();
    f.dev_mode(true);
    let live = f.live();
    let root = f
        .backend
        .asset_list(AssetListRequest {
            target: live.clone(),
            dir: String::new(),
            offset: 0,
            limit: 10,
        })
        .unwrap();
    assert_eq!(f.starts(), 1);
    assert_eq!(root.entries[0].name, "Data");
    let s = f.backend.asset_status(live.clone()).unwrap();
    assert_eq!(s.state, WorkerState::Ready);
    assert_eq!(s.entries, synthetic::files().len() as u32);
    assert!(!s.datacore && !s.audio, "listing reads no DataCore: {s:?}");
    let hits = f
        .backend
        .asset_search(AssetSearchRequest {
            target: live.clone(),
            query: "syn_config".into(),
            limit: 10,
        })
        .unwrap();
    assert_eq!(hits.total, 1);
    // A texture preview: decoded by the worker and cached; then served without one.
    let pv = |path: &str| {
        f.backend
            .asset_preview(AssetPathRequest {
                target: live.clone(),
                path: path.into(),
            })
            .unwrap()
    };
    let p = pv(synthetic::TEXTURE);
    assert_eq!(p.kind, PreviewKind::Image);
    assert!(!p.cached);
    assert!(
        p.image
            .as_deref()
            .unwrap()
            .starts_with("data:image/png;base64,iVBOR")
    );
    let x = pv(synthetic::XML);
    assert!(x.text.unwrap().contains("speed"));
    // One sound (PCM: converted to WAV, playable), cached.
    let snd = SoundRef {
        media_id: 123456,
        source: SourceKind::MediaFile,
        bank: String::new(),
        media_path: None,
    };
    let a = f
        .backend
        .asset_sound(AssetSoundRequest {
            target: live.clone(),
            sound: snd.clone(),
        })
        .unwrap();
    assert_eq!(
        (a.mime.as_str(), a.format.as_str()),
        ("audio/wav", "PCM (.wav)")
    );
    assert!(a.note.is_none() && !a.cached && a.data.starts_with("UklGR"));
    // A codec that is not converted stays WEM, with its codec named (never "Not Vorbis").
    let odd = SoundRef {
        media_id: 123457,
        ..snd.clone()
    };
    let o = f
        .backend
        .asset_sound(AssetSoundRequest {
            target: live.clone(),
            sound: odd.clone(),
        })
        .unwrap();
    assert!(o.mime.is_empty() && o.data.is_empty() && o.hint.is_none());
    assert_eq!(
        o.note.as_deref(),
        Some("PTADPCM (fmt 0x8311) audio cannot be converted; it is kept as .wem")
    );
    assert_eq!(o.format, "PTADPCM (fmt 0x8311) (.wem)");
    // The record's sounds read the DataCore and the audio index on demand.
    let rs = f
        .backend
        .asset_sounds(AssetRecordRequest {
            target: live.clone(),
            what: dcb::guid_text(1, 2),
        })
        .unwrap();
    assert_eq!(rs.name, "SYN_Item_002");
    let s = f.backend.asset_status(live.clone()).unwrap();
    assert!(s.datacore && s.audio);
    // Close: the cached things need no worker.
    f.backend.asset_close(live.clone()).unwrap();
    assert!(!f.backend.asset_worker_running(&live));
    let p = pv(&synthetic::TEXTURE.replace('\\', "/"));
    assert!(p.cached && p.image.is_some());
    let a = f
        .backend
        .asset_sound(AssetSoundRequest {
            target: live.clone(),
            sound: snd,
        })
        .unwrap();
    assert!(a.cached && a.mime == "audio/wav");
    let o = f
        .backend
        .asset_sound(AssetSoundRequest {
            target: live.clone(),
            sound: odd,
        })
        .unwrap();
    assert!(o.cached && o.note.unwrap().starts_with("PTADPCM"));
    assert!(!f.backend.asset_worker_running(&live));
    assert_eq!(f.starts(), 1, "cache hits start no worker");
    let c = f.backend.asset_cache_status().unwrap();
    assert_eq!(c.files, 3);
    assert!(c.bytes > 0);
    // A game patch (another build key) misses; the old build's files go on the next write.
    let cache = f.cache();
    let builds: Vec<_> = std::fs::read_dir(cache.root().join("LIVE"))
        .unwrap()
        .collect();
    assert_eq!(builds.len(), 1);
}

#[test]
fn extract_and_exports_respect_dialogs_and_the_game_folder() {
    let f = fake();
    f.dev_mode(true);
    let live = f.live();
    let req = AssetExtractRequest {
        target: live.clone(),
        paths: vec!["Data\\Textures".into(), synthetic::XML.into()],
        convert: true,
    };
    // Dialog closed: nothing written.
    let r = f.backend.asset_extract(req.clone(), &noop).unwrap();
    assert!(r.cancelled);
    let out = f.base.join("exports");
    f.answers.lock().unwrap().folder = Some(out.clone());
    let seen = Mutex::new(Vec::new());
    let r = f
        .backend
        .asset_extract(req.clone(), &|p| seen.lock().unwrap().push(p))
        .unwrap();
    assert_eq!((r.files, r.cancelled), (2, false));
    assert!(out.join("Textures/ships/syn_hull_diff.png").exists());
    assert!(out.join("syn_config.xml").exists());
    let seen = seen.lock().unwrap();
    assert_eq!(seen.last().unwrap().phase, JobPhase::Done);
    assert!(seen.iter().all(|p| p.kind == JobKind::Dev && p.cancellable));
    drop(seen);
    // Into the game folder (no Wine prefix here): the channel, a sibling channel and the
    // install root are all refused.
    for bad in [
        f.game.join("LIVE").join("x"),
        f.game.join("PTU").join("x"),
        f.game.join("x"),
        f.game.clone(),
    ] {
        f.answers.lock().unwrap().folder = Some(bad.clone());
        let e = f.backend.asset_extract(req.clone(), &noop).unwrap_err();
        assert_eq!(e.code, ErrorCode::Invalid, "{}", bad.display());
        assert!(
            e.message.contains("exports never go there"),
            "{}",
            e.message
        );
    }
    assert!(
        !f.game.join("LIVE").join("x").exists()
            && !f.game.join("PTU").exists()
            && !f.game.join("x").exists()
    );
    // No playable sounds on a synthetic record.
    f.answers.lock().unwrap().folder = Some(out.clone());
    let e = f
        .backend
        .asset_export_sounds(
            AssetExportSoundsRequest {
                target: live.clone(),
                what: dcb::guid_text(1, 2),
                sounds: vec![],
                wem: false,
            },
            &noop,
        )
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::NotFound);
    // Picked sounds: the PCM one as .wav, the PTADPCM one kept as .wem; the note says so.
    let pick = |id: u32| AssetSoundPick {
        trigger: "Play_Test".into(),
        sound: SoundRef {
            media_id: id,
            source: SourceKind::MediaFile,
            bank: String::new(),
            media_path: None,
        },
    };
    let r = f
        .backend
        .asset_export_sounds(
            AssetExportSoundsRequest {
                target: live.clone(),
                what: dcb::guid_text(1, 2),
                sounds: vec![pick(123456), pick(123457)],
                wem: false,
            },
            &noop,
        )
        .unwrap();
    assert_eq!(r.files, 2);
    assert!(out.join("Play_Test_123456.wav").exists());
    assert!(out.join("Play_Test_123457.wem").exists());
    assert_eq!(
        r.note.as_deref(),
        Some("1 .wav, 1 .wem (the .wem files were not converted; the log says why)")
    );
    // A model of a non-entity fails in its own worker, which is stopped afterwards.
    let starts = f.starts();
    let model = |save: PathBuf| {
        f.answers.lock().unwrap().save = Some(save);
        AssetExportModelRequest {
            target: live.clone(),
            what: dcb::guid_text(1, 3),
            name: "SYN_Item_003".into(),
            preset: ModelPreset::PrintShell,
            lod: 0,
            stl: false,
            force: false,
        }
    };
    let e = f
        .backend
        .asset_export_model(model(out.join("m.glb")), &noop)
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Invalid);
    assert_eq!(f.starts(), starts + 1, "a fresh worker for the model");
    // A cached model is copied without a worker (a sound first makes the build's folder).
    f.backend
        .asset_sound(AssetSoundRequest {
            target: live.clone(),
            sound: SoundRef {
                media_id: 123456,
                source: SourceKind::MediaFile,
                bank: String::new(),
                media_path: None,
            },
        })
        .unwrap();
    f.backend.asset_close(live.clone()).unwrap();
    let starts = f.starts();
    let fp = f.cache();
    let key = format!(
        "{}-print-lod0",
        assetcache::safe_name(&dcb::guid_text(1, 3))
    );
    let build = std::fs::read_dir(fp.root().join("LIVE"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .file_name()
        .to_string_lossy()
        .into_owned();
    fp.prepare("LIVE", &build, Kind::Models).unwrap();
    std::fs::write(
        fp.path("LIVE", &build, Kind::Models, &key, "glb"),
        b"glTF-fake",
    )
    .unwrap();
    let r = f
        .backend
        .asset_export_model(model(out.join("cached.glb")), &noop)
        .unwrap();
    assert!(r.cached);
    assert_eq!(std::fs::read(out.join("cached.glb")).unwrap(), b"glTF-fake");
    assert_eq!(f.starts(), starts, "no worker for a cache hit");
}

#[test]
fn the_cache_folder_is_validated_switched_moved_and_cleared() {
    let f = fake();
    f.dev_mode(true);
    let live = f.live();
    // Fill the default cache with one preview.
    f.backend
        .asset_preview(AssetPathRequest {
            target: live.clone(),
            path: synthetic::TEXTURE.into(),
        })
        .unwrap();
    let before = f.backend.asset_cache_status().unwrap();
    assert!(before.is_default && before.files == 1);
    // Inside the game folder: refused, setting unchanged.
    f.answers.lock().unwrap().folder = Some(f.game.join("LIVE").join("cache"));
    let e = f.backend.asset_cache_pick().unwrap_err();
    assert_eq!(e.code, ErrorCode::Invalid);
    assert!(
        e.message.contains("never goes into the game folder"),
        "{}",
        e.message
    );
    assert!(f.backend.asset_cache_status().unwrap().is_default);
    // A new folder: switched; the old one keeps its file and is offered for a move.
    let mine = f.base.join("my-cache");
    f.answers.lock().unwrap().folder = Some(mine.clone());
    let s = f.backend.asset_cache_pick().unwrap();
    assert_eq!(PathBuf::from(&s.path), mine.join(assetcache::SUBDIR));
    assert!(!s.is_default);
    assert_eq!(s.files, 0);
    let prev = s.previous.unwrap();
    assert_eq!(prev.files, 1);
    assert_eq!(PathBuf::from(&prev.path), PathBuf::from(&before.path));
    let toml = std::fs::read_to_string(f.base.join("app/config/config.toml")).unwrap();
    assert!(toml.contains("asset_cache_dir"), "{toml}");
    // Move: the file is now in the new folder, the old one is empty.
    let s = f.backend.asset_cache_move().unwrap();
    assert_eq!(s.files, 1);
    assert!(s.previous.is_none());
    assert_eq!(
        AssetCache::new(&before.path, before.max_bytes)
            .usage()
            .files,
        0
    );
    assert!(
        f.backend.asset_cache_move().is_err(),
        "nothing left to move"
    );
    // The cached preview is found in the new folder without a worker.
    f.backend.asset_close(live.clone()).unwrap();
    let p = f
        .backend
        .asset_preview(AssetPathRequest {
            target: live.clone(),
            path: synthetic::TEXTURE.into(),
        })
        .unwrap();
    assert!(p.cached);
    // Clear, then reset to the default (the custom folder is not deleted).
    let s = f.backend.asset_cache_clear().unwrap();
    assert_eq!(s.files, 0);
    let s = f.backend.asset_cache_reset().unwrap();
    assert!(s.is_default);
    assert!(mine.is_dir());
}

#[test]
fn a_cache_folder_set_into_the_game_folder_is_refused_at_use() {
    let f = fake();
    f.dev_mode(true);
    let live = f.live();
    // A hand-edited config.toml pointing into the install.
    let cfg_path = f.base.join("app/config/config.toml");
    std::fs::create_dir_all(cfg_path.parent().unwrap()).unwrap();
    let mut text = std::fs::read_to_string(&cfg_path).unwrap_or_default();
    text.push_str(&format!(
        "\n[dev]\nasset_cache_dir = {:?}\n",
        f.game.join("LIVE").display().to_string()
    ));
    std::fs::write(&cfg_path, text).unwrap();
    let s = f.backend.asset_cache_status().unwrap();
    assert!(
        s.problem
            .as_deref()
            .unwrap_or("")
            .contains("never goes into the game folder"),
        "{s:?}"
    );
    assert!(
        !f.game.join("LIVE").join(assetcache::SUBDIR).exists(),
        "status creates nothing"
    );
    let e = f
        .backend
        .asset_preview(AssetPathRequest {
            target: live.clone(),
            path: synthetic::TEXTURE.into(),
        })
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Invalid);
    assert!(f.backend.asset_cache_clear().is_err());
    assert!(!f.game.join("LIVE").join(assetcache::SUBDIR).exists());
    // Reset fixes it.
    let s = f.backend.asset_cache_reset().unwrap();
    assert!(s.problem.is_none() && s.is_default);
    assert!(
        f.backend
            .asset_preview(AssetPathRequest {
                target: live,
                path: synthetic::TEXTURE.into(),
            })
            .is_ok()
    );
}

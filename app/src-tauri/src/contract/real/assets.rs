//! Developer mode assets in the real backend (addendum 7.16): one `asset-worker` child
//! process for the selected channel, started on the first Files / Sounds request (never when
//! the Dev screen opens), stopped after [`IDLE`] without a request, when Developer mode is
//! turned off and when the app exits. Model exports run in a fresh worker that is stopped
//! right after, so the export's memory (up to about 11 GB for a capital ship with textures)
//! is returned at once.
//!
//! The asset cache (`starorg_core::assetcache`) answers previews, sounds and model exports it
//! already has without starting a worker. `locks/extract.lock` is held while a worker opens
//! the archive index, while a model exports, and while the first sounds request reads the
//! DataCore and the Wwise index, so none of these peaks overlap an extraction.

use super::dev::{Worker, WorkerKey, lock_or};
use super::dialogs::FileFilter;
use super::{RealBackend, err, internal, io};
use crate::contract::Progress;
use crate::contract::assets::*;
use crate::contract::dev::WorkerState;
use crate::contract::types::*;
use starorg_core::assetcache::{self, AssetCache, Kind};
use starorg_core::config::Config;
use starorg_core::discovery::Channel;
use starorg_core::lock;
use starorg_data::assets::files::FileSearch;
use starorg_data::assets::model::{self, ModelDone, ModelOptions};
use starorg_data::assetworker::{AssetRequest, AssetStatus as WorkerStatus, SoundDone, SoundsDone};
use starorg_data::devworker::ExportProgress;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The asset worker stops after this long without a request.
pub const IDLE: Duration = Duration::from_secs(5 * 60);
/// How long a request waits for `extract.lock` (an explorer or extraction start).
const LOCK_WAIT: Duration = Duration::from_secs(30);
const GLB_FILTER: FileFilter = FileFilter {
    name: "glTF binary",
    extensions: &["glb"],
};

/// How the worker process is started (tests run the app binary on a synthetic archive).
pub trait AssetSpawner: Send + Sync {
    fn command(&self, ch: &Channel) -> Result<Command, String>;
}

/// This binary, `asset-worker --p4k <Data.p4k>`.
pub struct AssetWorkerSpawner;

impl AssetSpawner for AssetWorkerSpawner {
    fn command(&self, ch: &Channel) -> Result<Command, String> {
        let exe = std::env::current_exe().map_err(|e| format!("cannot find own binary: {e}"))?;
        let mut c = Command::new(exe);
        c.arg("asset-worker").arg("--p4k").arg(&ch.p4k);
        Ok(c)
    }
}

pub struct AssetState {
    spawner: Mutex<Arc<dyn AssetSpawner>>,
    worker: Mutex<Option<Arc<Worker>>>,
    failed: Mutex<Option<(String, String, String)>>,
    start: Mutex<()>,
    /// The running streamed job: (job id, worker, request id).
    export: Mutex<Option<(String, Arc<Worker>, u64)>>,
    /// The cache folder before the last change, offered for "Move existing cache".
    previous: Mutex<Option<PathBuf>>,
}

impl Default for AssetState {
    fn default() -> Self {
        Self {
            spawner: Mutex::new(Arc::new(AssetWorkerSpawner)),
            worker: Mutex::new(None),
            failed: Mutex::new(None),
            start: Mutex::new(()),
            export: Mutex::new(None),
            previous: Mutex::new(None),
        }
    }
}

impl AssetState {
    pub fn stop(&self) {
        if let Some(w) = lock_or(&self.worker).take() {
            w.stop();
        }
    }
}

/// `extract.lock`, waiting up to [`LOCK_WAIT`] for an explorer start or an extraction.
fn wait_lock(path: &Path) -> CmdResult<lock::Lock> {
    let until = Instant::now() + LOCK_WAIT;
    loop {
        match lock::try_lock(path) {
            Ok(l) => return Ok(l),
            Err(lock::LockError::Held(_)) if Instant::now() < until => {
                std::thread::sleep(Duration::from_millis(250));
            }
            Err(e) => return Err(super::busy_or(e, "Reading game data")),
        }
    }
}

fn data_url(mime: &str, path: &Path) -> CmdResult<String> {
    let b = std::fs::read(path).map_err(io)?;
    Ok(format!("data:{mime};base64,{}", assetcache::base64(&b)))
}

fn export_result(path: Option<String>, files: u32, bytes: u64, cached: bool) -> AssetExportResult {
    AssetExportResult {
        cancelled: path.is_none(),
        path,
        files,
        bytes,
        cached,
        failed: Vec::new(),
        note: None,
    }
}

fn preview_key(path: &str) -> String {
    starorg_data::assets::norm_path(path).to_ascii_lowercase()
}

impl RealBackend {
    pub fn with_asset_spawner(self, s: Arc<dyn AssetSpawner>) -> Self {
        *lock_or(&self.assets.spawner) = s;
        self
    }

    fn config_now(&self) -> Config {
        Config::load(self.dirs()).unwrap_or_default()
    }

    /// The asset cache, refused (`invalid`) when its folder lies in a game folder or its Wine
    /// prefix (a hand-edited `config.toml`), checked at every use.
    pub(super) fn asset_cache(&self) -> CmdResult<AssetCache> {
        assetcache::resolve_checked(self.dirs(), &self.config_now(), None, &self.protected())
            .map_err(|e| err(ErrorCode::Invalid, e))
    }

    /// Game folders that the cache never goes into: every registered or discovered install.
    fn protected(&self) -> Vec<PathBuf> {
        let mut roots: Vec<PathBuf> = self
            .config_now()
            .installs
            .iter()
            .map(|i| i.path.clone())
            .collect();
        roots.extend(self.installs().into_iter().map(|f| f.install.root.clone()));
        assetcache::protected_roots(&roots)
    }

    fn asset_target(&self, r: &ChannelRef) -> CmdResult<(Channel, WorkerKey)> {
        let (_, ch, key) = self.dev_channel(r)?;
        Ok((ch, key))
    }

    fn spawn_asset(&self, ch: &Channel, key: &WorkerKey) -> CmdResult<Arc<Worker>> {
        let cmd = lock_or(&self.assets.spawner)
            .command(ch)
            .map_err(internal)?;
        Worker::spawn(cmd, key.clone()).map_err(|e| err(ErrorCode::Internal, e))
    }

    /// The shared worker for this channel's build, started when needed (lock held while it
    /// opens the archive).
    fn asset_worker(&self, r: &ChannelRef) -> CmdResult<Arc<Worker>> {
        let (ch, key) = self.asset_target(r)?;
        let _one = lock_or(&self.assets.start);
        {
            let mut cur = lock_or(&self.assets.worker);
            if let Some(w) = cur.as_ref() {
                if w.alive() && w.key == key {
                    w.touch();
                    return Ok(w.clone());
                }
                if let Some(old) = cur.take() {
                    old.stop();
                }
            }
        }
        let started = (|| {
            let _x = wait_lock(&self.dirs().extract_lock())?;
            self.spawn_asset(&ch, &key)
        })();
        match started {
            Ok(w) => {
                *lock_or(&self.assets.failed) = None;
                *lock_or(&self.assets.worker) = Some(w.clone());
                self.spawn_asset_idle_watch(&w);
                self.log(
                    LogLevel::Info,
                    Some(JobKind::Dev),
                    Some(&ch.name),
                    &format!(
                        "Asset browser opened: {} files in {} ms",
                        w.info.records, w.info.start_ms
                    ),
                );
                Ok(w)
            }
            Err(e) => {
                *lock_or(&self.assets.failed) =
                    Some((key.install.clone(), key.channel.clone(), e.message.clone()));
                self.log(
                    LogLevel::Warn,
                    Some(JobKind::Dev),
                    Some(&ch.name),
                    &e.message,
                );
                Err(e)
            }
        }
    }

    fn spawn_asset_idle_watch(&self, w: &Arc<Worker>) {
        let weak = Arc::downgrade(w);
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(15));
                let Some(w) = weak.upgrade() else { return };
                if !w.alive() {
                    return;
                }
                let idle = lock_or(&w.last_used).elapsed();
                if idle >= IDLE && !w.busy.load(Ordering::SeqCst) {
                    w.stop();
                    lock_or(&w.died).get_or_insert_with(|| "stopped after 5 idle minutes".into());
                    return;
                }
            }
        });
    }

    fn asset_worker_if_ready(&self, r: &ChannelRef) -> Option<Arc<Worker>> {
        let (_, key) = self.asset_target(r).ok()?;
        lock_or(&self.assets.worker)
            .as_ref()
            .filter(|w| w.alive() && w.key == key)
            .cloned()
    }

    pub(super) fn asset_cancel(&self, job: &str) -> Option<CmdResult<()>> {
        let cur = lock_or(&self.assets.export).clone();
        let (_, w, id) = cur.filter(|x| x.0 == job)?;
        Some(
            w.call::<bool>(&AssetRequest::Cancel { job: id })
                .map(|_| ()),
        )
    }

    /// A streamed worker job with Activity progress; `cancel_job` stops it.
    fn asset_stream(
        &self,
        w: &Arc<Worker>,
        channel: &str,
        label: &str,
        req: &AssetRequest,
        unit: &str,
        progress: Progress<'_>,
    ) -> CmdResult<serde_json::Value> {
        {
            let cur = lock_or(&self.assets.export);
            if cur.is_some() {
                return Err(err(ErrorCode::Busy, "Another asset export is running."));
            }
        }
        let mut job = self.job_start(JobKind::Dev, Some(channel), 1, label);
        job.cancellable = true;
        progress(job.clone());
        w.busy.store(true, Ordering::SeqCst);
        let job_id = job.job.clone();
        let cell = Mutex::new(job);
        let r = w.call_stream(
            req,
            &|id| *lock_or(&self.assets.export) = Some((job_id.clone(), w.clone(), id)),
            &|p: ExportProgress| {
                let mut j = lock_or(&cell);
                j.steps = p.total.max(1);
                let label = if unit == "%" {
                    format!("{label} {} %", p.done * 100 / p.total.max(1))
                } else {
                    format!("{} of {} {unit}", p.done, p.total)
                };
                self.job_step(&mut j, p.done.max(1), &label, Some(progress));
            },
        );
        w.busy.store(false, Ordering::SeqCst);
        *lock_or(&self.assets.export) = None;
        let job = cell.into_inner().unwrap_or_else(|p| p.into_inner());
        match &r {
            Ok(_) => self.job_end(
                job,
                JobPhase::Done,
                &format!("{label}: done"),
                Some(progress),
            ),
            Err(e) => {
                let phase = if e.code == ErrorCode::Cancelled {
                    JobPhase::Cancelled
                } else {
                    JobPhase::Failed
                };
                self.job_end(job, phase, &e.message, Some(progress));
            }
        }
        r
    }

    /// The cache row; reads only (creates and writes nothing).
    fn cache_status(&self) -> AssetCacheStatus {
        let c = assetcache::resolve(self.dirs(), &self.config_now(), None);
        let problem = assetcache::check_root(c.root(), &self.protected()).err();
        let default = assetcache::default_dir(self.dirs());
        let u = if problem.is_none() {
            c.usage()
        } else {
            assetcache::Usage::default()
        };
        let previous = lock_or(&self.assets.previous)
            .clone()
            .filter(|p| p != c.root())
            .map(|p| (p.clone(), AssetCache::new(&p, c.max_bytes()).usage()))
            .filter(|(_, u)| u.files > 0)
            .map(|(p, u)| AssetCachePrevious {
                path: p.display().to_string(),
                bytes: u.bytes,
                files: u.files,
            });
        AssetCacheStatus {
            path: c.root().display().to_string(),
            default_path: default.display().to_string(),
            is_default: c.root() == default,
            bytes: u.bytes,
            files: u.files,
            max_bytes: c.max_bytes(),
            other_drive: c.root() != default && assetcache::other_drive(c.root(), &default),
            previous,
            problem,
        }
    }

    /// Switch the cache folder (`None` = default). The old folder keeps its files; it is
    /// remembered for "Move existing cache".
    fn set_cache_dir(&self, dir: Option<&Path>) -> CmdResult<AssetCacheStatus> {
        let before = assetcache::resolve(self.dirs(), &self.config_now(), None)
            .root()
            .to_path_buf();
        let mut cfg = self.config_now();
        assetcache::set_configured_dir(&mut cfg, dir);
        cfg.save(self.dirs()).map_err(io)?;
        let after = assetcache::resolve(self.dirs(), &cfg, None)
            .root()
            .to_path_buf();
        if before != after {
            *lock_or(&self.assets.previous) = Some(before.clone());
            self.log(
                LogLevel::Info,
                Some(JobKind::Dev),
                None,
                &format!(
                    "Asset cache folder: {} (was {}; its files stay there)",
                    after.display(),
                    before.display()
                ),
            );
        }
        Ok(self.cache_status())
    }

    fn sound_paths(&self, c: &AssetCache, key: &WorkerKey, s: &SoundRef) -> (PathBuf, PathBuf) {
        let k = s.key();
        (
            c.path(&key.channel, &key.build, Kind::Audio, &k, "ogg"),
            c.path(&key.channel, &key.build, Kind::Audio, &k, "wem"),
        )
    }
}

impl AssetBackend for RealBackend {
    fn asset_status(&self, req: ChannelRef) -> CmdResult<AssetStatus> {
        let (ch, key) = self.asset_target(&req)?;
        let mut s = AssetStatus {
            state: WorkerState::Stopped,
            channel: Some(ch.name.clone()),
            entries: 0,
            datacore: false,
            audio: false,
            memory: None,
            error: None,
        };
        let w = lock_or(&self.assets.worker).clone();
        if let Some(w) = w.filter(|w| w.key == key) {
            if w.alive() {
                s.state = WorkerState::Ready;
                s.entries = w.info.records;
                if let Ok(st) = w.call::<WorkerStatus>(&AssetRequest::Status) {
                    s.datacore = st.datacore;
                    s.audio = st.audio;
                    s.memory = st.rss;
                }
                return Ok(s);
            }
            let why = lock_or(&w.died).clone();
            if why.as_deref() != Some("stopped after 5 idle minutes") {
                s.state = WorkerState::Failed;
                s.error = why;
            }
            return Ok(s);
        }
        if let Some((i, c, e)) = lock_or(&self.assets.failed).clone()
            && i == key.install
            && c == key.channel
        {
            s.state = WorkerState::Failed;
            s.error = Some(e);
        }
        Ok(s)
    }

    fn asset_close(&self, req: ChannelRef) -> CmdResult<AssetStatus> {
        self.asset_target(&req)?;
        self.assets.stop();
        self.asset_status(req)
    }

    fn asset_list(&self, req: AssetListRequest) -> CmdResult<DirPage> {
        let w = self.asset_worker(&req.target)?;
        w.call(&AssetRequest::List {
            dir: req.dir,
            offset: req.offset,
            limit: req.limit,
        })
    }

    fn asset_search(&self, req: AssetSearchRequest) -> CmdResult<FileSearch> {
        let w = self.asset_worker(&req.target)?;
        w.call(&AssetRequest::Search {
            query: req.query,
            limit: req.limit,
        })
    }

    fn asset_preview(&self, req: AssetPathRequest) -> CmdResult<AssetPreview> {
        let (_, key) = self.asset_target(&req.target)?;
        let c = self.asset_cache()?;
        let ext = starorg_data::assets::files::ext(&req.path);
        let (img_ext, mime) = match ext.as_str() {
            "jpg" | "jpeg" => ("jpg", "image/jpeg"),
            _ => ("png", "image/png"),
        };
        let pkey = preview_key(&req.path);
        let image_kind = matches!(ext.as_str(), "dds" | "png" | "jpg" | "jpeg");
        if image_kind
            && let Some(hit) = c.get(&key.channel, &key.build, Kind::Previews, &pkey, img_ext)
        {
            return Ok(AssetPreview {
                path: starorg_data::assets::norm_path(&req.path),
                size: 0,
                kind: PreviewKind::Image,
                format: ext,
                text: None,
                truncated: false,
                width: None,
                height: None,
                image: Some(data_url(mime, &hit)?),
                note: None,
                cached: true,
            });
        }
        let w = self.asset_worker(&req.target)?;
        c.prepare(&key.channel, &key.build, Kind::Previews)
            .map_err(io)?;
        let out = c.path(&key.channel, &key.build, Kind::Previews, &pkey, img_ext);
        let p: starorg_data::assets::files::Preview = w.call(&AssetRequest::Preview {
            path: req.path,
            out: out.clone(),
        })?;
        let image = if p.kind == PreviewKind::Image {
            let _ = c.written(&key.channel, &key.build);
            Some(data_url(p.mime.as_deref().unwrap_or(mime), &out)?)
        } else {
            None
        };
        Ok(AssetPreview {
            path: p.path,
            size: p.size,
            kind: p.kind,
            format: p.format,
            text: p.text,
            truncated: p.truncated,
            width: p.width,
            height: p.height,
            image,
            note: p.note,
            cached: false,
        })
    }

    fn asset_extract(
        &self,
        req: AssetExtractRequest,
        progress: Progress<'_>,
    ) -> CmdResult<AssetExportResult> {
        if req.paths.is_empty() {
            return Err(err(ErrorCode::Invalid, "nothing selected"));
        }
        let (ch, _) = self.asset_target(&req.target)?;
        let w = self.asset_worker(&req.target)?;
        let Some(dir) = self.cfg.dialogs.get().and_then(|d| {
            d.pick_folder("Extract to folder (for your own use; not inside the game folder)")
        }) else {
            return Ok(export_result(None, 0, 0, false));
        };
        if let Some(p) = assetcache::inside_protected(&dir, &self.protected()) {
            return Err(err(
                ErrorCode::Invalid,
                format!(
                    "{} is inside {}; exports never go there",
                    dir.display(),
                    p.display()
                ),
            ));
        }
        let v = self.asset_stream(
            &w,
            &ch.name,
            "Extracting game files",
            &AssetRequest::Extract {
                paths: req.paths,
                out_dir: dir,
                convert: req.convert,
            },
            "files",
            progress,
        )?;
        let d: starorg_data::assets::files::ExtractDone =
            serde_json::from_value(v).map_err(internal)?;
        self.log(
            LogLevel::Info,
            Some(JobKind::Dev),
            Some(&ch.name),
            &format!(
                "Extracted {} files ({} bytes) to {}",
                d.files, d.bytes, d.out_dir
            ),
        );
        let mut r = export_result(Some(d.out_dir), d.files, d.bytes, false);
        r.failed = d.failed;
        if d.merged > 0 || d.converted > 0 {
            r.note = Some(format!(
                "{} converted, {} texture mip files merged into their PNG",
                d.converted, d.merged
            ));
        }
        Ok(r)
    }

    fn asset_sounds(&self, req: AssetRecordRequest) -> CmdResult<RecordSounds> {
        let w = self.asset_worker(&req.target)?;
        // The first sounds request reads the DataCore and the Wwise index: a memory peak.
        let ready = w
            .call::<WorkerStatus>(&AssetRequest::Status)
            .is_ok_and(|s| s.audio && s.datacore);
        let _x = if ready {
            None
        } else {
            Some(wait_lock(&self.dirs().extract_lock())?)
        };
        w.busy.store(true, Ordering::SeqCst);
        let r = w.call(&AssetRequest::Sounds { what: req.what });
        w.busy.store(false, Ordering::SeqCst);
        r
    }

    fn asset_sound(&self, req: AssetSoundRequest) -> CmdResult<AssetAudio> {
        let (_, key) = self.asset_target(&req.target)?;
        let c = self.asset_cache()?;
        let skey = req.sound.key();
        let audio = |path: &Path, cached: bool, duration: Option<f64>| -> CmdResult<AssetAudio> {
            let b = std::fs::read(path).map_err(io)?;
            Ok(AssetAudio {
                key: skey.clone(),
                mime: "audio/ogg".into(),
                bytes: b.len() as u64,
                data: assetcache::base64(&b),
                duration,
                cached,
                note: None,
            })
        };
        if let Some(hit) = c.get(&key.channel, &key.build, Kind::Audio, &skey, "ogg") {
            return audio(&hit, true, None);
        }
        let not_vorbis = |bytes: u64, cached: bool| AssetAudio {
            key: skey.clone(),
            mime: String::new(),
            data: String::new(),
            bytes,
            duration: None,
            cached,
            note: Some("Not Vorbis audio: it cannot be played here; export it as .wem".into()),
        };
        if let Some(hit) = c.get(&key.channel, &key.build, Kind::Audio, &skey, "wem") {
            let n = std::fs::metadata(&hit).map(|m| m.len()).unwrap_or(0);
            return Ok(not_vorbis(n, true));
        }
        let w = self.asset_worker(&req.target)?;
        c.prepare(&key.channel, &key.build, Kind::Audio)
            .map_err(io)?;
        let (ogg, wem) = self.sound_paths(&c, &key, &req.sound);
        let d: SoundDone = w.call(&AssetRequest::Sound {
            sound: req.sound,
            out_ogg: ogg.clone(),
            out_wem: wem,
            raw: false,
        })?;
        let _ = c.written(&key.channel, &key.build);
        if d.ext == "ogg" {
            audio(&ogg, false, d.duration)
        } else {
            Ok(not_vorbis(d.bytes, false))
        }
    }

    fn asset_export_sounds(
        &self,
        req: AssetExportSoundsRequest,
        progress: Progress<'_>,
    ) -> CmdResult<AssetExportResult> {
        let (ch, key) = self.asset_target(&req.target)?;
        let picks = if req.sounds.is_empty() {
            let all = self.asset_sounds(AssetRecordRequest {
                target: req.target.clone(),
                what: req.what.clone(),
            })?;
            playable_picks(&all)
        } else {
            req.sounds
        };
        if picks.is_empty() {
            return Err(err(
                ErrorCode::NotFound,
                "this record has no playable sounds",
            ));
        }
        let Some(dir) = self.cfg.dialogs.get().and_then(|d| {
            d.pick_folder("Export sounds to folder (for your own use; not inside the game folder)")
        }) else {
            return Ok(export_result(None, 0, 0, false));
        };
        let guard = self.protected();
        if let Some(p) = assetcache::inside_protected(&dir, &guard) {
            return Err(err(
                ErrorCode::Invalid,
                format!(
                    "{} is inside {}; exports never go there",
                    dir.display(),
                    p.display()
                ),
            ));
        }
        let c = self.asset_cache()?;
        let mut files = 0u32;
        let mut bytes = 0u64;
        let mut todo = Vec::new();
        for p in &picks {
            let hit = (!req.wem)
                .then(|| {
                    starorg_data::assetworker::cached_sound(&c, &key.channel, &key.build, &p.sound)
                })
                .flatten();
            match hit.map(|(h, ext)| {
                let name = starorg_data::assets::audio::file_name(&p.trigger, &p.sound, ext);
                std::fs::create_dir_all(&dir).and_then(|()| std::fs::copy(&h, dir.join(&name)))
            }) {
                Some(Ok(n)) => {
                    files += 1;
                    bytes += n;
                }
                _ => todo.push((p.trigger.clone(), p.sound.clone())),
            }
        }
        let mut failed = Vec::new();
        let cached = todo.is_empty();
        if !todo.is_empty() {
            let w = self.asset_worker(&req.target)?;
            let v = self.asset_stream(
                &w,
                &ch.name,
                "Exporting sounds",
                &AssetRequest::ExportSounds {
                    sounds: todo.clone(),
                    out_dir: dir.clone(),
                    wem: req.wem,
                },
                "sounds",
                progress,
            )?;
            let d: SoundsDone = serde_json::from_value(v).map_err(internal)?;
            files += d.files;
            bytes += d.bytes;
            failed = d.failed.clone();
            starorg_data::assetworker::cache_written(&c, &key.channel, &key.build, &d);
        }
        self.log(
            LogLevel::Info,
            Some(JobKind::Dev),
            Some(&ch.name),
            &format!("Exported {files} sound files to {}", dir.display()),
        );
        let mut r = export_result(Some(dir.display().to_string()), files, bytes, cached);
        r.failed = failed;
        Ok(r)
    }

    fn asset_export_model(
        &self,
        req: AssetExportModelRequest,
        progress: Progress<'_>,
    ) -> CmdResult<AssetExportResult> {
        let (ch, key) = self.asset_target(&req.target)?;
        let o = ModelOptions {
            preset: req.preset,
            lod: req.lod.min(model::MAX_LOD),
            stl: req.stl,
        };
        let name = if req.name.is_empty() {
            &req.what
        } else {
            &req.name
        };
        let need = model::memory_need(o.preset);
        if !req.force
            && let Some(free) = model::mem_available()
            && free < need
        {
            return Err(err(
                ErrorCode::NeedsDecision,
                format!(
                    "Only {:.1} GB of memory is free; this export may need about {:.0} GB. {}",
                    free as f64 / 1_073_741_824.0,
                    need as f64 / 1_073_741_824.0,
                    model::FULL_MEMORY_NOTE
                ),
            ));
        }
        let Some(out) = self.cfg.dialogs.get().and_then(|d| {
            d.save_file(
                "Export model (for your own use; not inside the game folder)",
                &model::file_name(name, &o),
                GLB_FILTER,
            )
        }) else {
            return Ok(export_result(None, 0, 0, false));
        };
        let guard = self.protected();
        if let Some(p) = assetcache::inside_protected(&out, &guard) {
            return Err(err(
                ErrorCode::Invalid,
                format!(
                    "{} is inside {}; exports never go there",
                    out.display(),
                    p.display()
                ),
            ));
        }
        let c = self.asset_cache()?;
        let mkey = format!("{}-{}", assetcache::safe_name(&req.what), o.key());
        if let Some(hit) = c.get(&key.channel, &key.build, Kind::Models, &mkey, "glb") {
            let n = std::fs::copy(&hit, &out).map_err(io)?;
            let mut files = 1;
            let mut bytes = n;
            if o.stl {
                let glb = std::fs::read(&hit).map_err(io)?;
                let (_, len) =
                    starorg_data::assets::stl::glb_file_to_stl(&glb, &out.with_extension("stl"))
                        .map_err(|e| err(ErrorCode::Internal, e.message))?;
                files += 1;
                bytes += len;
            }
            return Ok(export_result(
                Some(out.display().to_string()),
                files,
                bytes,
                true,
            ));
        }
        // A fresh worker: the export's memory goes back to the OS when it stops.
        let _x = wait_lock(&self.dirs().extract_lock())?;
        let w = self.spawn_asset(&ch, &key)?;
        let r = self.asset_stream(
            &w,
            &ch.name,
            &format!("Exporting model {name}"),
            &AssetRequest::ExportModel {
                what: req.what.clone(),
                options: o,
                out: out.clone(),
            },
            "%",
            progress,
        );
        w.stop();
        drop(_x);
        let d: ModelDone = serde_json::from_value(r?).map_err(internal)?;
        let _ = c.put_copy(&key.channel, &key.build, Kind::Models, &mkey, "glb", &out);
        self.log(
            LogLevel::Info,
            Some(JobKind::Dev),
            Some(&ch.name),
            &format!(
                "Exported model {} ({} bytes, {} ms, peak memory {} MB)",
                d.name,
                d.bytes,
                d.ms,
                d.peak_rss.map(|b| b / 1_048_576).unwrap_or(0)
            ),
        );
        let files = 1 + d.stl_path.is_some() as u32;
        let bytes = d.bytes + d.stl_bytes.unwrap_or(0);
        Ok(export_result(Some(d.path), files, bytes, false))
    }

    fn asset_cache_status(&self) -> CmdResult<AssetCacheStatus> {
        Ok(self.cache_status())
    }

    fn asset_cache_pick(&self) -> CmdResult<AssetCacheStatus> {
        let Some(dir) = self
            .cfg
            .dialogs
            .get()
            .and_then(|d| d.pick_folder("Asset cache folder"))
        else {
            return Ok(self.cache_status());
        };
        let default = assetcache::default_dir(self.dirs());
        if dir == default || dir == self.dirs().cache {
            return self.set_cache_dir(None);
        }
        let protected = self.protected();
        let ok = assetcache::validate_dir(&dir, &protected, &default)
            .map_err(|e| err(ErrorCode::Invalid, e))?;
        assetcache::check_root(
            &assetcache::root_for(self.dirs(), Some(&ok.path)),
            &protected,
        )
        .map_err(|e| err(ErrorCode::Invalid, e))?;
        self.set_cache_dir(Some(&ok.path))
    }

    fn asset_cache_reset(&self) -> CmdResult<AssetCacheStatus> {
        self.set_cache_dir(None)
    }

    fn asset_cache_clear(&self) -> CmdResult<AssetCacheStatus> {
        let u = self.asset_cache()?.clear().map_err(io)?;
        self.log(
            LogLevel::Info,
            Some(JobKind::Dev),
            None,
            &format!("Asset cache cleared: {} files, {} bytes", u.files, u.bytes),
        );
        Ok(self.cache_status())
    }

    fn asset_cache_move(&self) -> CmdResult<AssetCacheStatus> {
        let Some(prev) = lock_or(&self.assets.previous).clone() else {
            return Err(err(
                ErrorCode::NotFound,
                "no previous cache folder to move from",
            ));
        };
        let cur = self.asset_cache()?;
        if prev == cur.root() {
            return Ok(self.cache_status());
        }
        let u = AssetCache::new(&prev, cur.max_bytes())
            .move_to(&cur)
            .map_err(io)?;
        *lock_or(&self.assets.previous) = None;
        self.log(
            LogLevel::Info,
            Some(JobKind::Dev),
            None,
            &format!(
                "Moved {} cached files ({} bytes) from {} to {}",
                u.files,
                u.bytes,
                prev.display(),
                cur.root().display()
            ),
        );
        Ok(self.cache_status())
    }
}

impl RealBackend {
    /// Whether the asset worker runs for this channel (tests: nothing starts it early).
    pub fn asset_worker_running(&self, r: &ChannelRef) -> bool {
        self.asset_worker_if_ready(r).is_some()
    }
}

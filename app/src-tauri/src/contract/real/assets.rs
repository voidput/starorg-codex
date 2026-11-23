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

use super::dev::{Worker, WorkerKey, lock_or, worker_error};
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
use starorg_core::memlimit;
use starorg_data::assets::files::FileSearch;
use starorg_data::assets::model::{self, ModelDone, ModelOptions};
use starorg_data::assets::wemcodec;
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

/// A streamed worker job: its Activity job id, the worker, the worker's request id once sent,
/// and a Stop that came before that id (sent as soon as it is known).
struct ExportJob {
    job: String,
    worker: Arc<Worker>,
    id: Option<u64>,
    cancel: bool,
}

pub struct AssetState {
    spawner: Mutex<Arc<dyn AssetSpawner>>,
    worker: Mutex<Option<Arc<Worker>>>,
    failed: Mutex<Option<(String, String, String)>>,
    start: Mutex<()>,
    /// The running streamed job.
    export: Mutex<Option<ExportJob>>,
    /// The cache folder before the last change, offered for "Move existing cache".
    previous: Mutex<Option<PathBuf>>,
    /// The mesh LODs a model export read, per (channel, build, record): this session only.
    lods: Mutex<std::collections::HashMap<(String, String, String), Vec<u32>>>,
    /// Calls writing or reading the asset cache right now ("Clear asset cache" waits).
    in_use: std::sync::atomic::AtomicUsize,
    /// Settings' measured folder sizes (see `storage.rs`).
    pub(super) sizes: super::storage::Sizes,
}

/// Held while a call uses the asset cache (see [`RealBackend::asset_cache_in_use`]).
pub struct AssetCacheUse<'a>(&'a AssetState);

impl Drop for AssetCacheUse<'_> {
    fn drop(&mut self) {
        self.0.in_use.fetch_sub(1, Ordering::SeqCst);
        // The call may have written to the cache: Settings measures again.
        self.0.sizes.changed();
    }
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
            lods: Mutex::new(Default::default()),
            in_use: Default::default(),
            sizes: Default::default(),
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

/// The print processing's notes as one line (`None` without any).
fn print_note(notes: &[String]) -> Option<String> {
    (!notes.is_empty()).then(|| notes.join(" "))
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

    /// Mark the asset cache in use until the guard drops: "Clear asset cache" is refused
    /// meanwhile. Public for tests.
    pub fn asset_cache_in_use(&self) -> AssetCacheUse<'_> {
        self.assets.in_use.fetch_add(1, Ordering::SeqCst);
        AssetCacheUse(&self.assets)
    }

    /// Why the asset cache cannot be cleared now (an asset job uses it), else `None`.
    pub(super) fn asset_cache_busy(&self) -> Option<String> {
        let busy = self.assets.in_use.load(Ordering::SeqCst) > 0
            || lock_or(&self.assets.export).is_some()
            || lock_or(&self.assets.worker)
                .as_ref()
                .is_some_and(|w| w.busy.load(Ordering::SeqCst));
        busy.then(|| {
            "An asset export or preview is running; clear the asset cache when it ends.".into()
        })
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
        Worker::spawn(self.capped(cmd), key.clone()).map_err(|e| err(ErrorCode::Internal, e))
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
        let send = {
            let mut cur = lock_or(&self.assets.export);
            let e = cur.as_mut().filter(|e| e.job == job)?;
            match e.id {
                Some(id) => (e.worker.clone(), id),
                // Stop before the worker has the request: sent when its id arrives.
                None => {
                    e.cancel = true;
                    return Some(Ok(()));
                }
            }
        };
        Some(
            send.0
                .call::<bool>(&AssetRequest::Cancel { job: send.1 })
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
        if lock_or(&self.assets.export).is_some() {
            return Err(err(ErrorCode::Busy, "Another asset export is running."));
        }
        let _in_use = self.asset_cache_in_use();
        let mut job = self.job_start(JobKind::Dev, Some(channel), 1, label);
        job.cancellable = true;
        let job_id = job.job.clone();
        {
            let mut cur = lock_or(&self.assets.export);
            if cur.is_some() {
                drop(cur);
                let e = err(ErrorCode::Busy, "Another asset export is running.");
                self.job_end(job, JobPhase::Failed, &e.message, Some(progress));
                return Err(e);
            }
            *cur = Some(ExportJob {
                job: job_id.clone(),
                worker: w.clone(),
                id: None,
                cancel: false,
            });
        }
        progress(job.clone());
        w.busy.store(true, Ordering::SeqCst);
        let cell = Mutex::new(job);
        let r = w.call_stream(
            req,
            &|id| {
                let stop = {
                    let mut cur = lock_or(&self.assets.export);
                    match cur.as_mut().filter(|e| e.job == job_id) {
                        Some(e) => {
                            e.id = Some(id);
                            e.cancel
                        }
                        None => false,
                    }
                };
                if stop {
                    let _ = w.call::<bool>(&AssetRequest::Cancel { job: id });
                }
            },
            &|p: ExportProgress| {
                let mut j = lock_or(&cell);
                j.steps = p.total.max(1);
                let label = if unit == "%" {
                    let pct = p.done * 100 / p.total.max(1);
                    match p.stage.as_deref() {
                        Some(s) => format!("{label} {pct} % · {s}"),
                        None => format!("{label} {pct} %"),
                    }
                } else {
                    format!("{} of {} {unit}", p.done, p.total)
                };
                self.job_step(&mut j, p.done.max(1), &label, Some(progress));
            },
        );
        w.busy.store(false, Ordering::SeqCst);
        *lock_or(&self.assets.export) = None;
        // The cache folder's overall cap (an export may have filled the asset cache).
        starorg_core::storage::enforce_configured(self.dirs(), &[]);
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
            self.asset_usage(&c)
        } else {
            assetcache::Usage::default()
        };
        let previous = lock_or(&self.assets.previous)
            .clone()
            .filter(|p| p != c.root())
            .map(|p| {
                (
                    p.clone(),
                    self.asset_usage(&AssetCache::new(&p, c.max_bytes())),
                )
            })
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
        self.sizes_changed();
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

    fn lods_key(key: &WorkerKey, what: &str) -> (String, String, String) {
        (
            key.channel.clone(),
            key.build.clone(),
            what.to_ascii_lowercase(),
        )
    }

    /// The mesh LODs of `what`, once an export read them (empty otherwise).
    /// In memory for the session, else what the asset cache recorded (`model::cached_lods`).
    fn known_lods(&self, key: &WorkerKey, what: &str) -> Vec<u32> {
        if let Some(l) = lock_or(&self.assets.lods).get(&Self::lods_key(key, what)) {
            return l.clone();
        }
        let l = self
            .asset_cache()
            .map(|c| model::cached_lods(&c, &key.channel, &key.build, what))
            .unwrap_or_default();
        if !l.is_empty() {
            lock_or(&self.assets.lods).insert(Self::lods_key(key, what), l.clone());
        }
        l
    }

    fn remember_lods(&self, key: &WorkerKey, what: &str, lods: &[u32]) {
        if !lods.is_empty() {
            lock_or(&self.assets.lods).insert(Self::lods_key(key, what), lods.to_vec());
            if let Ok(c) = self.asset_cache() {
                let _ = model::cache_lods(&c, &key.channel, &key.build, what, lods);
            }
        }
    }

    /// The memory guard before an export needing `need` (addendum 7.18): over the limit is
    /// refused; under it but over the free memory asks first unless `force`.
    fn model_guard(&self, what: &str, need: u64, force: bool) -> CmdResult<()> {
        match memlimit::check(need, self.memory_limit(), memlimit::meminfo().available) {
            memlimit::Fit::OverLimit { need, limit } => Err(err(
                ErrorCode::Invalid,
                memlimit::over_limit_message(what, need, limit),
            )),
            memlimit::Fit::LowFree { need, free } if !force => Err(err(
                ErrorCode::NeedsDecision,
                memlimit::low_free_message(what, need, free),
            )),
            _ => Ok(()),
        }
    }

    /// Where the worker writes a sound (it sets the extension: `ogg`, `opus`, `wav`, `wem`).
    fn sound_path(&self, c: &AssetCache, key: &WorkerKey, s: &SoundRef) -> PathBuf {
        c.path(&key.channel, &key.build, Kind::Audio, &s.key(), "wem")
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
        let _in_use = self.asset_cache_in_use();
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
        let _in_use = self.asset_cache_in_use();
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

    fn asset_model_paints(&self, req: AssetRecordRequest) -> CmdResult<ModelPaints> {
        let w = self.asset_worker(&req.target)?;
        // The first record request reads the DataCore: a memory peak.
        let ready = w
            .call::<WorkerStatus>(&AssetRequest::Status)
            .is_ok_and(|s| s.datacore);
        let _x = if ready {
            None
        } else {
            Some(wait_lock(&self.dirs().extract_lock())?)
        };
        w.busy.store(true, Ordering::SeqCst);
        let r = w.call(&AssetRequest::Paints { what: req.what });
        w.busy.store(false, Ordering::SeqCst);
        r
    }

    fn asset_sound_search(&self, req: AssetSoundSearchRequest) -> CmdResult<TriggerSearch> {
        let w = self.asset_worker(&req.target)?;
        // The first one reads the audio index (GameAudio documents and bank names) only.
        let ready = w
            .call::<WorkerStatus>(&AssetRequest::Status)
            .is_ok_and(|s| s.audio);
        let _x = if ready {
            None
        } else {
            Some(wait_lock(&self.dirs().extract_lock())?)
        };
        w.call(&AssetRequest::SoundSearch {
            query: req.query,
            limit: req.limit.min(500),
        })
    }

    fn asset_trigger(&self, req: AssetTriggerRequest) -> CmdResult<TriggerInfo> {
        let w = self.asset_worker(&req.target)?;
        let ready = w
            .call::<WorkerStatus>(&AssetRequest::Status)
            .is_ok_and(|s| s.audio);
        let _x = if ready {
            None
        } else {
            Some(wait_lock(&self.dirs().extract_lock())?)
        };
        w.call(&AssetRequest::Trigger { name: req.name })
    }

    fn asset_sound(&self, req: AssetSoundRequest) -> CmdResult<AssetAudio> {
        let _in_use = self.asset_cache_in_use();
        let (_, key) = self.asset_target(&req.target)?;
        let c = self.asset_cache()?;
        let skey = req.sound.key();
        let hint = webview_audio_hint();
        let playable =
            |path: &Path, ext: &str, cached: bool, duration: Option<f64>, codec: String| {
                let b = std::fs::read(path).map_err(io)?;
                Ok(AssetAudio {
                    key: skey.clone(),
                    mime: wemcodec::mime_of(ext).into(),
                    bytes: b.len() as u64,
                    data: assetcache::base64(&b),
                    duration,
                    cached,
                    note: None,
                    format: codec_label(&codec, ext),
                    hint: hint.clone(),
                })
            };
        // A kept WEM: why, from its codec (read back from the cached file when cached).
        let kept = |bytes: u64, cached: bool, codec: String, note: Option<String>| AssetAudio {
            key: skey.clone(),
            mime: String::new(),
            data: String::new(),
            bytes,
            duration: None,
            cached,
            note: Some(
                note.unwrap_or_else(|| "It cannot be played here; export it as .wem".into()),
            ),
            format: codec_label(&codec, "wem"),
            hint: None,
        };
        if let Some((hit, ext)) =
            starorg_data::assetworker::cached_sound(&c, &key.channel, &key.build, &req.sound)
        {
            if ext != "wem" {
                return playable(&hit, ext, true, None, String::new());
            }
            // A kept WEM (cached_sound passes only codecs this build does not convert).
            let n = std::fs::metadata(&hit).map(|m| m.len()).unwrap_or(0);
            let codec = starorg_data::assetworker::kept_codec(&hit);
            let note = codec.map(|c| {
                wemcodec::Unconverted {
                    codec: Some(c),
                    reason: "not supported".into(),
                }
                .message()
            });
            let name = codec.map(|c| c.to_string()).unwrap_or_default();
            return Ok(kept(n, true, name, note));
        }
        let w = self.asset_worker(&req.target)?;
        c.prepare(&key.channel, &key.build, Kind::Audio)
            .map_err(io)?;
        let out = self.sound_path(&c, &key, &req.sound);
        let d: SoundDone = w.call(&AssetRequest::Sound {
            sound: req.sound,
            out: out.clone(),
            raw: false,
        })?;
        if d.ext != "wem" && out.exists() {
            // A WEM an older build kept: the converted file replaces it.
            let _ = std::fs::remove_file(&out);
        }
        let _ = c.written(&key.channel, &key.build);
        if d.ext == "wem" {
            Ok(kept(d.bytes, false, d.codec, d.note))
        } else {
            playable(Path::new(&d.path), &d.ext, false, d.duration, d.codec)
        }
    }

    fn asset_export_sounds(
        &self,
        req: AssetExportSoundsRequest,
        progress: Progress<'_>,
    ) -> CmdResult<AssetExportResult> {
        let _in_use = self.asset_cache_in_use();
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
        let mut formats = std::collections::BTreeMap::<String, u32>::new();
        let mut todo = Vec::new();
        for p in &picks {
            let hit = (!req.wem)
                .then(|| {
                    starorg_data::assetworker::cached_sound(&c, &key.channel, &key.build, &p.sound)
                })
                .flatten();
            match hit.map(|(h, ext)| {
                let name = starorg_data::assets::audio::file_name(&p.trigger, &p.sound, ext);
                std::fs::create_dir_all(&dir)
                    .and_then(|()| std::fs::copy(&h, dir.join(&name)))
                    .map(|n| (n, ext))
            }) {
                Some(Ok((n, ext))) => {
                    files += 1;
                    bytes += n;
                    *formats.entry(ext.to_string()).or_default() += 1;
                }
                _ => todo.push((p.trigger.clone(), p.sound.clone())),
            }
        }
        let mut failed = Vec::new();
        let mut kept = Vec::new();
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
            for (e, n) in &d.formats {
                *formats.entry(e.clone()).or_default() += n;
            }
            kept = d.kept.clone();
            starorg_data::assetworker::cache_written(&c, &key.channel, &key.build, &d);
        }
        self.log(
            LogLevel::Info,
            Some(JobKind::Dev),
            Some(&ch.name),
            &format!(
                "Exported {files} sound files ({}) to {}",
                SoundsDone::formats_line(&formats),
                dir.display()
            ),
        );
        for k in &kept {
            self.log(
                LogLevel::Info,
                Some(JobKind::Dev),
                Some(&ch.name),
                &format!("Kept as .wem: {k}"),
            );
        }
        let mut r = export_result(Some(dir.display().to_string()), files, bytes, cached);
        r.failed = failed;
        r.note = export_note(&formats, kept.len());
        Ok(r)
    }

    fn asset_export_model(
        &self,
        req: AssetExportModelRequest,
        progress: Progress<'_>,
    ) -> CmdResult<AssetExportResult> {
        let _in_use = self.asset_cache_in_use();
        let (ch, key) = self.asset_target(&req.target)?;
        // A LOD the mesh does not have is the one below it (known after a first export).
        let asked = req.lod.min(model::MAX_LOD);
        let o = ModelOptions {
            preset: req.preset,
            lod: model::effective_lod(asked, &self.known_lods(&key, &req.what)),
            stl: req.stl,
            three_mf: req.three_mf,
            paint: req.paint.clone(),
            layers: req.layers,
            print: req.print.clone(),
        };
        if o.preset == model::ModelPreset::PrintShell {
            o.print.validate().map_err(|m| err(ErrorCode::Invalid, m))?;
        }
        let name = if req.name.is_empty() {
            &req.what
        } else {
            &req.name
        };
        // The memory limit (addendum 7.18): over it is refused; under it but over the free
        // memory asks first ("Export anyway"), and the fresh worker runs under the cap.
        let need = model::memory_need(&o, name);
        match memlimit::check(need, self.memory_limit(), memlimit::meminfo().available) {
            memlimit::Fit::Ok => {}
            memlimit::Fit::OverLimit { need, limit } => {
                return Err(err(
                    ErrorCode::Invalid,
                    memlimit::over_limit_message("Exporting this model", need, limit),
                ));
            }
            memlimit::Fit::LowFree { need, free } if !req.force => {
                return Err(err(
                    ErrorCode::NeedsDecision,
                    memlimit::low_free_message("This export", need, free),
                ));
            }
            memlimit::Fit::LowFree { .. } => {}
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
        let mkey = o.cache_key(&req.what);
        if let Some(hit) = c.get(&key.channel, &key.build, Kind::Models, &mkey, "glb") {
            let n = std::fs::copy(&hit, &out).map_err(io)?;
            let mut files = 1;
            let mut bytes = n;
            let mut notes = Vec::new();
            if (o.stl || o.three_mf) && model::print_memory(&o) == 0 {
                // A plain conversion (no voxel solid): cheap enough to do here.
                let glb = std::fs::read(&hit).map_err(io)?;
                let m = starorg_data::assets::print::write_meshes(&glb, &out, &o)
                    .map_err(|e| worker_error(&e.into()))?;
                let (n, len) = m.files_bytes();
                files += n;
                bytes += len;
                notes = m.notes;
            } else if o.stl || o.three_mf {
                // The voxel steps take seconds to minutes and GBs: a fresh worker under the
                // memory cap, with progress and cancel, on the copy just made.
                let _x = wait_lock(&self.dirs().extract_lock())?;
                let w = self.spawn_asset(&ch, &key)?;
                let r = self.asset_stream(
                    &w,
                    &ch.name,
                    &format!("Processing model {name} for printing"),
                    &AssetRequest::PrintModel {
                        glb: out.clone(),
                        options: o.clone(),
                        out: out.clone(),
                    },
                    "%",
                    progress,
                );
                w.stop();
                drop(_x);
                let d: model::PrintDone = serde_json::from_value(r?).map_err(internal)?;
                let (n, len) = starorg_data::assets::print::files_bytes(&d.meshes);
                files += n;
                bytes += len;
                notes = d.notes;
                self.log(
                    LogLevel::Info,
                    Some(JobKind::Dev),
                    Some(&ch.name),
                    &format!(
                        "Processed cached model {name} for printing ({} ms, peak memory {} MB)",
                        d.ms,
                        d.peak_rss.map(|b| b / 1_048_576).unwrap_or(0)
                    ),
                );
            }
            let mut r = export_result(Some(out.display().to_string()), files, bytes, true);
            r.note = print_note(&notes);
            return Ok(r);
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
                options: o.clone(),
                out: out.clone(),
            },
            "%",
            progress,
        );
        w.stop();
        drop(_x);
        let d: ModelDone = serde_json::from_value(r?).map_err(internal)?;
        self.remember_lods(&key, &req.what, &d.lods);
        let mkey = ModelOptions {
            lod: d.lod,
            ..o.clone()
        }
        .cache_key(&req.what);
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
        let (n, len) = starorg_data::assets::print::files_bytes(&d.meshes);
        let (files, bytes) = (1 + n, d.bytes + len);
        let mut r = export_result(Some(d.path), files, bytes, false);
        r.note = print_note(&d.notes);
        Ok(r)
    }

    fn asset_model_info(&self, req: AssetModelInfoRequest) -> CmdResult<AssetModelInfo> {
        let (_, key) = self.asset_target(&req.target)?;
        let c = self.asset_cache()?;
        let name = if req.name.is_empty() {
            &req.what
        } else {
            &req.name
        };
        let known = self.known_lods(&key, &req.what);
        let limit = self.memory_limit();
        let free = memlimit::meminfo().available;
        let lods = (0..=model::MAX_LOD)
            .map(|lod| {
                let o = view_options(model::effective_lod(lod, &known));
                let need = model::memory_need(&o, name);
                let hit = c.peek(
                    &key.channel,
                    &key.build,
                    Kind::Models,
                    &o.cache_key(&req.what),
                    "glb",
                );
                let bytes = hit
                    .as_ref()
                    .and_then(|p| std::fs::metadata(p).ok())
                    .map(|m| m.len());
                let triangles = hit
                    .as_deref()
                    .and_then(starorg_data::assets::stl::glb_file_triangle_count);
                AssetModelLod {
                    lod,
                    uses: o.lod,
                    need,
                    fit: memory_fit(memlimit::check(need, limit, free)),
                    cached: bytes.is_some(),
                    bytes,
                    triangles,
                }
            })
            .collect();
        Ok(AssetModelInfo {
            what: req.what,
            lods,
            default_lod: model::view_lod(&known),
            available: known,
            limit,
            free,
            max_bytes: c.max_bytes() / 4,
        })
    }

    fn asset_model_view(
        &self,
        req: AssetModelViewRequest,
        progress: Progress<'_>,
    ) -> CmdResult<AssetModelView> {
        let _in_use = self.asset_cache_in_use();
        let (ch, key) = self.asset_target(&req.target)?;
        let c = self.asset_cache()?;
        let name = if req.name.is_empty() {
            &req.what
        } else {
            &req.name
        };
        let asked = req.lod.min(model::MAX_LOD);
        let known = self.known_lods(&key, &req.what);
        let o = view_options(model::effective_lod(asked, &known));
        let path = |o: &ModelOptions| {
            c.path(
                &key.channel,
                &key.build,
                Kind::Models,
                &o.cache_key(&req.what),
                "glb",
            )
        };
        if let Some(hit) = c.get(
            &key.channel,
            &key.build,
            Kind::Models,
            &o.cache_key(&req.what),
            "glb",
        ) {
            let bytes = std::fs::metadata(&hit).map_err(io)?.len();
            return Ok(AssetModelView {
                what: req.what,
                lod: o.lod,
                asked,
                available: known,
                bytes,
                cached: true,
                triangles: starorg_data::assets::stl::glb_file_triangle_count(&hit),
                ms: None,
                peak_rss: None,
                chunk: MODEL_CHUNK,
            });
        }
        self.model_guard(
            "Viewing this model",
            model::memory_need(&o, name),
            req.force,
        )?;
        c.prepare(&key.channel, &key.build, Kind::Models)
            .map_err(io)?;
        // The worker writes straight into the cache (via `.part`), then moves it to the LOD
        // it has.
        let out = path(&o);
        let _x = wait_lock(&self.dirs().extract_lock())?;
        let w = self.spawn_asset(&ch, &key)?;
        let r = self.asset_stream(
            &w,
            &ch.name,
            &format!("Loading model {name} (LOD {asked})"),
            &AssetRequest::ExportModel {
                what: req.what.clone(),
                options: o.clone(),
                out: out.clone(),
            },
            "%",
            progress,
        );
        w.stop();
        drop(_x);
        let d: ModelDone = serde_json::from_value(r?).map_err(internal)?;
        self.remember_lods(&key, &req.what, &d.lods);
        let max = c.max_bytes() / 4;
        let have = view_options(d.lod);
        if d.bytes > max {
            let _ = std::fs::remove_file(&out);
            return Err(err(
                ErrorCode::Invalid,
                format!(
                    "This model is {} at LOD {}, more than the viewer keeps ({}: a quarter of the asset cache's size). Pick less detail, raise the cache size, or use Export model.",
                    memlimit::human(d.bytes),
                    d.lod,
                    memlimit::human(max)
                ),
            ));
        }
        if have.lod != o.lod {
            c.commit(
                &key.channel,
                &key.build,
                Kind::Models,
                &have.cache_key(&req.what),
                "glb",
                &out,
            )
            .map_err(io)?;
        } else {
            let _ = c.written(&key.channel, &key.build);
        }
        self.log(
            LogLevel::Info,
            Some(JobKind::Dev),
            Some(&ch.name),
            &format!(
                "Model view {} at LOD {} ({} bytes, {} triangles, {} ms, peak memory {} MB)",
                d.name,
                d.lod,
                d.bytes,
                d.triangles.unwrap_or(0),
                d.ms,
                d.peak_rss.map(|b| b / 1_048_576).unwrap_or(0)
            ),
        );
        Ok(AssetModelView {
            what: req.what,
            lod: d.lod,
            asked,
            available: d.lods,
            bytes: d.bytes,
            cached: false,
            triangles: d.triangles,
            ms: Some(d.ms),
            peak_rss: d.peak_rss,
            chunk: MODEL_CHUNK,
        })
    }

    fn asset_model_chunk(&self, req: AssetModelChunkRequest) -> CmdResult<Vec<u8>> {
        use std::io::{Read, Seek, SeekFrom};
        let (_, key) = self.asset_target(&req.target)?;
        let c = self.asset_cache()?;
        let o = view_options(req.lod.min(model::MAX_LOD));
        let Some(p) = c.peek(
            &key.channel,
            &key.build,
            Kind::Models,
            &o.cache_key(&req.what),
            "glb",
        ) else {
            return Err(err(
                ErrorCode::NotFound,
                "this model is not in the asset cache (it may have been cleared): load it again",
            ));
        };
        let mut f = std::fs::File::open(&p).map_err(io)?;
        let size = f.metadata().map_err(io)?.len();
        if req.offset > size {
            return Err(err(
                ErrorCode::Invalid,
                format!("offset {} is past the end ({size} bytes)", req.offset),
            ));
        }
        let len = u64::from(req.len.min(MODEL_CHUNK)).min(size - req.offset);
        f.seek(SeekFrom::Start(req.offset)).map_err(io)?;
        let mut buf = Vec::with_capacity(len as usize);
        f.take(len).read_to_end(&mut buf).map_err(io)?;
        Ok(buf)
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
        let u = self.asset_cache()?.clear();
        self.sizes_changed();
        let u = u.map_err(io)?;
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
        let u = AssetCache::new(&prev, cur.max_bytes()).move_to(&cur);
        self.sizes_changed();
        let u = u.map_err(io)?;
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

/// What the Model tab shows: the Print shell (geometry only, no interior or lights).
fn view_options(lod: u32) -> ModelOptions {
    ModelOptions {
        preset: ModelPreset::PrintShell,
        lod,
        stl: false,
        ..Default::default()
    }
}

/// `Vorbis (.ogg)`; the extension alone when the codec is not known (a cached file).
fn codec_label(codec: &str, ext: &str) -> String {
    match (codec.is_empty(), ext) {
        (true, "ogg") => "Ogg Vorbis (.ogg)".into(),
        (true, "opus") => "Ogg Opus (.opus)".into(),
        (true, "wav") => "WAV (.wav)".into(),
        (true, x) => format!(".{x}"),
        (false, x) => format!("{codec} (.{x})"),
    }
}

/// `2305 .ogg, 23 .wem (kept as .wem: their codec or stream is not converted; see the log)`.
fn export_note(formats: &std::collections::BTreeMap<String, u32>, kept: usize) -> Option<String> {
    if formats.is_empty() {
        return None;
    }
    let line = SoundsDone::formats_line(formats);
    Some(if kept > 0 || formats.contains_key("wem") {
        format!("{line} (the .wem files were not converted; the log says why)")
    } else {
        line
    })
}

/// Why the webview may not play a sound it is handed (addendum 7.16). On Linux the webview is
/// WebKitGTK, which plays `<audio>` through GStreamer and needs `autoaudiosink` (and
/// `wavparse` for `.wav`) from gst-plugins-good: without it every sound fails with
/// "NotSupportedError". `None` when the plugin is there or the plugin folders are not found.
pub fn webview_audio_hint() -> Option<String> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    gst_hint(&gst_plugin_dirs())
}

fn gst_plugin_dirs() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = Vec::new();
    for k in [
        "GST_PLUGIN_SYSTEM_PATH_1_0",
        "GST_PLUGIN_SYSTEM_PATH",
        "GST_PLUGIN_PATH_1_0",
        "GST_PLUGIN_PATH",
    ] {
        if let Some(p) = std::env::var_os(k) {
            v.extend(std::env::split_paths(&p));
        }
    }
    for d in [
        "/usr/lib/gstreamer-1.0",
        "/usr/lib64/gstreamer-1.0",
        "/usr/lib/x86_64-linux-gnu/gstreamer-1.0",
        "/usr/lib/aarch64-linux-gnu/gstreamer-1.0",
        "/usr/local/lib/gstreamer-1.0",
    ] {
        v.push(PathBuf::from(d));
    }
    v
}

/// The hint for these GStreamer plugin folders (see [`webview_audio_hint`]).
pub fn gst_hint(dirs: &[PathBuf]) -> Option<String> {
    let found: Vec<&PathBuf> = dirs.iter().filter(|d| d.is_dir()).collect();
    if found.is_empty() {
        return None;
    }
    let has = |lib: &str| found.iter().any(|d| d.join(lib).exists());
    if has("libgstautodetect.so") {
        return None;
    }
    Some(
        "The app's webview (WebKitGTK) plays sounds through GStreamer, and its audio output \
         (autoaudiosink, in gst-plugins-good) is not installed, so no sound can play here. \
         Install gst-plugins-good (Arch: pacman -S gst-plugins-good; Debian/Ubuntu: \
         gstreamer1.0-plugins-good) and restart the app, or export the sound and play it \
         in another player."
            .into(),
    )
}

#[cfg(test)]
mod sound_tests {
    use super::*;

    #[test]
    fn the_gstreamer_hint_names_the_missing_plugin() {
        let t = tempfile::tempdir().unwrap();
        let none = t.path().join("nope");
        // No plugin folder found: nothing is known.
        assert_eq!(gst_hint(std::slice::from_ref(&none)), None);
        let dir = t.path().join("gstreamer-1.0");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("libgstogg.so"), b"").unwrap();
        let h = gst_hint(&[none.clone(), dir.clone()]).unwrap();
        assert!(
            h.contains("gst-plugins-good") && h.contains("autoaudiosink"),
            "{h}"
        );
        std::fs::write(dir.join("libgstautodetect.so"), b"").unwrap();
        assert_eq!(gst_hint(&[none, dir]), None);
    }

    #[test]
    fn labels_and_export_notes_name_the_formats() {
        assert_eq!(codec_label("Vorbis", "ogg"), "Vorbis (.ogg)");
        assert_eq!(codec_label("", "opus"), "Ogg Opus (.opus)");
        assert_eq!(codec_label("fmt 0x8311", "wem"), "fmt 0x8311 (.wem)");
        let mut f = std::collections::BTreeMap::new();
        assert_eq!(export_note(&f, 0), None);
        f.insert("ogg".to_string(), 2305);
        assert_eq!(export_note(&f, 0).as_deref(), Some("2305 .ogg"));
        f.insert("wem".to_string(), 1);
        assert_eq!(
            export_note(&f, 1).as_deref(),
            Some("2305 .ogg, 1 .wem (the .wem files were not converted; the log says why)")
        );
    }
}

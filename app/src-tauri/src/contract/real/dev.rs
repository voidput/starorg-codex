//! Developer mode in the real backend (addendum 7): one `dev-worker` child process for the
//! selected channel, datasets from our GameData and local folders, and the dataset diff.
//!
//! - The worker is this binary (`starorg-strings-app dev-worker --p4k <P>`), started on the
//!   first Dev request, restarted when the channel or its build changes, stopped after
//!   [`IDLE`] without a request, when Developer mode is turned off, and when the app exits
//!   (its stdin closes; the worker exits on EOF). Starting holds `locks/extract.lock` until
//!   the worker is ready, so it never overlaps an extraction's peak.
//! - A worker crash is a `failed` status with the reason; the next request starts a new one.
//! - The webview never sends a path: exports and folders go through native dialogs here.

use super::dialogs::FileFilter;
use super::{Found, RealBackend, err, internal, io, now};
use crate::contract::Progress;
use crate::contract::dev::*;
use crate::contract::types::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use starorg_core::cache::KeyInputs;
use starorg_core::dev::dataset::{self, Dataset, RowsQuery};
use starorg_core::dev::diff::{self, TableDiff};
use starorg_core::dev::explorer::MAX_PAGE;
use starorg_core::dev::readable::{self, LabelKind, Lookup, MapLookup, NoLookup};
use starorg_core::dev::record;
use starorg_core::discovery::Channel;
use starorg_core::lock;
use starorg_data::devworker::{ExportDone, ExportProgress, Reply, Request, WorkerInfo};
use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};

/// The worker stops after this long without a request (7.2).
pub const IDLE: Duration = Duration::from_secs(5 * 60);
/// Longest wait for the worker to become ready.
const START_TIMEOUT: Duration = Duration::from_secs(180);
/// Longest wait for one reply (a 4 MB record is well under a second).
const REPLY_TIMEOUT: Duration = Duration::from_secs(60);
/// Longest gap between two progress lines of an export.
const EXPORT_GAP: Duration = Duration::from_secs(300);
/// Folders remembered as dataset sources.
pub const MAX_FOLDERS: usize = 5;
/// Parsed folder datasets kept (7.5).
const KEEP_DATASETS: usize = 2;
/// Label lookups kept, one per channel build (7.14).
const KEEP_LOOKUPS: usize = 3;
/// The dataset name of a build's GameData.
pub const GAMEDATA: &str = "gamedata.json";

const EXPORT_FILTER: FileFilter = FileFilter {
    name: "JSON",
    extensions: &["json"],
};

/// How the worker process is started (tests run the app binary on a synthetic DCB).
pub trait DevSpawner: Send + Sync {
    fn command(&self, ch: &Channel) -> Result<Command, String>;
}

/// This binary, `dev-worker --p4k <Data.p4k>`.
pub struct WorkerSpawner;

impl DevSpawner for WorkerSpawner {
    fn command(&self, ch: &Channel) -> Result<Command, String> {
        let exe = std::env::current_exe().map_err(|e| format!("cannot find own binary: {e}"))?;
        let mut c = Command::new(exe);
        c.arg("dev-worker").arg("--p4k").arg(&ch.p4k);
        Ok(c)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WorkerKey {
    pub(super) install: String,
    pub(super) channel: String,
    pub(super) build: String,
}

/// A running worker process and its reply routing.
pub struct Worker {
    pub(super) key: WorkerKey,
    stdin: Mutex<Option<ChildStdin>>,
    child: Mutex<Child>,
    pending: Mutex<HashMap<u64, mpsc::Sender<Reply>>>,
    next: AtomicU64,
    alive: AtomicBool,
    pub(super) busy: AtomicBool,
    pub(super) info: WorkerInfo,
    started: String,
    pub(super) last_used: Mutex<Instant>,
    /// Why it stopped, when it did by itself.
    pub(super) died: Mutex<Option<String>>,
    /// The memory limit it runs under (addendum 7.18), bytes.
    limit: Option<u64>,
}

/// Why a worker that ended by itself stopped: the memory limit's message when it was that
/// (`starorg_core::memlimit::exit_was_memory`), else `fallback`. Waits up to 2 s for it to
/// exit.
fn death_reason(child: &mut Child, limit: Option<u64>, fallback: &str) -> String {
    let until = Instant::now() + Duration::from_secs(2);
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(20)),
            _ => break None,
        }
    };
    match status {
        // Its stderr is not kept: only the watchdog's exit code counts as a memory stop.
        Some(s) if starorg_core::memlimit::exit_was_memory(&s, limit, None) => {
            starorg_core::memlimit::stopped_message(limit.unwrap_or_default())
        }
        Some(s) if !s.success() => starorg_core::memlimit::unexpected_message("the explorer", &s),
        _ => fallback.to_string(),
    }
}

pub(super) fn lock_or<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn worker_error(e: &starorg_data::devworker::ReplyError) -> AppError {
    let code = match e.code.as_str() {
        "not-found" => ErrorCode::NotFound,
        "invalid" => ErrorCode::Invalid,
        "busy" => ErrorCode::Busy,
        "cancelled" => ErrorCode::Cancelled,
        _ => ErrorCode::Internal,
    };
    err(code, e.message.clone())
}

impl Worker {
    /// Starts `cmd` (already capped by `RealBackend::capped`, whose limit it reads back).
    pub(super) fn spawn(mut cmd: Command, key: WorkerKey) -> Result<Arc<Worker>, String> {
        let limit = starorg_core::memlimit::command_limit(&cmd);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("cannot start the explorer: {e}"))?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().ok_or("no worker stdout")?;
        let (ready_tx, ready_rx) = mpsc::channel::<String>();
        let slot: Arc<Mutex<Option<Weak<Worker>>>> = Arc::new(Mutex::new(None));
        let reader_slot = slot.clone();
        std::thread::spawn(move || {
            let mut lines = BufReader::new(stdout).lines();
            match lines.next() {
                Some(Ok(l)) => {
                    let _ = ready_tx.send(l);
                }
                _ => {
                    let _ = ready_tx.send(String::new());
                    return;
                }
            }
            drop(ready_tx);
            // Wait until the parent has built the Worker.
            let w = loop {
                if let Some(w) = lock_or(&reader_slot).clone() {
                    break w;
                }
                std::thread::sleep(Duration::from_millis(5));
            };
            for line in lines {
                let Ok(line) = line else { break };
                let Ok(reply) = serde_json::from_str::<Reply>(&line) else {
                    continue;
                };
                let Some(w) = w.upgrade() else { return };
                let mut p = lock_or(&w.pending);
                let last = reply.progress.is_none();
                if let Some(tx) = p.get(&reply.id) {
                    let _ = tx.send(reply.clone());
                }
                if last {
                    p.remove(&reply.id);
                }
            }
            if let Some(w) = w.upgrade() {
                // Still marked alive: nobody stopped it, it ended by itself.
                let by_itself = w.alive.swap(false, Ordering::SeqCst);
                let why = if by_itself {
                    death_reason(&mut lock_or(&w.child), w.limit, "the explorer stopped")
                } else {
                    "the explorer stopped".into()
                };
                lock_or(&w.died).get_or_insert(why);
                // Every waiting call gets a closed channel.
                lock_or(&w.pending).clear();
            }
        });
        let first = match ready_rx.recv_timeout(START_TIMEOUT) {
            Ok(l) => l,
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("the explorer did not start in time".into());
            }
        };
        let ev: Value = serde_json::from_str(&first).unwrap_or(Value::Null);
        if first.is_empty() {
            // Stdout closed before the ready line: it died while starting.
            let why = death_reason(&mut child, limit, "the explorer stopped while starting");
            let _ = child.kill();
            let _ = child.wait();
            return Err(why);
        }
        if ev["event"] != "ready" {
            let _ = child.kill();
            let _ = child.wait();
            let why = ev["error"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| "the explorer stopped while starting".into());
            return Err(why);
        }
        let info: WorkerInfo =
            serde_json::from_value(ev["info"].clone()).map_err(|e| e.to_string())?;
        let w = Arc::new(Worker {
            key,
            stdin: Mutex::new(stdin),
            child: Mutex::new(child),
            pending: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
            alive: AtomicBool::new(true),
            busy: AtomicBool::new(false),
            info,
            started: now(),
            last_used: Mutex::new(Instant::now()),
            died: Mutex::new(None),
            limit,
        });
        *lock_or(&slot) = Some(Arc::downgrade(&w));
        Ok(w)
    }

    pub(super) fn alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    pub(super) fn touch(&self) {
        *lock_or(&self.last_used) = Instant::now();
    }

    /// "the explorer stopped; open it again", or why it stopped when that was the memory
    /// limit.
    fn stopped(&self, fallback: &str) -> AppError {
        let died = lock_or(&self.died).clone();
        match died {
            Some(d) if d.starts_with("Stopped: needed more than") => err(ErrorCode::Internal, d),
            _ => err(ErrorCode::Internal, fallback),
        }
    }

    fn send(&self, req: &impl Serialize) -> CmdResult<(u64, mpsc::Receiver<Reply>)> {
        if !self.alive() {
            return Err(self.stopped("the explorer stopped; open it again"));
        }
        self.touch();
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        lock_or(&self.pending).insert(id, tx);
        let mut line = serde_json::to_value(req).map_err(internal)?;
        line["id"] = Value::from(id);
        let text = serde_json::to_string(&line).map_err(internal)?;
        let mut stdin = lock_or(&self.stdin);
        let ok = stdin
            .as_mut()
            .is_some_and(|s| writeln!(s, "{text}").and_then(|_| s.flush()).is_ok());
        if !ok {
            lock_or(&self.pending).remove(&id);
            return Err(err(
                ErrorCode::Internal,
                "the explorer stopped; open it again",
            ));
        }
        Ok((id, rx))
    }

    fn finish(reply: Reply) -> CmdResult<Value> {
        match (reply.ok, reply.result, reply.error) {
            (true, Some(v), _) => Ok(v),
            (_, _, Some(e)) => Err(worker_error(&e)),
            _ => Err(err(ErrorCode::Internal, "empty reply from the explorer")),
        }
    }

    /// One request, one reply.
    pub fn call<T: serde::de::DeserializeOwned>(&self, req: &impl Serialize) -> CmdResult<T> {
        let (_, rx) = self.send(req)?;
        let reply = rx.recv_timeout(REPLY_TIMEOUT).map_err(|e| match e {
            mpsc::RecvTimeoutError::Timeout => {
                err(ErrorCode::Internal, "the explorer did not answer")
            }
            mpsc::RecvTimeoutError::Disconnected => {
                self.stopped("the explorer stopped; open it again")
            }
        })?;
        serde_json::from_value(Self::finish(reply)?).map_err(internal)
    }

    /// A request that streams progress; `on_start` gets the request id (for cancel).
    pub(super) fn call_stream(
        &self,
        req: &impl Serialize,
        on_start: &dyn Fn(u64),
        on_progress: &dyn Fn(ExportProgress),
    ) -> CmdResult<Value> {
        let (id, rx) = self.send(req)?;
        on_start(id);
        loop {
            let reply = rx
                .recv_timeout(EXPORT_GAP)
                .map_err(|_| self.stopped("the explorer stopped during the export"))?;
            self.touch();
            match reply.progress {
                Some(p) => on_progress(p),
                None => return Self::finish(reply),
            }
        }
    }

    /// Close stdin (the worker exits on EOF), then make sure it is gone.
    pub fn stop(&self) {
        self.alive.store(false, Ordering::SeqCst);
        lock_or(&self.stdin).take();
        let mut c = lock_or(&self.child);
        for _ in 0..40 {
            if matches!(c.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        let _ = c.kill();
        let _ = c.wait();
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Persisted in `app.json` (`dev`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DevFile {
    #[serde(default)]
    pub developer_mode: bool,
    /// Dataset folders, newest last, at most [`MAX_FOLDERS`].
    #[serde(default)]
    pub folders: Vec<String>,
}

struct DiffState {
    summary: DevDiffSummary,
    tables: Vec<TableDiff>,
    a: Vec<(String, Arc<Dataset>)>,
    b: Vec<(String, Arc<Dataset>)>,
    /// Label lookups of each side's build (7.14).
    la: Option<Arc<MapLookup>>,
    lb: Option<Arc<MapLookup>>,
}

/// (install, channel, GameData cache key, Strings table identity) of a lookup.
type LookupKey = (String, String, String, usize);

/// Headline stats of list rows by record GUID (7.17).
type StatsByGuid = HashMap<String, Vec<Stat>>;

/// File identity for the parse cache: re-read when size or mtime change.
type FileKey = (PathBuf, u64, u128);

#[derive(Default)]
struct DataCache {
    files: Vec<(FileKey, Arc<Dataset>)>,
    /// Table/row counts per file, for the source list.
    summaries: HashMap<FileKey, (u32, u32)>,
    builds: HashMap<(String, String), (String, Arc<Dataset>)>,
}

pub struct DevState {
    spawner: Mutex<Arc<dyn DevSpawner>>,
    worker: Mutex<Option<Arc<Worker>>>,
    /// The last start failure, per (install, channel).
    failed: Mutex<Option<(String, String, String)>>,
    start: Mutex<()>,
    data: Mutex<DataCache>,
    diff: Mutex<Option<DiffState>>,
    /// The running type export: (job id, worker, request id).
    export: Mutex<Option<(String, Arc<Worker>, u64)>>,
    lookups: Mutex<Vec<(LookupKey, Arc<MapLookup>)>>,
    /// Headline stats of the list rows (7.17), per (install, channel, GameData key).
    stats: Mutex<Option<(LookupKey, Arc<StatsByGuid>)>>,
    /// The comparison the current worker holds marks for: (worker, result).
    compared: Mutex<Option<(Weak<Worker>, DevCompare)>>,
}

impl Default for DevState {
    fn default() -> Self {
        Self {
            spawner: Mutex::new(Arc::new(WorkerSpawner)),
            worker: Mutex::new(None),
            failed: Mutex::new(None),
            start: Mutex::new(()),
            data: Mutex::new(DataCache::default()),
            diff: Mutex::new(None),
            export: Mutex::new(None),
            lookups: Mutex::new(Vec::new()),
            stats: Mutex::new(None),
            compared: Mutex::new(None),
        }
    }
}

impl DevState {
    pub fn set_spawner(&self, s: Arc<dyn DevSpawner>) {
        *lock_or(&self.spawner) = s;
    }

    /// Stop the worker (Developer mode off, or `dev_close`).
    pub fn stop(&self) {
        if let Some(w) = lock_or(&self.worker).take() {
            w.stop();
        }
    }
}

fn file_key(p: &Path) -> Option<FileKey> {
    let m = std::fs::metadata(p).ok()?;
    let t = m
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some((p.to_path_buf(), m.len(), t))
}

/// `*.json` directly inside, sorted by name, each at most the per-file cap; the folder total
/// is capped too (7.10).
fn json_files(dir: &Path) -> Vec<(String, PathBuf, u64)> {
    let mut v: Vec<(String, PathBuf, u64)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let name = p.file_name()?.to_str()?.to_string();
            let meta = e.metadata().ok()?;
            (meta.is_file()
                && name.to_ascii_lowercase().ends_with(".json")
                && meta.len() <= dataset::MAX_FILE)
                .then_some((name, p, meta.len()))
        })
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    let mut total = 0u64;
    v.retain(|f| {
        total += f.2;
        total <= dataset::MAX_FOLDER
    });
    v
}

/// GameData as a dataset: the model's tables; schema, build and DataCore counts as metadata.
fn gamedata_dataset(gd: &starorg_core::model::GameData) -> Dataset {
    let mut v = serde_json::to_value(gd).unwrap_or(Value::Null);
    if let Value::Object(m) = &mut v {
        for k in ["schema", "build", "datacore"] {
            if let Some(x) = m.shift_remove(k) {
                m.insert(format!("_{k}"), x);
            }
        }
    }
    let bytes = serde_json::to_string(&v)
        .map(|s| s.len() as u64)
        .unwrap_or(0);
    dataset::infer(GAMEDATA, bytes, v)
}

impl RealBackend {
    pub fn with_dev_spawner(self, s: Arc<dyn DevSpawner>) -> Self {
        self.dev.set_spawner(s);
        self
    }

    fn dev_on(&self) -> CmdResult<()> {
        if self.app_file().dev.developer_mode {
            Ok(())
        } else {
            Err(disabled())
        }
    }

    pub(super) fn dev_channel(&self, r: &ChannelRef) -> CmdResult<(Found, Channel, WorkerKey)> {
        self.dev_on()?;
        let f = self.resolve(r)?;
        let ch = f.channel(&r.channel)?.clone();
        let fp = ch
            .fingerprint()
            .map_err(|e| err(ErrorCode::NotFound, format!("{}: {e}", ch.p4k.display())))?;
        let build = KeyInputs::local(&fp, &self.ctx().starbreaker_commit).key();
        let key = WorkerKey {
            install: f.ui_id(),
            channel: ch.name.clone(),
            build,
        };
        Ok((f, ch, key))
    }

    /// The worker for this channel's current build, started when needed.
    fn worker(&self, r: &ChannelRef, progress: Option<Progress<'_>>) -> CmdResult<Arc<Worker>> {
        let (_, ch, key) = self.dev_channel(r)?;
        let _one = lock_or(&self.dev.start);
        {
            let mut cur = lock_or(&self.dev.worker);
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
        let mut job = self.job_start(JobKind::Dev, Some(&ch.name), 2, "Opening the game data");
        if let Some(p) = progress {
            p(job.clone());
        }
        let started = (|| {
            let _x = lock::try_lock(&self.dirs().extract_lock())
                .map_err(|e| super::busy_or(e, "Reading game data"))?;
            self.job_step(&mut job, 2, "Indexing records", progress);
            let cmd = lock_or(&self.dev.spawner).command(&ch).map_err(internal)?;
            Worker::spawn(self.capped(cmd), key.clone()).map_err(|e| err(ErrorCode::Internal, e))
        })();
        match started {
            Ok(w) => {
                *lock_or(&self.dev.failed) = None;
                *lock_or(&self.dev.worker) = Some(w.clone());
                self.spawn_idle_watch(&w);
                self.log(
                    LogLevel::Info,
                    Some(JobKind::Dev),
                    Some(&ch.name),
                    &format!(
                        "Explorer opened: {} records in {} ms",
                        w.info.records, w.info.start_ms
                    ),
                );
                self.job_end(job, JobPhase::Done, "Explorer ready", progress);
                Ok(w)
            }
            Err(e) => {
                *lock_or(&self.dev.failed) =
                    Some((key.install.clone(), key.channel.clone(), e.message.clone()));
                self.log(
                    LogLevel::Warn,
                    Some(JobKind::Dev),
                    Some(&ch.name),
                    &e.message,
                );
                self.job_end(job, JobPhase::Failed, &e.message, progress);
                Err(e)
            }
        }
    }

    /// Stop an idle worker after [`IDLE`] (not while it exports).
    fn spawn_idle_watch(&self, w: &Arc<Worker>) {
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

    /// Labels of `values` from the running explorer (none started for this), added to `out`.
    fn explorer_labels(
        &self,
        target: &ChannelRef,
        mut values: Vec<String>,
        out: &mut BTreeMap<String, Readable>,
    ) {
        values.truncate(MAX_PAGE);
        if values.is_empty() {
            return;
        }
        let Some(w) = self.worker_if_ready(target) else {
            return;
        };
        let got: Vec<Option<Readable>> = w
            .call(&Request::Labels {
                values: values.clone(),
            })
            .unwrap_or_default();
        out.extend(
            values
                .into_iter()
                .zip(got)
                .filter_map(|(g, r)| Some((g, r?))),
        );
    }

    /// The running worker for this target, without starting one.
    fn worker_if_ready(&self, r: &ChannelRef) -> Option<Arc<Worker>> {
        let (_, _, key) = self.dev_channel(r).ok()?;
        lock_or(&self.dev.worker)
            .as_ref()
            .filter(|w| w.alive() && w.key == key)
            .cloned()
    }

    fn status_of(&self, r: &ChannelRef) -> CmdResult<DevStatus> {
        let (f, ch, key) = self.dev_channel(r)?;
        let build = ch
            .build
            .as_ref()
            .map(|b| b.version.clone())
            .filter(|v| !v.is_empty());
        let mut s = DevStatus {
            state: WorkerState::Stopped,
            channel: Some(ch.name.clone()),
            build,
            records: 0,
            main: 0,
            types: 0,
            started: None,
            start_ms: None,
            memory: None,
            error: None,
            sections: None,
            compared: None,
        };
        let w = lock_or(&self.dev.worker).clone();
        if let Some(w) = w.filter(|w| w.key == key) {
            if w.alive() {
                s.state = WorkerState::Ready;
                s.records = w.info.records;
                s.main = w.info.main;
                s.types = w.info.types;
                s.started = Some(w.started.clone());
                s.start_ms = Some(w.info.start_ms);
                let now = w.call::<WorkerInfo>(&Request::Status).ok();
                s.memory = now.as_ref().and_then(|i| i.rss).or(w.info.rss);
                s.sections = now.and_then(|i| i.sections).or(w.info.sections.clone());
                s.compared = self.compared_for(&w);
                return Ok(s);
            }
            let why = lock_or(&w.died).clone();
            if why.as_deref() != Some("stopped after 5 idle minutes") {
                s.state = WorkerState::Failed;
                s.error = why;
            }
            return Ok(s);
        }
        if let Some((i, c, e)) = lock_or(&self.dev.failed).clone()
            && i == f.ui_id()
            && c == ch.name
        {
            s.state = WorkerState::Failed;
            s.error = Some(e);
        }
        Ok(s)
    }

    /// The comparison the worker `w` holds marks for.
    fn compared_for(&self, w: &Arc<Worker>) -> Option<DevCompare> {
        lock_or(&self.dev.compared)
            .as_ref()
            .filter(|(x, _)| x.upgrade().is_some_and(|x| Arc::ptr_eq(&x, w)))
            .map(|x| x.1.clone())
    }

    /// Headline stats by record GUID of the channel's build (7.17); empty while its
    /// GameData was not read (nothing is extracted for this).
    fn list_stats(&self, r: &ChannelRef) -> Arc<StatsByGuid> {
        let Some((f, ch)) = self
            .resolve(r)
            .ok()
            .and_then(|f| f.channel(&r.channel).ok().cloned().map(|c| (f, c)))
        else {
            return Arc::default();
        };
        let Ok(data) = self.loaded(&f, &ch, false) else {
            return Arc::default();
        };
        let key: LookupKey = (f.ui_id(), ch.name.clone(), data.key.clone(), 0);
        if let Some((k, m)) = lock_or(&self.dev.stats).as_ref()
            && *k == key
        {
            return m.clone();
        }
        let m = Arc::new(starorg_core::dev::sections::stats::by_guid(&data.game_data));
        *lock_or(&self.dev.stats) = Some((key, m.clone()));
        m
    }

    /// Cancel the running type export, when `job` is it.
    pub(super) fn dev_cancel(&self, job: &str) -> Option<CmdResult<()>> {
        let cur = lock_or(&self.dev.export).clone();
        let (j, w, id) = cur.filter(|x| x.0 == job)?;
        let _ = j;
        Some(w.call::<bool>(&Request::Cancel { job: id }).map(|_| ()))
    }

    // -------------------------------------------------------------------------------
    // Datasets
    // -------------------------------------------------------------------------------

    fn dev_file(&self) -> DevFile {
        self.app_file().dev
    }

    fn folder_path(&self, id: &str) -> CmdResult<PathBuf> {
        let n: usize = id
            .strip_prefix("folder:")
            .and_then(|x| x.parse().ok())
            .ok_or_else(|| err(ErrorCode::NotFound, format!("no source {id}")))?;
        self.dev_file()
            .folders
            .get(n)
            .map(PathBuf::from)
            .ok_or_else(|| err(ErrorCode::NotFound, format!("no source {id}")))
    }

    fn parse_file(&self, path: &Path) -> CmdResult<Arc<Dataset>> {
        let key = file_key(path)
            .ok_or_else(|| err(ErrorCode::NotFound, format!("{} is gone", path.display())))?;
        if let Some(d) = lock_or(&self.dev.data)
            .files
            .iter()
            .find(|x| x.0 == key)
            .map(|x| x.1.clone())
        {
            return Ok(d);
        }
        if key.1 > dataset::MAX_FILE {
            return Err(err(
                ErrorCode::Invalid,
                format!("{} is larger than 64 MiB", path.display()),
            ));
        }
        let bytes = std::fs::read(path).map_err(io)?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let d = Arc::new(Dataset::parse(&name, &bytes).map_err(|e| err(ErrorCode::Invalid, e))?);
        let mut c = lock_or(&self.dev.data);
        c.summaries.insert(
            key.clone(),
            (
                d.tables.len() as u32,
                d.tables.iter().map(|t| t.rows.len() as u32).sum(),
            ),
        );
        c.files.retain(|x| x.0.0 != key.0);
        c.files.insert(0, (key, d.clone()));
        c.files.truncate(KEEP_DATASETS);
        Ok(d)
    }

    fn build_dataset(&self, f: &Found, ch: &Channel) -> CmdResult<Arc<Dataset>> {
        let data = self.loaded(f, ch, false)?;
        let slot = (f.ui_id(), ch.name.clone());
        if let Some((k, d)) = lock_or(&self.dev.data).builds.get(&slot)
            && *k == data.key
        {
            return Ok(d.clone());
        }
        let d = Arc::new(gamedata_dataset(&data.game_data));
        lock_or(&self.dev.data)
            .builds
            .insert(slot, (data.key.clone(), d.clone()));
        Ok(d)
    }

    /// Every dataset of a source, by file name.
    fn source_datasets(
        &self,
        r: &ChannelRef,
        source: &str,
    ) -> CmdResult<Vec<(String, Arc<Dataset>)>> {
        if let Some(chan) = source.strip_prefix("build:") {
            let f = self.resolve(r)?;
            let ch = f.channel(chan)?.clone();
            return Ok(vec![(GAMEDATA.to_string(), self.build_dataset(&f, &ch)?)]);
        }
        if source.starts_with("folder:") {
            let dir = self.folder_path(source)?;
            return json_files(&dir)
                .into_iter()
                .map(|(name, p, _)| Ok((name, self.parse_file(&p)?)))
                .collect();
        }
        Err(dump_or_missing(source))
    }

    fn dataset(&self, r: &ChannelRef, source: &str, name: &str) -> CmdResult<Arc<Dataset>> {
        self.dev_on()?;
        if let Some(chan) = source.strip_prefix("build:") {
            if name != GAMEDATA {
                return Err(err(ErrorCode::NotFound, format!("no dataset {name}")));
            }
            let f = self.resolve(r)?;
            let ch = f.channel(chan)?.clone();
            return self.build_dataset(&f, &ch);
        }
        if source.starts_with("folder:") {
            let dir = self.folder_path(source)?;
            let (_, p, _) = json_files(&dir)
                .into_iter()
                .find(|x| x.0 == name)
                .ok_or_else(|| err(ErrorCode::NotFound, format!("no dataset {name}")))?;
            return self.parse_file(&p);
        }
        Err(dump_or_missing(source))
    }

    /// The label lookup of a channel's build (7.14): its GameData records, stock text and our
    /// enhanced text, built once per build and Strings table. `None` while the build's data
    /// was not read.
    fn lookup_for(&self, r: &ChannelRef, channel: &str) -> Option<Arc<MapLookup>> {
        let f = self.resolve(r).ok()?;
        let ch = f.channel(channel).ok()?.clone();
        let data = self.loaded(&f, &ch, false).ok()?;
        let table = self.build_table(&f, &ch, None, false).ok();
        let key: LookupKey = (
            f.ui_id(),
            ch.name.clone(),
            data.key.clone(),
            table.as_ref().map_or(0, |t| Arc::as_ptr(t) as usize),
        );
        if let Some(l) = lock_or(&self.dev.lookups)
            .iter()
            .find(|x| x.0 == key)
            .map(|x| x.1.clone())
        {
            return Some(l);
        }
        let stock = data.stock.clone();
        let mut m = MapLookup::from_gamedata(&data.game_data)
            .with_stock(Arc::new(move |k: &str| stock.value(k).map(str::to_string)))
            .with_bare_keys(true);
        if let Some(t) = table {
            m = m.with_enhanced(Arc::new(move |k: &str| {
                let row = t.rows.get(*t.index.get(&k.to_lowercase())?)?;
                (row.result != row.original).then(|| row.result.clone())
            }));
        }
        let m = Arc::new(m);
        let mut c = lock_or(&self.dev.lookups);
        c.retain(|x| !(x.0.0 == key.0 && x.0.1 == key.1));
        c.insert(0, (key, m.clone()));
        c.truncate(KEEP_LOOKUPS);
        Some(m)
    }

    /// The lookup of a dataset source: a build's own, else the target channel's.
    fn source_lookup(&self, r: &ChannelRef, source: &str) -> Option<Arc<MapLookup>> {
        let chan = source.strip_prefix("build:").unwrap_or(&r.channel);
        self.lookup_for(r, chan)
    }

    fn table_of(&self, d: &Dataset, name: &str) -> CmdResult<dataset::Table> {
        d.table(name)
            .cloned()
            .ok_or_else(|| err(ErrorCode::NotFound, format!("no table {name}")))
    }

    fn sources(&self, r: &ChannelRef) -> CmdResult<Vec<DevSource>> {
        self.dev_on()?;
        let f = self.resolve(r)?;
        let mut out = Vec::new();
        // The target's build first, then the install's other channels (for a diff).
        let mut chans: Vec<Channel> = f.install.channels.clone();
        chans.sort_by_key(|c| c.name != r.channel);
        for ch in &chans {
            let label = format!("{} build", ch.name);
            let build = ch
                .build
                .as_ref()
                .map(|b| b.version.clone())
                .filter(|v| !v.is_empty());
            let (state, detail, datasets) = match self.build_dataset(&f, ch) {
                Ok(d) => (
                    SourceState::Ready,
                    None,
                    vec![DevDatasetSummary {
                        file: GAMEDATA.into(),
                        bytes: d.info.bytes,
                        tables: d.tables.len() as u32,
                        rows: d.tables.iter().map(|t| t.rows.len() as u32).sum(),
                    }],
                ),
                Err(e) => (SourceState::Missing, Some(e.message), vec![]),
            };
            out.push(DevSource {
                id: format!("build:{}", ch.name),
                kind: SourceKind::Build,
                label,
                build,
                state,
                detail,
                datasets,
            });
        }
        for (i, dir) in self.dev_file().folders.iter().enumerate() {
            let p = PathBuf::from(dir);
            let label = p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| dir.clone());
            if !p.is_dir() {
                out.push(DevSource {
                    id: format!("folder:{i}"),
                    kind: SourceKind::Folder,
                    label,
                    build: None,
                    state: SourceState::Missing,
                    detail: Some(format!("{dir} is gone")),
                    datasets: vec![],
                });
                continue;
            }
            let mut datasets = Vec::new();
            for (name, path, bytes) in json_files(&p) {
                let counts = file_key(&path).and_then(|k| {
                    if let Some(c) = lock_or(&self.dev.data).summaries.get(&k) {
                        return Some(*c);
                    }
                    self.parse_file(&path).ok().map(|d| {
                        (
                            d.tables.len() as u32,
                            d.tables.iter().map(|t| t.rows.len() as u32).sum(),
                        )
                    })
                });
                let (tables, rows) = counts.unwrap_or((0, 0));
                datasets.push(DevDatasetSummary {
                    file: name,
                    bytes,
                    tables,
                    rows,
                });
            }
            out.push(DevSource {
                id: format!("folder:{i}"),
                kind: SourceKind::Folder,
                label,
                build: None,
                state: SourceState::Ready,
                detail: Some(dir.clone()),
                datasets,
            });
        }
        let on = self.settings().network.published_dumps;
        out.push(DevSource {
            id: format!("dump:{}", r.channel),
            kind: SourceKind::Dump,
            label: "Published dump".into(),
            build: None,
            state: SourceState::Off,
            detail: Some(if on {
                "Follow-up: not in this build".into()
            } else {
                "Off (Settings > Network)".into()
            }),
            datasets: vec![],
        });
        Ok(out)
    }
}

/// Marks of a GameData diff (A = other build, B = this one) by record GUID: added and changed
/// rows of B whose `id` (or `item.id`) is a GUID; and how many rows exist only in A.
pub fn compare_marks(
    tables: &[TableDiff],
    b: &[(String, Arc<Dataset>)],
) -> (Vec<(String, Change)>, u32) {
    let mut marks = Vec::new();
    let mut removed = 0;
    for t in tables {
        let rows = b
            .iter()
            .find(|x| x.0 == t.summary.dataset)
            .and_then(|x| x.1.table(&t.summary.table));
        for r in &t.records {
            let change = match r.state {
                RecordState::Added => Change::Added,
                RecordState::Changed => Change::Changed,
                RecordState::Removed => {
                    removed += 1;
                    continue;
                }
            };
            let row = rows.and_then(|tb| tb.rows.get(r.b? as usize));
            let id = row
                .and_then(|v| v.get("id").or_else(|| v.pointer("/item/id")))
                .and_then(Value::as_str)
                .filter(|g| starorg_core::dev::explorer::is_guid(g));
            if let Some(g) = id {
                marks.push((g.to_lowercase(), change));
            }
        }
    }
    (marks, removed)
}

fn dump_or_missing(source: &str) -> AppError {
    if source.starts_with("dump:") {
        dump_refusal(false)
    } else {
        err(ErrorCode::NotFound, format!("no source {source}"))
    }
}

fn identity_options(a: Option<&dataset::Table>, b: Option<&dataset::Table>) -> Vec<String> {
    let scalar = |t: Option<&dataset::Table>| -> Vec<String> {
        t.map(|t| {
            t.columns
                .iter()
                .filter(|c| c.kind.scalar())
                .map(|c| c.name.clone())
                .collect()
        })
        .unwrap_or_default()
    };
    let (sa, sb) = (scalar(a), scalar(b));
    match (a, b) {
        (Some(_), Some(_)) => sa.into_iter().filter(|c| sb.contains(c)).collect(),
        (Some(_), None) => sa,
        _ => sb,
    }
}

impl DevBackend for RealBackend {
    fn dev_status(&self, req: ChannelRef) -> CmdResult<DevStatus> {
        self.status_of(&req)
    }

    fn dev_open(&self, req: ChannelRef, progress: Progress<'_>) -> CmdResult<DevStatus> {
        self.worker(&req, Some(progress))?;
        self.status_of(&req)
    }

    fn dev_close(&self, req: ChannelRef) -> CmdResult<DevStatus> {
        self.dev_on()?;
        self.dev.stop();
        self.status_of(&req)
    }

    fn dev_types(&self, req: ChannelRef) -> CmdResult<Vec<TypeCount>> {
        self.worker(&req, None)?.call(&Request::Types)
    }

    fn dev_tree(&self, req: DevTreeRequest) -> CmdResult<TreePage> {
        self.worker(&req.target, None)?.call(&Request::Tree {
            root: req.root,
            node: req.node,
            offset: req.offset,
            limit: req.limit.min(MAX_PAGE as u32),
            main_only: req.main_only,
            player_only: req.player_only,
        })
    }

    fn dev_list(&self, req: DevListRequest) -> CmdResult<ListPage> {
        let w = self.worker(&req.target, None)?;
        let mut q = req.query;
        q.limit = q.limit.min(MAX_PAGE as u32);
        let mut page: ListPage = w.call(&Request::List(Box::new(q)))?;
        let stats = self.list_stats(&req.target);
        for it in &mut page.items {
            if let Some(s) = stats.get(&it.guid) {
                it.stats = s.clone();
            }
        }
        Ok(page)
    }

    fn dev_owner(&self, req: DevOwnerRequest) -> CmdResult<Vec<SearchHit>> {
        self.worker(&req.target, None)?
            .call(&Request::Owner { path: req.path })
    }

    fn dev_compare(&self, req: DevCompareRequest) -> CmdResult<Option<DevCompare>> {
        let w = self.worker(&req.target, None)?;
        let Some(other) = req.other.filter(|o| *o != req.target.channel) else {
            w.call::<usize>(&Request::Marks { marks: vec![] })?;
            *lock_or(&self.dev.compared) = None;
            return Ok(None);
        };
        let t0 = Instant::now();
        // The Diff screen's machinery: A = the other build, B = this one, so "added" are
        // records only here.
        let a = self.source_datasets(&req.target, &format!("build:{other}"))?;
        let b = self.source_datasets(&req.target, &format!("build:{}", req.target.channel))?;
        let ra: Vec<(String, &Dataset)> = a.iter().map(|x| (x.0.clone(), &*x.1)).collect();
        let rb: Vec<(String, &Dataset)> = b.iter().map(|x| (x.0.clone(), &*x.1)).collect();
        let tables = diff::diff_sources(&ra, &rb, &|_, _| {});
        let (marks, removed) = compare_marks(&tables, &b);
        let (added, changed) = marks.iter().fold((0, 0), |(a, c), m| match m.1 {
            Change::Added => (a + 1, c),
            Change::Changed => (a, c + 1),
        });
        let marked = w.call::<usize>(&Request::Marks { marks })? as u32;
        let out = DevCompare {
            other,
            added,
            changed,
            removed,
            marked,
            ms: t0.elapsed().as_millis() as u64,
        };
        *lock_or(&self.dev.compared) = Some((Arc::downgrade(&w), out.clone()));
        Ok(Some(out))
    }

    fn dev_search(&self, req: DevSearchRequest) -> CmdResult<SearchPage> {
        self.worker(&req.target, None)?.call(&Request::Search {
            query: req.query,
            limit: req.limit.min(MAX_PAGE as u32),
        })
    }

    fn dev_record(&self, req: DevRecordRequest) -> CmdResult<RecordView> {
        let mut v: RecordView = self.worker(&req.target, None)?.call(&Request::Record {
            what: req.what,
            pointer: req.pointer,
            depth: req.depth,
        })?;
        // The worker knows stock text only; our enhanced text comes from the Strings table.
        if let Some(l) = self.lookup_for(&req.target, &req.target.channel) {
            for (raw, r) in v.labels.iter_mut() {
                if r.kind == LabelKind::Loc
                    && let Some(e) = l.enhanced(raw.trim_start_matches('@'))
                    && e != r.text
                {
                    r.enhanced = Some(e);
                }
            }
        }
        Ok(v)
    }

    fn dev_export_record(&self, req: DevExportRecordRequest) -> CmdResult<DevExportResult> {
        let w = self.worker(&req.target, None)?;
        let view: RecordView = w.call(&Request::Record {
            what: req.what.clone(),
            pointer: String::new(),
            depth: Some(1),
        })?;
        let name = format!("{}.json", view.header.name);
        let Some(out) =
            self.cfg.dialogs.get().and_then(|d| {
                d.save_file("Export record (for your own use)", &name, EXPORT_FILTER)
            })
        else {
            return Ok(DevExportResult {
                cancelled: true,
                path: None,
                records: 0,
                bytes: 0,
            });
        };
        let done: ExportDone = w.call(&Request::ExportRecord {
            what: view.header.guid,
            out,
        })?;
        Ok(DevExportResult {
            cancelled: false,
            path: Some(done.path),
            records: done.records,
            bytes: done.bytes,
        })
    }

    fn dev_export_type(
        &self,
        req: DevExportTypeRequest,
        progress: Progress<'_>,
    ) -> CmdResult<DevExportResult> {
        let w = self.worker(&req.target, None)?;
        if lock_or(&self.dev.export).is_some() {
            return Err(err(ErrorCode::Busy, "Another export is running."));
        }
        let name = format!("{}.json", req.type_name);
        let Some(out) = self.cfg.dialogs.get().and_then(|d| {
            d.save_file(
                &format!("Export all {} records (for your own use)", req.type_name),
                &name,
                EXPORT_FILTER,
            )
        }) else {
            return Ok(DevExportResult {
                cancelled: true,
                path: None,
                records: 0,
                bytes: 0,
            });
        };
        let mut job = self.job_start(
            JobKind::Dev,
            Some(&req.target.channel),
            1,
            &format!("Exporting {}", req.type_name),
        );
        job.cancellable = true;
        progress(job.clone());
        w.busy.store(true, Ordering::SeqCst);
        let job_id = job.job.clone();
        let job_cell = Mutex::new(job);
        let r = w.call_stream(
            &Request::ExportType {
                name: req.type_name.clone(),
                out,
                main_only: req.main_only,
                ndjson: false,
            },
            &|id| *lock_or(&self.dev.export) = Some((job_id.clone(), w.clone(), id)),
            &|p| {
                let mut j = lock_or(&job_cell);
                j.steps = p.total.max(1);
                let step = p.done.max(1);
                let label = format!("{} of {} records", p.done, p.total);
                self.job_step(&mut j, step, &label, Some(progress));
            },
        );
        w.busy.store(false, Ordering::SeqCst);
        *lock_or(&self.dev.export) = None;
        let job = job_cell.into_inner().unwrap_or_else(|p| p.into_inner());
        match r {
            Ok(v) => {
                let done: ExportDone = serde_json::from_value(v).map_err(internal)?;
                self.job_end(job, JobPhase::Done, "Export finished", Some(progress));
                self.log(
                    LogLevel::Info,
                    Some(JobKind::Dev),
                    Some(&req.target.channel),
                    &format!("Exported {} {} records", done.records, req.type_name),
                );
                Ok(DevExportResult {
                    cancelled: false,
                    path: Some(done.path),
                    records: done.records,
                    bytes: done.bytes,
                })
            }
            Err(e) => {
                let phase = if e.code == ErrorCode::Cancelled {
                    JobPhase::Cancelled
                } else {
                    JobPhase::Failed
                };
                self.job_end(job, phase, &e.message, Some(progress));
                Err(e)
            }
        }
    }

    fn dev_sources(&self, req: ChannelRef) -> CmdResult<Vec<DevSource>> {
        self.sources(&req)
    }

    fn dev_add_source(&self, req: ChannelRef) -> CmdResult<Vec<DevSource>> {
        self.dev_on()?;
        let Some(dir) = self
            .cfg
            .dialogs
            .get()
            .and_then(|d| d.pick_folder("Choose a folder of JSON files"))
        else {
            return self.sources(&req);
        };
        let mut a = self.app_file();
        let text = dir.display().to_string();
        a.dev.folders.retain(|f| *f != text);
        a.dev.folders.push(text);
        while a.dev.folders.len() > MAX_FOLDERS {
            a.dev.folders.remove(0);
        }
        self.save_app_file(&a)?;
        *lock_or(&self.dev.diff) = None;
        self.sources(&req)
    }

    fn dev_remove_source(&self, req: DevSourceRef) -> CmdResult<Vec<DevSource>> {
        self.dev_on()?;
        let path = self.folder_path(&req.source)?;
        let mut a = self.app_file();
        let text = path.display().to_string();
        a.dev.folders.retain(|f| *f != text);
        self.save_app_file(&a)?;
        *lock_or(&self.dev.diff) = None;
        self.sources(&req.target)
    }

    fn dev_download_dump(
        &self,
        _req: ChannelRef,
        _progress: Progress<'_>,
    ) -> CmdResult<Vec<DevSource>> {
        self.dev_on()?;
        Err(dump_refusal(self.settings().network.published_dumps))
    }

    fn dev_dataset(&self, req: DevDatasetRef) -> CmdResult<DatasetInfo> {
        Ok(self
            .dataset(&req.target, &req.source, &req.dataset)?
            .info
            .clone())
    }

    fn dev_table(&self, req: DevTableRef) -> CmdResult<TableInfo> {
        let d = self.dataset(&req.target, &req.source, &req.dataset)?;
        Ok(self.table_of(&d, &req.table)?.info())
    }

    fn dev_rows(&self, req: DevRowsRequest) -> CmdResult<RowsPage> {
        let d = self.dataset(&req.target, &req.source, &req.dataset)?;
        let t = d
            .table(&req.table)
            .ok_or_else(|| err(ErrorCode::NotFound, format!("no table {}", req.table)))?;
        Ok(dataset::query_rows(
            t,
            &RowsQuery {
                columns: req.columns,
                search: req.search,
                filters: req.filters,
                sort: req.sort,
                offset: req.offset,
                limit: req.limit,
                group: req.group,
            },
            MAX_PAGE,
            self.source_lookup(&req.target, &req.source)
                .as_deref()
                .map(|l| l as &dyn Lookup),
        ))
    }

    fn dev_groups(&self, req: DevGroupsRequest) -> CmdResult<GroupPage> {
        let d = self.dataset(&req.target, &req.source, &req.dataset)?;
        let t = d
            .table(&req.table)
            .ok_or_else(|| err(ErrorCode::NotFound, format!("no table {}", req.table)))?;
        Ok(starorg_core::dev::group::group_rows(
            t,
            &starorg_core::dev::group::GroupQuery {
                by: req.by,
                open: req.open,
                columns: req.columns,
                search: req.search,
                filters: req.filters,
            },
            self.source_lookup(&req.target, &req.source)
                .as_deref()
                .map(|l| l as &dyn Lookup),
        ))
    }

    fn dev_row(&self, req: DevRowRequest) -> CmdResult<DevRowDetail> {
        let d = self.dataset(&req.target, &req.source, &req.dataset)?;
        let t = d
            .table(&req.table)
            .ok_or_else(|| err(ErrorCode::NotFound, format!("no table {}", req.table)))?;
        let row = t
            .rows
            .get(req.row as usize)
            .ok_or_else(|| err(ErrorCode::NotFound, format!("no row {}", req.row)))?;
        let (json, truncated) =
            record::view_json(row, "", None).map_err(|e| err(ErrorCode::Internal, e))?;
        // Loc text from the target channel's stock file, when its data is in memory/cache.
        let stock = self
            .resolve(&req.target)
            .ok()
            .and_then(|f| {
                let ch = f.channel(&req.target.channel).ok()?.clone();
                self.loaded(&f, &ch, false).ok()
            })
            .map(|l| l.stock.clone());
        let loc = row_loc(row, &|k| stock.as_ref()?.value(k).map(str::to_string));
        // Explorer links only while the explorer runs (never started for this).
        let links = match self.worker_if_ready(&req.target) {
            Some(w) => {
                let cands = std::cell::RefCell::new(Vec::new());
                let _ = explorer_links(row, &|s| {
                    cands.borrow_mut().push(s.to_string());
                    None
                });
                let cands = cands.into_inner();
                let found: Vec<Option<String>> = w
                    .call(&Request::Resolve {
                        values: cands.clone(),
                    })
                    .unwrap_or_default();
                let map: BTreeMap<String, String> = cands
                    .into_iter()
                    .zip(found)
                    .filter_map(|(c, g)| g.map(|g| (c, g)))
                    .collect();
                explorer_links(row, &|s| map.get(s).cloned())
            }
            None => vec![],
        };
        let mut labels = match self.source_lookup(&req.target, &req.source) {
            Some(l) => readable::labels_in(&*l, row),
            None => readable::labels_in(&NoLookup, row),
        };
        // GUIDs and record names the build's GameData does not name: ask the explorer, while
        // it runs.
        let mut open: Vec<String> = Vec::new();
        fn strings(v: &Value, out: &mut Vec<String>) {
            match v {
                Value::String(s) if asks_explorer(s) && !out.contains(s) => out.push(s.clone()),
                Value::Array(a) => a.iter().for_each(|x| strings(x, out)),
                Value::Object(m) => m.values().for_each(|x| strings(x, out)),
                _ => {}
            }
        }
        strings(row, &mut open);
        open.retain(|g| !labels.contains_key(g));
        self.explorer_labels(&req.target, open, &mut labels);
        Ok(DevRowDetail {
            row: req.row,
            json,
            truncated,
            loc,
            links,
            labels,
        })
    }

    fn dev_diff(&self, req: DevDiffRequest, progress: Progress<'_>) -> CmdResult<DevDiffSummary> {
        self.dev_on()?;
        let t0 = Instant::now();
        let mut job = self.job_start(
            JobKind::Dev,
            Some(&req.target.channel),
            3,
            "Reading datasets",
        );
        progress(job.clone());
        let r: CmdResult<DevDiffSummary> = (|| {
            let a = self.source_datasets(&req.target, &req.a)?;
            self.job_step(&mut job, 2, "Reading datasets", Some(progress));
            let b = self.source_datasets(&req.target, &req.b)?;
            self.job_step(&mut job, 3, "Comparing", Some(progress));
            let ra: Vec<(String, &Dataset)> = a.iter().map(|x| (x.0.clone(), &*x.1)).collect();
            let rb: Vec<(String, &Dataset)> = b.iter().map(|x| (x.0.clone(), &*x.1)).collect();
            let tables = diff::diff_sources(&ra, &rb, &|_, _| {});
            let la = self.source_lookup(&req.target, &req.a);
            let lb = self.source_lookup(&req.target, &req.b);
            let summary = DevDiffSummary {
                a: req.a.clone(),
                b: req.b.clone(),
                tables: tables.iter().map(|t| t.summary.clone()).collect(),
                ms: t0.elapsed().as_millis() as u64,
            };
            *lock_or(&self.dev.diff) = Some(DiffState {
                summary: summary.clone(),
                tables,
                a,
                b,
                la,
                lb,
            });
            Ok(summary)
        })();
        match &r {
            Ok(_) => self.job_end(job, JobPhase::Done, "Compared", Some(progress)),
            Err(e) => self.job_end(job, JobPhase::Failed, &e.message, Some(progress)),
        }
        r
    }

    fn dev_diff_summary(&self, _req: ChannelRef) -> CmdResult<DevDiffSummary> {
        self.dev_on()?;
        lock_or(&self.dev.diff)
            .as_ref()
            .map(|d| d.summary.clone())
            .ok_or_else(|| err(ErrorCode::NotFound, "No comparison yet: pick A and B."))
    }

    fn dev_diff_records(&self, req: DevDiffRecordsRequest) -> CmdResult<DevDiffRecords> {
        self.dev_on()?;
        let mut g = lock_or(&self.dev.diff);
        let st = g
            .as_mut()
            .ok_or_else(|| err(ErrorCode::NotFound, "No comparison yet: pick A and B."))?;
        let find = |side: &[(String, Arc<Dataset>)]| {
            side.iter()
                .find(|x| x.0 == req.dataset)
                .and_then(|x| x.1.table(&req.table).cloned())
        };
        let (ta, tb) = (find(&st.a), find(&st.b));
        let pos = st
            .tables
            .iter()
            .position(|t| t.summary.dataset == req.dataset && t.summary.table == req.table)
            .ok_or_else(|| err(ErrorCode::NotFound, format!("no table {}", req.table)))?;
        if req.identity.is_some() && st.tables[pos].summary.identity != req.identity {
            st.tables[pos] = diff::diff_table(
                &req.dataset,
                &req.table,
                ta.as_ref(),
                tb.as_ref(),
                req.identity.as_deref(),
            );
        }
        let in_group = diff_group_filter(&req, &st.a, &st.b);
        let la: &dyn Lookup = st.la.as_deref().map_or(&NoLookup, |l| l);
        let lb: &dyn Lookup = st.lb.as_deref().map_or(&NoLookup, |l| l);
        let id = st.tables[pos].summary.identity.clone();
        Ok(page_records(
            &st.tables[pos],
            &req,
            identity_options(ta.as_ref(), tb.as_ref()),
            &in_group,
            &|r| record_title(r, id.as_deref(), ta.as_ref(), tb.as_ref(), la, lb),
        ))
    }

    fn dev_diff_groups(&self, req: DevDiffGroupsRequest) -> CmdResult<GroupPage> {
        self.dev_on()?;
        let g = lock_or(&self.dev.diff);
        let st = g
            .as_ref()
            .ok_or_else(|| err(ErrorCode::NotFound, "No comparison yet: pick A and B."))?;
        // Node labels read with B's build (the side most records come from).
        let lb: &dyn Lookup = st.lb.as_deref().map_or(&NoLookup, |l| l);
        Ok(diff_tree(&st.tables, &st.a, &st.b, &req, &|c, v| {
            starorg_core::dev::group::lookup_label(lb, c, v)
        }))
    }

    fn dev_diff_record(&self, req: DevDiffRecordRequest) -> CmdResult<DevFieldDiff> {
        self.dev_on()?;
        let g = lock_or(&self.dev.diff);
        let st = g
            .as_ref()
            .ok_or_else(|| err(ErrorCode::NotFound, "No comparison yet: pick A and B."))?;
        let td = st
            .tables
            .iter()
            .find(|t| t.summary.dataset == req.dataset && t.summary.table == req.table)
            .ok_or_else(|| err(ErrorCode::NotFound, format!("no table {}", req.table)))?;
        let rec = td
            .records
            .iter()
            .find(|r| r.id == req.id)
            .ok_or_else(|| err(ErrorCode::NotFound, format!("no record {}", req.id)))?;
        let row = |side: &[(String, Arc<Dataset>)], i: Option<u32>| -> Value {
            i.and_then(|i| {
                side.iter()
                    .find(|x| x.0 == req.dataset)
                    .and_then(|x| x.1.table(&req.table))
                    .and_then(|t| t.rows.get(i as usize).cloned())
            })
            .unwrap_or(Value::Null)
        };
        // An added or removed record compares against an empty object.
        let or_empty = |v: Value| {
            if v.is_null() {
                Value::Object(Default::default())
            } else {
                v
            }
        };
        let (a, b) = (or_empty(row(&st.a, rec.a)), or_empty(row(&st.b, rec.b)));
        let la: &dyn Lookup = st.la.as_deref().map_or(&NoLookup, |l| l);
        let lb: &dyn Lookup = st.lb.as_deref().map_or(&NoLookup, |l| l);
        let mut changes = diff::field_changes(&a, &b);
        diff::annotate_changes(&mut changes, la, lb);
        diff::name_positions(&mut changes, &a, &b, la, lb);
        // Values neither build's GameData names (record names, GUIDs): ask the explorer.
        let mut open: Vec<String> = Vec::new();
        for c in &changes {
            for (v, l) in [(&c.old, &c.old_label), (&c.new, &c.new_label)] {
                if let (Some(Value::String(s)), None) = (v, l)
                    && asks_explorer(s)
                    && !open.contains(s)
                {
                    open.push(s.clone());
                }
            }
        }
        let mut found = BTreeMap::new();
        self.explorer_labels(&req.target, open, &mut found);
        for c in &mut changes {
            for (v, l) in [(&c.old, &mut c.old_label), (&c.new, &mut c.new_label)] {
                if l.is_none()
                    && let Some(Value::String(s)) = v
                {
                    *l = found.get(s).cloned();
                }
            }
        }
        let label = record_title(
            rec,
            td.summary.identity.as_deref(),
            side_table(&st.a, &req.dataset, &req.table),
            side_table(&st.b, &req.dataset, &req.table),
            la,
            lb,
        );
        Ok(DevFieldDiff {
            id: rec.id.clone(),
            label,
            state: rec.state,
            changes,
        })
    }
}

/// A dataset value worth asking the explorer about: a GUID, or a record-name shape
/// (`factionreputation_lawful_ftl`, `EntityClassDefinition.AEGS_Gladius`).
fn asks_explorer(s: &str) -> bool {
    starorg_core::dev::explorer::is_guid(s)
        || ((4..=200).contains(&s.len())
            && (s.contains('_') || s.contains('.'))
            && !s.starts_with('@')
            && !s.contains(char::is_whitespace)
            && !s.contains('/')
            && s.parse::<f64>().is_err())
}

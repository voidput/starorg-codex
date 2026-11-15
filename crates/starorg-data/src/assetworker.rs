//! The `asset-worker` process body (addendum 7.16): the archive's files, sounds and models
//! behind the same JSON-lines protocol as `dev-worker` ([`crate::devworker`]).
//!
//! The app starts `starorg-codex-app asset-worker --p4k <P>` the first time the Files
//! browser, a Sounds panel or a Model export needs it (never when the Dev screen opens). The
//! worker opens the archive index read only (count check as extraction does) and prints
//! `{"event":"ready","info":{...}}` ([`WorkerInfo`], `records` = archive entries) or
//! `{"event":"failed","error":"..."}`. The DataCore and the Wwise/ATL index are read on the
//! first request that needs them and kept while the worker lives.
//!
//! Requests ([`AssetRequest`]) and replies ([`Reply`]) are one line each; every request runs
//! on its own thread under `catch_unwind`, so a long export still answers `status` and
//! `cancel`. `extract`, `export-sounds` and `export-model` stream progress lines
//! (`{"id":n,"ok":true,"progress":{"done":..,"total":..}}`) and run one at a time (`busy`).
//! Model exports are sent to a fresh worker that the app stops afterwards, so their memory
//! (hundreds of MB to GB for a capital ship) goes back to the OS at once.
//!
//! The worker exits when stdin closes, or after `--idle-secs` (default 600) without a request
//! and with no job running.

use crate::assets::audio::{self, SoundRef};
use crate::assets::files;
use crate::assets::model::{self, ModelOptions};
use crate::assets::{AssetArchive, AssetError, AssetResult, peak_rss_bytes, wemcodec};
use crate::devworker::{ExportProgress, Reply, ReplyError, WorkerInfo, rss_bytes};
use crate::extract::panic_message;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Default self-exit after this long without a request (the app stops it after 5 minutes).
pub const IDLE_EXIT: Duration = Duration::from_secs(600);
/// Progress units of a model export.
pub const MODEL_STEPS: u32 = 1000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum AssetRequest {
    Status,
    List {
        #[serde(default)]
        dir: String,
        #[serde(default)]
        offset: u32,
        limit: u32,
    },
    Search {
        query: String,
        limit: u32,
    },
    /// Images are written to `out` (PNG or the original JPEG); text comes back inline.
    Preview {
        path: String,
        out: PathBuf,
    },
    /// Streamed: the files and folders, below `out_dir`.
    Extract {
        paths: Vec<String>,
        out_dir: PathBuf,
        #[serde(default)]
        convert: bool,
    },
    /// A record's (and its loadout's) audio triggers and their sounds.
    Sounds {
        what: String,
    },
    /// ATL triggers by name, event or bank (the Explorer's Sounds search, 7.17).
    SoundSearch {
        query: String,
        limit: u32,
    },
    /// One trigger and its sounds.
    Trigger {
        name: String,
    },
    /// One sound, written at `out` with the extension of what it became
    /// ([`wemcodec::EXTS`]): `.ogg`, `.opus` or `.wav`, else (or with `raw`) the `.wem`.
    Sound {
        sound: SoundRef,
        out: PathBuf,
        #[serde(default)]
        raw: bool,
    },
    /// Streamed: sounds as `<trigger>_<id>.<ogg|opus|wav|wem>` (and the `.wem` too with
    /// `wem`) in `out_dir`.
    ExportSounds {
        sounds: Vec<(String, SoundRef)>,
        out_dir: PathBuf,
        #[serde(default)]
        wem: bool,
    },
    /// Streamed: an entity as GLB (and STL).
    ExportModel {
        what: String,
        options: ModelOptions,
        out: PathBuf,
    },
    /// Stop the streamed job with this request id.
    Cancel {
        job: u64,
    },
    /// Test hook: panics inside the request guard (only with `--test-panic`).
    TestPanic,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetEnvelope {
    pub id: u64,
    #[serde(flatten)]
    pub req: AssetRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetStatus {
    pub entries: u32,
    pub datacore: bool,
    pub audio: bool,
    pub rss: Option<u64>,
    pub peak_rss: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoundDone {
    pub path: String,
    /// `ogg`, `opus`, `wav` or `wem`.
    pub ext: String,
    pub bytes: u64,
    pub duration: Option<f64>,
    /// The WEM's codec (`Vorbis`, `PCM`, ...); empty when unknown.
    #[serde(default)]
    pub codec: String,
    /// Why it was kept as `.wem` (not with `raw`).
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoundsDone {
    pub out_dir: String,
    pub files: u32,
    pub bytes: u64,
    pub failed: Vec<String>,
    pub failures: u32,
    /// The sounds written by this run (one per sound: its Ogg, or its WEM when it is kept as
    /// WEM), for the asset cache. A file left in the folder by an earlier run is never here.
    #[serde(default)]
    pub written: Vec<WrittenSound>,
    /// Sounds written by this run, by extension (`ogg`, `wav`, `opus`, `wem`; the extra
    /// `.wem` copies of `wem` not counted).
    #[serde(default)]
    pub formats: BTreeMap<String, u32>,
    /// The first 20 sounds kept as `.wem`, each with why.
    #[serde(default)]
    pub kept: Vec<String>,
}

impl SoundsDone {
    /// `2305 .ogg, 23 .wem` (most first).
    pub fn formats_line(formats: &BTreeMap<String, u32>) -> String {
        let mut v: Vec<_> = formats.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        v.iter()
            .map(|(e, n)| format!("{n} .{e}"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrittenSound {
    /// [`SoundRef::key`].
    pub key: String,
    /// `ogg`, `opus`, `wav` or `wem`.
    pub ext: String,
    pub path: String,
}

/// A cached sound: its converted file, else its kept WEM. A kept WEM of a codec this build
/// converts (cached by an older build, or a stream that did not convert) is not a hit, so it
/// is converted again; only its header is read.
pub fn cached_sound(
    c: &starorg_core::assetcache::AssetCache,
    channel: &str,
    build: &str,
    s: &SoundRef,
) -> Option<(PathBuf, &'static str)> {
    use starorg_core::assetcache::Kind;
    let k = s.key();
    let (p, x) = wemcodec::EXTS
        .iter()
        .find_map(|x| c.get(channel, build, Kind::Audio, &k, x).map(|p| (p, *x)))?;
    if x == "wem" && kept_codec(&p).is_none() {
        return None;
    }
    Some((p, x))
}

/// The codec of a kept `.wem` file when this build does not convert it (from its header).
pub fn kept_codec(p: &Path) -> Option<wemcodec::Codec> {
    use std::io::Read;
    let mut head = Vec::new();
    std::fs::File::open(p)
        .ok()?
        .take(64 * 1024)
        .read_to_end(&mut head)
        .ok()?;
    wemcodec::codec_of(&head)
        .ok()
        .filter(|c| c.format().is_none())
}

/// Put the sounds this run wrote into the asset cache.
pub fn cache_written(
    c: &starorg_core::assetcache::AssetCache,
    channel: &str,
    build: &str,
    d: &SoundsDone,
) {
    for w in &d.written {
        let _ = c.put_copy(
            channel,
            build,
            starorg_core::assetcache::Kind::Audio,
            &w.key,
            &w.ext,
            Path::new(&w.path),
        );
    }
}

/// Write one sound at `out` with its extension: converted ([`audio::decode`]) unless `raw`,
/// else (a codec that is not converted, a stream that does not convert) the WEM.
pub fn write_sound(
    a: &AssetArchive,
    s: &SoundRef,
    out: &Path,
    raw: bool,
) -> AssetResult<SoundDone> {
    a.check_out(out)?;
    let idx = a.audio()?;
    let wem = idx.media_bytes(a, s)?;
    let duration = audio::duration(&wem);
    let codec = wemcodec::codec_of(&wem).ok();
    let (bytes, ext, note) = if raw {
        (wem, "wem", None)
    } else {
        match audio::decode(&wem) {
            Ok(c) => (c.bytes, c.format.ext(), None),
            Err(u) if u.codec.is_some() => (wem, "wem", Some(u.message())),
            Err(u) => return Err(AssetError::invalid(u.message())),
        }
    };
    let path = out.with_extension(ext);
    write_file(&path, &bytes)?;
    Ok(SoundDone {
        path: path.display().to_string(),
        ext: ext.into(),
        bytes: bytes.len() as u64,
        duration,
        codec: codec.map(|c| c.to_string()).unwrap_or_default(),
        note,
    })
}

fn write_file(out: &Path, bytes: &[u8]) -> AssetResult<()> {
    crate::assets::write_atomic(out, bytes)
}

/// Export several sounds into `out_dir`; `step` as [`files::extract_all`].
pub fn export_sounds(
    a: &AssetArchive,
    sounds: &[(String, SoundRef)],
    out_dir: &Path,
    wem: bool,
    step: &mut dyn FnMut(usize, usize) -> bool,
) -> AssetResult<SoundsDone> {
    a.check_out(&out_dir.join("x"))?;
    let mut d = SoundsDone {
        out_dir: out_dir.display().to_string(),
        files: 0,
        bytes: 0,
        failed: Vec::new(),
        failures: 0,
        written: Vec::new(),
        formats: BTreeMap::new(),
        kept: Vec::new(),
    };
    let fail = |d: &mut SoundsDone, what: String| {
        d.failures += 1;
        if d.failed.len() < 20 {
            d.failed.push(what);
        }
    };
    for (k, (trigger, s)) in sounds.iter().enumerate() {
        let raw = out_dir.join(audio::file_name(trigger, s, "wem"));
        match write_sound(a, s, &raw, false) {
            Ok(x) => {
                d.files += 1;
                d.bytes += x.bytes;
                *d.formats.entry(x.ext.clone()).or_default() += 1;
                if let Some(n) = &x.note
                    && d.kept.len() < 20
                {
                    d.kept.push(format!("{trigger} {}: {n}", s.key()));
                }
                d.written.push(WrittenSound {
                    key: s.key(),
                    ext: x.ext.clone(),
                    path: x.path.clone(),
                });
                if wem && x.ext != "wem" {
                    match write_sound(a, s, &raw, true) {
                        Ok(y) => {
                            d.files += 1;
                            d.bytes += y.bytes;
                        }
                        Err(e) => fail(&mut d, format!("{}: {}", raw.display(), e.message)),
                    }
                }
            }
            Err(e) => fail(&mut d, format!("{trigger} {}: {}", s.key(), e.message)),
        }
        if !step(k + 1, sounds.len()) {
            return Err(AssetError::cancelled());
        }
    }
    Ok(d)
}

pub type Out = Arc<Mutex<Box<dyn Write + Send>>>;

fn write_line(out: &Out, v: &impl Serialize) {
    if let Ok(s) = serde_json::to_string(v) {
        let mut o = out.lock().unwrap_or_else(|p| p.into_inner());
        let _ = writeln!(o, "{s}");
        let _ = o.flush();
    }
}

fn reply(id: u64, r: AssetResult<Value>) -> Reply {
    match r {
        Ok(v) => Reply {
            id,
            ok: true,
            result: Some(v),
            error: None,
            progress: None,
        },
        Err(e) => Reply {
            id,
            ok: false,
            result: None,
            error: Some(ReplyError::from(e)),
            progress: None,
        },
    }
}

fn progress_line(id: u64, done: u32, total: u32) -> Reply {
    Reply {
        id,
        ok: true,
        result: None,
        error: None,
        progress: Some(ExportProgress { done, total }),
    }
}

fn guard(f: impl FnOnce() -> AssetResult<Value>) -> AssetResult<Value> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(p) => Err(AssetError::internal(format!(
            "panic: {}",
            panic_message(p.as_ref())
        ))),
    }
}

fn to_value(v: impl Serialize) -> AssetResult<Value> {
    serde_json::to_value(v).map_err(|e| AssetError::internal(e.to_string()))
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Shared state of the serve loop.
pub struct Server {
    pub archive: Arc<AssetArchive>,
    pub test_panic: bool,
    jobs: Mutex<HashMap<u64, Arc<AtomicBool>>>,
    running: Mutex<usize>,
    /// Requests of any kind not answered yet.
    inflight: AtomicUsize,
}

impl Server {
    pub fn new(archive: Arc<AssetArchive>, test_panic: bool) -> Arc<Self> {
        Arc::new(Self {
            archive,
            test_panic,
            jobs: Mutex::new(HashMap::new()),
            running: Mutex::new(0),
            inflight: AtomicUsize::new(0),
        })
    }

    fn busy(&self) -> bool {
        self.inflight.load(Ordering::SeqCst) > 0
    }

    /// Answer a request that finishes in one reply.
    pub fn handle(&self, req: &AssetRequest) -> AssetResult<Value> {
        let a = &*self.archive;
        match req {
            AssetRequest::Status => to_value(AssetStatus {
                entries: a.entries() as u32,
                datacore: a.db_ready(),
                audio: a.audio_ready(),
                rss: rss_bytes(),
                peak_rss: peak_rss_bytes(),
            }),
            AssetRequest::List { dir, offset, limit } => {
                to_value(files::list(a, dir, *offset as usize, *limit as usize)?)
            }
            AssetRequest::Search { query, limit } => {
                to_value(files::search(a, query, *limit as usize))
            }
            AssetRequest::Preview { path, out } => to_value(files::preview(a, path, out)?),
            AssetRequest::Sounds { what } => to_value(audio::record_sounds(a, what)?),
            AssetRequest::SoundSearch { query, limit } => {
                to_value(a.audio()?.search_triggers(query, *limit as usize))
            }
            AssetRequest::Trigger { name } => to_value(a.audio()?.trigger(a, name)?),
            AssetRequest::Sound { sound, out, raw } => to_value(write_sound(a, sound, out, *raw)?),
            AssetRequest::TestPanic if self.test_panic => {
                panic!("test panic inside the asset worker")
            }
            _ => Err(AssetError::invalid("unknown op")),
        }
    }

    /// Run a streamed job: progress lines, then the result.
    fn job(
        &self,
        id: u64,
        req: &AssetRequest,
        out: &Out,
        cancel: &AtomicBool,
    ) -> AssetResult<Value> {
        let a = &*self.archive;
        let mut last = Instant::now() - Duration::from_secs(1);
        let mut step = |done: usize, total: usize| {
            if done == total || last.elapsed() >= Duration::from_millis(150) {
                last = Instant::now();
                write_line(out, &progress_line(id, done as u32, total as u32));
            }
            !cancel.load(Ordering::SeqCst)
        };
        match req {
            AssetRequest::Extract {
                paths,
                out_dir,
                convert,
            } => {
                let plan = files::plan_extract(a, paths)?;
                step(0, plan.len());
                to_value(files::extract_all(a, &plan, out_dir, *convert, &mut step)?)
            }
            AssetRequest::ExportSounds {
                sounds,
                out_dir,
                wem,
            } => {
                step(0, sounds.len());
                to_value(export_sounds(a, sounds, out_dir, *wem, &mut step)?)
            }
            AssetRequest::ExportModel {
                what,
                options,
                out: path,
            } => {
                let last = Mutex::new(Instant::now() - Duration::from_secs(1));
                let tell = |f: f32, _stage: &str| {
                    let mut l = lock(&last);
                    if l.elapsed() >= Duration::from_millis(250) {
                        *l = Instant::now();
                        let done =
                            ((f.clamp(0.0, 1.0) * MODEL_STEPS as f32) as u32).min(MODEL_STEPS - 1);
                        write_line(out, &progress_line(id, done, MODEL_STEPS));
                    }
                    !cancel.load(Ordering::SeqCst)
                };
                write_line(out, &progress_line(id, 0, MODEL_STEPS));
                to_value(model::export_model(a, what, options, path, &tell)?)
            }
            _ => Err(AssetError::invalid("not a streamed op")),
        }
    }

    /// Start one request line on its own thread.
    pub fn dispatch(self: &Arc<Self>, line: &str, out: &Out) {
        let env = match serde_json::from_str::<AssetEnvelope>(line) {
            Ok(e) => e,
            Err(e) => {
                let id = serde_json::from_str::<Value>(line)
                    .ok()
                    .and_then(|v| v.get("id").and_then(Value::as_u64))
                    .unwrap_or(0);
                write_line(out, &reply(id, Err(AssetError::invalid(e.to_string()))));
                return;
            }
        };
        let id = env.id;
        match env.req {
            AssetRequest::Cancel { job } => {
                let hit = lock(&self.jobs).get(&job).cloned();
                if let Some(f) = &hit {
                    f.store(true, Ordering::SeqCst);
                }
                write_line(out, &reply(id, Ok(Value::Bool(hit.is_some()))));
            }
            req @ (AssetRequest::Extract { .. }
            | AssetRequest::ExportSounds { .. }
            | AssetRequest::ExportModel { .. }) => {
                {
                    let mut n = lock(&self.running);
                    if *n > 0 {
                        write_line(
                            out,
                            &reply(
                                id,
                                Err(AssetError::new("busy", "another export is running")),
                            ),
                        );
                        return;
                    }
                    *n += 1;
                }
                let flag = Arc::new(AtomicBool::new(false));
                lock(&self.jobs).insert(id, flag.clone());
                self.inflight.fetch_add(1, Ordering::SeqCst);
                let me = self.clone();
                let out = out.clone();
                std::thread::spawn(move || {
                    let r = guard(|| me.job(id, &req, &out, &flag));
                    lock(&me.jobs).remove(&id);
                    write_line(&out, &reply(id, r));
                    *lock(&me.running) -= 1;
                    me.inflight.fetch_sub(1, Ordering::SeqCst);
                });
            }
            req => {
                self.inflight.fetch_add(1, Ordering::SeqCst);
                let me = self.clone();
                let out = out.clone();
                std::thread::spawn(move || {
                    let r = guard(|| me.handle(&req));
                    write_line(&out, &reply(id, r));
                    me.inflight.fetch_sub(1, Ordering::SeqCst);
                });
            }
        }
    }

    /// Stop every job (EOF) and wait a little for every request to finish.
    fn stop_all(&self) {
        for f in lock(&self.jobs).values() {
            f.store(true, Ordering::SeqCst);
        }
        let until = Instant::now() + Duration::from_secs(10);
        while self.busy() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

/// Answer request lines from `input` until it closes or the worker is idle for `idle`.
pub fn serve<R: BufRead + Send + 'static>(
    server: &Arc<Server>,
    input: R,
    out: Out,
    idle: Duration,
) {
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in input.lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut last = Instant::now();
    loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => {
                if !line.trim().is_empty() {
                    last = Instant::now();
                    server.dispatch(&line, &out);
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if server.busy() {
                    last = Instant::now();
                } else if last.elapsed() >= idle {
                    return;
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                server.stop_all();
                return;
            }
        }
    }
}

#[derive(Serialize)]
struct Event<'a> {
    event: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    info: Option<&'a WorkerInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// `asset-worker` arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    pub p4k: PathBuf,
    pub idle: Duration,
    pub test_panic: bool,
}

/// `--p4k <P> [--idle-secs N] [--test-panic]`
pub fn parse_args(args: &[std::ffi::OsString]) -> Result<Args, String> {
    let mut p4k = None;
    let mut idle = IDLE_EXIT;
    let mut test_panic = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.to_str() {
            Some("--p4k") => p4k = it.next().map(PathBuf::from),
            Some("--idle-secs") => {
                let n: u64 = it
                    .next()
                    .and_then(|v| v.to_str())
                    .and_then(|v| v.parse().ok())
                    .ok_or("--idle-secs needs a number")?;
                idle = Duration::from_secs(n.max(1));
            }
            Some("--test-panic") => test_panic = true,
            _ => return Err(format!("unknown argument {}", a.to_string_lossy())),
        }
    }
    Ok(Args {
        p4k: p4k.ok_or("needs --p4k <Data.p4k>")?,
        idle,
        test_panic,
    })
}

/// The whole worker: open, print `ready`, serve stdin. Returns the exit code.
pub fn run(args: &Args) -> i32 {
    let started = Instant::now();
    let out: Out = Arc::new(Mutex::new(Box::new(std::io::stdout())));
    let archive = match AssetArchive::open(&args.p4k) {
        Ok(a) => Arc::new(a),
        Err(e) => {
            write_line(
                &out,
                &Event {
                    event: "failed",
                    info: None,
                    error: Some(e.message),
                },
            );
            return 1;
        }
    };
    let info = WorkerInfo {
        records: archive.entries() as u32,
        main: 0,
        types: 0,
        struct_defs: 0,
        dcb_bytes: 0,
        start_ms: started.elapsed().as_millis() as u64,
        rss: rss_bytes(),
        source: args.p4k.display().to_string(),
        sections: None,
    };
    write_line(
        &out,
        &Event {
            event: "ready",
            info: Some(&info),
            error: None,
        },
    );
    let server = Server::new(archive, args.test_panic);
    serve(
        &server,
        std::io::BufReader::new(std::io::stdin()),
        out,
        args.idle,
    );
    0
}

#[cfg(test)]
mod tests;

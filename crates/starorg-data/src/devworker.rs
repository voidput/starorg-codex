//! The `dev-worker` process body (addendum 7.2): the DataCore explorer behind a JSON-lines
//! protocol, so a DataCore panic never takes the app down.
//!
//! The app starts `starorg-strings-app dev-worker --p4k <P>` (or `--dcb <file> [--ini
//! <file>]`). The worker opens the archive read only (count check, size limits, CRC-32C as
//! extraction does), reads `Game2.dcb` and the stock `global.ini`, drops the archive index,
//! parses and indexes every record, then prints one event line:
//!
//! - `{"event":"ready","info":{...}}` ([`WorkerInfo`]), or
//! - `{"event":"failed","error":"..."}` and exits 1.
//!
//! Then it answers one request per stdin line, `{"id":1,"op":"tree",...}` ([`Request`]), with
//! one reply line `{"id":1,"ok":true,"result":...}` or `{"id":1,"ok":false,"error":{...}}`
//! ([`Reply`]). A type export also sends `{"id":n,"ok":true,"progress":{...}}` lines and keeps
//! answering other requests while it runs. Every request runs under `catch_unwind`: a panic
//! is an `internal` error reply. The worker exits 0 when stdin closes (EOF).

use crate::extract::{
    self, DCB_ENTRY, DCB_FALLBACK_ENTRY, ExtractError, INI_ENTRY, MAX_DCB_BYTES, MAX_INI_BYTES,
    guarded, panic_message,
};
use crate::starbreaker::DevDatabase;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use starorg_core::dev::explorer::{MAX_PAGE, TreeRoot};
use starorg_core::dev::record::{self, RecordHeader, RecordView};
use starorg_core::ini::StockIni;
use std::collections::VecDeque;
use std::io::{BufRead, BufWriter, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, TryRecvError};
use std::time::Instant;

/// Records kept as JSON values for back/forward (7.2).
pub const LRU_RECORDS: usize = 32;
pub const LRU_BYTES: usize = 16 * 1024 * 1024;
/// Records written per step of a type export before pending requests are answered.
const EXPORT_STEP: usize = 64;

/// What the worker reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DevInput {
    P4k(PathBuf),
    /// A `Game2.dcb` already extracted, and optionally a stock `global.ini`.
    Dcb {
        dcb: PathBuf,
        ini: Option<PathBuf>,
    },
}

/// The DataCore bytes and the stock text, after the archive is dropped.
pub struct LoadedDcb {
    pub dcb: Vec<u8>,
    pub ini: Option<Vec<u8>>,
    /// `Data\Game2.dcb` or the file read.
    pub source: String,
    /// The game folder (the p4k's), where exports never go.
    pub game_dir: Option<PathBuf>,
}

fn read_file(path: &Path, max: u64) -> Result<Vec<u8>, ExtractError> {
    let meta = std::fs::metadata(path)
        .map_err(|e| ExtractError::Output(format!("{}: {e}", path.display())))?;
    if meta.len() > max {
        return Err(ExtractError::TooLarge {
            name: path.display().to_string(),
            size: meta.len(),
            max,
        });
    }
    std::fs::read(path).map_err(|e| ExtractError::Output(format!("{}: {e}", path.display())))
}

/// Read the DataCore (and stock text) as extraction does; the archive is dropped on return.
pub fn load(input: &DevInput) -> Result<LoadedDcb, ExtractError> {
    match input {
        DevInput::P4k(p4k) => {
            let (archive, _) = guarded("archive open", || extract::open_checked(p4k))?;
            let ini = archive
                .entry(INI_ENTRY)
                .map(|i| extract::read_checked(&archive, &i, MAX_INI_BYTES))
                .transpose()?;
            let info = archive
                .entry(DCB_ENTRY)
                .or_else(|| archive.entry(DCB_FALLBACK_ENTRY))
                .ok_or(ExtractError::Missing(DCB_ENTRY))?;
            let dcb = extract::read_checked(&archive, &info, MAX_DCB_BYTES)?;
            drop(archive);
            Ok(LoadedDcb {
                dcb,
                ini,
                source: info.name,
                game_dir: p4k.parent().map(Path::to_path_buf),
            })
        }
        DevInput::Dcb { dcb, ini } => Ok(LoadedDcb {
            dcb: read_file(dcb, MAX_DCB_BYTES)?,
            ini: ini
                .as_deref()
                .map(|p| read_file(p, MAX_INI_BYTES))
                .transpose()?,
            source: dcb.display().to_string(),
            game_dir: None,
        }),
    }
}

/// Resident memory of this process, where the OS tells us cheaply (Linux).
pub fn rss_bytes() -> Option<u64> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = s.lines().find(|l| l.starts_with("VmRSS:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb * 1024)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerInfo {
    pub records: u32,
    pub main: u32,
    pub types: u32,
    pub struct_defs: u32,
    pub dcb_bytes: u64,
    /// Load + parse + index, milliseconds.
    pub start_ms: u64,
    pub rss: Option<u64>,
    pub source: String,
}

/// One request line (without its `id`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum Request {
    Status,
    Types,
    Tree {
        root: TreeRoot,
        #[serde(default)]
        node: String,
        #[serde(default)]
        offset: u32,
        limit: u32,
        main_only: bool,
    },
    Search {
        query: String,
        limit: u32,
    },
    /// `what`: a GUID, a record path or a record name.
    Record {
        what: String,
        #[serde(default)]
        pointer: String,
        #[serde(default)]
        depth: Option<u32>,
    },
    /// One record, pretty JSON in StarBreaker's form, to `out` (via `out.tmp`).
    ExportRecord {
        what: String,
        out: PathBuf,
    },
    /// Every record of a type as one JSON array (or NDJSON), streamed.
    ExportType {
        name: String,
        out: PathBuf,
        #[serde(default)]
        main_only: bool,
        #[serde(default)]
        ndjson: bool,
    },
    /// Stop the running export with this request id.
    Cancel {
        job: u64,
    },
    /// Test hook: panics inside the request guard (only with [`Hooks::panic_op`]).
    TestPanic,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub id: u64,
    #[serde(flatten)]
    pub req: Request,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplyError {
    /// `not-found`, `invalid`, `busy`, `cancelled`, `internal`.
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportProgress {
    pub done: u32,
    pub total: u32,
}

/// One reply line. Exactly one of `result`, `error`, `progress` is set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    pub id: u64,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ReplyError>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<ExportProgress>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportDone {
    pub path: String,
    pub records: u32,
    pub bytes: u64,
}

/// Test hooks for [`serve`]; all off in the app.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Hooks {
    pub panic_op: bool,
}

fn fail(code: &str, message: impl Into<String>) -> ReplyError {
    ReplyError {
        code: code.into(),
        message: message.into(),
    }
}

/// Exports never go inside the game folder or its Wine prefix.
pub fn check_out(out: &Path, game_dir: Option<&Path>) -> Result<(), ReplyError> {
    if out.as_os_str().is_empty() || out.is_dir() {
        return Err(fail(
            "invalid",
            format!("{} is not a file path", out.display()),
        ));
    }
    let abs = starorg_core::fsutil::canonical_lenient(out);
    let mut protected: Vec<PathBuf> = game_dir
        .map(starorg_core::fsutil::canonical_lenient)
        .into_iter()
        .collect();
    if let Some(g) = game_dir {
        protected.extend(starorg_core::discovery::enclosing_wine_prefix(
            &starorg_core::fsutil::canonical_lenient(g),
        ));
    }
    match protected.iter().find(|p| abs.starts_with(p)) {
        Some(p) => Err(fail(
            "invalid",
            format!(
                "{} is inside {}; exports never go there",
                abs.display(),
                p.display()
            ),
        )),
        None => Ok(()),
    }
}

fn tmp_path(out: &Path) -> PathBuf {
    let mut s = out.as_os_str().to_owned();
    s.push(".tmp");
    PathBuf::from(s)
}

/// One record in StarBreaker's pretty JSON form, written via `<out>.tmp`.
pub fn export_record(db: &DevDatabase, i: usize, out: &Path) -> Result<ExportDone, String> {
    let tmp = tmp_path(out);
    let r = (|| {
        let mut w = BufWriter::new(std::fs::File::create(&tmp).map_err(|e| e.to_string())?);
        db.write_export(i, &mut w, true)?;
        w.flush().map_err(|e| e.to_string())?;
        w.get_ref().sync_all().map_err(|e| e.to_string())?;
        drop(w);
        std::fs::rename(&tmp, out).map_err(|e| e.to_string())
    })();
    if r.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    r?;
    Ok(ExportDone {
        path: out.display().to_string(),
        records: 1,
        bytes: std::fs::metadata(out).map(|m| m.len()).unwrap_or(0),
    })
}

/// A type export in progress: `[`, record, `,` ..., `]` (or one compact record per line).
pub struct TypeExport {
    records: Vec<usize>,
    next: usize,
    w: Option<BufWriter<std::fs::File>>,
    tmp: PathBuf,
    out: PathBuf,
    ndjson: bool,
}

impl TypeExport {
    pub fn start(records: Vec<usize>, out: &Path, ndjson: bool) -> Result<Self, String> {
        let tmp = tmp_path(out);
        let mut w = BufWriter::new(std::fs::File::create(&tmp).map_err(|e| e.to_string())?);
        if !ndjson {
            w.write_all(b"[").map_err(|e| e.to_string())?;
        }
        Ok(Self {
            records,
            next: 0,
            w: Some(w),
            tmp,
            out: out.to_path_buf(),
            ndjson,
        })
    }

    pub fn total(&self) -> usize {
        self.records.len()
    }

    pub fn done(&self) -> usize {
        self.next
    }

    /// Write up to `n` more records; true when all are written.
    pub fn step(&mut self, db: &DevDatabase, n: usize) -> Result<bool, String> {
        let w = self.w.as_mut().ok_or("export already finished")?;
        let end = (self.next + n).min(self.records.len());
        for k in self.next..end {
            if self.ndjson {
                // StarBreaker's compact form still breaks lines; JSON strings never hold a raw
                // newline, so dropping them gives one record per line.
                let mut buf = Vec::new();
                db.write_export(self.records[k], &mut buf, false)?;
                buf.retain(|&b| b != b'\n' && b != b'\r');
                buf.push(b'\n');
                w.write_all(&buf).map_err(|e| e.to_string())?;
            } else {
                w.write_all(if k == 0 { b"\n" } else { b",\n" })
                    .map_err(|e| e.to_string())?;
                db.write_export(self.records[k], &mut *w, true)?;
            }
        }
        self.next = end;
        Ok(self.next == self.records.len())
    }

    /// Close the array, sync and rename into place.
    pub fn finish(mut self) -> Result<ExportDone, String> {
        let r = (|| {
            let mut w = self.w.take().ok_or("export already finished")?;
            if !self.ndjson {
                w.write_all(b"\n]\n").map_err(|e| e.to_string())?;
            }
            w.flush().map_err(|e| e.to_string())?;
            w.get_ref().sync_all().map_err(|e| e.to_string())?;
            drop(w);
            std::fs::rename(&self.tmp, &self.out).map_err(|e| e.to_string())
        })();
        if r.is_err() {
            let _ = std::fs::remove_file(&self.tmp);
        }
        r?;
        Ok(ExportDone {
            path: self.out.display().to_string(),
            records: self.records.len() as u32,
            bytes: std::fs::metadata(&self.out).map(|m| m.len()).unwrap_or(0),
        })
    }
}

impl Drop for TypeExport {
    /// A cancelled or failed export leaves nothing behind.
    fn drop(&mut self) {
        if self.w.take().is_some() {
            let _ = std::fs::remove_file(&self.tmp);
        }
    }
}

/// Cached record values (by index entry) with their compact JSON size.
#[derive(Default)]
struct Lru {
    items: VecDeque<(usize, Arc<Value>, usize)>,
    bytes: usize,
}

impl Lru {
    fn get(&mut self, i: usize) -> Option<(Arc<Value>, usize)> {
        let at = self.items.iter().position(|x| x.0 == i)?;
        let item = self.items.remove(at)?;
        let out = (item.1.clone(), item.2);
        self.items.push_front(item);
        Some(out)
    }

    fn put(&mut self, i: usize, v: Arc<Value>, size: usize) {
        if size > LRU_BYTES {
            return;
        }
        self.items.push_front((i, v, size));
        self.bytes += size;
        while self.items.len() > LRU_RECORDS || self.bytes > LRU_BYTES {
            match self.items.pop_back() {
                Some(x) => self.bytes -= x.2,
                None => break,
            }
        }
    }
}

/// The request side of a running worker: index, stock text, cache.
pub struct Session<'d, 'a> {
    pub db: &'d DevDatabase<'a>,
    pub stock: Option<StockIni>,
    pub info: WorkerInfo,
    pub game_dir: Option<PathBuf>,
    pub hooks: Hooks,
    lru: Lru,
}

impl<'d, 'a> Session<'d, 'a> {
    pub fn new(db: &'d DevDatabase<'a>, ini: Option<Vec<u8>>, info: WorkerInfo) -> Self {
        Self {
            db,
            stock: ini.map(StockIni::parse),
            info,
            game_dir: None,
            hooks: Hooks::default(),
            lru: Lru::default(),
        }
    }

    fn find(&self, what: &str) -> Result<usize, ReplyError> {
        self.db
            .index()
            .find(what)
            .ok_or_else(|| fail("not-found", format!("no record {what:?}")))
    }

    /// The record's value (cached) and its compact size.
    pub fn value(&mut self, i: usize) -> Result<(Arc<Value>, usize), ReplyError> {
        if let Some(x) = self.lru.get(i) {
            return Ok(x);
        }
        let v = self.db.record_value(i).map_err(|e| fail("internal", e))?;
        let size = serde_json::to_string(&v).map(|s| s.len()).unwrap_or(0);
        let v = Arc::new(v);
        self.lru.put(i, v.clone(), size);
        Ok((v, size))
    }

    pub fn record(
        &mut self,
        what: &str,
        pointer: &str,
        depth: Option<u32>,
    ) -> Result<RecordView, ReplyError> {
        let i = self.find(what)?;
        let (v, size) = self.value(i)?;
        let (json, truncated) = record::view_json(&v, pointer, depth.map(|d| d as usize))
            .map_err(|e| fail("not-found", e))?;
        let node = v.pointer(pointer).unwrap_or(&v);
        let loc = record::resolve_loc(&record::loc_keys(node), self.stock.as_ref());
        let e = self.db.index().entry(i).expect("found");
        Ok(RecordView {
            header: RecordHeader {
                guid: e.guid.clone(),
                name: e.name.clone(),
                struct_type: e.struct_type.clone(),
                path: e.path.clone(),
                tag: e.tag.clone(),
                main: e.main,
                bytes: size as u64,
            },
            json,
            pointer: pointer.to_string(),
            truncated,
            loc,
        })
    }

    /// Answer one request that finishes at once (everything but `export-type`).
    pub fn handle(&mut self, req: &Request) -> Result<Value, ReplyError> {
        let to =
            |x: Result<Value, serde_json::Error>| x.map_err(|e| fail("internal", e.to_string()));
        let db = self.db;
        let ix = db.index();
        match req {
            Request::Status => {
                let mut info = self.info.clone();
                info.rss = rss_bytes();
                to(serde_json::to_value(info))
            }
            Request::Types => to(serde_json::to_value(ix.types())),
            Request::Tree {
                root,
                node,
                offset,
                limit,
                main_only,
            } => to(serde_json::to_value(ix.tree(
                *root,
                node,
                *offset as usize,
                (*limit as usize).min(MAX_PAGE),
                *main_only,
            ))),
            Request::Search { query, limit } => {
                to(serde_json::to_value(ix.search(query, *limit as usize)))
            }
            Request::Record {
                what,
                pointer,
                depth,
            } => {
                let r = self.record(what, pointer, *depth)?;
                to(serde_json::to_value(r))
            }
            Request::ExportRecord { what, out } => {
                check_out(out, self.game_dir.as_deref())?;
                let i = self.find(what)?;
                let done = export_record(self.db, i, out).map_err(|e| fail("internal", e))?;
                to(serde_json::to_value(done))
            }
            Request::TestPanic if self.hooks.panic_op => {
                panic!("test panic inside the dev worker")
            }
            Request::TestPanic => Err(fail("invalid", "unknown op")),
            Request::ExportType { .. } | Request::Cancel { .. } => {
                Err(fail("invalid", "handled by the serve loop"))
            }
        }
    }
}

fn write_line(out: &mut impl Write, v: &impl Serialize) -> std::io::Result<()> {
    let s = serde_json::to_string(v).map_err(std::io::Error::other)?;
    writeln!(out, "{s}")?;
    out.flush()
}

fn reply(id: u64, r: Result<Value, ReplyError>) -> Reply {
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
            error: Some(e),
            progress: None,
        },
    }
}

/// Run `f` and turn a panic into an `internal` error.
fn guard<T>(f: impl FnOnce() -> Result<T, ReplyError>) -> Result<T, ReplyError> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(p) => Err(fail(
            "internal",
            format!("panic: {}", panic_message(p.as_ref())),
        )),
    }
}

/// Answer requests from `input` until it closes. Lines are read on their own thread so a
/// running type export can answer other requests and see `cancel` between records.
pub fn serve<R, W>(session: &mut Session, input: R, out: &mut W) -> std::io::Result<()>
where
    R: BufRead + Send + 'static,
    W: Write,
{
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in input.lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut job: Option<(u64, TypeExport, u32)> = None;
    loop {
        let line = if job.is_some() {
            match rx.try_recv() {
                Ok(l) => Some(l),
                Err(TryRecvError::Empty) => None,
                // EOF: the parent is gone; the export's Drop removes its temporary file.
                Err(TryRecvError::Disconnected) => return Ok(()),
            }
        } else {
            match rx.recv() {
                Ok(l) => Some(l),
                Err(_) => return Ok(()),
            }
        };
        if let Some(line) = line.filter(|l| !l.trim().is_empty()) {
            match serde_json::from_str::<Envelope>(&line) {
                Err(e) => {
                    let id = serde_json::from_str::<Value>(&line)
                        .ok()
                        .and_then(|v| v.get("id").and_then(Value::as_u64))
                        .unwrap_or(0);
                    write_line(out, &reply(id, Err(fail("invalid", e.to_string()))))?;
                }
                Ok(Envelope {
                    id,
                    req: Request::Cancel { job: target },
                }) => {
                    let hit = job.as_ref().is_some_and(|j| j.0 == target);
                    if hit {
                        // Drop removes the temporary file.
                        job = None;
                        write_line(
                            out,
                            &reply(target, Err(fail("cancelled", "export cancelled"))),
                        )?;
                    }
                    write_line(out, &reply(id, Ok(Value::Bool(hit))))?;
                }
                Ok(Envelope {
                    id,
                    req:
                        Request::ExportType {
                            name,
                            out: path,
                            main_only,
                            ndjson,
                        },
                }) => {
                    let started = guard(|| {
                        if job.is_some() {
                            return Err(fail("busy", "another export is running"));
                        }
                        check_out(&path, session.game_dir.as_deref())?;
                        let records = session.db.index().of_type(&name, main_only);
                        if records.is_empty() {
                            return Err(fail("not-found", format!("no records of type {name}")));
                        }
                        TypeExport::start(records, &path, ndjson).map_err(|e| fail("internal", e))
                    });
                    match started {
                        Ok(x) => {
                            let total = x.total() as u32;
                            write_line(
                                out,
                                &Reply {
                                    id,
                                    ok: true,
                                    result: None,
                                    error: None,
                                    progress: Some(ExportProgress { done: 0, total }),
                                },
                            )?;
                            job = Some((id, x, 0));
                        }
                        Err(e) => write_line(out, &reply(id, Err(e)))?,
                    }
                }
                Ok(Envelope { id, req }) => {
                    let r = guard(|| session.handle(&req));
                    write_line(out, &reply(id, r))?;
                }
            }
        }
        let stepped = job.as_mut().map(|(id, x, last)| {
            let db = session.db;
            let r = guard(|| x.step(db, EXPORT_STEP).map_err(|e| fail("internal", e)));
            let done = x.done() as u32;
            let total = x.total() as u32;
            // Progress about every 1 %.
            let tell = done - *last >= (total / 100).max(EXPORT_STEP as u32);
            if tell {
                *last = done;
            }
            (*id, r, tell, ExportProgress { done, total })
        });
        match stepped {
            None => {}
            Some((id, Ok(false), tell, p)) => {
                if tell {
                    write_line(
                        out,
                        &Reply {
                            id,
                            ok: true,
                            result: None,
                            error: None,
                            progress: Some(p),
                        },
                    )?;
                }
            }
            Some((id, Ok(true), _, _)) => {
                let (_, x, _) = job.take().expect("job");
                let r = x.finish().map_err(|e| fail("internal", e)).and_then(|d| {
                    serde_json::to_value(d).map_err(|e| fail("internal", e.to_string()))
                });
                write_line(out, &reply(id, r))?;
            }
            Some((id, Err(e), _, _)) => {
                job = None;
                write_line(out, &reply(id, Err(e)))?;
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

/// The whole worker: load, parse, index, print `ready`, serve stdin. Returns the exit code.
pub fn run(input: &DevInput) -> i32 {
    let started = Instant::now();
    let mut stdout = std::io::stdout();
    let failed = |stdout: &mut std::io::Stdout, e: String| {
        let _ = write_line(
            stdout,
            &Event {
                event: "failed",
                info: None,
                error: Some(e),
            },
        );
        1
    };
    let loaded = match load(input) {
        Ok(l) => l,
        Err(e) => return failed(&mut stdout, e.to_string()),
    };
    let db = match guarded("DataCore parse", || {
        DevDatabase::parse(&loaded.dcb).map_err(ExtractError::DataCore)
    }) {
        Ok(db) => db,
        Err(e) => return failed(&mut stdout, e.to_string()),
    };
    let ix = db.index();
    let summary = db.summary();
    let info = WorkerInfo {
        records: ix.len() as u32,
        main: ix.main_count() as u32,
        types: ix.types().len() as u32,
        struct_defs: summary.struct_defs as u32,
        dcb_bytes: loaded.dcb.len() as u64,
        start_ms: started.elapsed().as_millis() as u64,
        rss: rss_bytes(),
        source: loaded.source.clone(),
    };
    let mut session = Session::new(&db, loaded.ini.clone(), info.clone());
    session.game_dir = loaded.game_dir.clone();
    if write_line(
        &mut stdout,
        &Event {
            event: "ready",
            info: Some(&info),
            error: None,
        },
    )
    .is_err()
    {
        return 1;
    }
    let stdin = std::io::BufReader::new(std::io::stdin());
    match serve(&mut session, stdin, &mut stdout) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

/// `dev-worker` arguments: `--p4k <P>` or `--dcb <F> [--ini <F>]`.
pub fn parse_args(args: &[std::ffi::OsString]) -> Result<DevInput, String> {
    let mut p4k = None;
    let mut dcb = None;
    let mut ini = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.to_str() {
            Some("--p4k") => p4k = it.next().map(PathBuf::from),
            Some("--dcb") => dcb = it.next().map(PathBuf::from),
            Some("--ini") => ini = it.next().map(PathBuf::from),
            _ => return Err(format!("unknown argument {}", a.to_string_lossy())),
        }
    }
    match (p4k, dcb) {
        (Some(p), None) if ini.is_none() => Ok(DevInput::P4k(p)),
        (None, Some(d)) => Ok(DevInput::Dcb { dcb: d, ini }),
        _ => Err("needs --p4k <Data.p4k>, or --dcb <file> [--ini <file>]".into()),
    }
}

#[cfg(test)]
mod tests;

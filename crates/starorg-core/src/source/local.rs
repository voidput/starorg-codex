//! [`LocalExtraction`]: the v1 source. Runs the extraction worker as a child process
//! (spec 3.3).
//!
//! - A crash of any kind in the worker (panic, out-of-memory abort, stack overflow) is an
//!   exit status here, not a crash of the caller.
//! - All worker memory (about 890 MB at peak) is returned when it exits.
//! - The time budget is enforced by killing the worker.
//!
//! The worker protocol (files and the stdout line) is defined in `starorg_data::worker`;
//! this side only needs the file names, repeated here so `starorg-core` does not depend on
//! `starorg-data`.

use super::{Budget, GameSource, Loaded, Origin, SourceError, Warning};
use crate::background::Priority;
use crate::discovery::Channel;
use crate::model::GameData;
use serde::Deserialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const BASE_INI: &str = "base.ini";
const GAMEDATA_JSON: &str = "gamedata.json";
const META_JSON: &str = "meta.json";
const KEYREFS_JSON: &str = "keyrefs.json";
/// The prefix of a load's temp folder under `work_root` (`starorg-extract-XXXXXX`).
pub const WORK_PREFIX: &str = "starorg-extract-";
/// The lock file a running load holds inside its temp folder.
const WORK_LOCK: &str = ".starorg-extract.lock";
/// Temp folders with nothing newer than this are leftovers of a run that never cleaned up.
pub const WORK_STALE: Duration = Duration::from_secs(3600);
/// Keep at most this much of the worker's stderr for error messages.
const STDERR_KEEP: usize = 4096;

#[derive(Deserialize)]
struct Report {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Deserialize)]
struct Meta {
    starbreaker_commit: String,
    #[serde(default)]
    warnings: Vec<Warning>,
}

/// Extract from the channel's own `Data.p4k` with the worker process.
#[derive(Debug, Clone)]
pub struct LocalExtraction {
    /// The `starorg-codex` binary (CLI), started as `<worker> extract-worker ...`.
    pub worker: PathBuf,
    /// Where the worker writes its output; a fresh temp dir inside is used per load. `None`
    /// uses the system temp dir. (The M1 cache replaces this.)
    pub work_root: Option<PathBuf>,
    /// Read only the stock `global.ini` (skip DataCore); `game_data` comes back empty. Used
    /// as the text source of a published dump.
    pub ini_only: bool,
    /// Also run the loc-reference pass (`--keyrefs`): the app sets this, launch hooks do not.
    pub keyrefs: bool,
    /// How hard the worker may use the machine (`crate::background`): `Background` for the
    /// launch hooks and automatic loads, `Normal` for user-started ones.
    pub priority: Priority,
    /// Extra worker arguments, for tests (e.g. the hidden `--test-panic`).
    #[doc(hidden)]
    pub extra_args: Vec<String>,
}

impl LocalExtraction {
    pub fn new(worker: impl Into<PathBuf>) -> Self {
        Self {
            worker: worker.into(),
            work_root: None,
            ini_only: false,
            keyrefs: false,
            priority: crate::background::process_priority(),
            extra_args: Vec::new(),
        }
    }

    /// The same worker at `p`.
    pub fn with_priority(mut self, p: Priority) -> Self {
        self.priority = p;
        self
    }

    /// The same worker, reading only the stock `global.ini`.
    pub fn ini_only(mut self) -> Self {
        self.ini_only = true;
        self
    }

    /// The same worker, also running the loc-reference pass for Strings grouping.
    pub fn with_keyrefs(mut self) -> Self {
        self.keyrefs = true;
        self
    }

    /// Run the worker for `p4k` into `out`. On success `out` holds the worker's files.
    pub fn run_worker(&self, p4k: &Path, out: &Path, budget: &Budget) -> Result<(), SourceError> {
        let started = Instant::now();
        let mut cmd = Command::new(&self.worker);
        cmd.arg("extract-worker")
            .arg("--p4k")
            .arg(p4k)
            .arg("--out")
            .arg(out)
            .args(self.ini_only.then_some("--ini-only"))
            .args((self.keyrefs && !self.ini_only).then_some("--keyrefs"))
            .args(&self.extra_args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // The worker also stops itself at the budget's deadline, should this process end
        // before it could kill it.
        crate::background::prepare(&mut cmd, self.priority, budget.deadline(), false);
        let mut child = crate::background::spawn(&mut cmd, self.priority).map_err(|e| {
            SourceError::Worker(format!("cannot start {}: {e}", self.worker.display()))
        })?;
        let mut stdout = child.stdout.take().expect("piped stdout");
        let mut stderr = child.stderr.take().expect("piped stderr");
        let out_thread = std::thread::spawn(move || {
            let mut s = String::new();
            let _ = stdout.read_to_string(&mut s);
            s
        });
        let err_thread = std::thread::spawn(move || {
            let mut s = Vec::new();
            let _ = stderr.read_to_end(&mut s);
            let start = s.len().saturating_sub(STDERR_KEEP);
            String::from_utf8_lossy(&s[start..]).into_owned()
        });

        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if budget.expired() {
                let _ = child.kill();
                let _ = child.wait();
                let _ = out_thread.join();
                let _ = err_thread.join();
                // The worker renames its output into place only on success, but it might
                // have been killed right after; never report a partial result.
                let _ = std::fs::remove_dir_all(out);
                return Err(SourceError::Timeout(started.elapsed()));
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let stdout = out_thread.join().unwrap_or_default();
        let stderr = err_thread.join().unwrap_or_default();
        // It stopped itself at the deadline (it does, should this process be gone) just
        // before the loop above would have: the same stop. A memory stop stays a failure.
        if status.code() == Some(crate::background::EXIT_DEADLINE) && budget.expired() {
            let _ = std::fs::remove_dir_all(out);
            return Err(SourceError::Timeout(started.elapsed()));
        }
        let report = stdout
            .lines()
            .rev()
            .find_map(|l| serde_json::from_str::<Report>(l).ok());

        if status.success() && report.as_ref().is_some_and(|r| r.ok) && out.is_dir() {
            return Ok(());
        }
        let _ = std::fs::remove_dir_all(out);
        let detail = match report {
            Some(Report { error: Some(e), .. }) => e,
            _ => {
                let tail = stderr.trim();
                let how = describe_status(&status);
                if tail.is_empty() {
                    how
                } else {
                    format!("{how}; stderr: {tail}")
                }
            }
        };
        Err(SourceError::Worker(detail))
    }
}

/// Remove leftover `starorg-extract-*` folders in `work_root`: only those with nothing
/// modified for `older_than` and whose lock no running load holds (a load holds it until it
/// ends, so a second app instance or a CLI run in parallel keeps its folder). Returns the
/// folders removed. Errors are skipped: a folder that cannot be removed now is tried again on
/// the next start.
pub fn sweep_work(work_root: &Path, older_than: Duration) -> Vec<PathBuf> {
    let now = std::time::SystemTime::now();
    let mut removed = Vec::new();
    for e in std::fs::read_dir(work_root).into_iter().flatten().flatten() {
        let name = e.file_name();
        let Some(name) = name.to_str() else { continue };
        let p = e.path();
        let is_dir =
            std::fs::symlink_metadata(&p).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink());
        if !name.starts_with(WORK_PREFIX) || !is_dir {
            continue;
        }
        let stale = crate::storage::newest_mtime(&p)
            .is_some_and(|t| now.duration_since(t).unwrap_or_default() >= older_than);
        if !stale {
            continue;
        }
        // Taking the lock proves no live load holds it; release it before removing (Windows
        // cannot remove an open file).
        match crate::lock::try_lock(&p.join(WORK_LOCK)) {
            Ok(l) => drop(l),
            Err(_) => continue,
        }
        if std::fs::remove_dir_all(&p).is_ok() {
            removed.push(p);
        }
    }
    removed
}

fn describe_status(status: &std::process::ExitStatus) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            return format!("worker killed by signal {sig}");
        }
    }
    match status.code() {
        Some(c) => format!("worker exited with code {c}"),
        None => "worker ended abnormally".to_string(),
    }
}

impl GameSource for LocalExtraction {
    fn load(&self, channel: &Channel, budget: &Budget) -> Result<Loaded, SourceError> {
        let mut builder = tempfile::Builder::new();
        builder.prefix(WORK_PREFIX);
        let tmp = match &self.work_root {
            Some(root) => {
                std::fs::create_dir_all(root)?;
                builder.tempdir_in(root)?
            }
            None => builder.tempdir()?,
        };
        // Held for the whole load, so `sweep_work` in another process leaves this folder;
        // declared after `tmp`, so it is released before the folder is removed.
        let _held = crate::lock::try_lock(&tmp.path().join(WORK_LOCK)).ok();
        let out = tmp.path().join("out");
        self.run_worker(&channel.p4k, &out, budget)?;

        let bad = |what: &str, e: &dyn std::fmt::Display| {
            SourceError::Worker(format!("worker output {what} unreadable: {e}"))
        };
        let base_ini = std::fs::read(out.join(BASE_INI))?;
        let mut game_data: GameData =
            serde_json::from_slice(&std::fs::read(out.join(GAMEDATA_JSON))?)
                .map_err(|e| bad(GAMEDATA_JSON, &e))?;
        let meta: Meta = serde_json::from_slice(&std::fs::read(out.join(META_JSON))?)
            .map_err(|e| bad(META_JSON, &e))?;
        game_data.build = channel.build.clone();
        // Optional: a bad file only costs exact grouping, never the extraction.
        let keyrefs = std::fs::read(out.join(KEYREFS_JSON))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok());
        Ok(Loaded {
            base_ini,
            game_data,
            keyrefs,
            origin: Origin::LocalExtraction {
                starbreaker_commit: meta.starbreaker_commit,
            },
            warnings: meta.warnings,
        })
    }
}

#[cfg(test)]
mod tests;

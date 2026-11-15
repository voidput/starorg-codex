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
    /// The `starorg-strings` binary (CLI), started as `<worker> extract-worker ...`.
    pub worker: PathBuf,
    /// Where the worker writes its output; a fresh temp dir inside is used per load. `None`
    /// uses the system temp dir. (The M1 cache replaces this.)
    pub work_root: Option<PathBuf>,
    /// Extra worker arguments, for tests (e.g. the hidden `--test-panic`).
    #[doc(hidden)]
    pub extra_args: Vec<String>,
}

impl LocalExtraction {
    pub fn new(worker: impl Into<PathBuf>) -> Self {
        Self {
            worker: worker.into(),
            work_root: None,
            extra_args: Vec::new(),
        }
    }

    /// Run the worker for `p4k` into `out`. On success `out` holds the worker's files.
    pub fn run_worker(&self, p4k: &Path, out: &Path, budget: &Budget) -> Result<(), SourceError> {
        let started = Instant::now();
        let mut child = Command::new(&self.worker)
            .arg("extract-worker")
            .arg("--p4k")
            .arg(p4k)
            .arg("--out")
            .arg(out)
            .args(&self.extra_args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
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
        builder.prefix("starorg-extract-");
        let tmp = match &self.work_root {
            Some(root) => {
                std::fs::create_dir_all(root)?;
                builder.tempdir_in(root)?
            }
            None => builder.tempdir()?,
        };
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
        Ok(Loaded {
            base_ini,
            game_data,
            origin: Origin::LocalExtraction {
                starbreaker_commit: meta.starbreaker_commit,
            },
            warnings: meta.warnings,
        })
    }
}

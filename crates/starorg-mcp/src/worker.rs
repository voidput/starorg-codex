//! Extraction in a child process: one dataset per short-lived process.
//!
//! The server never extracts in its own process. A domain's records are held at once while
//! it parses (up to 4 GB for `ships`, about 1 GB for the pruned item domains, plus 0.9 GB of
//! DataCore), and the allocator keeps what it took; a worker returns all of it when it
//! exits, so the server's resident memory stays that of the cached JSON it serves. The
//! worker also measures its own peak RSS ([`Measurement`]).
//!
//! Protocol: `<exe> mcp-extract-worker --p4k <Data.p4k> --out <dir> --dataset <name>` writes
//! `<dir>/<name>.json` (atomically) and prints one JSON [`Measurement`] line on stdout.
//! Exit 0 on success, 1 on a failed extraction, 2 on bad arguments. It writes nothing but
//! that file, and refuses an `--out` inside the game's channel folder or Wine prefix.

use crate::rss;
use serde::{Deserialize, Serialize};
use starorg_gamedata::domain::{FileSource, NoFiles};
use starorg_gamedata::pipeline::{self, Game};
use starorg_gamedata::source::{self, ArchiveFiles, DataCoreSource};
use starorg_gamedata::{domains, output, strings};
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The hidden subcommand both binaries answer to.
pub const WORKER_ARG: &str = "mcp-extract-worker";

/// Give up on a worker after this long.
pub const WORKER_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// What one extraction cost.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Measurement {
    pub dataset: String,
    pub seconds: f64,
    /// Peak resident memory of the worker (`None` off Linux).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peak_rss_bytes: Option<u64>,
    /// Size of the JSON written.
    pub bytes: u64,
    #[serde(default)]
    pub warnings: usize,
}

/// Writes one dataset of an archive into a directory.
pub trait Extractor: Send + Sync {
    fn extract(&self, p4k: &Path, dataset: &str, out: &Path) -> Result<Measurement, String>;
}

/// Every dataset a worker can write: the record domains and the string table.
pub fn extractable() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = domains::all().iter().map(|d| d.name()).collect();
    v.push(strings::NAME);
    v
}

/// Run one extraction in this process (the worker's body).
pub fn extract_here(p4k: &Path, dataset: &str, out: &Path) -> Result<Measurement, String> {
    let started = Instant::now();
    let (text, warnings) = if dataset == strings::NAME {
        let files = source::open(p4k, false)?;
        let v = strings::dataset(
            &files.localization,
            &output::iso_now(),
            files.build.as_deref(),
            false,
        );
        (output::to_js_json(&v), 0)
    } else {
        let domain = domains::find(dataset).ok_or_else(|| format!("unknown dataset {dataset}"))?;
        let files = source::open(p4k, !domain.files().is_empty())?;
        let records = DataCoreSource::parse(&files.dcb)?;
        let archive = files.archive.map(ArchiveFiles::new);
        let file_source: &dyn FileSource = match &archive {
            Some(a) => a,
            None => &NoFiles,
        };
        let game = Game {
            records: &records,
            localization: &files.localization,
            files: file_source,
            build: files.build.clone(),
        };
        let d = pipeline::run(domain, &game, &output::iso_now()).map_err(|e| e.to_string())?;
        for w in &d.warnings {
            eprintln!("warning: {w}");
        }
        (d.to_json(), d.warnings.len())
    };
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let target = out.join(format!("{dataset}.json"));
    let tmp = out.join(format!("{dataset}.json.tmp-{}", std::process::id()));
    std::fs::write(&tmp, &text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &target).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{}: {e}", target.display())
    })?;
    Ok(Measurement {
        dataset: dataset.to_string(),
        seconds: (started.elapsed().as_secs_f64() * 100.0).round() / 100.0,
        peak_rss_bytes: rss::now().peak_bytes,
        bytes: text.len() as u64,
        warnings,
    })
}

/// `Some(exit code)` when `args` is a worker call (`args[1] == mcp-extract-worker`).
pub fn worker_main(args: &[OsString]) -> Option<i32> {
    if args.get(1).map(OsString::as_os_str) != Some(WORKER_ARG.as_ref()) {
        return None;
    }
    let mut p4k: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut dataset: Option<String> = None;
    let mut it = args[2..].iter();
    while let Some(a) = it.next() {
        match a.to_str() {
            Some("--p4k") => p4k = it.next().map(PathBuf::from),
            Some("--out") => out = it.next().map(PathBuf::from),
            Some("--dataset") => dataset = it.next().and_then(|d| d.to_str()).map(str::to_string),
            _ => {
                eprintln!("{WORKER_ARG}: unknown argument {}", a.to_string_lossy());
                return Some(2);
            }
        }
    }
    let (Some(p4k), Some(out), Some(dataset)) = (p4k, out, dataset) else {
        eprintln!("{WORKER_ARG}: needs --p4k, --out and --dataset");
        return Some(2);
    };
    if let Some(r) = protected_root(&p4k, &out) {
        eprintln!(
            "{WORKER_ARG}: --out {} is inside {}; refusing",
            out.display(),
            r.display()
        );
        return Some(2);
    }
    match extract_here(&p4k, &dataset, &out) {
        Ok(m) => {
            println!("{}", serde_json::to_string(&m).unwrap_or_default());
            Some(0)
        }
        Err(e) => {
            eprintln!("{WORKER_ARG}: {e}");
            Some(1)
        }
    }
}

/// The game folder or Wine prefix `out` would be inside, if any.
pub fn protected_root(p4k: &Path, out: &Path) -> Option<PathBuf> {
    let out_abs = starorg_core::fsutil::canonical_lenient(out);
    let p4k_abs = starorg_core::fsutil::canonical_lenient(p4k);
    let mut protected: Vec<PathBuf> = p4k_abs
        .parent()
        .filter(|d| d.join("build_manifest.id").is_file())
        .map(Path::to_path_buf)
        .into_iter()
        .collect();
    protected.extend(starorg_core::discovery::enclosing_wine_prefix(&p4k_abs));
    protected.into_iter().find(|r| out_abs.starts_with(r))
}

/// Starts `exe mcp-extract-worker` per dataset (the CLI binary or the app binary).
pub struct ProcessExtractor {
    pub exe: PathBuf,
    pub timeout: Duration,
}

impl ProcessExtractor {
    pub fn new(exe: PathBuf) -> Self {
        Self {
            exe,
            timeout: WORKER_TIMEOUT,
        }
    }
}

impl Extractor for ProcessExtractor {
    fn extract(&self, p4k: &Path, dataset: &str, out: &Path) -> Result<Measurement, String> {
        let mut cmd = Command::new(&self.exe);
        cmd.arg(WORKER_ARG)
            .arg("--p4k")
            .arg(p4k)
            .arg("--out")
            .arg(out)
            .arg("--dataset")
            .arg(dataset)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW: the app is a GUI process; no console flashes up.
            cmd.creation_flags(0x0800_0000);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("cannot start {}: {e}", self.exe.display()))?;
        // Drain both pipes on threads so a chatty worker never blocks on a full pipe.
        let mut so = child.stdout.take();
        let mut se = child.stderr.take();
        let t_out = std::thread::spawn(move || {
            let mut s = String::new();
            if let Some(p) = so.as_mut() {
                let _ = p.read_to_string(&mut s);
            }
            s
        });
        let t_err = std::thread::spawn(move || {
            let mut s = String::new();
            if let Some(p) = se.as_mut() {
                let _ = p.read_to_string(&mut s);
            }
            s
        });
        let deadline = Instant::now() + self.timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break s,
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "extracting {dataset} took over {} s; stopped",
                        self.timeout.as_secs()
                    ));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(e) => return Err(format!("waiting for the extraction worker: {e}")),
            }
        };
        let stdout = t_out.join().unwrap_or_default();
        let stderr = t_err.join().unwrap_or_default();
        if !status.success() {
            let tail: Vec<&str> = stderr.lines().rev().take(3).collect();
            let tail: Vec<&str> = tail.into_iter().rev().collect();
            return Err(format!(
                "extracting {dataset} failed ({status}): {}",
                tail.join(" | ")
            ));
        }
        stdout
            .lines()
            .rev()
            .find_map(|l| serde_json::from_str::<Measurement>(l).ok())
            .ok_or_else(|| format!("extracting {dataset}: the worker printed no result"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_args() {
        let a = |v: &[&str]| v.iter().map(OsString::from).collect::<Vec<_>>();
        assert_eq!(worker_main(&a(&["x", "status"])), None);
        assert_eq!(worker_main(&a(&["x", WORKER_ARG])), Some(2));
        assert_eq!(worker_main(&a(&["x", WORKER_ARG, "--bogus"])), Some(2));
        assert!(extractable().contains(&"game-item-tags"));
        assert!(extractable().contains(&strings::NAME));
    }

    #[test]
    fn refuses_out_inside_the_game_folder() {
        let dir = tempfile::tempdir().unwrap();
        let ch = dir.path().join("LIVE");
        std::fs::create_dir_all(&ch).unwrap();
        std::fs::write(ch.join("build_manifest.id"), "{}").unwrap();
        std::fs::write(ch.join("Data.p4k"), "").unwrap();
        assert!(protected_root(&ch.join("Data.p4k"), &ch.join("cache")).is_some());
        assert!(protected_root(&ch.join("Data.p4k"), &dir.path().join("elsewhere")).is_none());
    }

    #[test]
    fn a_missing_archive_fails_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let e = extract_here(&dir.path().join("nope.p4k"), "game-item-tags", dir.path());
        assert!(e.is_err());
        assert!(extract_here(dir.path(), "game-nope", dir.path()).is_err());
    }
}

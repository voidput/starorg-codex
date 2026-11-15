//! The `extract-worker` process body (spec 3.3).
//!
//! The parent starts `starorg-strings extract-worker --p4k <path> --out <dir>` and waits. On
//! success the worker has written `<dir>/` atomically (built in `<dir>.tmp-<pid>/`, files
//! fsynced, then renamed) with:
//!
//! - `base.ini`: the stock `global.ini`, byte for byte;
//! - `gamedata.json`: the serialised `GameData`;
//! - `meta.json`: [`WorkerMeta`].
//!
//! It prints one JSON line on stdout, [`WorkerReport`], and exits 0, or 1 on any error
//! (including a caught panic). On failure `<dir>` does not exist.

use crate::extract::{self, ExtractError, guarded};
use crate::starbreaker::STARBREAKER_COMMIT;
use serde::{Deserialize, Serialize};
use starorg_core::source::Warning;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const BASE_INI: &str = "base.ini";
pub const GAMEDATA_JSON: &str = "gamedata.json";
pub const META_JSON: &str = "meta.json";

/// Test hooks, reachable only through hidden CLI flags.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TestHook {
    #[default]
    None,
    /// Panic inside the guarded extraction, to prove panics become errors (release mode).
    Panic,
    /// Never finish, to prove the parent's timeout kills the worker.
    Hang,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerMeta {
    pub starbreaker_commit: String,
    pub parsed_entries: u64,
    /// False for a p4k v2 container, whose count cannot be checked.
    pub count_checked: bool,
    pub dcb_entry: String,
    pub warnings: Vec<Warning>,
}

/// The single stdout line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerReport {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn write_synced(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut f = std::fs::File::create(path)?;
    f.write_all(bytes)?;
    f.sync_all()
}

fn write_output(out: &Path, x: &extract::Extraction) -> std::io::Result<()> {
    let tmp = PathBuf::from(format!("{}.tmp-{}", out.display(), std::process::id()));
    if tmp.exists() {
        std::fs::remove_dir_all(&tmp)?;
    }
    std::fs::create_dir_all(&tmp)?;
    let game_data = &x.game_data;
    let meta = WorkerMeta {
        starbreaker_commit: STARBREAKER_COMMIT.to_string(),
        parsed_entries: x.parsed_entries,
        count_checked: x.count_check != crate::eocd::CountCheck::Unchecked,
        dcb_entry: x.dcb_entry.clone(),
        warnings: x
            .warnings
            .iter()
            .map(|(k, d)| Warning::new(k.clone(), d.clone()))
            .collect(),
    };
    let result = (|| {
        write_synced(&tmp.join(BASE_INI), &x.base_ini)?;
        write_synced(&tmp.join(GAMEDATA_JSON), &serde_json::to_vec(game_data)?)?;
        write_synced(&tmp.join(META_JSON), &serde_json::to_vec_pretty(&meta)?)?;
        if out.exists() {
            std::fs::remove_dir_all(out)?;
        }
        std::fs::rename(&tmp, out)
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&tmp);
    }
    result
}

/// Run the worker. Returns the process exit code (0 or 1) after printing the report.
pub fn run(p4k: &Path, out: &Path, hook: TestHook) -> i32 {
    let result = guarded("extraction", || {
        match hook {
            TestHook::None => {}
            TestHook::Panic => panic!("test panic inside the extraction worker"),
            TestHook::Hang => loop {
                std::thread::sleep(std::time::Duration::from_secs(3600));
            },
        }
        let x = extract::extract(p4k)?;
        write_output(out, &x)
            .map_err(|e| ExtractError::Output(format!("{}: {e}", out.display())))?;
        Ok(())
    });
    let report = match &result {
        Ok(()) => WorkerReport {
            ok: true,
            error: None,
        },
        Err(e) => WorkerReport {
            ok: false,
            error: Some(e.to_string()),
        },
    };
    let line = serde_json::to_string(&report).unwrap_or_else(|_| "{\"ok\":false}".to_string());
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{line}");
    let _ = stdout.flush();
    if result.is_ok() { 0 } else { 1 }
}

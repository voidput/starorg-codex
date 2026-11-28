//! The worker body: one dataset of an archive written into a folder.
//!
//! Kept apart from the process handling in [`crate::worker`] because this file is part of
//! every dataset's code key (`build.rs`): a change here extracts every dataset again, a change
//! to how workers are started does not.

use crate::rss;
use crate::worker::Measurement;
use starorg_gamedata::domain::{FileSource, NoFiles};
use starorg_gamedata::pipeline::{self, Game};
use starorg_gamedata::source::{self, ArchiveFiles, DataCoreSource};
use starorg_gamedata::{domains, output, strings};
use std::path::Path;
use std::time::Instant;

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
        sha256: Some(crate::stamps::sha256(text.as_bytes())),
    })
}

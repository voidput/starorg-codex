//! Extraction steps inside the worker process (spec 3.3).

use crate::eocd::{self, CountCheck, EocdError};
use crate::starbreaker::{Archive, ArchiveError, DataCoreSummary, EntryInfo, parse_datacore};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

/// Entry names inside the p4k use backslashes; written as literals, never built with
/// `Path::join`.
pub const INI_ENTRY: &str = "Data\\Localization\\english\\global.ini";
pub const DCB_ENTRY: &str = "Data\\Game2.dcb";
/// Used only when `Game2.dcb` is absent from an archive that passed the count check.
pub const DCB_FALLBACK_ENTRY: &str = "Data\\Game.dcb";
pub const MAX_INI_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_DCB_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    #[error(transparent)]
    Archive(#[from] ArchiveError),
    #[error("entry-count check failed: {0}")]
    Count(#[from] EocdError),
    #[error("{0} not found in archive")]
    Missing(&'static str),
    #[error("{name} declares {size} bytes, more than the {max} allowed")]
    TooLarge { name: String, size: u64, max: u64 },
    #[error("{name}: read {got} bytes, entry declares {declared}")]
    SizeMismatch {
        name: String,
        declared: u64,
        got: u64,
    },
    #[error("{name}: CRC-32C {got:08x} does not match the entry's {expected:08x}")]
    Crc {
        name: String,
        expected: u32,
        got: u32,
    },
    #[error("DataCore parse failed: {0}")]
    DataCore(String),
    #[error("cannot write output: {0}")]
    Output(String),
    #[error("panic during {stage}: {message}")]
    Panic {
        stage: &'static str,
        message: String,
    },
}

/// What the worker extracted.
#[derive(Debug, Clone)]
pub struct Extraction {
    /// Stock `global.ini`, byte for byte.
    pub base_ini: Vec<u8>,
    pub datacore: DataCoreSummary,
    /// Entries StarBreaker parsed.
    pub parsed_entries: u64,
    pub count_check: CountCheck,
    /// Name of the DataCore entry used.
    pub dcb_entry: String,
    /// Non-fatal findings, as `(kind, detail)`.
    pub warnings: Vec<(String, String)>,
}

/// Text of a panic payload.
pub fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

/// Run `f` and turn a panic into [`ExtractError::Panic`].
pub fn guarded<T>(
    stage: &'static str,
    f: impl FnOnce() -> Result<T, ExtractError>,
) -> Result<T, ExtractError> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(payload) => Err(ExtractError::Panic {
            stage,
            message: panic_message(payload.as_ref()),
        }),
    }
}

/// Check the declared size before reading, then the size and checksum after.
///
/// The `crc32` field of CIG's p4k entries is CRC-32C (Castagnoli), not the ZIP CRC-32: on
/// LIVE 4.10 a sample of 3,000 entries (stored, zstd and encrypted) all match CRC-32C and
/// none match CRC-32.
fn read_checked(archive: &Archive, info: &EntryInfo, max: u64) -> Result<Vec<u8>, ExtractError> {
    if info.uncompressed_size > max {
        return Err(ExtractError::TooLarge {
            name: info.name.clone(),
            size: info.uncompressed_size,
            max,
        });
    }
    let bytes = archive.read(info)?;
    if bytes.len() as u64 != info.uncompressed_size {
        return Err(ExtractError::SizeMismatch {
            name: info.name.clone(),
            declared: info.uncompressed_size,
            got: bytes.len() as u64,
        });
    }
    let got = crc32c::crc32c(&bytes);
    if got != info.crc32 {
        return Err(ExtractError::Crc {
            name: info.name.clone(),
            expected: info.crc32,
            got,
        });
    }
    Ok(bytes)
}

/// Open the archive and run the entry-count check only (steps 1 and 2).
pub fn open_checked(p4k: &Path) -> Result<(Archive, CountCheck), ExtractError> {
    let archive = Archive::open(p4k)?;
    let mut file = std::fs::File::open(p4k).map_err(|e| ArchiveError::Open(e.to_string()))?;
    let declared = eocd::read_declared_entry_count(&mut file)?;
    let check = eocd::check_entry_count(declared, archive.parsed_entries() as u64)?;
    Ok((archive, check))
}

/// Steps 1 to 5 of spec 3.3.
pub fn extract(p4k: &Path) -> Result<Extraction, ExtractError> {
    let mut warnings = Vec::new();
    let (archive, count_check) = guarded("archive open", || open_checked(p4k))?;
    if count_check == CountCheck::Unchecked {
        warnings.push((
            "count-unchecked".to_string(),
            "p4k v2 container: entry count not checked".to_string(),
        ));
    }
    let parsed_entries = archive.parsed_entries() as u64;

    let ini_info = archive
        .entry(INI_ENTRY)
        .ok_or(ExtractError::Missing(INI_ENTRY))?;
    let base_ini = read_checked(&archive, &ini_info, MAX_INI_BYTES)?;

    let dcb_info = match archive.entry(DCB_ENTRY) {
        Some(i) => i,
        None => {
            let i = archive
                .entry(DCB_FALLBACK_ENTRY)
                .ok_or(ExtractError::Missing(DCB_ENTRY))?;
            warnings.push((
                "dcb-fallback".to_string(),
                format!("{DCB_ENTRY} missing, using {DCB_FALLBACK_ENTRY}"),
            ));
            i
        }
    };
    let dcb = read_checked(&archive, &dcb_info, MAX_DCB_BYTES)?;

    // Step 4: free the archive index (about 400 MB) before parsing DataCore.
    drop(archive);

    let datacore = guarded("DataCore parse", || {
        parse_datacore(&dcb).map_err(ExtractError::DataCore)
    })?;

    Ok(Extraction {
        base_ini,
        datacore,
        parsed_entries,
        count_check,
        dcb_entry: dcb_info.name,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guarded_turns_panic_into_error() {
        let r: Result<(), _> = guarded("test stage", || panic!("boom {}", 42));
        match r {
            Err(ExtractError::Panic { stage, message }) => {
                assert_eq!(stage, "test stage");
                assert_eq!(message, "boom 42");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn missing_archive_is_an_error() {
        assert!(matches!(
            extract(Path::new("/nonexistent/Data.p4k")),
            Err(ExtractError::Archive(_))
        ));
    }

    #[test]
    fn entry_names_use_backslashes() {
        for n in [INI_ENTRY, DCB_ENTRY, DCB_FALLBACK_ENTRY] {
            assert!(!n.contains('/'), "{n}");
        }
    }
}

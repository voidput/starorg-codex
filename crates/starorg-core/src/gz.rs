//! gzip for the large files the app keeps on disk: `global.ini` backups and the game-data
//! cache entry (`base.ini`, `gamedata.json`, `keyrefs.json`). The compressed file sits beside
//! the plain name as `<name>.gz`; readers take either, so files written before compression
//! stay readable. Hashes in manifests are always of the uncompressed bytes.

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// The level: 6 (zlib's default) packs `global.ini` to about a fifth in a few hundred ms.
const LEVEL: u32 = 6;
/// Never inflate more than this (a damaged or hostile file cannot fill the memory).
pub const MAX_PLAIN: u64 = 4 * 1024 * 1024 * 1024;

/// `<plain>.gz`
pub fn gz_path(plain: &Path) -> PathBuf {
    let mut s = plain.as_os_str().to_owned();
    s.push(".gz");
    PathBuf::from(s)
}

pub fn compress(bytes: &[u8]) -> io::Result<Vec<u8>> {
    let mut e = GzEncoder::new(Vec::with_capacity(bytes.len() / 4), Compression::new(LEVEL));
    e.write_all(bytes)?;
    e.finish()
}

/// Inflate at most `max` bytes; more is an error, not a truncation.
pub fn decompress(bytes: &[u8], max: u64) -> io::Result<Vec<u8>> {
    let mut out = Vec::with_capacity(bytes.len().saturating_mul(4));
    GzDecoder::new(bytes).take(max + 1).read_to_end(&mut out)?;
    if out.len() as u64 > max {
        return Err(io::Error::other(format!(
            "compressed file inflates past {max} bytes"
        )));
    }
    Ok(out)
}

/// The bytes of `plain`: from `<plain>.gz` when that exists, else the plain file.
pub fn read_either(plain: &Path) -> io::Result<Vec<u8>> {
    let gz = gz_path(plain);
    match std::fs::read(&gz) {
        Ok(b) => decompress(&b, MAX_PLAIN)
            .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", gz.display()))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => std::fs::read(plain),
        Err(e) => Err(e),
    }
}

/// Whether `<plain>.gz` or `plain` is there.
pub fn exists_either(plain: &Path) -> bool {
    gz_path(plain).is_file() || plain.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_either_name() {
        let t = tempfile::tempdir().unwrap();
        let plain = t.path().join("global.ini");
        assert!(!exists_either(&plain));
        std::fs::write(&plain, b"old plain").unwrap();
        assert_eq!(read_either(&plain).unwrap(), b"old plain");
        let text = b"key=value\r\n".repeat(1000);
        std::fs::write(gz_path(&plain), compress(&text).unwrap()).unwrap();
        assert_eq!(read_either(&plain).unwrap(), text, ".gz wins");
        assert!(std::fs::metadata(gz_path(&plain)).unwrap().len() < 200);
        std::fs::remove_file(&plain).unwrap();
        assert!(exists_either(&plain));
    }

    #[test]
    fn limits_and_damage_are_errors() {
        let z = compress(&[0u8; 10_000]).unwrap();
        assert!(decompress(&z, 9_999).is_err());
        assert_eq!(decompress(&z, 10_000).unwrap().len(), 10_000);
        assert!(decompress(b"not gzip", 100).is_err());
    }

    /// Real data, read only: `STARORG_GZ_SAMPLE=<file> cargo test -p starorg-core --lib
    /// gz::tests::real_ratio -- --ignored --nocapture` prints size, ratio and times.
    #[test]
    #[ignore = "needs a real file in STARORG_GZ_SAMPLE"]
    fn real_ratio() {
        let p = std::env::var("STARORG_GZ_SAMPLE").expect("set STARORG_GZ_SAMPLE");
        for p in p.split(',') {
            let b = std::fs::read(p).unwrap();
            let t = std::time::Instant::now();
            let z = compress(&b).unwrap();
            let c = t.elapsed();
            let t = std::time::Instant::now();
            let back = decompress(&z, MAX_PLAIN).unwrap();
            let d = t.elapsed();
            assert_eq!(back, b);
            println!(
                "{p}: {} -> {} bytes ({:.1} %), compress {c:?}, inflate {d:?}",
                b.len(),
                z.len(),
                z.len() as f64 * 100.0 / b.len() as f64
            );
        }
    }
}

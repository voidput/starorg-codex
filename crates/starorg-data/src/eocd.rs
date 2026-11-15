//! Declared entry count of a `Data.p4k` (spec 3.3 step 2).
//!
//! At the pinned StarBreaker commit, `parse_entries` stops without an error when an entry
//! after the first fails to parse (upstream `dd7a6095`, meant for embedded `.pak` archives
//! whose stored count is wrong). A new CIG extra-field tag would therefore hide every later
//! entry. StarBreaker keeps its count and locator code private, so this module reads the three
//! fixed structures at the end of the archive itself and the worker compares the declared
//! count with what StarBreaker parsed:
//!
//! - end of central directory (EOCD, 22 bytes + comment),
//! - ZIP64 end of central directory locator (20 bytes, right before the EOCD),
//! - ZIP64 end of central directory record (56 bytes + extensible data).
//!
//! This duplicates a little of StarBreaker's parser on purpose; it is tested against
//! hand-built byte tails. The newer "p4k v2" container has no such tail; then the count is
//! unknown and the caller falls back to weaker checks plus a warning.

use std::io::{Read, Seek, SeekFrom};

const EOCD_SIG: u32 = 0x0605_4B50;
const LOCATOR_SIG: u32 = 0x0706_4B50;
const EOCD64_SIG: u32 = 0x0606_4B50;
const EOCD_LEN: usize = 22;
const LOCATOR_LEN: usize = 20;
const EOCD64_LEN: usize = 56;
/// Largest tail that can hold the structures: EOCD + max comment + locator + EOCD64.
pub const MAX_TAIL: usize = EOCD_LEN + 0xFFFF + LOCATOR_LEN + EOCD64_LEN;
/// Trailer of a p4k v2 container (as detected by StarBreaker).
const V2_MAGIC: [u8; 6] = [2, 0, 0x4a, 0x69, 0x6a, 0x69];
const V2_FOOTER_LEN: usize = 175;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclaredCount {
    /// Total entries from the EOCD, or from the ZIP64 record when the EOCD defers to it.
    Known(u64),
    /// A p4k v2 container: the count is not known to this reader.
    UnknownV2,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EocdError {
    #[error("no end-of-central-directory record in the last {0} bytes")]
    NotFound(usize),
    #[error("ZIP64 locator missing before the end-of-central-directory record")]
    NoLocator,
    #[error("ZIP64 end-of-central-directory record not found")]
    NoZip64Record,
    #[error("archive declares {declared} entries but {parsed} were parsed")]
    CountMismatch { declared: u64, parsed: u64 },
    #[error("cannot read archive tail: {0}")]
    Io(String),
}

/// Outcome of [`check_entry_count`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountCheck {
    /// Declared and parsed counts are equal.
    Matches(u64),
    /// v2 container: the count could not be checked (warn and use weaker checks).
    Unchecked,
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}
fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().expect("4 bytes"))
}
fn u64_at(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().expect("8 bytes"))
}

/// Read the declared entry count from the last bytes of an archive.
///
/// `tail` is the end of the file and `tail_offset` the absolute file offset of `tail[0]`
/// (used to place the ZIP64 record, whose locator holds an absolute offset).
pub fn declared_entry_count(tail: &[u8], tail_offset: u64) -> Result<DeclaredCount, EocdError> {
    if tail.len() >= V2_FOOTER_LEN && tail.ends_with(&V2_MAGIC) {
        return Ok(DeclaredCount::UnknownV2);
    }
    let eocd = find_eocd(tail).ok_or(EocdError::NotFound(tail.len()))?;
    let entries_on_disk = u16_at(tail, eocd + 8);
    let total16 = u16_at(tail, eocd + 10);
    let cd_size = u32_at(tail, eocd + 12);
    let cd_offset = u32_at(tail, eocd + 16);
    let disk = u16_at(tail, eocd + 4);
    let start_disk = u16_at(tail, eocd + 6);
    let zip64 = disk == 0xFFFF
        || start_disk == 0xFFFF
        || entries_on_disk == 0xFFFF
        || total16 == 0xFFFF
        || cd_size == 0xFFFF_FFFF
        || cd_offset == 0xFFFF_FFFF;
    if !zip64 {
        return Ok(DeclaredCount::Known(u64::from(total16)));
    }

    let loc = eocd.checked_sub(LOCATOR_LEN).ok_or(EocdError::NoLocator)?;
    if u32_at(tail, loc) != LOCATOR_SIG {
        return Err(EocdError::NoLocator);
    }
    let abs = u64_at(tail, loc + 8);
    // Where the locator says the record is, if that is inside the tail and has the signature.
    let from_locator = abs
        .checked_sub(tail_offset)
        .and_then(|r| usize::try_from(r).ok())
        .filter(|&r| r + EOCD64_LEN <= loc && u32_at(tail, r) == EOCD64_SIG);
    // Otherwise the usual place: directly before the locator (no extensible data).
    let record = from_locator
        .or_else(|| {
            loc.checked_sub(EOCD64_LEN)
                .filter(|&r| u32_at(tail, r) == EOCD64_SIG)
        })
        .ok_or(EocdError::NoZip64Record)?;
    Ok(DeclaredCount::Known(u64_at(tail, record + 32)))
}

/// Search backwards for the EOCD signature.
///
/// Real `Data.p4k` files carry trailing bytes after the EOCD and its 16-byte comment (about
/// 1 to 4 KB on LIVE, HOTFIX and PTU 4.10/5.0), so the comment rarely reaches the end of the
/// file. Preference order:
///
/// 1. the last signature whose comment ends exactly at the end of the data (plain ZIP; this
///    also skips a fake signature inside a comment);
/// 2. otherwise the last signature whose comment fits inside the data (CIG's layout).
fn find_eocd(tail: &[u8]) -> Option<usize> {
    if tail.len() < EOCD_LEN {
        return None;
    }
    let last = tail.len() - EOCD_LEN;
    let first = last.saturating_sub(0xFFFF);
    let comment_end = |i: usize| i + EOCD_LEN + usize::from(u16_at(tail, i + 20));
    let mut candidates = (first..=last)
        .rev()
        .filter(|&i| u32_at(tail, i) == EOCD_SIG && comment_end(i) <= tail.len());
    let mut fallback = None;
    for i in candidates.by_ref() {
        if comment_end(i) == tail.len() {
            return Some(i);
        }
        fallback.get_or_insert(i);
    }
    fallback
}

/// Read the tail of an open archive file and return its declared entry count.
pub fn read_declared_entry_count<F: Read + Seek>(file: &mut F) -> Result<DeclaredCount, EocdError> {
    let io = |e: std::io::Error| EocdError::Io(e.to_string());
    let len = file.seek(SeekFrom::End(0)).map_err(io)?;
    let tail_len = len.min(MAX_TAIL as u64);
    let tail_offset = len - tail_len;
    file.seek(SeekFrom::Start(tail_offset)).map_err(io)?;
    let mut tail = vec![0u8; tail_len as usize];
    file.read_exact(&mut tail).map_err(io)?;
    declared_entry_count(&tail, tail_offset)
}

/// Compare the declared count with the number of entries StarBreaker parsed. Any difference
/// fails: fewer parsed means entries were silently dropped.
pub fn check_entry_count(declared: DeclaredCount, parsed: u64) -> Result<CountCheck, EocdError> {
    match declared {
        DeclaredCount::UnknownV2 => Ok(CountCheck::Unchecked),
        DeclaredCount::Known(d) if d == parsed => Ok(CountCheck::Matches(d)),
        DeclaredCount::Known(declared) => Err(EocdError::CountMismatch { declared, parsed }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eocd(total: u16, cd_size: u32, cd_offset: u32, comment: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&EOCD_SIG.to_le_bytes());
        v.extend_from_slice(&0u16.to_le_bytes()); // disk
        v.extend_from_slice(&0u16.to_le_bytes()); // start disk
        v.extend_from_slice(&total.to_le_bytes()); // entries on disk
        v.extend_from_slice(&total.to_le_bytes()); // total entries
        v.extend_from_slice(&cd_size.to_le_bytes());
        v.extend_from_slice(&cd_offset.to_le_bytes());
        v.extend_from_slice(&(comment.len() as u16).to_le_bytes());
        v.extend_from_slice(comment);
        v
    }

    fn eocd64(total: u64) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&EOCD64_SIG.to_le_bytes());
        v.extend_from_slice(&44u64.to_le_bytes()); // size of remaining record
        v.extend_from_slice(&45u16.to_le_bytes());
        v.extend_from_slice(&45u16.to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes());
        v.extend_from_slice(&total.to_le_bytes()); // entries on disk
        v.extend_from_slice(&total.to_le_bytes()); // total entries
        v.extend_from_slice(&123u64.to_le_bytes());
        v.extend_from_slice(&456u64.to_le_bytes());
        assert_eq!(v.len(), EOCD64_LEN);
        v
    }

    fn locator(abs: u64) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&LOCATOR_SIG.to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes());
        v.extend_from_slice(&abs.to_le_bytes());
        v.extend_from_slice(&1u32.to_le_bytes());
        v
    }

    /// Junk before the structures, as in a real tail that starts mid-directory.
    fn junk(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i * 7 + 3) as u8).collect()
    }

    fn zip64_tail(
        total: u64,
        junk_len: usize,
        locator_abs: Option<u64>,
        tail_offset: u64,
    ) -> Vec<u8> {
        let mut t = junk(junk_len);
        let rec_abs = tail_offset + t.len() as u64;
        t.extend(eocd64(total));
        t.extend(locator(locator_abs.unwrap_or(rec_abs)));
        t.extend(eocd(0xFFFF, 0xFFFF_FFFF, 0xFFFF_FFFF, b""));
        t
    }

    #[test]
    fn plain_eocd() {
        let mut t = junk(100);
        t.extend(eocd(1234, 10, 20, b""));
        assert_eq!(declared_entry_count(&t, 0), Ok(DeclaredCount::Known(1234)));
    }

    #[test]
    fn plain_eocd_with_comment_containing_a_fake_signature() {
        let mut comment = EOCD_SIG.to_le_bytes().to_vec();
        comment.extend_from_slice(&[0u8; 30]);
        let mut t = junk(10);
        t.extend(eocd(7, 10, 20, &comment));
        assert_eq!(declared_entry_count(&t, 0), Ok(DeclaredCount::Known(7)));
    }

    #[test]
    fn eocd_followed_by_trailing_bytes() {
        // Layout of the real archives: ZIP64 record, locator, EOCD with a 16-byte comment,
        // then a few KB of trailing data.
        let off = 159_674_367_903;
        let mut t = junk(64);
        let rec_abs = off + t.len() as u64;
        t.extend(eocd64(1_399_137));
        t.extend(locator(rec_abs));
        t.extend(eocd(0xFFFF, 0xFFFF_FFFF, 0xFFFF_FFFF, &[0xAB; 16]));
        t.extend(junk(3935));
        assert_eq!(
            declared_entry_count(&t, off),
            Ok(DeclaredCount::Known(1_399_137))
        );
    }

    #[test]
    fn comment_longer_than_data_is_not_an_eocd() {
        let mut t = junk(10);
        let mut e = eocd(3, 0, 0, b"");
        e[20..22].copy_from_slice(&500u16.to_le_bytes());
        t.extend(e);
        assert_eq!(declared_entry_count(&t, 0), Err(EocdError::NotFound(32)));
    }

    #[test]
    fn zip64_record_via_locator() {
        let off = 150_000_000_000;
        let t = zip64_tail(1_234_567, 64, None, off);
        assert_eq!(
            declared_entry_count(&t, off),
            Ok(DeclaredCount::Known(1_234_567))
        );
    }

    #[test]
    fn zip64_record_with_stale_locator_offset() {
        let t = zip64_tail(99, 64, Some(5), 1000);
        assert_eq!(declared_entry_count(&t, 1000), Ok(DeclaredCount::Known(99)));
    }

    #[test]
    fn zip64_without_locator_is_an_error() {
        let mut t = junk(64);
        t.extend(eocd(0xFFFF, 0xFFFF_FFFF, 0xFFFF_FFFF, b""));
        assert_eq!(declared_entry_count(&t, 0), Err(EocdError::NoLocator));
        let mut t = eocd(0xFFFF, 0, 0, b"");
        t.truncate(EOCD_LEN);
        assert_eq!(declared_entry_count(&t, 0), Err(EocdError::NoLocator));
    }

    #[test]
    fn zip64_without_record_is_an_error() {
        let mut t = junk(64);
        t.extend(locator(0));
        t.extend(eocd(0xFFFF, 0xFFFF_FFFF, 0xFFFF_FFFF, b""));
        assert_eq!(declared_entry_count(&t, 0), Err(EocdError::NoZip64Record));
    }

    #[test]
    fn missing_eocd() {
        assert_eq!(
            declared_entry_count(&junk(500), 0),
            Err(EocdError::NotFound(500))
        );
        assert_eq!(declared_entry_count(&[], 0), Err(EocdError::NotFound(0)));
    }

    #[test]
    fn v2_container_is_unknown() {
        let mut t = junk(V2_FOOTER_LEN);
        t.extend_from_slice(&V2_MAGIC);
        assert_eq!(declared_entry_count(&t, 0), Ok(DeclaredCount::UnknownV2));
    }

    #[test]
    fn count_check() {
        assert_eq!(
            check_entry_count(DeclaredCount::Known(10), 10),
            Ok(CountCheck::Matches(10))
        );
        // Injected lower parsed count: StarBreaker stopped early. Must fail.
        assert_eq!(
            check_entry_count(DeclaredCount::Known(1_234_567), 1_234_000),
            Err(EocdError::CountMismatch {
                declared: 1_234_567,
                parsed: 1_234_000
            })
        );
        assert!(check_entry_count(DeclaredCount::Known(5), 6).is_err());
        assert_eq!(
            check_entry_count(DeclaredCount::UnknownV2, 3),
            Ok(CountCheck::Unchecked)
        );
    }

    #[test]
    fn reads_from_a_file() {
        let off_junk = 200_000; // longer than MAX_TAIL, so only the tail is read
        let mut bytes = junk(off_junk);
        let rec_abs = bytes.len() as u64;
        bytes.extend(eocd64(42));
        bytes.extend(locator(rec_abs));
        bytes.extend(eocd(0xFFFF, 0xFFFF_FFFF, 0xFFFF_FFFF, b""));
        let mut cur = std::io::Cursor::new(bytes);
        assert_eq!(
            read_declared_entry_count(&mut cur),
            Ok(DeclaredCount::Known(42))
        );
    }
}

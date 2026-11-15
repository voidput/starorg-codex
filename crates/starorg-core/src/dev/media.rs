//! What the explorer knows about models and sounds without the asset worker (addendum
//! 7.17): hashes of the archive's geometry file names and ATL trigger names, read by the
//! dev worker while the archive is open, so "has model" / "has sounds" are checked against
//! the files that exist instead of guessed from a field being set.

use std::collections::HashSet;
use std::hash::Hasher;

/// A stable 64-bit hash of a name (FNV-1a over the bytes), the same in every process.
pub fn hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Hashes are already uniform; the set uses them as they are.
#[derive(Default, Clone, Copy)]
pub struct IdHasher(u64);

impl Hasher for IdHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 = (self.0 << 8) | u64::from(*b);
        }
    }
    fn write_u64(&mut self, n: u64) {
        self.0 = n;
    }
}

pub type HashBuild = std::hash::BuildHasherDefault<IdHasher>;
pub type HashedSet = HashSet<u64, HashBuild>;

/// Geometry files (normalised paths) and ATL trigger names, as hashes.
#[derive(Debug, Default, Clone)]
pub struct MediaNames {
    pub geometry: HashedSet,
    pub triggers: HashedSet,
}

impl MediaNames {
    pub fn has_geometry(&self, norm_path: &str) -> bool {
        self.geometry.contains(&hash(norm_path))
    }
    pub fn has_trigger(&self, name: &str) -> bool {
        self.triggers.contains(&hash(name))
    }
    /// Bytes held by the two sets, roughly.
    pub fn bytes(&self) -> usize {
        (self.geometry.capacity() + self.triggers.capacity()) * 9
    }
}

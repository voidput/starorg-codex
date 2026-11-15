//! Build identity from `<channel>/build_manifest.id` (spec 4.1).

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;

/// Game version taken from `Data.Branch`: the first `major.minor[.patch]` in the string.
/// Compared as three numbers; a missing patch is 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GameVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl fmt::Display for GameVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl GameVersion {
    /// Find the first `N.N` or `N.N.N` in `s`. `sc-alpha-4.10.0-hotfix` gives 4.10.0.
    pub fn from_branch(s: &str) -> Option<Self> {
        let b = s.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if b[i].is_ascii_digit() && (i == 0 || !b[i - 1].is_ascii_digit()) {
                let mut nums = Vec::with_capacity(3);
                let mut j = i;
                loop {
                    let start = j;
                    while j < b.len() && b[j].is_ascii_digit() {
                        j += 1;
                    }
                    if start == j {
                        break;
                    }
                    match s[start..j].parse::<u32>() {
                        Ok(n) => nums.push(n),
                        Err(_) => break,
                    }
                    if nums.len() == 3 || j >= b.len() || b[j] != b'.' {
                        break;
                    }
                    if !b.get(j + 1).is_some_and(u8::is_ascii_digit) {
                        break;
                    }
                    j += 1;
                }
                if nums.len() >= 2 {
                    return Some(Self {
                        major: nums[0],
                        minor: nums[1],
                        patch: nums.get(2).copied().unwrap_or(0),
                    });
                }
                i = j.max(i + 1);
            } else {
                i += 1;
            }
        }
        None
    }
}

/// What we use of a channel's build manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildInfo {
    /// `Data.Branch`, e.g. `sc-alpha-4.10.0-hotfix`.
    pub branch: String,
    /// From `branch`. `None` means unknown: build ranges are not enforced.
    pub game_version: Option<GameVersion>,
    /// `Data.RequestedP4ChangeNum`: one counter across all branches; identifies the build.
    pub change: Option<u64>,
    /// `Data.Version`, display only. Numbered per branch; never compared.
    pub version: String,
}

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("build manifest is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("build manifest has no Data.Branch")]
    NoBranch,
}

#[derive(Deserialize)]
struct Manifest {
    #[serde(rename = "Data")]
    data: ManifestData,
}

#[derive(Deserialize)]
struct ManifestData {
    #[serde(rename = "Branch")]
    branch: Option<String>,
    #[serde(rename = "RequestedP4ChangeNum")]
    change: Option<serde_json::Value>,
    #[serde(rename = "Version")]
    version: Option<String>,
}

impl BuildInfo {
    pub fn from_json(bytes: &[u8]) -> Result<Self, BuildError> {
        let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
        let m: Manifest = serde_json::from_slice(bytes)?;
        let branch = m
            .data
            .branch
            .filter(|b| !b.is_empty())
            .ok_or(BuildError::NoBranch)?;
        let change = match m.data.change {
            Some(serde_json::Value::String(s)) => s.trim().parse().ok(),
            Some(serde_json::Value::Number(n)) => n.as_u64(),
            _ => None,
        }
        .filter(|&c| c != 0);
        Ok(Self {
            game_version: GameVersion::from_branch(&branch),
            branch,
            change,
            version: m.data.version.unwrap_or_default(),
        })
    }

    pub fn read(path: &Path) -> Result<Self, BuildError> {
        let bytes = std::fs::read(path).map_err(|source| BuildError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Self::from_json(&bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(major: u32, minor: u32, patch: u32) -> Option<GameVersion> {
        Some(GameVersion {
            major,
            minor,
            patch,
        })
    }

    #[test]
    fn branch_to_game_version() {
        assert_eq!(GameVersion::from_branch("sc-alpha-4.10.0"), v(4, 10, 0));
        assert_eq!(
            GameVersion::from_branch("sc-alpha-4.10.0-hotfix"),
            v(4, 10, 0)
        );
        assert_eq!(GameVersion::from_branch("sc-alpha-5.0.0"), v(5, 0, 0));
        assert_eq!(GameVersion::from_branch("sc-alpha-4.2"), v(4, 2, 0));
        assert_eq!(GameVersion::from_branch("sc-alpha-4.2-ptu"), v(4, 2, 0));
        assert_eq!(GameVersion::from_branch("4.10.3.1"), v(4, 10, 3));
        assert_eq!(GameVersion::from_branch("sc-alpha-4.10."), v(4, 10, 0));
        assert_eq!(GameVersion::from_branch("sc-alpha-4"), None);
        assert_eq!(GameVersion::from_branch("sc-main"), None);
        assert_eq!(GameVersion::from_branch(""), None);
        assert!(v(4, 10, 0) < v(4, 10, 1) && v(4, 10, 1) < v(4, 11, 0));
        assert!(v(4, 9, 0) < v(4, 10, 0), "compared as numbers, not text");
    }

    #[test]
    fn live_and_hotfix_manifests() {
        let live = br#"{ "Data": { "Branch": "sc-alpha-4.10.0", "RequestedP4ChangeNum": "13165946",
            "Shelved_Change": "", "Version": "4.10.200.58746" } }"#;
        let b = BuildInfo::from_json(live).unwrap();
        assert_eq!(b.game_version, v(4, 10, 0));
        assert_eq!(b.change, Some(13165946));
        assert_eq!(b.version, "4.10.200.58746");

        let hotfix = br#"{ "Data": { "Branch": "sc-alpha-4.10.0-hotfix",
            "RequestedP4ChangeNum": "12572603", "Version": "1.0.191.55227" } }"#;
        let b = BuildInfo::from_json(hotfix).unwrap();
        assert_eq!(
            b.game_version,
            v(4, 10, 0),
            "game version from Branch, not Version"
        );
        assert_eq!(b.version, "1.0.191.55227");
    }

    #[test]
    fn odd_manifests() {
        let b = BuildInfo::from_json(br#"{"Data":{"Branch":"sc-main","RequestedP4ChangeNum":0}}"#)
            .unwrap();
        assert_eq!(b.game_version, None);
        assert_eq!(b.change, None);
        assert!(matches!(
            BuildInfo::from_json(br#"{"Data":{}}"#),
            Err(BuildError::NoBranch)
        ));
        assert!(BuildInfo::from_json(b"not json").is_err());
        let with_bom = b"\xEF\xBB\xBF{\"Data\":{\"Branch\":\"sc-alpha-4.1.1\"}}";
        assert_eq!(
            BuildInfo::from_json(with_bom).unwrap().game_version,
            v(4, 1, 1)
        );
    }
}

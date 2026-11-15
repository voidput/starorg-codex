//! Backups of `global.ini` (spec 4.6). `user.cfg` is never backed up as a whole file.
//!
//! A backup is `backups/<YYYYmmdd-HHMMSS>-<reason>/` with `manifest.json` and, if the file
//! existed, `global.ini`. `existed: false` is a real state: a clean game has no loose file.
//!
//! Retention: the pinned `pre-existing` one; the newest 10; and the newest backup made under
//! each of the last 3 game builds.

use crate::build::BuildInfo;
use crate::fsutil;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub const KEEP_NEWEST: usize = 10;
pub const KEEP_BUILDS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    PreExisting,
    BeforeApply,
    BeforeRestore,
    Manual,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Reason::PreExisting => "pre-existing",
            Reason::BeforeApply => "before-apply",
            Reason::BeforeRestore => "before-restore",
            Reason::Manual => "manual",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileOrigin {
    Ours,
    Foreign,
    None,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildRecord {
    pub branch: Option<String>,
    pub game_version: Option<String>,
    pub change: Option<u64>,
    pub version: Option<String>,
}

impl From<Option<&BuildInfo>> for BuildRecord {
    fn from(b: Option<&BuildInfo>) -> Self {
        match b {
            None => Self::default(),
            Some(b) => Self {
                branch: Some(b.branch.clone()),
                game_version: b.game_version.map(|v| v.to_string()),
                change: b.change,
                version: Some(b.version.clone()),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRecord {
    pub existed: bool,
    pub on_disk_name: Option<String>,
    pub real_path: PathBuf,
    pub size: Option<u64>,
    pub sha256: Option<String>,
    pub origin: FileOrigin,
    pub foreign_tool: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub created: String,
    pub created_unix: u64,
    /// Order among backups of the channel (several can share a second).
    #[serde(default)]
    pub seq: u64,
    pub reason: Reason,
    pub app_version: String,
    pub build: BuildRecord,
    pub file: FileRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Backup {
    pub id: String,
    pub manifest: Manifest,
    #[serde(skip)]
    pub dir: PathBuf,
}

impl Backup {
    pub fn pinned(&self) -> bool {
        self.manifest.reason == Reason::PreExisting
    }
    pub fn ini_path(&self) -> PathBuf {
        self.dir.join("global.ini")
    }
}

pub fn backups_dir(channel_data: &Path) -> PathBuf {
    channel_data.join("backups")
}

/// All backups of a channel, oldest first.
pub fn list(channel_data: &Path) -> Vec<Backup> {
    let dir = backups_dir(channel_data);
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<Backup> = rd
        .filter_map(Result::ok)
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| {
            let m: Manifest = fsutil::read_json(&e.path().join("manifest.json")).ok()??;
            Some(Backup {
                id: e.file_name().to_string_lossy().into_owned(),
                manifest: m,
                dir: e.path(),
            })
        })
        .collect();
    out.sort_by(|a, b| {
        (a.manifest.seq, a.manifest.created_unix, &a.id).cmp(&(
            b.manifest.seq,
            b.manifest.created_unix,
            &b.id,
        ))
    });
    out
}

pub fn find(channel_data: &Path, id: &str) -> Option<Backup> {
    list(channel_data).into_iter().find(|b| b.id == id)
}

/// What the current file is, for the manifest.
pub struct Current<'a> {
    /// Real path of the loose file (after links), whether it exists or not.
    pub real_path: &'a Path,
    /// The file's bytes, when it exists.
    pub bytes: Option<&'a [u8]>,
    pub origin: FileOrigin,
    pub foreign_tool: Option<String>,
}

/// Make a backup. Returns `None` when skipped because the file equals the newest backup
/// (never for `pre-existing`).
pub fn create(
    channel_data: &Path,
    reason: Reason,
    app_version: &str,
    build: Option<&BuildInfo>,
    cur: &Current<'_>,
) -> std::io::Result<Option<Backup>> {
    let sha = cur.bytes.map(fsutil::sha256_hex);
    let existing = list(channel_data);
    if reason != Reason::PreExisting
        && let Some(newest) = existing.last()
        && newest.manifest.file.sha256 == sha
    {
        return Ok(None);
    }
    let seq = existing.iter().map(|b| b.manifest.seq).max().unwrap_or(0) + 1;
    let now = fsutil::unix_now();
    let base = format!("{}-{}", fsutil::stamp(now), reason.as_str());
    let dir = backups_dir(channel_data);
    std::fs::create_dir_all(&dir)?;
    let mut id = base.clone();
    let mut n = 2;
    while dir.join(&id).exists() {
        id = format!("{base}-{n}");
        n += 1;
    }
    let manifest = Manifest {
        created: fsutil::rfc3339(now),
        created_unix: now,
        seq,
        reason,
        app_version: app_version.to_string(),
        build: build.into(),
        file: FileRecord {
            existed: cur.bytes.is_some(),
            on_disk_name: cur
                .bytes
                .and(cur.real_path.file_name())
                .map(|n| n.to_string_lossy().into_owned()),
            real_path: cur.real_path.to_path_buf(),
            size: cur.bytes.map(|b| b.len() as u64),
            sha256: sha,
            origin: if cur.bytes.is_some() {
                cur.origin
            } else {
                FileOrigin::None
            },
            foreign_tool: cur.foreign_tool.clone(),
        },
    };
    let tmp = dir.join(format!(".{id}.tmp-{}", std::process::id()));
    let result = (|| {
        std::fs::create_dir_all(&tmp)?;
        if let Some(b) = cur.bytes {
            let mut f = std::fs::File::create(tmp.join("global.ini"))?;
            std::io::Write::write_all(&mut f, b)?;
            f.sync_all()?;
        }
        let mut f = std::fs::File::create(tmp.join("manifest.json"))?;
        std::io::Write::write_all(
            &mut f,
            &serde_json::to_vec_pretty(&manifest).map_err(std::io::Error::other)?,
        )?;
        f.sync_all()?;
        drop(f);
        fsutil::fsync_dir(&tmp)?;
        fsutil::rename_retry(&tmp, &dir.join(&id))?;
        fsutil::fsync_dir(&dir)
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e);
    }
    Ok(Some(Backup {
        dir: dir.join(&id),
        id,
        manifest,
    }))
}

/// Ids retention keeps (spec 4.6).
pub fn keep_set(all: &[Backup]) -> BTreeSet<String> {
    let mut keep = BTreeSet::new();
    for b in all.iter().filter(|b| b.pinned()) {
        keep.insert(b.id.clone());
    }
    for b in all.iter().rev().take(KEEP_NEWEST) {
        keep.insert(b.id.clone());
    }
    let mut builds: Vec<Option<u64>> = Vec::new();
    for b in all.iter().rev() {
        let c = b.manifest.build.change;
        if builds.contains(&c) {
            continue;
        }
        if builds.len() == KEEP_BUILDS {
            break;
        }
        builds.push(c);
        keep.insert(b.id.clone());
    }
    keep
}

/// Delete backups outside the retention set. Returns the ids removed.
pub fn prune(channel_data: &Path) -> std::io::Result<Vec<String>> {
    let all = list(channel_data);
    let keep = keep_set(&all);
    let mut removed = Vec::new();
    for b in all {
        if !keep.contains(&b.id) {
            std::fs::remove_dir_all(&b.dir)?;
            removed.push(b.id);
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake(id: &str, t: u64, reason: Reason, change: u64) -> Backup {
        Backup {
            id: id.into(),
            dir: PathBuf::new(),
            manifest: Manifest {
                created: String::new(),
                created_unix: t,
                seq: t,
                reason,
                app_version: String::new(),
                build: BuildRecord {
                    change: Some(change),
                    ..Default::default()
                },
                file: FileRecord {
                    existed: false,
                    on_disk_name: None,
                    real_path: PathBuf::new(),
                    size: None,
                    sha256: None,
                    origin: FileOrigin::None,
                    foreign_tool: None,
                },
            },
        }
    }

    #[test]
    fn retention_keeps_pinned_newest_10_and_last_3_builds() {
        let mut all = vec![fake("pin", 0, Reason::PreExisting, 1)];
        all.push(fake("b2", 1, Reason::BeforeApply, 2));
        all.push(fake("b3-old", 2, Reason::BeforeApply, 3));
        all.push(fake("b3", 3, Reason::BeforeApply, 3));
        for i in 0..20 {
            all.push(fake(&format!("n{i:02}"), 10 + i, Reason::BeforeApply, 4));
        }
        let keep = keep_set(&all);
        assert!(keep.contains("pin"));
        assert!(keep.contains("b3"), "newest under build 3");
        assert!(keep.contains("b2"), "newest under build 2");
        assert!(!keep.contains("b3-old"));
        assert!(keep.contains("n19") && keep.contains("n10") && !keep.contains("n09"));
        assert_eq!(keep.len(), 1 + 2 + 10);
    }

    #[test]
    fn create_skip_same_hash_and_missing_file() {
        let t = tempfile::tempdir().unwrap();
        let real = t.path().join("global.ini");
        let cur = Current {
            real_path: &real,
            bytes: None,
            origin: FileOrigin::None,
            foreign_tool: None,
        };
        let pre = create(t.path(), Reason::PreExisting, "0", None, &cur)
            .unwrap()
            .unwrap();
        assert!(!pre.manifest.file.existed);
        assert!(!pre.ini_path().exists());
        let cur = Current {
            real_path: &real,
            bytes: Some(b"abc"),
            origin: FileOrigin::Foreign,
            foreign_tool: Some("Smart Citizen".into()),
        };
        let b = create(t.path(), Reason::BeforeApply, "0", None, &cur)
            .unwrap()
            .unwrap();
        assert_eq!(std::fs::read(b.ini_path()).unwrap(), b"abc");
        assert!(
            create(t.path(), Reason::BeforeApply, "0", None, &cur)
                .unwrap()
                .is_none()
        );
        let l = list(t.path());
        assert_eq!(l.len(), 2);
        assert!(l[0].pinned());
        assert_eq!(l[1].manifest.file.origin, FileOrigin::Foreign);
    }
}

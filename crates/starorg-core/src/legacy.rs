//! One-time move of the pre-0.3.0 app folders (`starorg-strings`, `StarOrg Tools\Strings`,
//! the `tools.starorg.strings` webview storage) to their StarOrg Codex names.
//!
//! The app and the CLI both call [`migrate_from_env`] when they start (workers never do).
//! For each folder of [`Layout::pairs`]:
//!
//! - only the old one exists: rename it. When that fails because the two are on different
//!   file systems, copy it to a temporary sibling, check every file against the original,
//!   then rename the copy into place and keep the old folder. Any other failure leaves both
//!   alone and the old folder stays in use ([`crate::appdirs::LegacyPair::current`]).
//! - both exist: the old one is never touched, only reported ([`Event::BothExist`]).
//! - neither, or only the new one: nothing to do.
//!
//! The work runs under an OS lock beside the folders, so two first starts at the same time
//! (app and a launch hook) move each folder once; the second one finds the new folder and
//! does nothing. Data is never deleted: the old folder goes away only by being renamed.

use crate::appdirs::{Layout, LegacyPair};
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

/// What happened to one folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Renamed to the new name.
    Moved { old: PathBuf, new: PathBuf },
    /// On another file system: copied and checked; the old folder is kept.
    Copied { old: PathBuf, new: PathBuf },
    /// Both exist: the old one was left alone.
    BothExist { old: PathBuf, new: PathBuf },
    /// Not moved; the old folder stays in use.
    Failed {
        old: PathBuf,
        new: PathBuf,
        error: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Warn,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub events: Vec<Event>,
}

impl Report {
    /// Something was moved or copied in this run.
    pub fn migrated(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, Event::Moved { .. } | Event::Copied { .. }))
    }

    /// At most one line per kind of outcome, for Activity or stderr.
    pub fn lines(&self) -> Vec<(Level, String)> {
        let mut out = Vec::new();
        let moved: Vec<String> = self
            .events
            .iter()
            .filter_map(|e| match e {
                Event::Moved { old, new } => {
                    Some(format!("{} -> {}", old.display(), new.display()))
                }
                Event::Copied { old, new } => Some(format!(
                    "{} -> {} (copied: another drive; the old folder was kept)",
                    old.display(),
                    new.display()
                )),
                _ => None,
            })
            .collect();
        if !moved.is_empty() {
            out.push((
                Level::Info,
                format!(
                    "App folders moved to the StarOrg Codex names: {}",
                    moved.join("; ")
                ),
            ));
        }
        let both: Vec<String> = self
            .events
            .iter()
            .filter_map(|e| match e {
                Event::BothExist { old, .. } => Some(old.display().to_string()),
                _ => None,
            })
            .collect();
        if !both.is_empty() {
            out.push((
                Level::Info,
                format!(
                    "Old StarOrg Strings folders left alone because the new ones already exist (delete them when you no longer need them): {}",
                    both.join("; ")
                ),
            ));
        }
        let failed: Vec<String> = self
            .events
            .iter()
            .filter_map(|e| match e {
                Event::Failed { old, new, error } => {
                    Some(format!("{} -> {}: {error}", old.display(), new.display()))
                }
                _ => None,
            })
            .collect();
        if !failed.is_empty() {
            out.push((
                Level::Warn,
                format!(
                    "App folders could not be moved to the StarOrg Codex names; the old ones stay in use: {}",
                    failed.join("; ")
                ),
            ));
        }
        out
    }
}

/// Move the folders of the process environment's [`Layout`].
pub fn migrate_from_env() -> Report {
    match Layout::from_env() {
        Some(l) => migrate(&l.pairs, &l.lock),
        None => Report::default(),
    }
}

/// Move `pairs` under the lock file `lock`.
pub fn migrate(pairs: &[LegacyPair], lock: &Path) -> Report {
    migrate_with(pairs, lock, &crate::fsutil::rename_retry)
}

/// The old folder still waits for a move.
fn pending(p: &LegacyPair) -> bool {
    exists(&p.old) && !exists(&p.new)
}

/// Exists, or is a link (a dangling link is still something we must not overwrite).
fn exists(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok()
}

/// [`migrate`] with the first rename (old -> new) swapped, so tests can fail it.
pub fn migrate_with(
    pairs: &[LegacyPair],
    lock: &Path,
    rename: &dyn Fn(&Path, &Path) -> std::io::Result<()>,
) -> Report {
    let mut report = Report::default();
    // The common case after the first start: nothing to move, no lock, no writes.
    if !pairs.iter().any(pending) {
        report.events = pairs
            .iter()
            .filter(|p| exists(&p.old) && exists(&p.new))
            .map(|p| Event::BothExist {
                old: p.old.clone(),
                new: p.new.clone(),
            })
            .collect();
        return report;
    }
    let held = take_lock(lock);
    let file = match held {
        Ok(f) => f,
        Err(e) => {
            report.events = pairs
                .iter()
                .filter(|p| pending(p))
                .map(|p| Event::Failed {
                    old: p.old.clone(),
                    new: p.new.clone(),
                    error: format!("cannot take the lock {}: {e}", lock.display()),
                })
                .collect();
            return report;
        }
    };
    // Decide again under the lock: another process may have moved them meanwhile.
    for p in pairs {
        if !exists(&p.old) {
            continue;
        }
        if exists(&p.new) {
            report.events.push(Event::BothExist {
                old: p.old.clone(),
                new: p.new.clone(),
            });
            continue;
        }
        report.events.push(move_one(p, rename));
    }
    drop(file);
    if !report
        .events
        .iter()
        .any(|e| matches!(e, Event::Failed { .. }))
    {
        // Done for good: every later start finds the new folders and takes no lock. A
        // process still waiting on the old file finds the new folders too.
        let _ = std::fs::remove_file(lock);
    }
    report
}

fn take_lock(path: &Path) -> std::io::Result<std::fs::File> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let f = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    // Blocks until a concurrent first start has finished its moves.
    f.lock()?;
    Ok(f)
}

fn move_one(p: &LegacyPair, rename: &dyn Fn(&Path, &Path) -> std::io::Result<()>) -> Event {
    let failed = |error: String| Event::Failed {
        old: p.old.clone(),
        new: p.new.clone(),
        error,
    };
    let Some(parent) = p.new.parent() else {
        return failed("no parent folder".into());
    };
    if let Err(e) = std::fs::create_dir_all(parent) {
        return failed(format!("cannot create {}: {e}", parent.display()));
    }
    clean_partials(&p.new);
    match rename(&p.old, &p.new) {
        Ok(()) => {
            let _ = crate::fsutil::fsync_dir(parent);
            Event::Moved {
                old: p.old.clone(),
                new: p.new.clone(),
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => match copy_verified(p) {
            Ok(()) => Event::Copied {
                old: p.old.clone(),
                new: p.new.clone(),
            },
            Err(c) => failed(format!("rename: {e}; copy: {c}")),
        },
        Err(e) => failed(format!("rename: {e}")),
    }
}

/// `<new>.migrating-<pid>`: a copy in progress.
fn partial(new: &Path) -> PathBuf {
    let name = new
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    new.with_file_name(format!("{name}.migrating-{}", std::process::id()))
}

/// Remove copies a crashed run left behind (only our own `<new>.migrating-*` folders).
fn clean_partials(new: &Path) {
    let (Some(parent), Some(name)) = (new.parent(), new.file_name()) else {
        return;
    };
    let prefix = format!("{}.migrating-", name.to_string_lossy());
    let Ok(rd) = std::fs::read_dir(parent) else {
        return;
    };
    for e in rd.flatten() {
        if e.file_name().to_string_lossy().starts_with(&prefix) {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

fn copy_verified(p: &LegacyPair) -> Result<(), String> {
    let tmp = partial(&p.new);
    let r = copy_tree(&p.old, &tmp)
        .and_then(|()| verify_tree(&p.old, &tmp))
        .and_then(|()| std::fs::rename(&tmp, &p.new).map_err(|e| format!("rename copy: {e}")));
    if r.is_err() {
        let _ = std::fs::remove_dir_all(&tmp);
    }
    r
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    let m = std::fs::symlink_metadata(from).map_err(|e| format!("{}: {e}", from.display()))?;
    let ft = m.file_type();
    let err = |e: std::io::Error| format!("{}: {e}", from.display());
    if ft.is_symlink() {
        copy_link(from, to)
    } else if ft.is_dir() {
        std::fs::create_dir(to).map_err(|e| format!("{}: {e}", to.display()))?;
        for e in std::fs::read_dir(from).map_err(err)? {
            let e = e.map_err(err)?;
            copy_tree(&e.path(), &to.join(e.file_name()))?;
        }
        let _ = std::fs::set_permissions(to, m.permissions());
        Ok(())
    } else if ft.is_file() {
        std::fs::copy(from, to).map(|_| ()).map_err(err)
    } else {
        Err(format!("{}: not a file or folder", from.display()))
    }
}

#[cfg(unix)]
fn copy_link(from: &Path, to: &Path) -> Result<(), String> {
    let target = std::fs::read_link(from).map_err(|e| format!("{}: {e}", from.display()))?;
    std::os::unix::fs::symlink(target, to).map_err(|e| format!("{}: {e}", to.display()))
}

#[cfg(not(unix))]
fn copy_link(from: &Path, _to: &Path) -> Result<(), String> {
    Err(format!("{}: links are not copied", from.display()))
}

/// Every entry of `a` is in `b` with the same type, size and SHA-256 (links: same target),
/// and `b` holds nothing else.
fn verify_tree(a: &Path, b: &Path) -> Result<(), String> {
    let ma = std::fs::symlink_metadata(a).map_err(|e| format!("{}: {e}", a.display()))?;
    let mb = std::fs::symlink_metadata(b).map_err(|e| format!("{}: {e}", b.display()))?;
    let differ = || format!("copy of {} differs from the original", a.display());
    let (ta, tb) = (ma.file_type(), mb.file_type());
    if ta.is_symlink() || tb.is_symlink() {
        let (la, lb) = (std::fs::read_link(a).ok(), std::fs::read_link(b).ok());
        return if ta.is_symlink() && tb.is_symlink() && la == lb {
            Ok(())
        } else {
            Err(differ())
        };
    }
    if ta.is_dir() && tb.is_dir() {
        let names = |d: &Path| -> Result<Vec<std::ffi::OsString>, String> {
            let mut v: Vec<_> = std::fs::read_dir(d)
                .map_err(|e| format!("{}: {e}", d.display()))?
                .map(|e| e.map(|e| e.file_name()))
                .collect::<Result<_, _>>()
                .map_err(|e| format!("{}: {e}", d.display()))?;
            v.sort();
            Ok(v)
        };
        let na = names(a)?;
        if na != names(b)? {
            return Err(differ());
        }
        for n in na {
            verify_tree(&a.join(&n), &b.join(&n))?;
        }
        return Ok(());
    }
    if ta.is_file() && tb.is_file() && ma.len() == mb.len() {
        let h =
            |p: &Path| crate::fsutil::sha256_file(p).map_err(|e| format!("{}: {e}", p.display()));
        return if h(a)? == h(b)? {
            Ok(())
        } else {
            Err(differ())
        };
    }
    Err(differ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::appdirs::{AppDirs, Layout};
    use std::ffi::OsString;

    fn tree(root: &Path) {
        std::fs::create_dir_all(root.join("channels/abc/LIVE/backups")).unwrap();
        std::fs::write(root.join("config.toml"), "schema = 1\n").unwrap();
        std::fs::write(root.join("channels/abc/LIVE/profile.toml"), "x = 1\n").unwrap();
        std::fs::write(
            root.join("channels/abc/LIVE/backups/global.ini"),
            vec![7u8; 70_000],
        )
        .unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("config.toml", root.join("link")).unwrap();
    }

    fn pair(t: &Path, name: &str) -> LegacyPair {
        LegacyPair {
            old: t.join(format!("{name}-old")),
            new: t.join(format!("{name}-new")),
        }
    }

    fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
        let mut out = Vec::new();
        fn walk(base: &Path, d: &Path, out: &mut Vec<(String, Vec<u8>)>) {
            let mut es: Vec<_> = std::fs::read_dir(d).unwrap().flatten().collect();
            es.sort_by_key(|e| e.file_name());
            for e in es {
                let p = e.path();
                let rel = p.strip_prefix(base).unwrap().display().to_string();
                let ft = std::fs::symlink_metadata(&p).unwrap().file_type();
                if ft.is_symlink() {
                    let l = std::fs::read_link(&p).unwrap();
                    out.push((rel, format!("-> {}", l.display()).into_bytes()));
                } else if ft.is_dir() {
                    out.push((rel, b"<dir>".to_vec()));
                    walk(base, &p, out);
                } else {
                    out.push((rel, std::fs::read(&p).unwrap()));
                }
            }
        }
        walk(root, root, &mut out);
        out
    }

    fn leftovers(t: &Path) -> Vec<String> {
        std::fs::read_dir(t)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".migrating-"))
            .collect()
    }

    #[test]
    fn moves_every_old_folder_once() {
        let t = tempfile::tempdir().unwrap();
        let (a, b, c) = (
            pair(t.path(), "a"),
            pair(t.path(), "b"),
            pair(t.path(), "c"),
        );
        tree(&a.old);
        tree(&b.old);
        let want = snapshot(&a.old);
        let lock = t.path().join("m.lock");
        let r = migrate(&[a.clone(), b.clone(), c.clone()], &lock);
        assert_eq!(r.events.len(), 2, "{r:?}");
        assert!(r.migrated());
        assert!(matches!(&r.events[0], Event::Moved { .. }));
        assert!(!a.old.exists() && !b.old.exists() && !c.new.exists());
        assert_eq!(snapshot(&a.new), want);
        assert!(!lock.exists(), "lock file removed after a complete move");
        let lines = r.lines();
        assert_eq!(lines.len(), 1, "one Activity line: {lines:?}");
        assert!(lines[0].1.contains("a-old -> "), "{lines:?}");
        // The second start: nothing to do, no lock taken.
        let r2 = migrate(&[a.clone(), b, c], &lock);
        assert!(r2.events.is_empty(), "{r2:?}");
        assert!(!lock.exists());
        assert_eq!(a.current(), a.new.as_path());
    }

    #[test]
    fn cross_device_copies_verifies_and_keeps_the_old_folder() {
        let t = tempfile::tempdir().unwrap();
        let a = pair(t.path(), "a");
        tree(&a.old);
        let want = snapshot(&a.old);
        let exdev = |_: &Path, _: &Path| -> std::io::Result<()> {
            Err(std::io::Error::from(std::io::ErrorKind::CrossesDevices))
        };
        let r = migrate_with(std::slice::from_ref(&a), &t.path().join("m.lock"), &exdev);
        assert_eq!(
            r.events,
            vec![Event::Copied {
                old: a.old.clone(),
                new: a.new.clone()
            }]
        );
        assert_eq!(snapshot(&a.new), want, "the copy is complete");
        assert_eq!(snapshot(&a.old), want, "the old folder is kept as it was");
        assert!(leftovers(t.path()).is_empty());
        assert!(r.lines()[0].1.contains("old folder was kept"));
        // Next start: both exist, the old one is only reported.
        let r2 = migrate_with(std::slice::from_ref(&a), &t.path().join("m.lock"), &exdev);
        assert!(matches!(r2.events[..], [Event::BothExist { .. }]), "{r2:?}");
    }

    #[cfg(unix)]
    #[test]
    fn failed_copy_leaves_no_new_folder_and_keeps_using_the_old_one() {
        use std::os::unix::fs::PermissionsExt;
        if crate::fsutil::current_uid() == Some(0) {
            return; // root reads anything
        }
        let t = tempfile::tempdir().unwrap();
        let a = pair(t.path(), "a");
        tree(&a.old);
        let secret = a.old.join("channels/abc/LIVE/profile.toml");
        std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o000)).unwrap();
        let exdev = |_: &Path, _: &Path| -> std::io::Result<()> {
            Err(std::io::Error::from(std::io::ErrorKind::CrossesDevices))
        };
        let lock = t.path().join("m.lock");
        let r = migrate_with(std::slice::from_ref(&a), &lock, &exdev);
        std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(r.events[..], [Event::Failed { .. }]), "{r:?}");
        assert!(!a.new.exists());
        assert!(
            leftovers(t.path()).is_empty(),
            "the partial copy is removed"
        );
        assert_eq!(a.current(), a.old.as_path());
        assert_eq!(r.lines()[0].0, Level::Warn);
        assert!(lock.exists(), "kept after a failure; harmless");
    }

    #[test]
    fn other_rename_errors_touch_nothing() {
        let t = tempfile::tempdir().unwrap();
        let a = pair(t.path(), "a");
        tree(&a.old);
        let want = snapshot(&a.old);
        let denied = |_: &Path, _: &Path| -> std::io::Result<()> {
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        };
        let r = migrate_with(std::slice::from_ref(&a), &t.path().join("m.lock"), &denied);
        assert!(matches!(r.events[..], [Event::Failed { .. }]), "{r:?}");
        assert!(!a.new.exists());
        assert_eq!(snapshot(&a.old), want);
        assert_eq!(a.current(), a.old.as_path());
    }

    #[test]
    fn both_exist_never_touches_the_old_folder() {
        let t = tempfile::tempdir().unwrap();
        let a = pair(t.path(), "a");
        tree(&a.old);
        std::fs::create_dir_all(&a.new).unwrap();
        std::fs::write(a.new.join("config.toml"), "new\n").unwrap();
        let want_old = snapshot(&a.old);
        let mtime = std::fs::metadata(&a.old).unwrap().modified().unwrap();
        let lock = t.path().join("m.lock");
        let r = migrate(std::slice::from_ref(&a), &lock);
        assert!(matches!(r.events[..], [Event::BothExist { .. }]), "{r:?}");
        assert!(!r.migrated());
        assert_eq!(snapshot(&a.old), want_old);
        assert_eq!(
            std::fs::metadata(&a.old).unwrap().modified().unwrap(),
            mtime
        );
        assert_eq!(
            std::fs::read_to_string(a.new.join("config.toml")).unwrap(),
            "new\n"
        );
        assert!(!lock.exists(), "no lock taken when nothing moves");
        let lines = r.lines();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].1.contains("left alone"), "{lines:?}");
        assert_eq!(a.current(), a.new.as_path());
    }

    #[test]
    fn concurrent_first_starts_move_each_folder_once() {
        let t = tempfile::tempdir().unwrap();
        let pairs = vec![pair(t.path(), "a"), pair(t.path(), "b")];
        for p in &pairs {
            tree(&p.old);
        }
        let want = snapshot(&pairs[0].old);
        let lock = t.path().join("m.lock");
        let go = std::sync::Barrier::new(8);
        let reports: Vec<Report> = std::thread::scope(|s| {
            let hs: Vec<_> = (0..8)
                .map(|_| {
                    s.spawn(|| {
                        go.wait();
                        migrate(&pairs, &lock)
                    })
                })
                .collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let moved: usize = reports
            .iter()
            .flat_map(|r| &r.events)
            .filter(|e| matches!(e, Event::Moved { .. }))
            .count();
        assert_eq!(moved, 2, "{reports:?}");
        assert!(
            reports
                .iter()
                .flat_map(|r| &r.events)
                .all(|e| matches!(e, Event::Moved { .. })),
            "nobody failed or saw a half-moved state: {reports:?}"
        );
        for p in &pairs {
            assert!(!p.old.exists());
            assert_eq!(snapshot(&p.new), want);
        }
    }

    #[test]
    fn stale_partial_copies_are_cleaned_before_a_copy() {
        let t = tempfile::tempdir().unwrap();
        let a = pair(t.path(), "a");
        tree(&a.old);
        let stale = t.path().join("a-new.migrating-1");
        std::fs::create_dir_all(stale.join("x")).unwrap();
        let r = migrate(std::slice::from_ref(&a), &t.path().join("m.lock"));
        assert!(matches!(r.events[..], [Event::Moved { .. }]));
        assert!(leftovers(t.path()).is_empty());
    }

    fn vars(list: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> + use<> {
        let m: std::collections::HashMap<String, OsString> = list
            .iter()
            .map(|(k, v)| (k.to_string(), OsString::from(v)))
            .collect();
        move |k| m.get(k).cloned()
    }

    #[test]
    fn linux_layout_uses_the_old_folder_until_it_is_moved() {
        let t = tempfile::tempdir().unwrap();
        let h = t.path().to_str().unwrap();
        let l = Layout::from_vars(vars(&[("HOME", h)]), false).unwrap();
        assert_eq!(l.config.new, t.path().join(".config/starorg-codex"));
        assert_eq!(l.config.old, t.path().join(".config/starorg-strings"));
        assert_eq!(l.cache.new, t.path().join(".cache/starorg-codex"));
        assert!(l.pairs.iter().any(
            |p| p.new == t.path().join(".local/share/tools.starorg.codex")
                && p.old == t.path().join(".local/share/tools.starorg.strings")
        ));
        assert_eq!(
            l.lock,
            t.path().join(".local/share/.starorg-codex-migration.lock")
        );
        // Fresh install: the new names.
        assert_eq!(l.dirs().data, t.path().join(".local/share/starorg-codex"));
        // Not moved yet: the old data folder is used where it is.
        std::fs::create_dir_all(&l.data.old).unwrap();
        assert_eq!(l.dirs().data, l.data.old);
        assert_eq!(l.dirs().config, l.config.new);
        let r = migrate(&l.pairs, &l.lock);
        assert!(r.migrated());
        assert_eq!(l.dirs().data, l.data.new);
        assert_eq!(
            l.dirs(),
            AppDirs {
                config: l.config.new.clone(),
                data: l.data.new.clone(),
                cache: l.cache.new.clone(),
            }
        );
    }

    #[test]
    fn xdg_overrides_are_followed() {
        let t = tempfile::tempdir().unwrap();
        let p = |s: &str| t.path().join(s).to_str().unwrap().to_string();
        let l = Layout::from_vars(
            vars(&[
                ("HOME", &p("home")),
                ("XDG_CONFIG_HOME", &p("c")),
                ("XDG_DATA_HOME", &p("d")),
                ("XDG_CACHE_HOME", &p("k")),
            ]),
            false,
        )
        .unwrap();
        assert_eq!(l.config.old, t.path().join("c/starorg-strings"));
        assert_eq!(l.data.new, t.path().join("d/starorg-codex"));
        assert_eq!(l.cache.new, t.path().join("k/starorg-codex"));
        assert_eq!(l.pairs.len(), 5, "{:?}", l.pairs);
    }

    #[test]
    fn windows_layout_moves_the_roaming_and_local_roots() {
        let t = tempfile::tempdir().unwrap();
        let roaming = t.path().join("Roaming");
        let local = t.path().join("Local");
        let l = Layout::from_vars(
            vars(&[
                ("APPDATA", roaming.to_str().unwrap()),
                ("LOCALAPPDATA", local.to_str().unwrap()),
            ]),
            true,
        )
        .unwrap();
        assert_eq!(
            l.pairs[0].old,
            roaming.join("StarOrg Tools").join("Strings")
        );
        assert_eq!(l.pairs[0].new, roaming.join("StarOrg Tools").join("Codex"));
        assert_eq!(l.pairs[1].old, local.join("StarOrg Tools").join("Strings"));
        assert_eq!(l.pairs[2].old, local.join("tools.starorg.strings"));
        assert_eq!(l.pairs[2].new, local.join("tools.starorg.codex"));
        let d = l.dirs();
        assert_eq!(d.config, roaming.join("StarOrg Tools/Codex/config"));
        assert_eq!(d.cache, local.join("StarOrg Tools/Codex/cache"));
        std::fs::create_dir_all(roaming.join("StarOrg Tools/Strings/data")).unwrap();
        assert_eq!(l.dirs().data, roaming.join("StarOrg Tools/Strings/data"));
        assert_eq!(l.dirs().cache, local.join("StarOrg Tools/Codex/cache"));
        // Without %LOCALAPPDATA% everything is under the roaming root, moved once.
        let one = Layout::from_vars(vars(&[("APPDATA", roaming.to_str().unwrap())]), true).unwrap();
        assert_eq!(one.pairs.len(), 2, "{:?}", one.pairs);
    }
}

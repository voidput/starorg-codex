use super::*;
use std::time::SystemTime;

/// Best effort: set `p`'s modification time `secs` ago (folders cannot be opened for that
/// on every OS; the tests below check what they could set).
fn age(p: &Path, secs: u64) {
    let t = SystemTime::now() - Duration::from_secs(secs);
    let f = std::fs::File::options()
        .write(true)
        .open(p)
        .or_else(|_| std::fs::File::open(p));
    if let Ok(f) = f {
        let _ = f.set_modified(t);
    }
}

fn is_old(p: &Path) -> bool {
    crate::storage::newest_mtime(p)
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|e| e >= WORK_STALE)
}

/// A `<root>/<name>/out/base.ini` with everything aged `secs` (where the OS lets us).
fn leftover(root: &Path, name: &str, secs: u64) -> PathBuf {
    let d = root.join(name);
    let f = d.join("out").join("base.ini");
    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    std::fs::write(&f, b"x").unwrap();
    for p in [f, d.join("out"), d.clone()] {
        age(&p, secs);
    }
    d
}

#[test]
fn sweep_removes_only_old_unheld_extract_folders() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path();
    let held = leftover(root, "starorg-extract-held12", 7200);
    let lock = crate::lock::try_lock(&held.join(WORK_LOCK)).unwrap();
    // Taking the lock wrote its file: age it all again.
    for p in [held.join(WORK_LOCK), held.clone()] {
        age(&p, 7200);
    }
    let old = leftover(root, "starorg-extract-old123", 7200);
    let fresh = leftover(root, "starorg-extract-new123", 60);
    let other = leftover(root, "someone-else", 7200);
    let removed = sweep_work(root, WORK_STALE);
    assert!(fresh.exists(), "younger than the threshold");
    assert!(other.exists(), "not ours");
    assert!(held.exists(), "a live load holds its lock");
    // Folder times cannot be set on every OS; a folder that still looks new is kept (the
    // safe side), so check removal only where the ageing worked.
    if !removed.is_empty() || is_old(&old) {
        assert_eq!(removed, vec![old.clone()]);
        assert!(!old.exists());
    }
    drop(lock);
    // Once the load ended (lock released), the old folder goes too.
    let again = sweep_work(root, Duration::ZERO);
    assert!(!held.exists());
    assert!(again.contains(&held));
    assert!(other.exists());
}

#[test]
fn a_new_file_inside_keeps_an_old_folder() {
    let t = tempfile::tempdir().unwrap();
    let d = leftover(t.path(), "starorg-extract-busy12", 7200);
    std::fs::write(d.join("out").join("gamedata.json"), b"{}").unwrap();
    assert!(sweep_work(t.path(), WORK_STALE).is_empty());
    assert!(d.exists());
    // With a zero threshold it is old enough, and nothing holds it.
    assert_eq!(sweep_work(t.path(), Duration::ZERO), vec![d.clone()]);
    assert!(!d.exists());
}

#[test]
fn sweep_of_a_missing_folder_is_a_no_op() {
    let t = tempfile::tempdir().unwrap();
    assert!(sweep_work(&t.path().join("nope"), WORK_STALE).is_empty());
}

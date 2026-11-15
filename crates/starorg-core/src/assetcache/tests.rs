use super::*;
use std::time::{Duration, SystemTime};

fn write(c: &AssetCache, ch: &str, build: &str, kind: Kind, key: &str, n: usize) -> PathBuf {
    c.prepare(ch, build, kind).unwrap();
    let part = AssetCache::part_path(&c.path(ch, build, kind, key, "bin"));
    std::fs::write(&part, vec![7u8; n]).unwrap();
    c.commit(ch, build, kind, key, "bin", &part)
        .unwrap()
        .unwrap()
}

fn age(p: &Path, secs: u64) {
    let f = std::fs::File::options().write(true).open(p).unwrap();
    f.set_modified(SystemTime::now() - Duration::from_secs(secs))
        .unwrap();
}

#[test]
fn miss_then_hit_and_hit_touches_the_file() {
    let t = tempfile::tempdir().unwrap();
    let c = AssetCache::new(t.path().join("assets"), 1 << 20);
    assert!(c.get("LIVE", "b1", Kind::Audio, "123", "ogg").is_none());
    c.prepare("LIVE", "b1", Kind::Audio).unwrap();
    let part = AssetCache::part_path(&c.path("LIVE", "b1", Kind::Audio, "123", "ogg"));
    std::fs::write(&part, b"OggS").unwrap();
    let p = c
        .commit("LIVE", "b1", Kind::Audio, "123", "ogg", &part)
        .unwrap()
        .unwrap();
    assert!(!part.exists());
    assert!(t.path().join("assets").join(MARKER).exists());
    age(&p, 5000);
    let hit = c.get("LIVE", "b1", Kind::Audio, "123", "ogg").unwrap();
    assert_eq!(hit, p);
    let m = std::fs::metadata(&p).unwrap().modified().unwrap();
    assert!(SystemTime::now().duration_since(m).unwrap() < Duration::from_secs(60));
    // Another build is a miss.
    assert!(c.get("LIVE", "b2", Kind::Audio, "123", "ogg").is_none());
    assert_eq!(c.usage(), Usage { bytes: 4, files: 1 });
}

#[test]
fn a_new_build_invalidates_the_old_one() {
    let t = tempfile::tempdir().unwrap();
    let c = AssetCache::new(t.path(), 1 << 20);
    write(&c, "LIVE", "old", Kind::Models, "ship", 10);
    write(&c, "PTU", "ptu1", Kind::Audio, "1", 10);
    assert!(c.get("LIVE", "old", Kind::Models, "ship", "bin").is_some());
    write(&c, "LIVE", "new", Kind::Audio, "1", 10);
    assert!(c.get("LIVE", "old", Kind::Models, "ship", "bin").is_none());
    assert!(!t.path().join("LIVE").join("old").exists());
    // Other channels keep theirs.
    assert!(c.get("PTU", "ptu1", Kind::Audio, "1", "bin").is_some());
}

#[test]
fn eviction_removes_least_recently_used_first() {
    let t = tempfile::tempdir().unwrap();
    let c = AssetCache::new(t.path(), 1000);
    let a = write(&c, "LIVE", "b", Kind::Audio, "a", 200);
    let b = write(&c, "LIVE", "b", Kind::Audio, "b", 200);
    let d = write(&c, "LIVE", "b", Kind::Audio, "d", 200);
    age(&a, 300);
    age(&b, 200);
    age(&d, 100);
    // `a` is used again: it is now the newest.
    assert!(c.get("LIVE", "b", Kind::Audio, "a", "bin").is_some());
    // 600 + 240 + 240 > 1000: the two oldest (b, then d) go.
    write(&c, "LIVE", "b", Kind::Previews, "e", 240);
    write(&c, "LIVE", "b", Kind::Previews, "f", 240);
    assert!(a.exists());
    assert!(!b.exists());
    assert!(c.usage().bytes <= 1000);
    // Larger than a quarter of the cap: not kept.
    c.prepare("LIVE", "b", Kind::Models).unwrap();
    let part = AssetCache::part_path(&c.path("LIVE", "b", Kind::Models, "big", "glb"));
    std::fs::write(&part, vec![0u8; 300]).unwrap();
    assert!(
        c.commit("LIVE", "b", Kind::Models, "big", "glb", &part)
            .unwrap()
            .is_none()
    );
    assert!(!part.exists());
}

#[test]
fn stale_parts_are_removed_and_fresh_ones_kept() {
    let t = tempfile::tempdir().unwrap();
    let c = AssetCache::new(t.path(), 1 << 20);
    let d = c.prepare("LIVE", "b", Kind::Audio).unwrap();
    let stale = d.join("x.ogg.part");
    let fresh = d.join("y.ogg.part");
    std::fs::write(&stale, b"1").unwrap();
    std::fs::write(&fresh, b"1").unwrap();
    age(&stale, 7200);
    c.evict().unwrap();
    assert!(!stale.exists());
    assert!(fresh.exists());
}

#[test]
fn clear_and_move_touch_only_our_files() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("mine");
    std::fs::create_dir_all(root.join("LIVE")).unwrap();
    std::fs::write(root.join("notes.txt"), b"keep").unwrap();
    std::fs::write(root.join("LIVE").join("other.txt"), b"keep").unwrap();
    let c = AssetCache::new(&root, 1 << 20);
    write(&c, "LIVE", "b", Kind::Audio, "1", 5);
    write(&c, "LIVE", "b", Kind::Models, "m", 7);
    let to = t.path().join("elsewhere");
    let moved = c.move_to(&to).unwrap();
    assert_eq!(
        moved,
        Usage {
            bytes: 12,
            files: 2
        }
    );
    assert!(root.join("notes.txt").exists());
    assert!(root.join("LIVE").join("other.txt").exists());
    assert!(!root.join(MARKER).exists());
    let c2 = AssetCache::new(&to, 1 << 20);
    assert!(c2.get("LIVE", "b", Kind::Models, "m", "bin").is_some());
    assert!(to.join(MARKER).exists());
    let cleared = c2.clear().unwrap();
    assert_eq!(cleared.files, 2);
    assert_eq!(c2.usage(), Usage::default());
    assert!(!to.join("LIVE").exists());
}

#[test]
fn names_are_safe() {
    assert_eq!(
        safe_name("AEGS_Javelin-full-lod0"),
        "AEGS_Javelin-full-lod0"
    );
    let h = safe_name("Data\\Textures\\x.dds");
    assert_eq!(h.len(), 32);
    assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(safe_name(".."), "..");
    assert_eq!(safe_name(&"a".repeat(200)).len(), 32);
}

#[test]
fn setting_round_trips_and_resolves() {
    let t = tempfile::tempdir().unwrap();
    let dirs = crate::appdirs::AppDirs::under(t.path());
    let mut cfg = Config::default();
    assert_eq!(resolve(&dirs, &cfg, None).root(), default_dir(&dirs));
    assert_eq!(resolve(&dirs, &cfg, None).max_bytes(), DEFAULT_MAX_BYTES);
    let mine = t.path().join("cache2");
    set_configured_dir(&mut cfg, Some(&mine));
    let text = toml::to_string(&cfg).unwrap();
    assert!(text.contains("[dev]"), "{text}");
    let back: Config = toml::from_str(&text).unwrap();
    assert_eq!(configured_dir(&back).as_deref(), Some(mine.as_path()));
    assert_eq!(resolve(&dirs, &back, None).root(), mine);
    let cli = t.path().join("cli");
    assert_eq!(resolve(&dirs, &back, Some(&cli)).root(), cli);
    set_configured_dir(&mut cfg, None);
    assert!(configured_dir(&cfg).is_none());
    assert!(!cfg.extra.contains_key("dev"));
    let mut capped: Config = toml::from_str("[dev]\nasset_cache_max_mb = 10\n").unwrap();
    assert_eq!(configured_max(&capped), 10 * 1024 * 1024);
    set_configured_dir(&mut capped, Some(&mine));
    set_configured_dir(&mut capped, None);
    assert_eq!(configured_max(&capped), 10 * 1024 * 1024);
}

#[test]
fn validation_refuses_the_game_folder_and_files() {
    let t = tempfile::tempdir().unwrap();
    let game = t
        .path()
        .join("prefix")
        .join("drive_c")
        .join("Games")
        .join("StarCitizen");
    std::fs::create_dir_all(&game).unwrap();
    let protected = protected_roots(std::slice::from_ref(&game));
    let default = t.path().join("default");
    // Inside the game folder, and inside its Wine prefix.
    let e = validate_dir(&game.join("cache"), &protected, &default).unwrap_err();
    assert!(e.contains("never goes into the game folder"), "{e}");
    let e = validate_dir(&t.path().join("prefix").join("x"), &protected, &default).unwrap_err();
    assert!(e.contains("never goes into"), "{e}");
    assert!(!game.join("cache").exists());
    // A file.
    let f = t.path().join("file");
    std::fs::write(&f, b"x").unwrap();
    assert!(
        validate_dir(&f, &protected, &default)
            .unwrap_err()
            .contains("is a file")
    );
    // Relative.
    assert!(validate_dir(Path::new("rel"), &protected, &default).is_err());
    // Fine: created, writable, same drive as the default.
    let ok = validate_dir(&t.path().join("new").join("cache"), &protected, &default).unwrap();
    assert!(ok.path.is_dir());
    assert!(!ok.other_drive);
    assert_eq!(
        std::fs::read_dir(&ok.path).unwrap().count(),
        0,
        "probe removed"
    );
}

#[cfg(unix)]
#[test]
fn validation_refuses_a_read_only_folder() {
    use std::os::unix::fs::PermissionsExt;
    if crate::fsutil::current_uid() == Some(0) {
        return; // root writes anywhere
    }
    let t = tempfile::tempdir().unwrap();
    let ro = t.path().join("ro");
    std::fs::create_dir(&ro).unwrap();
    std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o555)).unwrap();
    let e = validate_dir(&ro, &[], t.path()).unwrap_err();
    std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(e.contains("not writable"), "{e}");
}

#[test]
fn base64_matches_rfc4648_vectors() {
    for (i, o) in [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ] {
        assert_eq!(base64(i.as_bytes()), o);
    }
}

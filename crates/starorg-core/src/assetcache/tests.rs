use super::*;

const B: &str = "00000000000000000000000b";
const B1: &str = "0000000000000000000000b1";
const B2: &str = "0000000000000000000000b2";
const OLD: &str = "000000000000000000000001";
const NEW: &str = "000000000000000000000002";
const PTU1: &str = "0000000000000000000000f1";
use std::time::{Duration, SystemTime};

fn write(c: &AssetCache, ch: &str, build: &str, kind: Kind, key: &str, n: usize) -> PathBuf {
    c.prepare(ch, build, kind).unwrap();
    let part = AssetCache::part_path(&c.path(ch, build, kind, key, "ogg"));
    std::fs::write(&part, vec![7u8; n]).unwrap();
    c.commit(ch, build, kind, key, "ogg", &part)
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
    assert!(c.get("LIVE", B1, Kind::Audio, "123", "ogg").is_none());
    c.prepare("LIVE", B1, Kind::Audio).unwrap();
    let part = AssetCache::part_path(&c.path("LIVE", B1, Kind::Audio, "123", "ogg"));
    std::fs::write(&part, b"OggS").unwrap();
    let p = c
        .commit("LIVE", B1, Kind::Audio, "123", "ogg", &part)
        .unwrap()
        .unwrap();
    assert!(!part.exists());
    assert!(t.path().join("assets").join(MARKER).exists());
    age(&p, 5000);
    let hit = c.get("LIVE", B1, Kind::Audio, "123", "ogg").unwrap();
    assert_eq!(hit, p);
    let m = std::fs::metadata(&p).unwrap().modified().unwrap();
    assert!(SystemTime::now().duration_since(m).unwrap() < Duration::from_secs(60));
    // Another build is a miss.
    assert!(c.get("LIVE", B2, Kind::Audio, "123", "ogg").is_none());
    assert_eq!(c.usage(), Usage { bytes: 4, files: 1 });
}

#[test]
fn a_new_build_invalidates_the_old_one() {
    let t = tempfile::tempdir().unwrap();
    let c = AssetCache::new(t.path(), 1 << 20);
    write(&c, "LIVE", OLD, Kind::Models, "ship", 10);
    write(&c, "PTU", PTU1, Kind::Audio, "1", 10);
    assert!(c.get("LIVE", OLD, Kind::Models, "ship", "ogg").is_some());
    write(&c, "LIVE", NEW, Kind::Audio, "1", 10);
    assert!(c.get("LIVE", OLD, Kind::Models, "ship", "ogg").is_none());
    assert!(!t.path().join("LIVE").join(OLD).exists());
    // Other channels keep theirs.
    assert!(c.get("PTU", PTU1, Kind::Audio, "1", "ogg").is_some());
}

#[test]
fn eviction_removes_least_recently_used_first() {
    let t = tempfile::tempdir().unwrap();
    let c = AssetCache::new(t.path(), 1000);
    let a = write(&c, "LIVE", B, Kind::Audio, "a", 200);
    let b = write(&c, "LIVE", B, Kind::Audio, B, 200);
    let d = write(&c, "LIVE", B, Kind::Audio, "d", 200);
    age(&a, 300);
    age(&b, 200);
    age(&d, 100);
    // `a` is used again: it is now the newest.
    assert!(c.get("LIVE", B, Kind::Audio, "a", "ogg").is_some());
    // 600 + 240 + 240 > 1000: the two oldest (b, then d) go.
    write(&c, "LIVE", B, Kind::Previews, "e", 240);
    write(&c, "LIVE", B, Kind::Previews, "f", 240);
    assert!(a.exists());
    assert!(!b.exists());
    assert!(c.usage().bytes <= 1000);
    // Larger than a quarter of the cap: not kept.
    c.prepare("LIVE", B, Kind::Models).unwrap();
    let part = AssetCache::part_path(&c.path("LIVE", B, Kind::Models, "big", "glb"));
    std::fs::write(&part, vec![0u8; 300]).unwrap();
    assert!(
        c.commit("LIVE", B, Kind::Models, "big", "glb", &part)
            .unwrap()
            .is_none()
    );
    assert!(!part.exists());
}

#[test]
fn stale_parts_are_removed_and_fresh_ones_kept() {
    let t = tempfile::tempdir().unwrap();
    let c = AssetCache::new(t.path(), 1 << 20);
    let d = c.prepare("LIVE", B, Kind::Audio).unwrap();
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
    write(&c, "LIVE", B, Kind::Audio, "1", 5);
    write(&c, "LIVE", B, Kind::Models, "m", 7);
    let to = AssetCache::new(t.path().join("elsewhere"), 1 << 20);
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
    assert!(to.get("LIVE", B, Kind::Models, "m", "ogg").is_some());
    assert!(to.root().join(MARKER).exists());
    let cleared = to.clear().unwrap();
    assert_eq!(cleared.files, 2);
    assert_eq!(to.usage(), Usage::default());
    assert!(!to.root().join("LIVE").exists());
}

/// The verifier's repro: a user folder with look-alike paths and a symlink to elsewhere. Nothing
/// of it is counted, evicted, pruned, cleared or moved; nothing outside the root is touched.
#[cfg(unix)]
#[test]
fn user_files_and_symlinks_are_never_touched() {
    let t = tempfile::tempdir().unwrap();
    let user = t.path().join("userdata");
    let outside = t.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("precious.txt"), b"precious").unwrap();
    std::fs::write(outside.join("big.bin"), vec![1u8; 4096]).unwrap();
    let keep = [
        "Music/Album/audio/concert.flac",
        "Music/Album/other/notes.txt",
        "photos/2024/previews/thumb.jpg",
        "blender/shipyard/models/my-own-work.blend",
        "LIVE/oldbuild/audio/song.ogg",
    ];
    for k in keep {
        let p = user.join(k);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, b"user").unwrap();
    }
    // A cache root chosen inside the user's folder, as the setting does.
    let root = user.join(SUBDIR);
    let c = AssetCache::new(&root, 1000);
    write(&c, "LIVE", B, Kind::Audio, "1", 10);
    // Inside our root: a symlinked kind dir and build dir pointing outside, a symlinked file.
    let b2 = root.join("LIVE").join(B2);
    std::fs::create_dir_all(&b2).unwrap();
    std::os::unix::fs::symlink(&outside, b2.join("audio")).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("LIVE").join(OLD)).unwrap();
    std::os::unix::fs::symlink(
        outside.join("big.bin"),
        c.kind_dir("LIVE", B, Kind::Audio).join("x.ogg"),
    )
    .unwrap();
    // Look-alike folders of the user's beside the root.
    std::fs::create_dir_all(user.join("LIVE").join(OLD).join("audio")).unwrap();
    std::fs::write(
        user.join("LIVE").join(OLD).join("audio").join("a.ogg"),
        b"user",
    )
    .unwrap();
    assert_eq!(
        c.usage(),
        Usage {
            bytes: 10,
            files: 1
        },
        "links and look-alikes not counted"
    );
    // Fill past the cap, prune, clear, move: none reaches the user's files.
    for i in 0..5 {
        write(&c, "LIVE", NEW, Kind::Models, &format!("m{i}"), 240);
    }
    c.prune_builds("LIVE", NEW);
    c.evict().unwrap();
    let to = AssetCache::new(t.path().join("moved").join(SUBDIR), 1 << 20);
    c.move_to(&to).unwrap();
    to.clear().unwrap();
    c.clear().unwrap();
    assert_eq!(
        std::fs::read(outside.join("precious.txt")).unwrap(),
        b"precious"
    );
    assert_eq!(std::fs::read(outside.join("big.bin")).unwrap().len(), 4096);
    for k in keep {
        assert!(user.join(k).exists(), "{k} kept");
    }
    assert!(
        user.join("LIVE")
            .join(OLD)
            .join("audio")
            .join("a.ogg")
            .exists()
    );
    // A user folder without our marker is never ours, even with our exact layout.
    let fake = AssetCache::new(user.join("Music"), 1);
    std::fs::create_dir_all(user.join("Music/LIVE").join(B).join("audio")).unwrap();
    std::fs::write(user.join("Music/LIVE").join(B).join("audio/1.ogg"), b"user").unwrap();
    assert_eq!(fake.usage(), Usage::default());
    fake.clear().unwrap();
    fake.evict().unwrap();
    assert!(user.join("Music/LIVE").join(B).join("audio/1.ogg").exists());
    // Writes refuse a symlinked folder of the layout.
    assert!(c.prepare("LIVE", B2, Kind::Audio).is_err());
    assert!(
        std::fs::read_dir(&outside).unwrap().count() == 2,
        "nothing written outside"
    );
}

#[test]
fn chosen_folders_get_their_own_subfolder_and_are_checked_at_use() {
    let t = tempfile::tempdir().unwrap();
    let dirs = crate::appdirs::AppDirs::under(t.path());
    let mut cfg = Config::default();
    let chosen = t.path().join("Downloads");
    set_configured_dir(&mut cfg, Some(&chosen));
    assert_eq!(resolve(&dirs, &cfg, None).root(), chosen.join(SUBDIR));
    let game = t.path().join("pfx").join("drive_c").join("SC");
    std::fs::create_dir_all(&game).unwrap();
    let prot = protected_roots(std::slice::from_ref(&game));
    assert!(resolve_checked(&dirs, &cfg, None, &prot).is_ok());
    let e = resolve_checked(&dirs, &cfg, Some(&game.join("LIVE")), &prot).unwrap_err();
    assert!(e.contains("never goes into the game folder"), "{e}");
    set_configured_dir(&mut cfg, Some(&t.path().join("pfx")));
    assert!(
        resolve_checked(&dirs, &cfg, None, &prot).is_err(),
        "inside the prefix"
    );
    assert!(
        !game.join("LIVE").exists() && !t.path().join("pfx").join(SUBDIR).exists(),
        "nothing created"
    );
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
    assert_eq!(resolve(&dirs, &back, None).root(), mine.join(SUBDIR));
    let cli = t.path().join("cli");
    assert_eq!(resolve(&dirs, &back, Some(&cli)).root(), cli.join(SUBDIR));
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
    let ok = validate_dir(&t.path().join(NEW).join("cache"), &protected, &default).unwrap();
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

#[test]
fn every_sound_format_is_one_of_ours() {
    for x in ["ogg", "opus", "wav", "wem"] {
        assert!(file_name_ok(&format!("123.{x}")), "{x}");
    }
    assert!(!file_name_ok("123.mp3"));
    let t = tempfile::tempdir().unwrap();
    let c = AssetCache::new(t.path().join("assets"), 1 << 20);
    c.prepare("LIVE", B, Kind::Audio).unwrap();
    for x in ["opus", "wav"] {
        std::fs::write(c.path("LIVE", B, Kind::Audio, "9", x), b"x").unwrap();
    }
    assert_eq!(c.usage().files, 2, "counted (and evicted) like the rest");
}

#[test]
fn peek_finds_a_file_without_touching_it() {
    let t = tempfile::tempdir().unwrap();
    let c = AssetCache::new(t.path().join("assets"), 1 << 20);
    assert!(
        c.peek("LIVE", B1, Kind::Models, "x-print-lod3", "glb")
            .is_none()
    );
    c.prepare("LIVE", B1, Kind::Models).unwrap();
    let p = c.path("LIVE", B1, Kind::Models, "x-print-lod3", "glb");
    std::fs::write(&p, b"glTF").unwrap();
    age(&p, 5000);
    let before = std::fs::metadata(&p).unwrap().modified().unwrap();
    assert_eq!(
        c.peek("LIVE", B1, Kind::Models, "x-print-lod3", "glb"),
        Some(p.clone())
    );
    assert_eq!(std::fs::metadata(&p).unwrap().modified().unwrap(), before);
    // Another LOD is another file.
    assert!(
        c.peek("LIVE", B1, Kind::Models, "x-print-lod4", "glb")
            .is_none()
    );
    std::fs::write(&p, b"").unwrap();
    assert!(
        c.peek("LIVE", B1, Kind::Models, "x-print-lod3", "glb")
            .is_none(),
        "empty"
    );
}

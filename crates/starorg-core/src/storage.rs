//! What the app keeps on this PC, per area (Settings > Storage, `docs/user-guide.md`).
//!
//! Reads sizes only; never writes. Links are not followed, so a folder linked into the
//! cache is not counted (and a loop cannot hang the walk).
//!
//! | Area | Where | Bound |
//! |---|---|---|
//! | Game data | `<cache>/<install>/<CHANNEL>/<key>/`, `<cache>/dumps/` | one entry per build key |
//! | WHERE TO GET datasets | `<cache>/mcp/<CHANNEL>/<build>-<fp>/` | 3 builds per channel |
//! | UEX market data | `<cache>/mcp/uex/` | 3 files, replaced on fetch |
//! | Asset exports | the asset cache root ([`crate::assetcache`]) | 1 GiB, LRU |
//! | Temporary | `<cache>/work/starorg-extract-*` | removed after each load; leftovers after 1 h |
//! | Live telemetry schemas | `<cache>/telemetry/<key>.fds` ([`telemetry_dir`]) | 3 builds (`starorg_telemetry::schema::KEEP_KEYS`) |
//! | Live telemetry recordings | `<data>/telemetry/recordings/*.jsonl` ([`telemetry_recordings_dir`]) | 20 files, 256 MiB (`starorg_telemetry::record::Caps`) |
//! | Live telemetry session history | `<data>/telemetry/sessions/history.json` ([`telemetry_sessions_dir`]) | 2000 rows, 2 MiB (`starorg_telemetry::gamelog::stints`) |
//! | Backups | `<data>/channels/<install>/<CHANNEL>/backups/`, `<data>/hooks/backups/` | [`crate::backup::KEEP_NEWEST`] + builds |
//!
//! **One cap for the cache folder** ([`DEFAULT_CAP`], 1 GiB; `[storage] cache_max_mb` in
//! `config.toml`). After a write ([`enforce`]: a game-data entry stored, a dataset extracted,
//! an asset job, app start) the least recently used goes first, until the folder is under the
//! cap:
//! 1. asset exports in `<cache>/assets` (LRU files);
//! 2. WHERE TO GET build folders and game-data entries that are not a channel's newest
//!    (an older build);
//! 3. a channel's newest WHERE TO GET build folder, oldest-used channel first.
//!
//! Never removed: a channel's newest game-data entry, anything written or used in the last
//! [`crate::assetcache::IN_USE`] or named by the caller as in use, UEX files, `work/` (see
//! [`crate::source::local::sweep_work`]), `dumps/`, the live telemetry schemas (a few hundred
//! KB, pruned to 3 builds by their writer), backups, the live telemetry recordings (in the data
//! folder, bounded by their writer) and everything outside the cache folder. The cap can therefore stay exceeded when what is left is all in use.

use crate::appdirs::AppDirs;
use crate::assetcache::{AssetCache, IN_USE, Usage};
use crate::config::Config;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Default cap of the cache folder: 1 GiB.
pub const DEFAULT_CAP: u64 = 1024 * 1024 * 1024;
const TABLE: &str = "storage";
const KEY_CAP: &str = "cache_max_mb";

/// One storage area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Area {
    GameData,
    WhereToGet,
    Uex,
    AssetExports,
    Temporary,
    /// Live telemetry (experimental): the game's service schema per build, read from the
    /// user's own `StarCitizen.exe` ([`telemetry_dir`]).
    Telemetry,
    /// Live telemetry (experimental): recorded sessions ("Record sessions"), in the data
    /// folder ([`telemetry_recordings_dir`]); not part of the cache or its cap.
    TelemetryRecordings,
    /// Live telemetry (experimental): the server and shard join history read from Game.log
    /// ("Read Game.log events"), in the data folder ([`telemetry_sessions_dir`]); not part of
    /// the cache or its cap.
    TelemetrySessions,
    Backups,
}

impl Area {
    pub const ALL: [Area; 9] = [
        Area::GameData,
        Area::WhereToGet,
        Area::Uex,
        Area::AssetExports,
        Area::Temporary,
        Area::Telemetry,
        Area::TelemetryRecordings,
        Area::TelemetrySessions,
        Area::Backups,
    ];
}

/// The `<cache>` subfolders that are not game data.
const MCP: &str = "mcp";
const UEX: &str = "uex";
const WORK: &str = "work";
const ASSETS: &str = "assets";
const DUMPS: &str = "dumps";
const TELEMETRY: &str = "telemetry";

/// Where live telemetry keeps the schema it read from each build's `StarCitizen.exe`
/// (`<cache>/telemetry/`), so the cache folder's size and Settings > Storage count it.
pub fn telemetry_dir(dirs: &AppDirs) -> PathBuf {
    dirs.cache.join(TELEMETRY)
}

/// Where live telemetry keeps recorded sessions (`<data>/telemetry/recordings/`): the user's
/// own sessions, not a cache, so they are outside the cache folder and its cap and bounded by
/// their writer (`starorg_telemetry::record`).
pub fn telemetry_recordings_dir(dirs: &AppDirs) -> PathBuf {
    dirs.data.join(TELEMETRY).join("recordings")
}

/// Where live telemetry keeps the server and shard join history read from Game.log
/// (`<data>/telemetry/sessions/`): one small file bounded by its writer
/// (`starorg_telemetry::gamelog::stints`), outside the cache folder and its cap.
pub fn telemetry_sessions_dir(dirs: &AppDirs) -> PathBuf {
    dirs.data.join(TELEMETRY).join("sessions")
}

/// `[storage] cache_max_mb`, else [`DEFAULT_CAP`].
pub fn configured_cap(cfg: &Config) -> u64 {
    cfg.extra
        .get(TABLE)
        .and_then(|t| t.as_table())
        .and_then(|t| t.get(KEY_CAP))
        .and_then(|v| v.as_integer())
        .filter(|&mb| mb > 0)
        .map(|mb| mb as u64 * 1024 * 1024)
        .unwrap_or(DEFAULT_CAP)
}

/// The newest modification time in `p` (itself and everything below; links not followed).
pub fn newest_mtime(p: &Path) -> Option<SystemTime> {
    newest(p, true)
}

/// When a build folder was last used: its newest file (readers touch the file they use;
/// a folder's own time only says when a file was added), else the folder's time.
fn last_used(p: &Path) -> SystemTime {
    newest(p, false)
        .filter(|t| *t > SystemTime::UNIX_EPOCH)
        .or_else(|| newest(p, true))
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

fn newest(p: &Path, with_dirs: bool) -> Option<SystemTime> {
    let m = std::fs::symlink_metadata(p).ok()?;
    let mut newest = if with_dirs || m.is_file() {
        m.modified().ok()?
    } else {
        SystemTime::UNIX_EPOCH
    };
    let mut stack = vec![p.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let Ok(m) = std::fs::symlink_metadata(e.path()) else {
                continue;
            };
            if (with_dirs || m.is_file())
                && let Ok(t) = m.modified()
            {
                newest = newest.max(t);
            }
            if m.is_dir() && !m.file_type().is_symlink() {
                stack.push(e.path());
            }
        }
    }
    Some(newest)
}

/// Bytes and files under `p` (a file or a folder); links are not followed.
pub fn usage(p: &Path) -> Usage {
    let mut u = Usage::default();
    let Ok(m) = std::fs::symlink_metadata(p) else {
        return u;
    };
    if m.file_type().is_symlink() {
        return u;
    }
    if m.is_file() {
        return Usage {
            bytes: m.len(),
            files: 1,
        };
    }
    let mut stack = vec![p.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let Ok(m) = std::fs::symlink_metadata(e.path()) else {
                continue;
            };
            if m.file_type().is_symlink() {
                continue;
            }
            if m.is_dir() {
                stack.push(e.path());
            } else if m.is_file() {
                u.bytes += m.len();
                u.files += 1;
            }
        }
    }
    u
}

fn add(a: Usage, b: Usage) -> Usage {
    Usage {
        bytes: a.bytes + b.bytes,
        files: a.files + b.files,
    }
}

fn entries(dir: &Path) -> Vec<(String, PathBuf)> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        .collect()
}

/// One area's size and where it lives (the folder shown; some areas span several).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AreaUsage {
    pub area: Area,
    pub path: PathBuf,
    pub usage: Usage,
}

/// Every area, plus the cache folder as a whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// `<cache>`, everything in it (the asset cache only when it is the default folder).
    pub cache_path: PathBuf,
    pub cache: Usage,
    /// In [`Area::ALL`] order.
    pub areas: Vec<AreaUsage>,
}

impl Report {
    pub fn area(&self, a: Area) -> Usage {
        self.areas
            .iter()
            .find(|x| x.area == a)
            .map(|x| x.usage)
            .unwrap_or_default()
    }

    /// The cache folder, plus the asset cache when it is elsewhere, plus backups, telemetry
    /// recordings and the telemetry session history.
    pub fn total(&self, assets_inside_cache: bool) -> Usage {
        let mut t = add(
            add(
                add(self.cache, self.area(Area::Backups)),
                self.area(Area::TelemetryRecordings),
            ),
            self.area(Area::TelemetrySessions),
        );
        if !assets_inside_cache {
            t = add(t, self.area(Area::AssetExports));
        }
        t
    }
}

/// Backups of every channel (`<data>/channels/*/*/backups`) and of the launch hooks.
fn backups(dirs: &AppDirs) -> Usage {
    let mut u = usage(&dirs.data.join("hooks").join("backups"));
    for (_, install) in entries(&dirs.data.join("channels")) {
        for (_, ch) in entries(&install) {
            u = add(u, usage(&crate::backup::backups_dir(&ch)));
        }
    }
    u
}

/// Measure every area. `assets` is the asset cache in use (its own folder may be outside
/// `<cache>`); only its files are counted for that area.
pub fn report(dirs: &AppDirs, assets: &AssetCache) -> Report {
    let mut game = Usage::default();
    let mut wtg = Usage::default();
    let mut uex = Usage::default();
    let mut temp = Usage::default();
    let mut tele = Usage::default();
    let mut other = Usage::default();
    for (name, p) in entries(&dirs.cache) {
        match name.as_str() {
            MCP => {
                for (n, q) in entries(&p) {
                    if n == UEX {
                        uex = add(uex, usage(&q));
                    } else {
                        wtg = add(wtg, usage(&q));
                    }
                }
            }
            WORK => temp = add(temp, usage(&p)),
            TELEMETRY => tele = add(tele, usage(&p)),
            ASSETS => other = add(other, usage(&p)),
            _ => game = add(game, usage(&p)),
        }
    }
    let cache = [game, wtg, uex, temp, tele, other]
        .into_iter()
        .fold(Usage::default(), add);
    let mcp = dirs.cache.join(MCP);
    let areas = vec![
        AreaUsage {
            area: Area::GameData,
            path: dirs.cache.clone(),
            usage: game,
        },
        AreaUsage {
            area: Area::WhereToGet,
            path: mcp.clone(),
            usage: wtg,
        },
        AreaUsage {
            area: Area::Uex,
            path: mcp.join(UEX),
            usage: uex,
        },
        AreaUsage {
            area: Area::AssetExports,
            path: assets.root().to_path_buf(),
            usage: assets.usage(),
        },
        AreaUsage {
            area: Area::Temporary,
            path: dirs.cache.join(WORK),
            usage: temp,
        },
        AreaUsage {
            area: Area::Telemetry,
            path: telemetry_dir(dirs),
            usage: tele,
        },
        AreaUsage {
            area: Area::TelemetryRecordings,
            path: telemetry_recordings_dir(dirs),
            usage: usage(&telemetry_recordings_dir(dirs)),
        },
        AreaUsage {
            area: Area::TelemetrySessions,
            path: telemetry_sessions_dir(dirs),
            usage: usage(&telemetry_sessions_dir(dirs)),
        },
        AreaUsage {
            area: Area::Backups,
            path: dirs.data.join("channels"),
            usage: backups(dirs),
        },
    ];
    Report {
        cache_path: dirs.cache.clone(),
        cache,
        areas,
    }
}

/// What [`enforce`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Enforced {
    pub cap: u64,
    pub before: u64,
    pub after: u64,
    /// Folders (for assets: the asset cache root) and the bytes freed there.
    pub removed: Vec<(PathBuf, u64)>,
}

/// A removable folder: (tier, last used, path).
type Candidate = (u8, SystemTime, PathBuf);

fn real_dir(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
}

/// Each channel folder's build folders, newest-used first.
fn by_channel(channel_dirs: impl Iterator<Item = PathBuf>) -> Vec<Vec<(SystemTime, PathBuf)>> {
    channel_dirs
        .filter(|d| real_dir(d))
        .map(|ch| {
            let mut v: Vec<(SystemTime, PathBuf)> = entries(&ch)
                .into_iter()
                .filter(|(n, p)| !n.contains(".tmp-") && real_dir(p))
                .map(|(_, p)| (last_used(&p), p))
                .collect();
            v.sort_by_key(|x| std::cmp::Reverse(x.0));
            v
        })
        .collect()
}

/// Bring `<cache>` under `cap` (see the module docs for the order and what is never
/// removed). `in_use`: folders the caller is using now (its build's entry or datasets).
pub fn enforce(cache: &Path, cap: u64, in_use: &[PathBuf]) -> Enforced {
    enforce_at(cache, cap, in_use, SystemTime::now())
}

/// [`enforce`] with the cap from `config.toml`.
pub fn enforce_configured(dirs: &AppDirs, in_use: &[PathBuf]) -> Enforced {
    let cap = configured_cap(&Config::load(dirs).unwrap_or_default());
    enforce(&dirs.cache, cap, in_use)
}

fn enforce_at(cache: &Path, cap: u64, in_use: &[PathBuf], now: SystemTime) -> Enforced {
    let before = usage(cache).bytes;
    let mut out = Enforced {
        cap,
        before,
        after: before,
        removed: Vec::new(),
    };
    if before <= cap || !real_dir(cache) {
        return out;
    }
    let mut total = before;
    // 1. Asset exports (only our files, under a root that holds the marker).
    let assets = AssetCache::new(cache.join(ASSETS), u64::MAX);
    let freed = assets.shrink(total - cap);
    if freed > 0 {
        total -= freed.min(total);
        out.removed.push((assets.root().to_path_buf(), freed));
    }
    // 2 and 3. Build folders.
    let mut cands: Vec<Candidate> = Vec::new();
    for (name, p) in entries(cache) {
        if !real_dir(&p) || [ASSETS, WORK, DUMPS, TELEMETRY].contains(&name.as_str()) {
            continue;
        }
        if name == MCP {
            let chans = entries(&p)
                .into_iter()
                .filter(|(n, _)| n != UEX)
                .map(|(_, q)| q);
            for builds in by_channel(chans) {
                for (i, (t, b)) in builds.into_iter().enumerate() {
                    cands.push((if i == 0 { 3 } else { 2 }, t, b));
                }
            }
        } else {
            // `<install>/<CHANNEL>/<key>`: never a channel's newest entry.
            let chans = entries(&p).into_iter().map(|(_, q)| q);
            for builds in by_channel(chans) {
                for (t, b) in builds.into_iter().skip(1) {
                    cands.push((2, t, b));
                }
            }
        }
    }
    cands.retain(|(_, t, p)| {
        now.duration_since(*t).unwrap_or_default() >= IN_USE
            && !in_use.iter().any(|u| u.starts_with(p) || p.starts_with(u))
    });
    cands.sort_by_key(|(tier, t, _)| (*tier, *t));
    for (_, _, p) in cands {
        if total <= cap {
            break;
        }
        let n = usage(&p).bytes;
        if std::fs::remove_dir_all(&p).is_ok() {
            total -= n.min(total);
            out.removed.push((p, n));
        }
    }
    out.after = total;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(p: &Path, n: usize) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, vec![1u8; n]).unwrap();
    }

    #[test]
    fn every_area_is_measured_on_its_own() {
        let t = tempfile::tempdir().unwrap();
        let dirs = AppDirs::under(t.path());
        put(&dirs.cache.join("6ae3d86ff920ccda/LIVE/k1/base.ini"), 100);
        put(&dirs.cache.join("dumps/LIVE/x.ini"), 5);
        put(&dirs.cache.join("mcp/LIVE/b-fp/game-ships.json"), 30);
        put(&dirs.cache.join("mcp/uex/uex-terminals.json"), 7);
        put(
            &dirs.cache.join("work/starorg-extract-abc/out/base.ini"),
            11,
        );
        put(
            &dirs
                .data
                .join("channels/6ae3d86ff920ccda/LIVE/backups/1-manual/global.ini"),
            40,
        );
        put(
            &dirs
                .data
                .join("channels/6ae3d86ff920ccda/LIVE/profile.toml"),
            999,
        );
        put(&dirs.data.join("hooks/backups/a.sh"), 2);
        put(&telemetry_dir(&dirs).join("sc-alpha-5.0.0-ptu-1.fds"), 9);
        put(
            &telemetry_recordings_dir(&dirs).join("20261123-120000-PTU.jsonl"),
            13,
        );
        put(&telemetry_sessions_dir(&dirs).join("history.json"), 4);
        let assets = AssetCache::new(crate::assetcache::default_dir(&dirs), 1 << 20);
        let b = "0000000000000000000000b1";
        assets
            .prepare("LIVE", b, crate::assetcache::Kind::Audio)
            .unwrap();
        put(
            &assets.path("LIVE", b, crate::assetcache::Kind::Audio, "1", "ogg"),
            50,
        );
        let r = report(&dirs, &assets);
        assert_eq!(r.area(Area::GameData).bytes, 105);
        assert_eq!(r.area(Area::WhereToGet).bytes, 30);
        assert_eq!(r.area(Area::Uex).bytes, 7);
        assert_eq!(r.area(Area::Temporary).bytes, 11);
        assert_eq!(r.area(Area::Telemetry).bytes, 9, "not game data");
        assert_eq!(r.area(Area::TelemetryRecordings).bytes, 13);
        assert_eq!(r.area(Area::TelemetrySessions).bytes, 4);
        assert_eq!(
            r.area(Area::AssetExports),
            Usage {
                bytes: 50,
                files: 1
            }
        );
        assert_eq!(
            r.area(Area::Backups).bytes,
            42,
            "profile.toml is not a backup"
        );
        // The cache folder holds the asset cache's marker file too.
        let marker = crate::assetcache::MARKER;
        let m = std::fs::metadata(assets.root().join(marker)).unwrap().len();
        assert_eq!(r.cache.bytes, 105 + 30 + 7 + 11 + 9 + 50 + m);
        assert_eq!(r.total(true).bytes, r.cache.bytes + 42 + 13 + 4);
        assert_eq!(r.total(false).bytes, r.cache.bytes + 42 + 13 + 4 + 50);
    }

    #[cfg(unix)]
    #[test]
    fn links_are_not_followed() {
        let t = tempfile::tempdir().unwrap();
        let outside = t.path().join("outside");
        put(&outside.join("big.bin"), 4096);
        let dirs = AppDirs::under(&t.path().join("app"));
        std::fs::create_dir_all(&dirs.cache).unwrap();
        std::os::unix::fs::symlink(&outside, dirs.cache.join("linked")).unwrap();
        let assets = AssetCache::new(crate::assetcache::default_dir(&dirs), 1);
        let r = report(&dirs, &assets);
        assert_eq!(r.cache, Usage::default());
    }

    fn aged(p: &Path, secs: u64) {
        let f = std::fs::File::options().write(true).open(p).unwrap();
        f.set_modified(SystemTime::now() - std::time::Duration::from_secs(secs))
            .unwrap();
    }

    /// A build folder with one file of `n` bytes, last used `secs` ago.
    fn build(dir: &Path, n: usize, secs: u64) -> PathBuf {
        let f = dir.join("data.json");
        put(&f, n);
        aged(&f, secs);
        dir.to_path_buf()
    }

    #[test]
    fn the_cap_evicts_assets_then_old_builds_then_other_channels() {
        let t = tempfile::tempdir().unwrap();
        let dirs = AppDirs::under(t.path());
        let c = &dirs.cache;
        // Assets: 300 bytes, an hour old.
        let assets = AssetCache::new(c.join(ASSETS), 1 << 20);
        let b = "0000000000000000000000b1";
        assets
            .prepare("LIVE", b, crate::assetcache::Kind::Audio)
            .unwrap();
        let a = assets.path("LIVE", b, crate::assetcache::Kind::Audio, "1", "ogg");
        put(&a, 300);
        aged(&a, 3600);
        // WHERE TO GET: LIVE has a current and an older build; PTU one build, used long ago.
        let live_new = build(&c.join("mcp/LIVE/4.1-aa"), 200, 600);
        let live_old = build(&c.join("mcp/LIVE/4.0-bb"), 200, 7200);
        let ptu = build(&c.join("mcp/PTU/4.2-cc"), 200, 9000);
        let uex = build(&c.join("mcp/uex"), 50, 99_000);
        // Game data: a channel's only (newest) entry always stays.
        let gd = build(&c.join("inst/LIVE/key1"), 400, 99_000);
        // Temp: never touched by the cap.
        let work = build(&c.join("work/starorg-extract-x"), 10, 99_000);
        let total = usage(c).bytes;
        // Under the cap: nothing.
        assert!(enforce(c, total, &[]).removed.is_empty());
        // Need 300: the assets go first, nothing else.
        let r = enforce(c, total - 300, &[]);
        assert_eq!(r.removed.len(), 1);
        assert!(!a.exists() && live_old.exists());
        // Need 200 more: the older LIVE build (not PTU's current one, though used longer ago).
        let total = usage(c).bytes;
        let r = enforce(c, total - 200, &[]);
        assert_eq!(r.removed, vec![(live_old.clone(), 200)]);
        assert!(ptu.exists());
        // Next the oldest-used channel's current build (PTU); PTU is in use here, so LIVE's.
        let total = usage(c).bytes;
        let r = enforce(c, total - 200, std::slice::from_ref(&ptu));
        assert_eq!(r.removed, vec![(live_new.clone(), 200)]);
        let r = enforce(c, 0, &[]);
        assert_eq!(r.removed, vec![(ptu.clone(), 200)]);
        assert!(r.after > 0, "the rest is never removed");
        assert!(gd.exists() && uex.exists() && work.exists());
    }

    #[test]
    fn recent_folders_and_backups_are_never_removed_by_the_cap() {
        let t = tempfile::tempdir().unwrap();
        let dirs = AppDirs::under(t.path());
        let c = &dirs.cache;
        let fresh = build(&c.join("mcp/LIVE/4.1-aa"), 200, 10);
        let fresh_old = build(&c.join("mcp/LIVE/4.0-bb"), 200, 20);
        let old_gd = build(&c.join("inst/LIVE/key0"), 200, 9000);
        let new_gd = build(&c.join("inst/LIVE/key1"), 200, 100);
        let bk = dirs
            .data
            .join("channels/inst/LIVE/backups/1-manual/global.ini.gz");
        put(&bk, 500);
        let r = enforce(c, 0, &[]);
        assert_eq!(
            r.removed,
            vec![(old_gd.clone(), 200)],
            "only the older entry"
        );
        assert!(fresh.exists() && fresh_old.exists() && new_gd.exists() && bk.exists());
        let mut cfg = Config::default();
        assert_eq!(configured_cap(&cfg), DEFAULT_CAP);
        cfg.extra = toml::from_str("[storage]\ncache_max_mb = 5\n").unwrap();
        assert_eq!(configured_cap(&cfg), 5 << 20);
    }

    #[test]
    fn nothing_there_is_zero() {
        let t = tempfile::tempdir().unwrap();
        let dirs = AppDirs::under(&t.path().join("none"));
        let assets = AssetCache::new(crate::assetcache::default_dir(&dirs), 1);
        let r = report(&dirs, &assets);
        assert_eq!(r.areas.len(), Area::ALL.len());
        assert_eq!(r.total(true), Usage::default());
    }
}

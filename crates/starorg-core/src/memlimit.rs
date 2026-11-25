//! Memory limit for the Developer mode and MCP worker processes (addendum 7.18).
//!
//! The explorer (`dev-worker`), the asset workers (`asset-worker`, one fresh per model export)
//! and the MCP extraction worker (`mcp-extract-worker`) can each take several GB. This module
//! holds what the app, the CLI and the workers share:
//!
//! - **The setting**: `[dev] memory_limit_gb` in `config.toml`; the slider runs from
//!   [`MIN_GB`] to total RAM minus [`RESERVE_GB`] in [`STEP_GB`] steps, default 5 GB or half
//!   the RAM when that is less ([`bounds`]). The CLI's `--memory-limit` and
//!   [`ENV_LIMIT_GB`] override it ([`effective_limit`]).
//! - **Estimates** of the measured peaks ([`estimates`], [`dataset_estimate`],
//!   [`model_estimate`]) and the soft guard before a start ([`check`]). The guard and every
//!   "fits" compare with [`stop_at`] (90% of the limit), the point where the watchdog stops a
//!   worker, so a job that passes the guard is not then stopped.
//! - **The guard while it runs** (primary): the limit travels to the child in [`ENV_CHILD`]
//!   ([`apply`]); the child calls [`arm_from_env`] first thing, which starts a watchdog
//!   thread that stops the process cleanly (exit [`EXIT_MEMORY`], our own message) at
//!   [`STOP_PERCENT`] of the limit, measured every 25 ms as anonymous memory (Linux
//!   `RssAnon` + `VmSwap`; the archive is read with `pread` into the page cache, which is not
//!   counted) or every 100 ms as committed private bytes (Windows).
//! - **The hard cap** (last resort), with [`headroom`] above the limit (10%, at least
//!   512 MB) so the watchdog always acts first: on Linux the child lowers its own
//!   `RLIMIT_DATA` (an allocation past it fails and the worker aborts; no kernel OOM kill,
//!   so no desktop "out of memory" notification). Workers are made non-dumpable
//!   (`PR_SET_DUMPABLE` 0), so such an abort is never handed to systemd-coredump and no crash
//!   reporter pops up (`RLIMIT_CORE=0` would not stop a core-pattern pipe). Not `RLIMIT_AS`:
//!   thread stacks and allocator arenas reserve far more address space than they use. A
//!   cgroup v2 `memory.max` (`systemd-run --user --scope -p MemoryMax=...`) is opt-in with
//!   [`ENV_CGROUP`]: it also caps page cache and child processes, but a hit is a kernel OOM
//!   kill, which KDE and GNOME announce with a system notification. Windows has no hard cap:
//!   a Job Object with a process memory limit needs `unsafe` FFI, which the workspace
//!   forbids.
//! - **The verdict** on a child that died ([`exit_was_memory`], [`stopped_message`],
//!   [`unexpected_message`]).
//!
//! Normal setup and apply (the `extract-worker`) are never limited.

use crate::config::Config;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

pub const GIB: u64 = 1024 * 1024 * 1024;
/// Slider bounds and step, in GB (GiB). The minimum leaves the workers' baseline (0.92-0.94 GB
/// while the archive index is built) under the watchdog's 90%.
pub const MIN_GB: f64 = 1.5;
pub const STEP_GB: f64 = 0.5;
/// Left for the system and the game when the slider is at its top.
pub const RESERVE_GB: f64 = 2.0;
/// The default when the machine has at least twice this much. 5 GB, not 4: the MCP `ships`
/// extraction peaks at 3.9-4.1 GB, above the watchdog's 90% of 4 GB.
pub const DEFAULT_GB: f64 = 5.0;
/// Top of the slider when the total RAM cannot be read.
pub const UNKNOWN_MAX_GB: f64 = 64.0;
/// The CLI's environment override (`STARORG_MEMORY_LIMIT_GB=6`; `0` = no limit).
pub const ENV_LIMIT_GB: &str = "STARORG_MEMORY_LIMIT_GB";
/// Parent -> child: the limit in bytes. Set by [`apply`]; read by [`arm_from_env`].
pub const ENV_CHILD: &str = "STARORG_WORKER_MEMORY_LIMIT";
/// Set to `1` to hold the hard cap with a cgroup (`systemd-run --user --scope`) instead of
/// `RLIMIT_DATA` (Linux; off by default: a hit is a kernel OOM kill with a desktop
/// notification).
pub const ENV_CGROUP: &str = "STARORG_MEMORY_CGROUP";
/// A worker stopped by its own watchdog exits with this code.
pub const EXIT_MEMORY: i32 = 7;
/// The watchdog stops a worker at this share of the limit (percent).
pub const STOP_PERCENT: u64 = 90;
/// How often the watchdog looks (Windows: a `sysinfo` refresh per look, so less often).
pub const POLL: Duration = if cfg!(windows) {
    Duration::from_millis(100)
} else {
    Duration::from_millis(25)
};
/// The watchdog's stderr line starts with this.
pub const MARKER: &str = "memory-limit:";

const TABLE: &str = "dev";
const KEY: &str = "memory_limit_gb";

// ---------------------------------------------------------------------------------------
// Machine memory
// ---------------------------------------------------------------------------------------

/// Total and currently available RAM, in bytes (`None` when it cannot be read).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemInfo {
    pub total: Option<u64>,
    pub available: Option<u64>,
}

/// `MemTotal` and `MemAvailable` of a `/proc/meminfo` text.
pub fn parse_meminfo(text: &str) -> MemInfo {
    let field = |name: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(name))
            .and_then(|v| v.trim().strip_suffix("kB"))
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(|kb| kb * 1024)
    };
    MemInfo {
        total: field("MemTotal:"),
        available: field("MemAvailable:"),
    }
}

/// This machine's RAM now: Linux `/proc/meminfo`, Windows `GlobalMemoryStatusEx` (through
/// `sysinfo`).
pub fn meminfo() -> MemInfo {
    #[cfg(windows)]
    {
        use sysinfo::{MemoryRefreshKind, System};
        let Ok(mut sys) = System::new() else {
            return MemInfo::default();
        };
        sys.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
        let t = sys.total_memory();
        MemInfo {
            total: (t > 0).then_some(t),
            available: (t > 0).then_some(sys.available_memory()),
        }
    }
    #[cfg(not(windows))]
    {
        std::fs::read_to_string("/proc/meminfo")
            .map(|t| parse_meminfo(&t))
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------------------
// The setting
// ---------------------------------------------------------------------------------------

/// The slider: smallest, largest, step and default, in GB.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub min_gb: f64,
    pub max_gb: f64,
    pub step_gb: f64,
    pub default_gb: f64,
}

fn floor_step(gb: f64) -> f64 {
    (gb / STEP_GB).floor() * STEP_GB
}

/// The slider for a machine with `total` bytes of RAM: 1.5 GB .. total - 2 GB in 0.5 GB
/// steps, default 5 GB or half the RAM when that is less (never below 1.5 GB; see
/// [`low_ram_note`] for machines where that is most of the RAM).
pub fn bounds(total: Option<u64>) -> Bounds {
    let max_gb = match total {
        Some(t) => floor_step(t as f64 / GIB as f64 - RESERVE_GB).max(MIN_GB),
        None => UNKNOWN_MAX_GB,
    };
    let half = total.map_or(DEFAULT_GB, |t| floor_step(t as f64 / GIB as f64 / 2.0));
    let default_gb = DEFAULT_GB.min(half).clamp(MIN_GB, max_gb);
    Bounds {
        min_gb: MIN_GB,
        max_gb,
        step_gb: STEP_GB,
        default_gb,
    }
}

/// Shown with the slider when the machine has less than [`MIN_GB`] + [`RESERVE_GB`] of RAM:
/// the workers then compete with the system and the game for most of it.
pub fn low_ram_note(total: Option<u64>) -> Option<String> {
    let t = total?;
    (t < gb_bytes(MIN_GB + RESERVE_GB)).then(|| {
        format!(
            "This computer has {} of RAM. Each Developer mode or MCP worker needs at least about 1 GB, so the limit cannot go below {}; close the game and other programs before using them, or expect them to stop.",
            human(t),
            human(gb_bytes(MIN_GB))
        )
    })
}

impl Bounds {
    /// `gb` rounded to the nearest step and kept inside the slider.
    pub fn clamp(&self, gb: f64) -> f64 {
        if !gb.is_finite() {
            return self.default_gb;
        }
        ((gb / self.step_gb).round() * self.step_gb).clamp(self.min_gb, self.max_gb)
    }
}

fn dev_table(cfg: &Config) -> Option<&toml::Table> {
    cfg.extra.get(TABLE)?.as_table()
}

/// `[dev] memory_limit_gb`, when set to a positive number. Not clamped: a hand-edited value
/// below the slider's minimum is honoured (tests use that).
pub fn configured_gb(cfg: &Config) -> Option<f64> {
    let v = dev_table(cfg)?.get(KEY)?;
    let gb = v.as_float().or_else(|| v.as_integer().map(|i| i as f64))?;
    (gb.is_finite() && gb > 0.0).then_some(gb)
}

/// Set (`Some`) or reset to the default (`None`) `[dev] memory_limit_gb`.
pub fn set_configured_gb(cfg: &mut Config, gb: Option<f64>) {
    let t = cfg
        .extra
        .entry(TABLE)
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if !t.is_table() {
        *t = toml::Value::Table(toml::Table::new());
    }
    let t = t.as_table_mut().expect("table");
    match gb {
        Some(g) => {
            t.insert(KEY.into(), toml::Value::Float(g));
        }
        None => {
            t.remove(KEY);
        }
    }
    if t.is_empty() {
        cfg.extra.remove(TABLE);
    }
}

/// The limit in GB the app uses: the setting, else the default for this machine.
pub fn limit_gb(cfg: &Config, total: Option<u64>) -> f64 {
    configured_gb(cfg).unwrap_or_else(|| bounds(total).default_gb)
}

/// A `--memory-limit` / [`ENV_LIMIT_GB`] value: GB, `0` or `off` = no limit.
pub fn parse_gb(s: &str) -> Result<Option<f64>, String> {
    let t = s.trim().trim_end_matches(['G', 'g', 'B', 'b']).trim();
    if t.eq_ignore_ascii_case("off") || t.eq_ignore_ascii_case("none") {
        return Ok(None);
    }
    match t.parse::<f64>() {
        Ok(0.0) => Ok(None),
        Ok(g) if g.is_finite() && g > 0.0 => Ok(Some(g)),
        _ => Err(format!(
            "memory limit {s:?} is not a number of GB (for example 4 or 2.5; 0 = no limit)"
        )),
    }
}

/// `None` for a missing or empty value (`STARORG_MEMORY_LIMIT_GB=` is unset).
fn set_value(v: Option<&str>) -> Option<&str> {
    v.filter(|e| !e.trim().is_empty())
}

/// The CLI's limit in bytes: `flag` (`--memory-limit`), else [`ENV_LIMIT_GB`], else the
/// setting, else the default. `None` = no limit (`0`).
pub fn effective_limit(
    flag: Option<&str>,
    env: Option<&str>,
    cfg: &Config,
    total: Option<u64>,
) -> Result<Option<u64>, String> {
    let gb = match set_value(flag).or(set_value(env)) {
        Some(s) => parse_gb(s)?,
        None => Some(limit_gb(cfg, total)),
    };
    Ok(gb.map(gb_bytes))
}

pub fn gb_bytes(gb: f64) -> u64 {
    (gb * GIB as f64).round() as u64
}

/// "1.5 GB", "4 GB", "0.9 GB", "64 MB" (MB below 0.1 GB).
pub fn human(bytes: u64) -> String {
    if bytes < GIB / 10 {
        return format!("{} MB", (bytes as f64 / (1024.0 * 1024.0)).round() as u64);
    }
    let gb = bytes as f64 / GIB as f64;
    let s = format!("{gb:.1}");
    format!("{} GB", s.strip_suffix(".0").unwrap_or(&s))
}

// ---------------------------------------------------------------------------------------
// Estimates (measured on LIVE 4.10, addendum 7.16 and docs/mcp.md)
// ---------------------------------------------------------------------------------------

const fn gb10(tenths: u64) -> u64 {
    tenths * GIB / 10
}

/// The explorer worker: about 0.92 GB while it starts (archive index), 0.5 GB after.
pub const EXPLORER: u64 = gb10(10);
/// The shared asset worker (Files, previews, sounds): the archive index, 0.9 GB.
pub const ASSET_WORKER: u64 = gb10(9);

/// One measured peak the Settings slider lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Estimate {
    pub id: &'static str,
    pub label: &'static str,
    pub bytes: u64,
}

/// The measured peaks the slider shows, smallest first.
pub fn estimates() -> Vec<Estimate> {
    let e = |id, label, bytes| Estimate { id, label, bytes };
    vec![
        e("files", "Files and sounds (archive index)", ASSET_WORKER),
        e("explorer", "Explorer (DataCore records)", EXPLORER),
        e(
            "model-print",
            "Model export, Print shell (capital ships 1.8 GB)",
            gb10(14),
        ),
        e(
            "mcp-dataset",
            "MCP: most datasets (item-tags 1.2 GB, armor 0.9 GB)",
            gb10(15),
        ),
        e(
            "mcp-npc-loadouts",
            "MCP: NPC worn gear (Loot screen, Read game data step 5)",
            gb10(19),
        ),
        e("mcp-ship-components", "MCP: ship components", gb10(22)),
        e("mcp-ships", "MCP: ships", gb10(41)),
        e(
            "model-full",
            "Model export, Full (textures), most ships",
            gb10(70),
        ),
        e(
            "model-full-capital",
            "Model export, Full, capital ships (Idris)",
            gb10(110),
        ),
    ]
}

/// One MCP dataset's extraction peak (pruned domains; docs/mcp.md).
pub fn dataset_estimate(dataset: &str) -> u64 {
    let d = dataset.strip_prefix("game-").unwrap_or(dataset);
    match d {
        "ships" => gb10(41),
        "ship-components" => gb10(22),
        "item-sources" => gb10(15),
        "blueprints" => gb10(14),
        "item-tags" => gb10(12),
        "armor" => gb10(9),
        // Pruned items and actors plus the archive index (loadout files): 1.9 GB measured.
        "npc-loadouts" => gb10(19),
        // The string table reads global.ini only (the archive index is the peak).
        "strings-english" => gb10(9),
        // Every other domain: DataCore plus a little (0.85 to 1.3 GB).
        _ => gb10(13),
    }
}

/// Ships whose Full export is the biggest measured (Idris 10.9 GB at LOD 1); a name holding
/// one of these (case-insensitive) is estimated as one.
const CAPITAL: &[&str] = &[
    "idris",
    "polaris",
    "kraken",
    "bengal",
    "nautilus",
    "pioneer",
    "endeavor",
    "890jump",
    "carrack",
    "reclaimer",
    "hull_c",
    "hull_d",
    "hull_e",
    "orion",
    "liberator",
    "perseus",
    "hammerhead",
    "ironclad",
    "galaxy",
    "merchantman",
];

pub fn is_capital(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    CAPITAL.iter().any(|c| n.contains(c))
}

/// A model export's peak: Print shell about 1.4 GB at LOD 0 (Idris 1.75 GB) and 1.2 GB at
/// LOD 1-4 (LIVE 4.10: Gladius and Javelin 0.98-1.06 GB), capital ships 1.8 / 1.6 GB (Carrack
/// and 890 Jump 1.41-1.43 GB at LOD 1, 1.65-1.71 GB at LOD 0), Full about
/// 6.4-6.9 GB (Javelin, LOD 1 / 0), capital ships about 11 GB (Idris 10.9 GB at LOD 1).
pub fn model_estimate(full: bool, lod: u32, name: &str) -> u64 {
    match (full, is_capital(name)) {
        (false, false) if lod == 0 => gb10(14),
        (false, false) => gb10(12),
        (false, true) if lod == 0 => gb10(18),
        (false, true) => gb10(16),
        (true, true) => gb10(110),
        (true, false) if lod == 0 => gb10(70),
        (true, false) => gb10(65),
    }
}

// ---------------------------------------------------------------------------------------
// The soft guard
// ---------------------------------------------------------------------------------------

/// What [`check`] says about starting something that needs `need` bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    Ok,
    /// More than the limit: refused (the hard cap would stop it anyway).
    OverLimit {
        need: u64,
        limit: u64,
    },
    /// Under the limit, but more than is free now: ask first ("Run anyway").
    LowFree {
        need: u64,
        free: u64,
    },
}

/// Where the watchdog stops a worker under `limit`: [`STOP_PERCENT`] of it. The guard and
/// every "fits" use this, not the limit itself, so a job that passes is not then stopped.
pub fn stop_at(limit: u64) -> u64 {
    limit / 100 * STOP_PERCENT
}

/// The smallest slider value (GB, a [`STEP_GB`] multiple) under which `need` fits.
pub fn limit_for(need: u64) -> f64 {
    let gb = need as f64 * 100.0 / STOP_PERCENT as f64 / GIB as f64;
    let mut l = ((gb / STEP_GB).ceil() * STEP_GB).max(MIN_GB);
    while stop_at(gb_bytes(l)) < need {
        l += STEP_GB;
    }
    l
}

/// Whether `need` fits under `limit` (what the watchdog allows).
pub fn fits(need: u64, limit: u64) -> bool {
    need <= stop_at(limit)
}

pub fn check(need: u64, limit: Option<u64>, available: Option<u64>) -> Fit {
    if let Some(l) = limit
        && !fits(need, l)
    {
        return Fit::OverLimit { need, limit: l };
    }
    if let Some(f) = available
        && need > f
    {
        return Fit::LowFree { need, free: f };
    }
    Fit::Ok
}

/// "Exporting this model needs about 7 GB, more than the 5 GB memory limit allows. Set it to
/// at least 8 GB ..."
pub fn over_limit_message(what: &str, need: u64, limit: u64) -> String {
    let to = limit_for(need);
    format!(
        "{what} needs about {}, more than the {} memory limit allows (a worker is stopped at {}% of it). Set the limit to at least {} (Settings > Advanced > Memory limit; command line: --memory-limit {} or STARORG_MEMORY_LIMIT_GB={}), or pick a smaller dataset or export.",
        human(need),
        human(limit),
        STOP_PERCENT,
        human(gb_bytes(to)),
        to,
        to
    )
}

pub fn low_free_message(what: &str, need: u64, free: u64) -> String {
    format!(
        "{what} may need about {}, and only {} of memory is free now (is the game running?). Close something first, or run it anyway: the memory limit still applies.",
        human(need),
        human(free)
    )
}

const RAISE: &str = "Raise the limit (Settings > Advanced > Memory limit; CLI --memory-limit) or pick a smaller dataset or export.";

/// What a worker stopped at its limit is reported as.
pub fn stopped_message(limit: u64) -> String {
    format!("Stopped: needed more than {}. {RAISE}", human(limit))
}

// ---------------------------------------------------------------------------------------
// Parent side: start a child under the cap
// ---------------------------------------------------------------------------------------

/// How the hard cap is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hard {
    /// cgroup v2 `memory.max` of a transient systemd scope.
    Cgroup,
    /// `RLIMIT_DATA` of the process.
    Rlimit,
    /// Only the watchdog (Windows; or neither of the above worked).
    Watchdog,
}

impl Hard {
    pub fn label(self) -> &'static str {
        match self {
            Hard::Cgroup => "cgroup memory.max",
            Hard::Rlimit => "RLIMIT_DATA",
            Hard::Watchdog => "watchdog only",
        }
    }
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

/// Run `cmd` to completion within `limit`, killed after that; true when it exited 0.
fn run_quick(mut cmd: Command, limit: Duration) -> bool {
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let Ok(mut child) = cmd.spawn() else {
        return false;
    };
    let until = Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(Some(s)) => return s.success(),
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

/// The hard cap's distance above the limit: 10%, at least 512 MB, so the watchdog (at
/// [`STOP_PERCENT`]) always acts first and the hard cap is a last resort.
pub fn headroom(limit: u64) -> u64 {
    (limit / 10).max(512 * 1024 * 1024)
}

/// Whether [`ENV_CGROUP`] asks for the cgroup cap.
pub fn cgroup_wanted() -> bool {
    std::env::var(ENV_CGROUP).is_ok_and(|v| v.trim() == "1")
}

/// `systemd-run`, when [`ENV_CGROUP`] asks for it and a user systemd answers a test scope
/// (once per process; a few tens of ms). `None` off Linux or in a container without a
/// user bus.
pub fn systemd_run() -> Option<&'static Path> {
    static FOUND: OnceLock<Option<PathBuf>> = OnceLock::new();
    if !cfg!(target_os = "linux") || !cgroup_wanted() {
        return None;
    }
    FOUND
        .get_or_init(|| {
            let p = find_in_path("systemd-run")?;
            let mut c = Command::new(&p);
            c.args([
                "--user",
                "--scope",
                "--quiet",
                "--collect",
                "-p",
                "MemoryMax=268435456",
                "--",
                "true",
            ]);
            run_quick(c, Duration::from_secs(5)).then_some(p)
        })
        .as_deref()
}

/// `cmd` set up to run under `limit` bytes (see the module docs); unchanged for `None`.
/// Set stdio after this call; everything else of `cmd` (arguments, environment, folder) is
/// kept.
pub fn apply(cmd: Command, limit: Option<u64>) -> Command {
    apply_via(cmd, limit, systemd_run())
}

/// [`apply`] without the cgroup, whatever [`ENV_CGROUP`] says: the child holds the cap
/// itself (watchdog and `RLIMIT_DATA`).
pub fn apply_in_process(cmd: Command, limit: Option<u64>) -> Command {
    apply_via(cmd, limit, None)
}

/// [`apply`] with an explicit `systemd-run` (tests of the cgroup path).
pub fn apply_via(cmd: Command, limit: Option<u64>, scope: Option<&Path>) -> Command {
    let Some(limit) = limit else { return cmd };
    let mut out = match scope {
        Some(sr) => {
            let mut c = Command::new(sr);
            c.args(["--user", "--scope", "--quiet", "--collect", "-p"])
                .arg(format!("MemoryMax={}", limit + headroom(limit)))
                .args(["-p", "MemorySwapMax=0", "--"])
                .arg(cmd.get_program())
                .args(cmd.get_args());
            for (k, v) in cmd.get_envs() {
                match v {
                    Some(v) => c.env(k, v),
                    None => c.env_remove(k),
                };
            }
            if let Some(d) = cmd.get_current_dir() {
                c.current_dir(d);
            }
            c
        }
        None => cmd,
    };
    out.env(ENV_CHILD, limit.to_string());
    out
}

/// The limit [`apply`] put on `cmd`, if any.
pub fn command_limit(cmd: &Command) -> Option<u64> {
    cmd.get_envs()
        .find(|(k, _)| *k == ENV_CHILD)
        .and_then(|(_, v)| v)
        .and_then(|v| v.to_str())
        .and_then(|v| v.parse().ok())
}

/// Whether a child started under `limit` that ended with `status` (and that we did not stop
/// ourselves) was stopped for memory: its watchdog's [`EXIT_MEMORY`], or, past the hard cap,
/// Rust's "memory allocation of N bytes failed" in its `stderr` when the caller kept it. Any
/// other abort or kill is not called a memory stop (see [`unexpected_message`]).
pub fn exit_was_memory(status: &ExitStatus, limit: Option<u64>, stderr: Option<&str>) -> bool {
    if limit.is_none() || status.success() {
        return false;
    }
    status.code() == Some(EXIT_MEMORY)
        || stderr.is_some_and(|e| e.contains("memory allocation of") && e.contains("failed"))
}

/// A worker that ended by itself for no reason we can name.
pub fn unexpected_message(what: &str, status: &ExitStatus) -> String {
    format!("{what} stopped unexpectedly ({status})")
}

// ---------------------------------------------------------------------------------------
// Child side: hold the cap
// ---------------------------------------------------------------------------------------

/// The cap a process armed itself with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Armed {
    pub limit: u64,
    pub hard: Hard,
}

/// This process's cgroup v2 `memory.max`, when one is set (Linux).
pub fn cgroup_memory_max() -> Option<u64> {
    let cg = std::fs::read_to_string("/proc/self/cgroup").ok()?;
    let rel = cg.lines().find_map(|l| l.strip_prefix("0::"))?.trim();
    let p = Path::new("/sys/fs/cgroup").join(rel.trim_start_matches('/'));
    let v = std::fs::read_to_string(p.join("memory.max")).ok()?;
    v.trim().parse().ok()
}

/// `RssAnon` + `VmSwap` of a `/proc/<pid>/status` text: anonymous memory, resident or
/// swapped out (file pages of the archive read are not counted).
pub fn parse_usage(status: &str) -> Option<u64> {
    let kb = |name: &str| {
        status
            .lines()
            .find_map(|l| l.strip_prefix(name))
            .and_then(|v| v.trim().strip_suffix("kB"))
            .and_then(|v| v.trim().parse::<u64>().ok())
    };
    let anon = kb("RssAnon:")?;
    Some((anon + kb("VmSwap:").unwrap_or(0)) * 1024)
}

/// Reads this process's memory as the watchdog counts it: Linux [`parse_usage`] of
/// `/proc/self/status`, Windows committed private bytes (one `sysinfo` instance, refreshing
/// only this process).
pub struct UsageProbe {
    #[cfg(windows)]
    sys: Option<(sysinfo::System, sysinfo::Pid)>,
}

impl UsageProbe {
    pub fn new() -> Self {
        Self {
            #[cfg(windows)]
            sys: sysinfo::get_current_pid()
                .ok()
                .and_then(|pid| sysinfo::System::new().ok().map(|s| (s, pid))),
        }
    }

    pub fn read(&mut self) -> Option<u64> {
        #[cfg(windows)]
        {
            use sysinfo::{ProcessRefreshKind, ProcessesToUpdate};
            let (sys, pid) = self.sys.as_mut()?;
            sys.refresh_processes_specifics(
                ProcessesToUpdate::Some(&[*pid]),
                false,
                ProcessRefreshKind::nothing().with_memory(),
            );
            sys.process(*pid).map(|p| p.virtual_memory())
        }
        #[cfg(not(windows))]
        {
            parse_usage(&std::fs::read_to_string("/proc/self/status").ok()?)
        }
    }
}

impl Default for UsageProbe {
    fn default() -> Self {
        Self::new()
    }
}

/// One reading of [`UsageProbe`].
pub fn own_usage() -> Option<u64> {
    UsageProbe::new().read()
}

#[cfg(target_os = "linux")]
fn set_rlimit_data(limit: u64) -> bool {
    use rustix::process::{
        DumpableBehavior, Resource, Rlimit, getrlimit, set_dumpable_behavior, setrlimit,
    };
    // An abort at the hard cap leaves no core dump: a non-dumpable process is never handed to
    // the core pattern (systemd-coredump), so no crash reporter notification. RLIMIT_CORE=0
    // alone would not stop a pipe handler.
    let _ = set_dumpable_behavior(DumpableBehavior::NotDumpable);
    let cur = getrlimit(Resource::Data);
    let max = cur.maximum;
    let soft = max.map_or(limit, |m| m.min(limit));
    setrlimit(
        Resource::Data,
        Rlimit {
            current: Some(soft),
            maximum: max,
        },
    )
    .is_ok()
}

#[cfg(not(target_os = "linux"))]
fn set_rlimit_data(_limit: u64) -> bool {
    false
}

/// The hard part of [`arm`], [`headroom`] above the limit: the cgroup when it already holds
/// us there, else `RLIMIT_DATA` (Linux), else nothing.
fn hard_cap(limit: u64) -> Hard {
    let cap = limit + headroom(limit);
    if cfg!(target_os = "linux") {
        if cgroup_memory_max().is_some_and(|m| m <= cap) {
            return Hard::Cgroup;
        }
        if set_rlimit_data(cap) {
            return Hard::Rlimit;
        }
    }
    Hard::Watchdog
}

/// Called when the watchdog stops the process, before it exits.
pub type OnStop = Box<dyn Fn(&str) + Send + 'static>;

/// Hold this process to `limit` bytes: the hard cap ([`headroom`] above), then the watchdog
/// thread, which calls
/// `on_stop` with [`stopped_message`] and exits with `exit_code` at [`STOP_PERCENT`] of the
/// limit.
pub fn arm(limit: u64, exit_code: i32, on_stop: OnStop) -> Armed {
    let hard = hard_cap(limit);
    let stop_at = stop_at(limit);
    // The first look happens here, before the caller starts its work: a worker that is over
    // the limit from the start (or gets ready within one POLL, as optimised builds do) is
    // stopped, not reported ready before the thread's first look.
    let mut probe = UsageProbe::new();
    if probe.read().is_some_and(|u| u >= stop_at) {
        on_stop(&stopped_message(limit));
        std::process::exit(exit_code);
    }
    let _ = std::thread::Builder::new()
        .name("memory-watchdog".into())
        .spawn(move || {
            loop {
                std::thread::sleep(POLL);
                if probe.read().is_some_and(|u| u >= stop_at) {
                    on_stop(&stopped_message(limit));
                    std::process::exit(exit_code);
                }
            }
        });
    Armed { limit, hard }
}

/// For a worker: arm from [`ENV_CHILD`] when the parent set it, with a [`MARKER`] line on
/// stderr at the start and when stopping, exit [`EXIT_MEMORY`].
pub fn arm_from_env() -> Option<Armed> {
    let limit: u64 = std::env::var(ENV_CHILD).ok()?.trim().parse().ok()?;
    if limit == 0 {
        return None;
    }
    let a = arm(limit, EXIT_MEMORY, Box::new(|m| eprintln!("{MARKER} {m}")));
    eprintln!("{MARKER} {} ({})", human(limit), a.hard.label());
    Some(a)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_with(gb: Option<f64>) -> Config {
        let mut c = Config::default();
        set_configured_gb(&mut c, gb);
        c
    }

    #[test]
    fn slider_bounds() {
        let b = bounds(Some(32 * GIB));
        assert_eq!(
            (b.min_gb, b.max_gb, b.step_gb, b.default_gb),
            (1.5, 30.0, 0.5, 5.0)
        );
        // Half the RAM when that is less than 5 GB.
        let b = bounds(Some(6 * GIB));
        assert_eq!((b.max_gb, b.default_gb), (4.0, 3.0));
        // 7.6 GB usable: max floors to a step.
        let b = bounds(Some(gb_bytes(7.6)));
        assert_eq!((b.max_gb, b.default_gb), (5.5, 3.5));
        // Tiny machines: never below 1.5 GB, with a note.
        let b = bounds(Some(2 * GIB));
        assert_eq!((b.max_gb, b.default_gb), (1.5, 1.5));
        assert!(low_ram_note(Some(2 * GIB)).unwrap().contains("2 GB of RAM"));
        assert_eq!(low_ram_note(Some(8 * GIB)), None);
        assert_eq!(low_ram_note(None), None);
        let b = bounds(None);
        assert_eq!((b.max_gb, b.default_gb), (UNKNOWN_MAX_GB, 5.0));
        let b = bounds(Some(16 * GIB));
        assert_eq!(b.clamp(3.2), 3.0);
        assert_eq!(b.clamp(3.3), 3.5);
        assert_eq!(b.clamp(0.1), 1.5);
        assert_eq!(b.clamp(99.0), 14.0);
        assert_eq!(b.clamp(f64::NAN), 5.0);
    }

    /// One source of truth: whatever the guard lets through is under the watchdog's stop,
    /// and `limit_for` always gives a limit the job passes.
    #[test]
    fn the_guard_never_passes_what_the_watchdog_stops() {
        let mut need = 50 * 1024 * 1024;
        while need < 20 * GIB {
            for tenth in 10..=300 {
                let limit = GIB / 10 * tenth;
                if check(need, Some(limit), None) == Fit::Ok {
                    assert!(need <= stop_at(limit), "{need} at {limit}");
                    assert!(fits(need, limit));
                } else {
                    assert!(need > stop_at(limit));
                }
            }
            let l = limit_for(need);
            assert_eq!(check(need, Some(gb_bytes(l)), None), Fit::Ok, "{need}");
            if l > MIN_GB {
                assert_ne!(
                    check(need, Some(gb_bytes(l - STEP_GB)), None),
                    Fit::Ok,
                    "{need}: {l} is not the smallest"
                );
            }
            need = need * 23 / 20;
        }
    }

    #[test]
    fn usage_counts_anonymous_and_swapped() {
        let s = "VmRSS:\t 900 kB\nRssAnon:\t 600 kB\nRssFile:\t 300 kB\nVmSwap:\t 100 kB\n";
        assert_eq!(parse_usage(s), Some(700 * 1024));
        assert_eq!(parse_usage("RssAnon: 5 kB\n"), Some(5 * 1024));
        assert_eq!(parse_usage("nothing"), None);
    }

    #[test]
    fn config_round_trip() {
        let mut c = cfg_with(Some(6.5));
        assert_eq!(configured_gb(&c), Some(6.5));
        let text = toml::to_string(&c).unwrap();
        assert!(
            text.contains("[dev]") && text.contains("memory_limit_gb = 6.5"),
            "{text}"
        );
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(configured_gb(&back), Some(6.5));
        // An integer by hand works too; zero, negative and junk do not count.
        let hand: Config = toml::from_str("[dev]\nmemory_limit_gb = 3\n").unwrap();
        assert_eq!(configured_gb(&hand), Some(3.0));
        for bad in ["0", "-1", "\"lots\""] {
            let c: Config = toml::from_str(&format!("[dev]\nmemory_limit_gb = {bad}\n")).unwrap();
            assert_eq!(configured_gb(&c), None, "{bad}");
        }
        // Reset keeps other [dev] keys and drops an empty table.
        let mut both: Config =
            toml::from_str("[dev]\nmemory_limit_gb = 2.5\nasset_cache_max_mb = 100\n").unwrap();
        set_configured_gb(&mut both, None);
        assert!(
            toml::to_string(&both)
                .unwrap()
                .contains("asset_cache_max_mb")
        );
        set_configured_gb(&mut c, None);
        assert!(c.extra.is_empty());
        assert_eq!(limit_gb(&c, Some(32 * GIB)), 5.0);
        assert_eq!(limit_gb(&cfg_with(Some(9.0)), Some(32 * GIB)), 9.0);
    }

    #[test]
    fn cli_limit_resolution() {
        let c = cfg_with(Some(6.0));
        let t = Some(32 * GIB);
        assert_eq!(
            effective_limit(Some("2.5"), Some("8"), &c, t),
            Ok(Some(gb_bytes(2.5)))
        );
        assert_eq!(effective_limit(None, Some("8"), &c, t), Ok(Some(8 * GIB)));
        assert_eq!(effective_limit(None, Some(" "), &c, t), Ok(Some(6 * GIB)));
        // An empty value (STARORG_MEMORY_LIMIT_GB=) is unset, not an error.
        assert_eq!(
            effective_limit(Some(""), Some(""), &c, t),
            Ok(Some(6 * GIB))
        );
        assert_eq!(effective_limit(None, None, &c, t), Ok(Some(6 * GIB)));
        assert_eq!(
            effective_limit(None, None, &Config::default(), t),
            Ok(Some(5 * GIB))
        );
        assert_eq!(effective_limit(Some("0"), None, &c, t), Ok(None));
        assert_eq!(effective_limit(Some("off"), None, &c, t), Ok(None));
        assert_eq!(parse_gb("4G"), Ok(Some(4.0)));
        assert_eq!(parse_gb("1.5gb"), Ok(Some(1.5)));
        assert!(parse_gb("-2").is_err() && parse_gb("lots").is_err());
    }

    #[test]
    fn estimates_and_guard() {
        assert_eq!(dataset_estimate("game-ships"), gb10(41));
        assert_eq!(dataset_estimate("game-item-tags"), gb10(12));
        assert_eq!(dataset_estimate("game-missions"), gb10(13));
        assert_eq!(dataset_estimate("game-npc-loadouts"), gb10(19));
        assert_eq!(
            model_estimate(true, 1, "EntityClassDefinition.AEGS_Idris_M"),
            gb10(110)
        );
        assert_eq!(model_estimate(true, 0, "AEGS_Javelin"), gb10(70));
        assert_eq!(model_estimate(true, 1, "AEGS_Javelin"), gb10(65));
        assert_eq!(model_estimate(false, 0, "AEGS_Gladius"), gb10(14));
        assert_eq!(model_estimate(false, 0, "rsi_polaris"), gb10(18));
        // Less detail, less memory (the model viewer's slider shows each step).
        assert_eq!(model_estimate(false, 3, "AEGS_Gladius"), gb10(12));
        assert_eq!(model_estimate(false, 4, "AEGS_Idris_M"), gb10(16));
        let list = estimates();
        assert!(list.windows(2).all(|w| w[0].bytes <= w[1].bytes));
        assert!(list.iter().any(|e| e.id == "mcp-ships"));
        let l = Some(gb_bytes(1.5));
        assert_eq!(
            check(gb10(41), l, Some(20 * GIB)),
            Fit::OverLimit {
                need: gb10(41),
                limit: gb_bytes(1.5)
            }
        );
        assert_eq!(check(gb10(9), l, Some(20 * GIB)), Fit::Ok);
        assert_eq!(
            check(gb10(13), l, Some(GIB)),
            Fit::LowFree {
                need: gb10(13),
                free: GIB
            }
        );
        assert_eq!(check(gb10(13), None, None), Fit::Ok);
        let m = over_limit_message("Extracting game-ships", gb10(41), gb_bytes(1.5));
        assert!(
            m.contains("4.1 GB")
                && m.contains("1.5 GB memory limit allows")
                && m.contains("at least 5 GB")
                && m.contains("--memory-limit 5"),
            "{m}"
        );
        // The reviewer's repro: item-sources (1.5 GB) under a 1.5 GB limit would reach the
        // watchdog's 1.35 GB, so it is refused up front.
        assert!(matches!(
            check(
                dataset_estimate("game-item-sources"),
                Some(gb_bytes(1.5)),
                None
            ),
            Fit::OverLimit { .. }
        ));
        assert_eq!(limit_for(dataset_estimate("game-item-sources")), 2.0);
        assert_eq!(limit_for(gb10(41)), 5.0);
        assert_eq!(limit_for(gb10(110)), 12.5);
        assert_eq!(limit_for(1), MIN_GB);
        assert_eq!(
            stopped_message(gb_bytes(1.5)),
            format!("Stopped: needed more than 1.5 GB. {RAISE}")
        );
        assert_eq!(human(4 * GIB), "4 GB");
        assert_eq!(human(300 * 1024 * 1024), "0.3 GB");
        assert_eq!(human(ASSET_WORKER), "0.9 GB");
        assert_eq!(human(64 * 1024 * 1024), "64 MB");
    }

    #[test]
    fn meminfo_parse() {
        let m = parse_meminfo(
            "MemTotal:       32768000 kB\nMemFree: 1 kB\nMemAvailable:   16384000 kB\n",
        );
        assert_eq!(m.total, Some(32_768_000 * 1024));
        assert_eq!(m.available, Some(16_384_000 * 1024));
        assert_eq!(parse_meminfo("junk"), MemInfo::default());
        if cfg!(any(target_os = "linux", windows)) {
            let now = meminfo();
            assert!(now.total.is_some_and(|t| t > 0));
            assert!(now.available.is_some());
            assert!(own_usage().is_some());
        }
    }

    #[test]
    fn apply_carries_the_limit() {
        let mut c = Command::new("some-worker");
        c.arg("dev-worker").env("KEEP", "1").env_remove("DROP");
        let c = apply_in_process(c, Some(3 * GIB));
        assert_eq!(command_limit(&c), Some(3 * GIB));
        assert_eq!(c.get_program(), "some-worker");
        assert!(c.get_envs().any(|(k, v)| k == "KEEP" && v.is_some()));
        // The opt-in cgroup: the same command inside a scope, the cap above the limit.
        let mut c = Command::new("some-worker");
        c.arg("dev-worker").env("KEEP", "1");
        let c = apply_via(c, Some(3 * GIB), Some(Path::new("systemd-run")));
        let args: Vec<String> = c
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let cap = 3 * GIB + headroom(3 * GIB);
        assert!(args.contains(&format!("MemoryMax={cap}")), "{args:?}");
        assert_eq!(args.last().map(String::as_str), Some("dev-worker"));
        assert!(args.contains(&"some-worker".to_string()));
        assert_eq!(command_limit(&c), Some(3 * GIB));
        assert!(c.get_envs().any(|(k, v)| k == "KEEP" && v.is_some()));
        assert_eq!(command_limit(&apply(Command::new("x"), None)), None);
        // Headroom: 10%, at least 512 MB.
        assert_eq!(headroom(GIB), 512 * 1024 * 1024);
        assert_eq!(headroom(20 * GIB), 2 * GIB);
    }

    #[cfg(unix)]
    #[test]
    fn exits_that_mean_the_limit() {
        use std::os::unix::process::ExitStatusExt;
        let l = Some(GIB);
        let abort = ExitStatus::from_raw(6);
        assert!(exit_was_memory(
            &ExitStatus::from_raw(EXIT_MEMORY << 8),
            l,
            None
        ));
        // An abort or kill is a memory stop only with Rust's allocation-failure line.
        assert!(!exit_was_memory(&abort, l, None));
        assert!(!exit_was_memory(&ExitStatus::from_raw(9), l, None));
        assert!(!exit_was_memory(&abort, l, Some("thread 'main' panicked")));
        assert!(exit_was_memory(
            &abort,
            l,
            Some("memory allocation of 8388608 bytes failed\n")
        ));
        assert!(!exit_was_memory(&ExitStatus::from_raw(1 << 8), l, None));
        assert!(!exit_was_memory(
            &ExitStatus::from_raw(EXIT_MEMORY << 8),
            None,
            None
        ));
        assert!(unexpected_message("the explorer", &abort).contains("stopped unexpectedly"));
    }
}

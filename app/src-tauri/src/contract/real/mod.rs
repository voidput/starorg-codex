//! The real backend: [`Backend`] over `starorg-core` and `starorg-data` (issue #15).
//!
//! - Same core calls as the CLI (`crates/starorg-cli/src/*_cmds.rs`), so the app and the CLI
//!   share one apply, one status, one set of locks and one data folder (spec 4.2).
//! - Guards and refusals come back as reports (`skipped` + reason, `needs-setup`,
//!   `needs-decision`, ...), exactly as M1 returns them; `Err` only when a call cannot run.
//! - Every method is blocking and runs on Tauri's blocking pool (`contract::run`).
//! - Extraction runs in a separate worker process: this same binary started as
//!   `starorg-codex-app extract-worker ...` (see `main.rs`, spec 3.3).
//! - The core has no cancellation points inside an apply, so every job reports
//!   `cancellable: false` and `cancel_job` refuses.
//! - StarOrg Tools ([`starorg`]): loopback PKCE sign-in, pull, push and the hangar against
//!   starorg.tools or dev.starorg.tools (SC-ORG-BOT#1659), only while the network switch and
//!   the matching "What to sync" switch are on.

mod assets;
pub(crate) mod convert;
mod dev;
mod diag;
pub mod dialogs;
mod loot;
mod memory;
mod starorg;
mod storage;
mod table;
mod uex;

pub use assets::{AssetSpawner, AssetWorkerSpawner};
pub use dev::{DevSpawner, WorkerSpawner};
pub use dialogs::{DialogSlot, Dialogs, FileFilter, TauriDialogs};
pub use starorg::{Browser, StarOrgDeps};
pub use uex::UexDeps;

use super::types::*;
use super::{Backend, Progress, schema};
use crate::contract::strings_query;
use convert::{err, internal, kebab, path_text};
use serde::{Deserialize, Serialize};
use starorg_core::appdirs::AppDirs;
use starorg_core::apply::{self, ApplyOpts, Ctx, Target};
use starorg_core::backup;
use starorg_core::blueprints::{
    self, Choice,
    catalog::{BlueprintCatalog, GameDataCatalog},
    finder as bp_finder, import as bp_import,
    logscan::{self, Store},
    repair as bp_repair,
};
use starorg_core::cache::{self, KeyInputs};
use starorg_core::config::{self, Config, Outputs};
use starorg_core::discovery::{self, Channel, DiscoveryEnv, DiscoveryRequest, Install};
use starorg_core::fleet::Catalog;
use starorg_core::fleet::import as fleet_import;
use starorg_core::fleet::store::{self as fleet_store, FleetFile};
use starorg_core::fsutil;
use starorg_core::hooks;
use starorg_core::ini::StockIni;
use starorg_core::lock::{self, LockError};
use starorg_core::model::GameData;
use starorg_core::patch;
use starorg_core::source::{Budget, GameSource, local::LocalExtraction};
use starorg_core::state::{self, ChannelState};
use starorg_core::usercfg;
use starorg_core::validate;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use table::Table;

/// The UEX files WHERE TO GET's shop and sell lines read (`starorg_mcp` `Store::uex`), as
/// `fetch_uex` writes them.
pub const UEX_FILES: &[&str] = starorg_net::uex::FILES;

/// What is in a UEX folder: file metadata only, never the contents.
pub fn obtain_status_in(dir: &Path) -> ObtainStatus {
    let files: Vec<UexFile> = UEX_FILES
        .iter()
        .map(|name| {
            let meta = std::fs::metadata(dir.join(name))
                .ok()
                .filter(|m| m.is_file());
            UexFile {
                name: (*name).into(),
                present: meta.is_some(),
                modified: meta
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| fsutil::rfc3339(d.as_secs())),
            }
        })
        .collect();
    use starorg_net::uex;
    let ready = |names: &[&str]| {
        names
            .iter()
            .all(|n| files.iter().any(|f| f.name == *n && f.present))
    };
    ObtainStatus {
        uex_folder: convert::path_text(dir),
        shops_ready: ready(&[uex::PRICES_FILE, uex::TERMINALS_FILE]),
        prices_ready: ready(&[uex::COMMODITY_PRICES_FILE, uex::TERMINALS_FILE]),
        files,
    }
}

/// Makes the game source for loads (extraction worker by default; a file in tests).
pub trait SourceFactory: Send + Sync {
    fn source(&self, dirs: &AppDirs) -> Result<Box<dyn GameSource>, String>;
    /// Whether apply and the preview read obtain hints from the MCP datasets (spec
    /// obtain-hints; the real worker only: a test source has no `Data.p4k` to read them
    /// from).
    fn obtain_hints(&self) -> bool {
        false
    }
}

/// The extraction worker: this binary, `extract-worker` subcommand.
pub struct WorkerSource;

impl SourceFactory for WorkerSource {
    fn source(&self, dirs: &AppDirs) -> Result<Box<dyn GameSource>, String> {
        let exe = std::env::current_exe().map_err(|e| format!("cannot find own binary: {e}"))?;
        // App-started extractions also run the loc-reference pass (Strings grouping).
        let mut s = LocalExtraction::new(exe).with_keyrefs();
        s.work_root = Some(dirs.cache.join("work"));
        Ok(Box::new(s))
    }

    fn obtain_hints(&self) -> bool {
        true
    }
}

/// Everything the real backend needs from outside.
pub struct RealConfig {
    /// App folders and core settings (the CLI builds the same `Ctx`).
    pub ctx: Ctx,
    /// Where discovery looks; tests pin it to a fake install with `game_dir`.
    pub discovery: DiscoveryRequest,
    pub env: DiscoveryEnv,
    pub source: Arc<dyn SourceFactory>,
    pub dialogs: DialogSlot,
}

impl RealConfig {
    /// The app's own setup: XDG / %APPDATA% folders, process discovery, the worker.
    pub fn from_env() -> Result<Self, String> {
        let dirs =
            AppDirs::from_env().ok_or("cannot find the home directory for the app folders")?;
        Ok(Self {
            ctx: Ctx::new(
                dirs,
                env!("CARGO_PKG_VERSION"),
                starorg_data::starbreaker::STARBREAKER_COMMIT,
            ),
            discovery: DiscoveryRequest::default(),
            env: DiscoveryEnv::from_process(),
            source: Arc::new(WorkerSource),
            dialogs: DialogSlot::default(),
        })
    }
}

/// App-only settings beside `config.toml` (the core never reads them).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct AppFile {
    #[serde(default)]
    appearance: Option<Appearance>,
    #[serde(default)]
    network: Option<NetworkSettings>,
    #[serde(default)]
    safe_gpu: bool,
    /// Per channel: characters ticked for a StarOrg push.
    #[serde(default)]
    sync: BTreeMap<String, Vec<String>>,
    /// Developer mode switch and dataset folders (addendum 7).
    #[serde(default)]
    dev: dev::DevFile,
    /// StarOrg Tools account card per environment (no token; see `starorg`).
    #[serde(default)]
    starorg: BTreeMap<String, starorg::Account>,
    /// Where the UEX token is (no token; see `uex`).
    #[serde(default)]
    uex_token: Option<TokenStoreKind>,
}

const APP_JSON: &str = "app.json";
/// Our record of the last apply's numbers (M1's `applied.json` keeps no counts).
const LAST_APPLY_JSON: &str = "app-last-apply.json";
const LOG_KEEP: usize = 500;
/// Update checker state (last check, result, skipped version, repo), in the data folder.
pub const UPDATE_JSON: &str = "app-update.json";

/// One install as the UI knows it.
#[derive(Debug, Clone)]
struct Found {
    install: Install,
    /// Config id, when registered.
    id: Option<String>,
    found_by: FoundBy,
    label: String,
}

impl Found {
    /// Before registration the UI uses a stable discovery-time id from the root.
    fn discovery_id(&self) -> String {
        discovery_id(&self.install.root)
    }

    fn ui_id(&self) -> String {
        self.id.clone().unwrap_or_else(|| self.discovery_id())
    }

    fn channel(&self, name: &str) -> CmdResult<&Channel> {
        self.install.channel(name).ok_or_else(|| {
            err(
                ErrorCode::NotFound,
                format!(
                    "channel {name} not found in {}",
                    self.install.root.display()
                ),
            )
        })
    }
}

/// The Activity line for a schema-1 `profile.toml` rewrite: names the settings whose value
/// changed (they now follow the current default) and only counts the ones saved at the
/// current default (nothing changes for them).
fn migration_note(m: &state::Migration) -> String {
    let mut parts = Vec::new();
    if !m.followed.is_empty() {
        let keys: Vec<&str> = m
            .followed
            .iter()
            .map(|k| k.rsplit('.').next().unwrap_or(k))
            .collect();
        parts.push(if keys.len() == 1 {
            format!(
                "{} now follows the current default (it was saved at the old one)",
                keys[0]
            )
        } else {
            format!(
                "{} now follow the current defaults (they were saved at the old ones)",
                keys.join(", ")
            )
        });
    }
    match m.unpinned.len() {
        0 => {}
        1 => parts.push("removed 1 saved default (no change)".into()),
        n => parts.push(format!("removed {n} saved defaults (no change)")),
    }
    format!(
        "Profile updated: {}; a backup is in {}",
        parts.join("; "),
        state::profile_backup_name(m.from)
    )
}

fn discovery_id(root: &Path) -> String {
    let h = fsutil::sha256_hex(fsutil::canonical_lenient(root).to_string_lossy().as_bytes());
    format!("found-{}", &h[..10])
}

/// Loaded game data of one build, kept for the Strings screen and previews.
struct LoadedData {
    key: String,
    stock: Arc<StockIni>,
    game_data: Arc<GameData>,
    /// Strings grouping of this build, computed once per cache entry (addendum 8.2).
    groups: Arc<starorg_core::groups::GroupIndex>,
    /// Key owners of this build, built on the first Strings table (about 55 ms on LIVE).
    owners: std::sync::OnceLock<Arc<starorg_core::groups::owners::OwnerIndex>>,
}

impl LoadedData {
    fn new(key: String, l: starorg_core::source::Loaded) -> Self {
        let stock = StockIni::parse(l.base_ini);
        let groups = starorg_core::groups::assign(&stock, &l.game_data, l.keyrefs.as_ref());
        Self {
            key,
            stock: Arc::new(stock),
            game_data: Arc::new(l.game_data),
            groups: Arc::new(groups),
            owners: std::sync::OnceLock::new(),
        }
    }

    fn owners(&self) -> &Arc<starorg_core::groups::owners::OwnerIndex> {
        self.owners.get_or_init(|| {
            Arc::new(starorg_core::groups::owners::OwnerIndex::build(
                &self.stock,
                &self.game_data,
            ))
        })
    }

    fn group_meta(&self) -> strings_query::GroupMeta {
        strings_query::GroupMeta {
            state: if self.groups.exact {
                GroupState::Exact
            } else {
                GroupState::Approximate
            },
            manufacturers: self
                .groups
                .manufacturers
                .iter()
                .map(|m| (m.code.clone(), m.label.clone()))
                .collect(),
        }
    }
}

/// One build's finder catalogue.
type BpCatalog = Arc<Vec<bp_finder::CatalogItem>>;

/// Draft-profile tables kept for the preview (each holds a channel's generated text).
const PREVIEW_TABLES: usize = 3;

#[derive(Default)]
struct Mem {
    manual: Vec<PathBuf>,
    log: VecDeque<LogEntry>,
    next_log: u64,
    next_job: u64,
    job: Option<JobProgress>,
    data: HashMap<(String, String), Arc<LoadedData>>,
    tables: HashMap<(String, String), (String, Arc<Table>)>,
    /// Tables of draft profiles (the Enhancements preview), newest last, at most
    /// [`PREVIEW_TABLES`]: switching an option back and forth does not build again.
    previews: VecDeque<((String, String), String, Arc<Table>)>,
    /// Blueprint finder catalogue per channel, by build key (a few hundred KB; addendum §7.19).
    bp_catalogs: HashMap<(String, String), (String, BpCatalog)>,
    /// The fleet file picked for the last dry run (name, bytes): the confirm step imports it
    /// without a second picker.
    last_fleet: Option<(String, Vec<u8>)>,
    /// The Smart Citizen export picked for the last dry run, per channel (install, channel):
    /// the confirm step (`reuse_last`) imports it without a second picker.
    last_sc_pick: Option<((String, String), PathBuf)>,
    /// Channel data dirs whose `profile.toml` migration failed: warned about once.
    profile_migrate_failed: HashSet<PathBuf>,
    /// The channel `list_characters` last scanned (install id, channel) and when: the
    /// Blueprints screen's current channel. The finder skips its own log scan of it for
    /// [`SCAN_FRESH`] and keeps its game data resident (see `bp_catalog`).
    listed: Option<((String, String), std::time::Instant)>,
}

/// How long a log scan by `list_characters` stands in for the finder's scan of that channel.
const SCAN_FRESH: std::time::Duration = std::time::Duration::from_secs(10);

pub struct RealBackend {
    cfg: RealConfig,
    mem: Mutex<Mem>,
    /// Held while this app reads a channel's game data (see `loaded`) or applies.
    reads: Mutex<()>,
    /// Held while an install is saved to config.toml (`register`).
    registering: Mutex<()>,
    /// Held across a blueprint log scan in this process (and `list_characters` noting the
    /// channel it scanned): a second caller waits for the running scan instead of losing the
    /// store lock and reading the store mid-scan.
    scans: Mutex<()>,
    update: crate::update::Updater,
    dev: dev::DevState,
    assets: assets::AssetState,
    mcp: crate::mcp::McpHost,
    starorg: starorg::StarOrgDeps,
    starorg_mem: starorg::StarOrgMem,
    uex: uex::UexDeps,
}

/// Longest header line accepted (the core has no limit; the line goes into every block).
pub const MAX_HEADER: usize = 80;
/// Mission list cap range, as the Enhancements screen offers it.
pub const MAX_LIST: std::ops::RangeInclusive<u32> = 1..=200;

/// Values the core would ignore or that have no sane limit there are refused up front:
/// unknown stat lines and tag elements, an over-long header, a huge list cap.
fn check_profile(p: &Profile) -> CmdResult<()> {
    let sch = schema::profile_schema();
    let bad = |m: String| Err(err(ErrorCode::Invalid, m));
    for l in &p.lines_off {
        if !sch
            .categories
            .iter()
            .any(|c| c.lines.iter().any(|x| x.key == *l))
        {
            return bad(format!("unknown stat line {l}"));
        }
    }
    for (kind, t) in [
        (TagKind::Components, &p.tags.components),
        (TagKind::ShipWeapons, &p.tags.ship_weapons),
        (TagKind::Missiles, &p.tags.missiles),
    ] {
        let known = sch.tag_kinds.iter().find(|k| k.kind == kind);
        for e in t.elements.iter().chain(t.styles.keys()) {
            if !known.is_some_and(|k| k.elements.iter().any(|x| x.id == *e)) {
                return bad(format!("unknown tag element {e}"));
            }
        }
    }
    if p.header.chars().count() > MAX_HEADER {
        return bad(format!(
            "The header line is longer than {MAX_HEADER} characters."
        ));
    }
    if !MAX_LIST.contains(&p.mission.max_list) {
        return bad(format!(
            "The list length must be {} to {}.",
            MAX_LIST.start(),
            MAX_LIST.end()
        ));
    }
    Ok(())
}

/// `user-<slug>`; a title with letters the slug drops (non-ASCII) or no slug at all gets a
/// short hash of the title, so distinct titles do not collapse into one id.
fn user_patch_id(title: &str, slug: &str) -> String {
    let lossy = title
        .chars()
        .any(|c| c.is_alphanumeric() && !c.is_ascii_alphanumeric());
    if slug.is_empty() || lossy {
        let h = fsutil::sha256_parts(&[title.trim().as_bytes()]);
        let h = &h[..8.min(h.len())];
        if slug.is_empty() {
            format!("user-{h}")
        } else {
            format!("user-{slug}-{h}")
        }
    } else {
        format!("user-{slug}")
    }
}

fn now() -> String {
    fsutil::rfc3339(fsutil::unix_now())
}

fn io(e: impl std::fmt::Display) -> AppError {
    err(ErrorCode::Internal, e.to_string())
}

fn busy_or(e: LockError, what: &str) -> AppError {
    match e {
        LockError::Held(_) => err(ErrorCode::Busy, format!("{what} is busy; try again")),
        other => io(other),
    }
}

const JSON_FILTER: FileFilter = FileFilter {
    name: "Owned-blueprints export (JSON)",
    extensions: &["json"],
};
const FLEET_FILTER: FileFilter = FileFilter {
    name: "Fleet export or list",
    extensions: &["json", "csv", "txt"],
};
const PATCH_FILTER: FileFilter = FileFilter {
    name: "Patch file",
    extensions: &["toml"],
};
const TEXT_FILTER: FileFilter = FileFilter {
    name: "Text file",
    extensions: &["txt"],
};

impl RealBackend {
    pub fn new(cfg: RealConfig) -> Self {
        let update = crate::update::Updater::github(cfg.ctx.dirs.data.join(UPDATE_JSON));
        let starorg = starorg::StarOrgDeps::system(cfg.ctx.dirs.config.join("starorg-tokens"));
        Self {
            cfg,
            mem: Mutex::new(Mem::default()),
            reads: Mutex::new(()),
            registering: Mutex::new(()),
            scans: Mutex::new(()),
            update,
            dev: dev::DevState::default(),
            assets: assets::AssetState::default(),
            mcp: crate::mcp::McpHost::default(),
            starorg,
            starorg_mem: starorg::StarOrgMem::default(),
            uex: uex::UexDeps::default(),
        }
    }

    /// Swap how UEX market data is downloaded (tests: a fake transport).
    pub fn with_uex(mut self, deps: UexDeps) -> Self {
        self.uex = deps;
        self
    }

    /// Swap the StarOrg Tools hosts, token store and browser (tests: a loopback mock).
    pub fn with_starorg(mut self, deps: StarOrgDeps) -> Self {
        self.starorg = deps;
        self
    }

    /// Swap the update checker (tests: a fake HTTP layer). Keeps the current host slot.
    pub fn with_updater(mut self, u: crate::update::Updater) -> Self {
        let host = self.update.host();
        self.update = u.with_host(host);
        self
    }

    /// Where the app puts the opener and installer once its window exists.
    pub fn update_host(&self) -> crate::update::HostSlot {
        self.update.host()
    }

    fn updates_on(&self) -> bool {
        self.settings().network.check_updates
    }

    fn mem(&self) -> MutexGuard<'_, Mem> {
        self.mem.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn ctx(&self) -> &Ctx {
        &self.cfg.ctx
    }

    fn dirs(&self) -> &AppDirs {
        &self.cfg.ctx.dirs
    }

    /// One Activity line per outcome of the start-up folder move (moved, left alone, failed).
    pub fn log_folder_migration(&self, r: &starorg_core::legacy::Report) {
        for (level, line) in r.lines() {
            let level = match level {
                starorg_core::legacy::Level::Info => LogLevel::Info,
                starorg_core::legacy::Level::Warn => LogLevel::Warn,
            };
            self.log(level, None, None, &line);
        }
    }

    fn log(&self, level: LogLevel, job: Option<JobKind>, channel: Option<&str>, msg: &str) {
        let mut m = self.mem();
        m.next_log += 1;
        let e = LogEntry {
            id: m.next_log,
            time: now(),
            level,
            job,
            channel: channel.map(str::to_string),
            message: msg.to_string(),
        };
        m.log.push_back(e);
        while m.log.len() > LOG_KEEP {
            m.log.pop_front();
        }
    }

    // -----------------------------------------------------------------------------------
    // Jobs
    // -----------------------------------------------------------------------------------

    fn job_start(
        &self,
        kind: JobKind,
        channel: Option<&str>,
        steps: u32,
        label: &str,
    ) -> JobProgress {
        let mut m = self.mem();
        m.next_job += 1;
        let p = JobProgress {
            job: format!("job-{}", m.next_job),
            kind,
            channel: channel.map(str::to_string),
            phase: JobPhase::Running,
            step: 1,
            steps,
            label: label.to_string(),
            cancellable: false,
        };
        // One slot for overview.job: a second job (remove while an apply runs, or an apply
        // skipped on the lock) does not hide the one already running.
        if m.job.is_none() {
            m.job = Some(p.clone());
        }
        p
    }

    fn job_step(
        &self,
        p: &mut JobProgress,
        step: u32,
        label: &str,
        progress: Option<Progress<'_>>,
    ) {
        p.step = step;
        p.label = label.to_string();
        {
            let mut m = self.mem();
            if m.job.as_ref().is_none_or(|j| j.job == p.job) {
                m.job = Some(p.clone());
            }
        }
        if let Some(cb) = progress {
            cb(p.clone());
        }
    }

    fn job_end(
        &self,
        mut p: JobProgress,
        phase: JobPhase,
        label: &str,
        progress: Option<Progress<'_>>,
    ) {
        p.phase = phase;
        p.step = p.steps;
        p.label = label.to_string();
        // A job may have written to the cache or the backups: Settings measures again.
        self.sizes_changed();
        {
            let mut m = self.mem();
            if m.job.as_ref().is_some_and(|j| j.job == p.job) {
                m.job = None;
            }
        }
        if let Some(cb) = progress {
            cb(p);
        }
    }

    // -----------------------------------------------------------------------------------
    // Installs
    // -----------------------------------------------------------------------------------

    fn config(&self) -> Config {
        Config::load(self.dirs()).unwrap_or_default()
    }

    fn found_by(f: &discovery::FoundBy) -> FoundBy {
        use discovery::FoundBy as D;
        match f {
            D::GameDir => FoundBy::GameDir,
            D::PrefixOption | D::WinePrefixEnv => FoundBy::Prefix,
            D::LugConfig => FoundBy::LugConfig,
            D::DefaultPrefix => FoundBy::DefaultPrefix,
            D::LauncherLog => FoundBy::LauncherLog,
            D::DefaultPath => FoundBy::CommonPath,
        }
    }

    /// Discover a folder the user named (picked, or saved in config.toml): as an install
    /// root first, then as a Wine prefix.
    fn discover_folder(&self, dir: &Path) -> Option<Install> {
        let as_root = DiscoveryRequest {
            game_dir: Some(dir.to_path_buf()),
            prefix: None,
        };
        if let Some(i) = discovery::discover(&as_root, &self.cfg.env)
            .installs
            .into_iter()
            .next()
        {
            return Some(i);
        }
        let as_prefix = DiscoveryRequest {
            game_dir: None,
            prefix: Some(dir.to_path_buf()),
        };
        discovery::discover(&as_prefix, &self.cfg.env)
            .installs
            .into_iter()
            .next()
    }

    /// Every install: discovery, then saved installs discovery missed, then folders the
    /// user picked this session. One entry per root.
    fn installs(&self) -> Vec<Found> {
        let cfg = self.config();
        let outputs = Outputs::load(self.dirs()).unwrap_or_default();
        let mut out: Vec<Found> = Vec::new();
        let mut push = |install: Install, found_by: FoundBy, label: String| {
            let key = fsutil::canonical_lenient(&install.root);
            if out
                .iter()
                .any(|f| fsutil::canonical_lenient(&f.install.root) == key)
            {
                return;
            }
            let id = config::lookup_install(&cfg, &outputs, &install).map(|(id, _)| id);
            out.push(Found {
                install,
                id,
                found_by,
                label,
            });
        };
        for i in discovery::discover(&self.cfg.discovery, &self.cfg.env).installs {
            let fb = Self::found_by(&i.found_by);
            let label = i.found_by.describe().to_string();
            push(i, fb, label);
        }
        // With a pinned game_dir (tests, `--game-dir`) nothing else is looked at.
        if self.cfg.discovery.game_dir.is_none() {
            for e in &cfg.installs {
                if e.path.is_dir()
                    && let Some(i) = self.discover_folder(&e.path)
                {
                    push(i, FoundBy::Config, "Saved install".into());
                }
            }
        }
        let manual = self.mem().manual.clone();
        for dir in manual {
            if let Some(i) = self.discover_folder(&dir) {
                push(i, FoundBy::Manual, "Chosen folder".into());
            }
        }
        out
    }

    fn find(&self, install: &str) -> CmdResult<Found> {
        self.installs()
            .into_iter()
            .find(|f| f.id.as_deref() == Some(install) || f.discovery_id() == install)
            .ok_or_else(|| {
                err(
                    ErrorCode::NotFound,
                    format!("install {install} not found (moved or unplugged?)"),
                )
            })
    }

    /// Give the install an id in config.toml (commands that write; CLI `register`).
    fn register(&self, f: &mut Found) -> CmdResult<String> {
        if let Some(id) = &f.id {
            return Ok(id.clone());
        }
        // Two wizard calls can register at once (Enhancements' read, Review): one at a time,
        // and the second finds the id the first saved.
        let _one = self.registering.lock().unwrap_or_else(|e| e.into_inner());
        let mut cfg = Config::load(self.dirs()).map_err(io)?;
        let outputs = Outputs::load(self.dirs()).unwrap_or_default();
        let (id, _) =
            config::register_install(self.dirs(), &mut cfg, &outputs, &f.install).map_err(io)?;
        f.id = Some(id.clone());
        Ok(id)
    }

    fn data_dir(&self, f: &Found, ch: &Channel) -> PathBuf {
        Target::new(&f.install, f.id.clone(), ch).data_dir(self.dirs())
    }

    fn is_set_up(&self, f: &Found, ch: &Channel) -> bool {
        f.id.is_some()
            && ChannelState::load(&self.data_dir(f, ch))
                .map(|s| s.consent)
                .unwrap_or(false)
    }

    fn language(ch: &Channel) -> (Language, Option<String>) {
        let line = usercfg::locate(&ch.dir)
            .ok()
            .and_then(|t| usercfg::read(&t).ok())
            .map(|x| match x {
                None => usercfg::LanguageLine::NoFile,
                Some(t) => usercfg::inspect(&t),
            });
        match line {
            Some(usercfg::LanguageLine::English { .. }) => (Language::English, None),
            Some(usercfg::LanguageLine::Other { value, .. }) => (Language::Other, Some(value)),
            _ => (Language::Unknown, None),
        }
    }

    fn cache_state(&self, f: &Found, ch: &Channel) -> CacheState {
        let Some(id) = &f.id else {
            return CacheState::Missing;
        };
        let dir = self.dirs().channel_cache(id, &ch.name);
        let fresh = ch.fingerprint().ok().is_some_and(|fp| {
            cache::has_entry(&dir, &KeyInputs::local(&fp, &self.ctx().starbreaker_commit))
        });
        if fresh {
            return CacheState::Fresh;
        }
        let any = std::fs::read_dir(&dir).is_ok_and(|mut rd| rd.next().is_some());
        if any {
            CacheState::Stale
        } else {
            CacheState::Missing
        }
    }

    /// Rewrite a schema-1 `profile.toml` once ([`state::migrate_profile_file`]), saying so
    /// in Activity when it changed a setting. Readers already see the migrated settings,
    /// so a failure here only leaves the file as it was.
    fn migrate_profile(&self, f: &Found, ch: &Channel) {
        let data = self.data_dir(f, ch);
        match state::migrate_profile_file(&data) {
            Ok(Some(m)) if !m.is_empty() => {
                self.log(LogLevel::Info, None, Some(&ch.name), &migration_note(&m));
            }
            Ok(_) => {}
            Err(e) => {
                if self.mem().profile_migrate_failed.insert(data) {
                    self.log(
                        LogLevel::Warn,
                        None,
                        Some(&ch.name),
                        &format!("profile.toml could not be updated to the new format: {e}"),
                    );
                }
            }
        }
    }

    fn summary(&self, f: &Found, ch: &Channel) -> ChannelSummary {
        self.migrate_profile(f, ch);
        let t = Target::new(&f.install, f.id.clone(), ch);
        let st = apply::current_inputs(self.ctx(), &t, None, None)
            .and_then(|inp| apply::status(self.ctx(), &t, &inp));
        let (status, interrupted, applied) = match st {
            Ok(s) => (convert::state(&s.state), s.interrupted_apply, s.applied),
            Err(e) => {
                self.log(
                    LogLevel::Warn,
                    None,
                    Some(&ch.name),
                    &format!("status: {e}"),
                );
                (ApplyState::NotApplied, false, None)
            }
        };
        let (language, language_value) = Self::language(ch);
        let foreign_tool = match &status {
            ApplyState::ChangedByAnotherTool { tool } => tool.clone(),
            _ => None,
        };
        let data = t.data_dir(self.dirs());
        let last_apply = fsutil::read_json::<LastApply>(&data.join(LAST_APPLY_JSON))
            .ok()
            .flatten()
            .filter(|l| applied.as_ref().is_some_and(|a| a.time == l.time))
            .or_else(|| {
                applied.as_ref().map(|a| LastApply {
                    time: a.time.clone(),
                    result: ApplyResult::Applied,
                    changed: 0,
                    warnings: a.warnings.len() as u32,
                    dropped: 0,
                    duration_ms: 0,
                })
            });
        ChannelSummary {
            channel: ch.name.clone(),
            build: convert::build(ch.build.as_ref()),
            build_error: ch.build_error.clone(),
            set_up: self.is_set_up(f, ch),
            foreign_file: matches!(status, ApplyState::ChangedByAnotherTool { .. }),
            status,
            language,
            language_value,
            foreign_tool,
            game_running: starorg_core::running::game_running(
                t.prefix.as_deref(),
                &ch.name,
                &ch.dir,
            )
            .is_some(),
            interrupted_apply: interrupted,
            p4k_time: std::fs::metadata(&ch.p4k)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| fsutil::rfc3339(d.as_secs())),
            cache: self.cache_state(f, ch),
            applied_build: applied.as_ref().map(convert::applied_build),
            last_apply,
        }
    }

    fn install_summary(&self, f: &Found) -> InstallSummary {
        InstallSummary {
            id: f.ui_id(),
            root: path_text(&f.install.root),
            prefix: f.install.prefix.as_deref().map(path_text),
            found_by: f.found_by,
            found_by_label: f.label.clone(),
            saved: f.id.is_some(),
            channels: f
                .install
                .channels
                .iter()
                .map(|c| self.summary(f, c))
                .collect(),
        }
    }

    // -----------------------------------------------------------------------------------
    // Game data and the Strings table
    // -----------------------------------------------------------------------------------

    /// Stock text and GameData of the channel's build. `extract`: run the worker when the
    /// cache has no entry (slow); otherwise only a cached entry is used.
    fn loaded(&self, f: &Found, ch: &Channel, extract: bool) -> CmdResult<Arc<LoadedData>> {
        let id = f.id.clone().ok_or_else(|| {
            err(
                ErrorCode::NeedsDecision,
                format!("{} is not set up yet", ch.name),
            )
        })?;
        let fp = ch
            .fingerprint()
            .map_err(|e| err(ErrorCode::NotFound, format!("{}: {e}", ch.p4k.display())))?;
        let k = KeyInputs::local(&fp, &self.ctx().starbreaker_commit);
        let key = k.key();
        let slot = (id.clone(), ch.name.clone());
        let in_mem = || self.mem().data.get(&slot).filter(|d| d.key == key).cloned();
        let from_disk = || {
            self.ctx()
                .use_cache
                .then(|| {
                    cache::load(
                        &self.dirs().channel_cache(&id, &ch.name),
                        &k,
                        self.ctx().min_stock_keys,
                    )
                    .ok()
                })
                .flatten()
        };
        if let Some(d) = in_mem() {
            return Ok(d);
        }
        // Reads in this app run one at a time: a second caller (Review asking again while
        // Enhancements' read runs, Apply right after Review) waits and then uses that read,
        // instead of the core's "another extraction is running" skip.
        let _reading = extract.then(|| self.reads.lock().unwrap_or_else(|e| e.into_inner()));
        if let Some(d) = in_mem() {
            return Ok(d);
        }
        let l = match from_disk() {
            Some(l) => l,
            None if extract => {
                let t = Target::new(&f.install, Some(id.clone()), ch);
                let src = self.cfg.source.source(self.dirs()).map_err(internal)?;
                let mut w = Vec::new();
                apply::load_channel(self.ctx(), &t, src.as_ref(), &Budget::unlimited(), &mut w)
                    .map_err(|e| {
                        // Only another process can hold the lock now (a launch hook, the CLI).
                        let code = if e.contains("another extraction") {
                            ErrorCode::Busy
                        } else {
                            ErrorCode::Internal
                        };
                        err(code, format!("reading game data: {e}"))
                    })?
            }
            None => {
                return Err(err(
                    ErrorCode::NotFound,
                    format!(
                        "The game data of {} has not been read for this build yet. Apply (or a dry run) reads it.",
                        ch.name
                    ),
                ));
            }
        };
        let d = Arc::new(LoadedData::new(key, l));
        // One build per channel in memory.
        self.mem().data.insert(slot, d.clone());
        Ok(d)
    }

    /// Build the table with the channel's inputs, or with `profile`/`disabled` instead of
    /// the saved profile.
    fn build_table(
        &self,
        f: &Found,
        ch: &Channel,
        profile: Option<(&starorg_core::enhance::profile::Profile, &[String])>,
        extract: bool,
    ) -> CmdResult<Arc<Table>> {
        let d = self.loaded(f, ch, extract)?;
        let t = Target::new(&f.install, f.id.clone(), ch);
        let inp = apply::current_inputs(self.ctx(), &t, None, None).map_err(internal)?;
        let (eprof, patches) = match profile {
            Some((p, dis)) => (p.clone(), patch::load_patches(self.dirs(), dis)),
            None => (
                inp.profile
                    .enhance()
                    .map_err(|e| err(ErrorCode::Invalid, format!("profile.toml: {e}")))?,
                inp.patches.clone(),
            ),
        };
        let provider = self.obtain_provider(f, extract);
        let (hints, hint_warnings, _) =
            apply::obtain_hints(&d.game_data, &eprof, provider.as_ref(), &ch.name);
        if extract && let Some(w) = hint_warnings.first() {
            self.log(
                LogLevel::Warn,
                Some(JobKind::Apply),
                Some(&ch.name),
                &format!("{}: {}", ch.name, w.detail),
            );
        }
        let hints_tag = format!(
            "{}:{}",
            hints.as_ref().map_or(0, |h| h.items.len()),
            hint_warnings.len()
        );
        let key = fsutil::sha256_parts(&[
            d.key.as_bytes(),
            eprof.hash().as_bytes(),
            patches.hash().as_bytes(),
            inp.overrides_hash.as_bytes(),
            inp.owned.hash.as_bytes(),
            inp.ship_names_hash.as_bytes(),
            hints_tag.as_bytes(),
        ]);
        let slot = (f.id.clone().unwrap_or_default(), ch.name.clone());
        {
            let mut m = self.mem();
            if let Some((k, t)) = m.tables.get(&slot)
                && *k == key
            {
                return Ok(t.clone());
            }
            if profile.is_some()
                && let Some(i) = m
                    .previews
                    .iter()
                    .position(|(s, k, _)| *s == slot && *k == key)
            {
                let hit = m.previews.remove(i).expect("position is in range");
                let t = hit.2.clone();
                m.previews.push_back(hit);
                return Ok(t);
            }
        }
        let tbl = Arc::new(table::build(&table::Inputs {
            stock: &d.stock,
            game_data: &d.game_data,
            build: ch.build.as_ref(),
            profile: &eprof,
            patches: &patches,
            overrides: &inp.overrides,
            owned: &inp.owned,
            groups: &d.groups,
            owners: d.owners(),
            ship_names: inp.ship_names.active(),
            obtain: hints.as_ref(),
        }));
        let mut m = self.mem();
        if profile.is_none() {
            m.tables.insert(slot, (key, tbl.clone()));
        } else {
            m.previews.push_back((slot, key, tbl.clone()));
            while m.previews.len() > PREVIEW_TABLES {
                m.previews.pop_front();
            }
        }
        Ok(tbl)
    }

    fn profile_file(&self, f: &Found, ch: &Channel) -> CmdResult<state::Profile> {
        self.migrate_profile(f, ch);
        state::Profile::load_file(&state::Profile::path(&self.data_dir(f, ch))).map_err(io)
    }

    fn overrides_text(data: &Path) -> CmdResult<String> {
        match fsutil::read_limited(&data.join(state::OVERRIDES_INI), state::OVERRIDES_LIMIT) {
            Ok(Some(b)) => String::from_utf8(b)
                .map_err(|_| err(ErrorCode::Invalid, "overrides.ini is not valid UTF-8")),
            Ok(None) => Ok(String::new()),
            Err(e) => Err(io(e)),
        }
    }

    fn override_of(data: &Path, key: &str) -> Option<String> {
        let text = Self::overrides_text(data).ok()?;
        state::parse_overrides(&text)
            .0
            .into_iter()
            .rev()
            .find(|o| o.key.eq_ignore_ascii_case(key))
            .map(|o| o.value)
    }

    /// Replace (or with `None` remove) the line for `key`, keeping comments and order.
    fn write_override(data: &Path, key: &str, value: Option<&str>) -> CmdResult<()> {
        let text = Self::overrides_text(data)?;
        let mut out = String::new();
        let mut done = false;
        for line in text.split_inclusive('\n') {
            let bare = line.trim_end_matches(['\n', '\r']);
            let is_key = bare
                .split_once('=')
                .is_some_and(|(k, _)| !bare.starts_with(';') && k.eq_ignore_ascii_case(key));
            if is_key {
                if let (Some(v), false) = (value, done) {
                    out.push_str(&format!("{key}={v}\n"));
                    done = true;
                }
                continue;
            }
            out.push_str(line);
        }
        if let (Some(v), false) = (value, done) {
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&format!("{key}={v}\n"));
        }
        std::fs::create_dir_all(data).map_err(io)?;
        state::save_overrides(data, &out).map_err(io)
    }

    fn forget_tables(&self, f: &Found, ch: &Channel) {
        let slot = (f.id.clone().unwrap_or_default(), ch.name.clone());
        let mut m = self.mem();
        m.tables.remove(&slot);
        m.previews.retain(|(s, _, _)| *s != slot);
    }

    // -----------------------------------------------------------------------------------
    // Backups
    // -----------------------------------------------------------------------------------

    fn offers(&self, s: &ChannelSummary, list: &[backup::Backup]) -> RestoreOffers {
        let views: Vec<BackupView> = list.iter().map(convert::backup_view).collect();
        let undo = views
            .iter()
            .rev()
            .find(|b| {
                b.reason == BackupReason::BeforeApply
                    && b.origin == FileOrigin::Ours
                    && b.build.change == s.build.change
            })
            .map(|b| b.id.clone());
        let foreign = views
            .iter()
            .find(|b| b.pinned && b.origin == FileOrigin::Foreign)
            .map(|b| ForeignRestore {
                backup: b.id.clone(),
                tool: b.foreign_tool.clone(),
                made_for: b.build.clone(),
                current: s.build.clone(),
                build_mismatch: b.build.change != s.build.change,
            });
        RestoreOffers {
            remove: !matches!(s.status, ApplyState::NotApplied),
            remove_foreign: matches!(s.status, ApplyState::ChangedByAnotherTool { .. }),
            undo_last_apply: undo,
            restore_foreign: foreign,
        }
    }

    fn backup_list(&self, f: &Found, ch: &Channel) -> BackupList {
        let data = self.data_dir(f, ch);
        let list = if f.id.is_some() {
            backup::list(&data)
        } else {
            Vec::new()
        };
        let s = self.summary(f, ch);
        BackupList {
            channel: ch.name.clone(),
            folder: path_text(&backup::backups_dir(&data)),
            offers: self.offers(&s, &list),
            backups: list.iter().rev().map(convert::backup_view).collect(),
            keep_newest: backup::KEEP_NEWEST as u32,
            keep_builds: backup::KEEP_BUILDS as u32,
        }
    }

    fn channel_lock(&self, id: &str, ch: &Channel) -> CmdResult<lock::Lock> {
        lock::try_lock(&self.dirs().channel_lock(id, &ch.name))
            .map_err(|e| busy_or(e, &format!("{} (another StarOrg Codex run)", ch.name)))
    }

    // -----------------------------------------------------------------------------------
    // Settings
    // -----------------------------------------------------------------------------------

    fn app_file(&self) -> AppFile {
        fsutil::read_json(&self.dirs().config.join(APP_JSON))
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    fn save_app_file(&self, a: &AppFile) -> CmdResult<()> {
        fsutil::write_json(&self.dirs().config.join(APP_JSON), a).map_err(io)
    }

    fn settings(&self) -> Settings {
        let a = self.app_file();
        Settings {
            appearance: a.appearance.unwrap_or(Appearance {
                theme: Theme::Dark,
                accent: Accent::Cyan,
                palette: Palette::Void,
                reduce_effects: false,
            }),
            // published_dumps: Developer mode's Diff may download published LIVE builds
            // (addendum §1). An older app.json could only store it off.
            network: NetworkSettings {
                check_updates: a.network.as_ref().is_some_and(|n| n.check_updates),
                published_dumps: a.network.as_ref().is_some_and(|n| n.published_dumps),
                starorg: a.network.as_ref().is_some_and(|n| n.starorg),
                uex: a.network.as_ref().is_some_and(|n| n.uex),
            },
            default_channel: self.config().default_channel,
            safe_gpu: a.safe_gpu,
            advanced: AdvancedSettings {
                developer_mode: a.dev.developer_mode,
            },
            starorg: starorg::settings_of(&self.config().sync),
        }
    }

    // -----------------------------------------------------------------------------------
    // Blueprints
    // -----------------------------------------------------------------------------------

    fn blueprint_lock(&self, id: &str, ch: &Channel) -> CmdResult<lock::Lock> {
        let p = self.dirs().locks().join(format!(
            "{id}-{}-blueprints.lock",
            ch.name.to_ascii_uppercase()
        ));
        lock::try_lock(&p).map_err(|e| busy_or(e, &format!("{} blueprint store", ch.name)))
    }

    /// Read new log lines into the store (registered installs only; waits for a scan running
    /// in this app, skipped while another process holds the store lock).
    fn scan(&self, f: &Found, ch: &Channel) {
        let _one = self.scans_lock();
        self.scan_held(f, ch);
    }

    fn scans_lock(&self) -> MutexGuard<'_, ()> {
        self.scans.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// [`Self::scan`] with `scans` already held by the caller.
    fn scan_held(&self, f: &Found, ch: &Channel) {
        let Some(id) = &f.id else { return };
        let Ok(_l) = self.blueprint_lock(id, ch) else {
            return;
        };
        let data = self.data_dir(f, ch);
        let Ok(mut store) = Store::load(&data) else {
            return;
        };
        let r = logscan::scan_if(
            self.prefs().import_game_log,
            &ch.dir,
            &data,
            &mut store,
            None,
        );
        let repaired = r.repaired.as_ref().filter(|x| x.changed());
        if r.disabled && repaired.is_none() {
            return;
        }
        if let Err(e) = store.save(&data) {
            self.log(
                LogLevel::Warn,
                Some(JobKind::Import),
                Some(&ch.name),
                &format!("blueprint scan not saved: {e}"),
            );
            return;
        }
        self.after_merge(&ch.name, &store);
        if let Some(x) = repaired {
            self.log(
                LogLevel::Info,
                Some(JobKind::Import),
                Some(&ch.name),
                &format!(
                    "Owned blueprints updated for this version: {}. Backup in {}",
                    x.summary(),
                    x.backup
                        .as_ref()
                        .map_or_else(|| "-".into(), |b| b.display().to_string())
                ),
            );
            self.forget_tables(f, ch);
        }
        if !r.merged.is_empty() {
            self.log(
                LogLevel::Info,
                Some(JobKind::Import),
                Some(&ch.name),
                &format!(
                    "Merged {} reset character{} into the newest one (same account and name)",
                    r.merged.len(),
                    if r.merged.len() == 1 { "" } else { "s" }
                ),
            );
            self.forget_tables(f, ch);
        }
        if r.disabled {
            return;
        }
        if !r.new_blueprints.is_empty() {
            self.log(
                LogLevel::Info,
                Some(JobKind::Import),
                Some(&ch.name),
                &format!(
                    "Read {} new blueprints from the logs",
                    r.new_blueprints.len()
                ),
            );
        }
    }

    /// The finder catalogue of a channel's build from cached game data only (never extracts).
    /// A build already in memory is reused; otherwise the cache entry is read, the catalogue
    /// kept and the game data dropped again, so "All channels" does not hold every build.
    /// `current` (the channel the screens work on): read through `loaded`, so the build stays
    /// in memory for Strings, Ships and Patches instead of being read from disk again.
    fn bp_catalog(&self, f: &Found, ch: &Channel, current: bool) -> Option<BpCatalog> {
        let id = f.id.clone()?;
        let fp = ch.fingerprint().ok()?;
        let k = KeyInputs::local(&fp, &self.ctx().starbreaker_commit);
        let key = k.key();
        let slot = (id.clone(), ch.name.clone());
        if let Some((kk, c)) = self.mem().bp_catalogs.get(&slot)
            && *kk == key
        {
            return Some(c.clone());
        }
        let resident = self.mem().data.get(&slot).filter(|d| d.key == key).cloned();
        let d = match resident {
            Some(d) => d,
            None if current => self.loaded(f, ch, false).ok()?,
            None if self.ctx().use_cache => {
                let l = cache::load(
                    &self.dirs().channel_cache(&id, &ch.name),
                    &k,
                    self.ctx().min_stock_keys,
                )
                .ok()?;
                Arc::new(LoadedData::new(key.clone(), l))
            }
            None => return None,
        };
        let c = Arc::new(bp_finder::catalog(&d.game_data, &d.stock, Some(&d.groups)));
        self.mem().bp_catalogs.insert(slot, (key, c.clone()));
        Some(c)
    }

    /// One channel's part of the finder: its catalogue and every character's owned set, read
    /// from the channel data folder as stored (ownership is per channel and per character).
    fn finder_input(&self, f: &Found, ch: &Channel, current: bool) -> bp_finder::ChannelInput {
        let mut inp = bp_finder::ChannelInput {
            channel: ch.name.clone(),
            ..Default::default()
        };
        if f.id.is_none() {
            return inp;
        }
        let data = self.data_dir(f, ch);
        // Reading never creates folders: only a channel that already has blueprint data gets
        // the incremental log scan, unless `list_characters` scanned it a moment ago. Held
        // across the check, so a `list_characters` scan running now is waited for and reused.
        if blueprints::dir(&data).is_dir() {
            let _one = self.scans_lock();
            if !self.just_listed(f, ch) {
                self.scan_held(f, ch);
            }
        }
        inp.characters = blueprints::load_characters(&data).unwrap_or_default();
        for g in inp.characters.characters.keys() {
            match blueprints::load_set(&data, g) {
                Ok(mut set) => {
                    if set.geid.is_none() {
                        // Placed by its file name; said once per read (no ids in the log).
                        self.log(
                            LogLevel::Warn,
                            Some(JobKind::Import),
                            Some(&ch.name),
                            "a blueprint set file has no character id; read it as its file name says",
                        );
                        set.geid = Some(g.clone());
                    }
                    inp.sets.push(set);
                }
                Err(_) => {
                    self.log(
                        LogLevel::Warn,
                        Some(JobKind::Import),
                        Some(&ch.name),
                        "a blueprint set file could not be read; its blueprints are not shown",
                    );
                    inp.unreadable += 1;
                }
            }
        }
        inp.unattributed = blueprints::load_unattributed(&data).ok();
        inp.catalog = self.bp_catalog(f, ch, current).map(|c| c.as_ref().clone());
        inp
    }

    /// `list_characters` scanned this channel's logs less than [`SCAN_FRESH`] ago.
    fn just_listed(&self, f: &Found, ch: &Channel) -> bool {
        let Some(id) = &f.id else { return false };
        self.mem()
            .listed
            .as_ref()
            .is_some_and(|((i, c), at)| i == id && *c == ch.name && at.elapsed() < SCAN_FRESH)
    }

    /// The channel `list_characters` last served (any age): the Blueprints screen's channel.
    fn is_listed(&self, f: &Found, ch: &Channel) -> bool {
        let Some(id) = &f.id else { return false };
        self.mem()
            .listed
            .as_ref()
            .is_some_and(|((i, c), _)| i == id && *c == ch.name)
    }

    /// After characters were merged: StarOrg push ticks that name a merged geid follow it.
    fn after_merge(&self, channel: &str, store: &Store) {
        if store.characters.aliases.is_empty() {
            return;
        }
        let mut a = self.app_file();
        let Some(ticked) = a.sync.get_mut(channel) else {
            return;
        };
        let mut next: Vec<String> = Vec::new();
        for g in ticked.iter() {
            let c = store.characters.canonical(g);
            if !next.contains(&c) {
                next.push(c);
            }
        }
        if *ticked != next {
            *ticked = next;
            if let Err(e) = self.save_app_file(&a) {
                self.log(
                    LogLevel::Warn,
                    None,
                    Some(channel),
                    &format!("push characters not updated after a merge: {}", e.message),
                );
            }
        }
    }

    /// `reuse_last`: the export the last real import on this channel read (`import.json`).
    /// `None` opens the picker: not asked for, never imported, or the file is gone.
    fn reuse_export(&self, req: &ImportRequest) -> CmdResult<Option<PathBuf>> {
        if !req.reuse_last {
            return Ok(None);
        }
        // The file the dry run just read (the import dialog's confirm), while it still exists.
        let key = (req.target.install.clone(), req.target.channel.clone());
        if let Some((k, p)) = &self.mem().last_sc_pick
            && *k == key
            && p.is_file()
        {
            return Ok(Some(p.clone()));
        }
        let f = self.resolve(&req.target)?;
        let ch = f.channel(&req.target.channel)?;
        let data = self.data_dir(&f, ch);
        // The file the last real import read; gone (moved, renamed) means the picker.
        Ok(bp_import::last_import(&data)
            .smart_citizen
            .filter(|p| p.is_file()))
    }

    /// The folder of the export the last real import on this channel read, when it still
    /// exists (from our own `import.json`); `None` lets the picker open in Downloads.
    fn last_export_dir(&self, req: &ImportRequest) -> Option<PathBuf> {
        let f = self.resolve(&req.target).ok()?;
        let ch = f.channel(&req.target.channel).ok()?;
        bp_import::last_import(&self.data_dir(&f, ch))
            .smart_citizen?
            .parent()
            .filter(|d| d.is_dir())
            .map(Path::to_path_buf)
    }

    fn characters_view(&self, f: &Found, ch: &Channel) -> CmdResult<CharactersView> {
        let data = self.data_dir(f, ch);
        if f.id.is_none() {
            return Ok(CharactersView {
                channel: ch.name.clone(),
                characters: Vec::new(),
                active: ActiveCharacter {
                    mode: CharacterMode::Auto,
                    character: None,
                    resolved: None,
                    fallback_from: None,
                    larger: None,
                },
                unattributed: 0,
                last_scan: None,
                notice: None,
                last_import: None,
            });
        }
        let chars = blueprints::load_characters(&data).map_err(io)?;
        let mut by_name: BTreeMap<String, Vec<&blueprints::Character>> = BTreeMap::new();
        for c in chars.characters.values() {
            by_name
                .entry(blueprints::normalize(&c.name))
                .or_default()
                .push(c);
        }
        let notice = by_name.values().find(|v| v.len() > 1).map(|v| {
            let accounts: std::collections::BTreeSet<&str> =
                v.iter().filter_map(|c| c.account_id.as_deref()).collect();
            let why = if accounts.len() == v.len() {
                "on different accounts in the game logs"
            } else {
                "but the game logs do not show they are the same character"
            };
            format!(
                "{} characters are named {} {why}, so their blueprints are kept apart. To mark blueprints from all of them, choose Any of my characters; or re-import from Smart Citizen into your current character (the newest).",
                v.len(),
                v[0].name
            )
        });
        let setting = ChannelState::load(&data)
            .ok()
            .and_then(|s| s.active_character);
        // A setting written before a merge names the old geid: show the kept one.
        let (mode, character) = match setting.as_deref() {
            None => (CharacterMode::Auto, None),
            Some("any") => (CharacterMode::Union, None),
            Some(g) => (CharacterMode::Manual, Some(chars.canonical(g))),
        };
        let pick = blueprints::auto_character(&data, &chars).unwrap_or_default();
        Ok(CharactersView {
            channel: ch.name.clone(),
            characters: chars
                .characters
                .iter()
                .map(|(g, c)| CharacterView {
                    id: g.clone(),
                    name: c.name.clone(),
                    last_seen: c.last_seen.clone(),
                    owned: blueprints::load_set(&data, g)
                        .map(|s| s.entries.len() as u32)
                        .unwrap_or(0),
                })
                .collect(),
            active: ActiveCharacter {
                mode,
                character,
                resolved: pick.used,
                fallback_from: pick.fallback_from,
                larger: pick.larger,
            },
            unattributed: blueprints::load_unattributed(&data)
                .map(|s| s.entries.len() as u32)
                .unwrap_or(0),
            last_scan: std::fs::metadata(blueprints::dir(&data).join(blueprints::SCAN_JSON))
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| fsutil::rfc3339(d.as_secs())),
            notice,
            last_import: bp_import::last_import(&data)
                .smart_citizen
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())),
        })
    }

    // -----------------------------------------------------------------------------------
    // Hooks
    // -----------------------------------------------------------------------------------

    /// Wine prefix and the folders hook edits must never touch (CLI `places`).
    fn hook_places(&self) -> (Option<PathBuf>, Vec<PathBuf>) {
        let inst = self.installs().into_iter().next().map(|f| f.install);
        let prefix = inst.as_ref().and_then(|i| {
            i.prefix
                .clone()
                .or_else(|| starorg_core::running::enclosing_prefix(&i.root))
        });
        let mut forbidden = Vec::new();
        if let Some(i) = &inst {
            forbidden.push(i.root.clone());
        }
        if let Some(p) = &prefix {
            forbidden.push(p.join("drive_c"));
        }
        (prefix, forbidden)
    }

    fn block_opts() -> hooks::BlockOpts {
        hooks::BlockOpts {
            bin: None,
            game_dir: None,
            timeout: None,
            notify: false,
        }
    }

    fn cli_on_path() -> bool {
        let exe = if cfg!(windows) {
            "starorg-codex.exe"
        } else {
            "starorg-codex"
        };
        std::env::var_os("PATH")
            .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(exe).is_file()))
    }

    fn hooks_view(&self) -> HooksStatus {
        let supported = cfg!(target_os = "linux");
        if !supported {
            return HooksStatus {
                supported,
                cli_on_path: Self::cli_on_path(),
                scripts: Vec::new(),
            };
        }
        let (prefix, forbidden) = self.hook_places();
        let scripts = hooks::Kind::ALL
            .iter()
            .map(|&kind| {
                let k = match kind {
                    hooks::Kind::Prelaunch => HookKind::Prelaunch,
                    hooks::Kind::Postexit => HookKind::Postexit,
                };
                let Some(path) = prefix.as_ref().map(|p| p.join(kind.script_name())) else {
                    return HookScript {
                        kind: k,
                        path: None,
                        is_link: false,
                        status: HookState::Missing,
                    };
                };
                match hooks::locate(kind, &path, &forbidden) {
                    Err((true, _)) => HookScript {
                        kind: k,
                        path: Some(path_text(&path)),
                        is_link: false,
                        status: HookState::Missing,
                    },
                    Err((false, why)) => HookScript {
                        kind: k,
                        path: Some(path_text(&path)),
                        is_link: false,
                        status: HookState::Broken { why },
                    },
                    Ok(s) => {
                        let text = hooks::read(&s).unwrap_or_default();
                        let status =
                            match hooks::inspect(&text, &hooks::block(kind, &Self::block_opts())) {
                                hooks::BlockState::Absent => HookState::Absent,
                                hooks::BlockState::Current => HookState::Current,
                                hooks::BlockState::Outdated => HookState::Outdated,
                                hooks::BlockState::Broken { why } => HookState::Broken { why },
                            };
                        HookScript {
                            kind: k,
                            path: Some(path_text(&s.path)),
                            is_link: s.is_link,
                            status,
                        }
                    }
                }
            })
            .collect();
        HooksStatus {
            supported,
            cli_on_path: Self::cli_on_path(),
            scripts,
        }
    }

    fn hooks_edit(&self, install_: bool) -> CmdResult<HooksStatus> {
        if !cfg!(target_os = "linux") {
            return Err(err(
                ErrorCode::Invalid,
                "LUG Helper hooks are Linux only; on Windows use Check now.",
            ));
        }
        let (prefix, forbidden) = self.hook_places();
        let prefix = prefix.ok_or_else(|| {
            err(
                ErrorCode::NotFound,
                "No Wine prefix found for the install; hooks live in the prefix.",
            )
        })?;
        let backup_dir = self.dirs().data.join("hooks").join("backups");
        let mut problems = Vec::new();
        for kind in hooks::Kind::ALL {
            let path = prefix.join(kind.script_name());
            let script = match hooks::locate(kind, &path, &forbidden) {
                Ok(s) => s,
                Err((_, why)) => {
                    problems.push(format!("{}: {why}", kind.script_name()));
                    continue;
                }
            };
            let text = hooks::read(&script).map_err(io)?;
            let want = hooks::block(kind, &Self::block_opts());
            let state = hooks::inspect(&text, &want);
            let new = match (install_, state) {
                (true, hooks::BlockState::Current) | (false, hooks::BlockState::Absent) => None,
                (true, _) => Some(hooks::install(&text, &want)),
                (false, _) => {
                    Some(hooks::uninstall(&text).map(|o| o.unwrap_or_else(|| text.clone())))
                }
            };
            match new {
                None => {}
                Some(Err(e)) => problems.push(format!("{}: {e}", kind.script_name())),
                Some(Ok(new)) => match hooks::write(&script, &text, &new, &backup_dir) {
                    Ok(b) => self.log(
                        LogLevel::Info,
                        Some(JobKind::Hook),
                        None,
                        &format!(
                            "{} {} (backup {})",
                            if install_ {
                                "Added the StarOrg block to"
                            } else {
                                "Removed the StarOrg block from"
                            },
                            kind.script_name(),
                            b.display()
                        ),
                    ),
                    Err(e) => problems.push(format!("{}: {e}", kind.script_name())),
                },
            }
        }
        if !problems.is_empty() {
            let mut e = err(ErrorCode::Invalid, "Some hook scripts were not changed.");
            e.detail = Some(problems.join("\n"));
            return Err(e);
        }
        Ok(self.hooks_view())
    }

    // -----------------------------------------------------------------------------------
    // Fleet and ship names (addendum §6)
    // -----------------------------------------------------------------------------------

    /// The build's hull catalogue; `None` while its game data has not been read (and
    /// `extract` is off).
    fn hull_catalog(&self, f: &Found, ch: &Channel, extract: bool) -> CmdResult<Option<Catalog>> {
        if f.id.is_none() && !extract {
            return Ok(None);
        }
        match self.loaded(f, ch, extract) {
            Ok(d) => Ok(Some(Catalog::build(&d.game_data, &d.stock))),
            Err(e) if !extract && e.code == ErrorCode::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn fleet_lock(&self) -> CmdResult<lock::Lock> {
        lock::try_lock(&self.dirs().locks().join("fleet.lock"))
            .map_err(|e| busy_or(e, "another fleet import is running"))
    }

    /// Hulls new to the fleet go to the end of the shared order list.
    fn append_to_order(&self, added: &[String]) -> CmdResult<usize> {
        if added.is_empty() {
            return Ok(0);
        }
        let mut s = fleet_store::load_shared(self.dirs()).map_err(io)?;
        let n = s.append(added);
        if n > 0 {
            fleet_store::save_shared(self.dirs(), &s).map_err(io)?;
        }
        Ok(n)
    }

    /// "Remove all ships first" on a real import; the fleet lock is held.
    fn prune_shared_order(
        &self,
        out: &fleet_import::Outcome,
        cat: &Catalog,
    ) -> CmdResult<Vec<RemovedShip>> {
        let mut s = fleet_store::load_shared(self.dirs()).map_err(io)?;
        let removed = super::fleet::prune_order(&mut s, out, cat);
        if !removed.is_empty() {
            fleet_store::save_shared(self.dirs(), &s).map_err(io)?;
        }
        Ok(removed)
    }

    fn fleet_view_of(
        &self,
        f: &Found,
        ch: &Channel,
        cat: Option<&Catalog>,
    ) -> CmdResult<FleetView> {
        let fleet = FleetFile::load(self.dirs()).map_err(io)?;
        let data = self.data_dir(f, ch);
        let profile = self.profile_file(f, ch)?;
        let eff = fleet_store::effective(self.dirs(), &profile);
        let shared = fleet_store::load_shared(self.dirs()).unwrap_or_default();
        let (_, overrides, _) = state::load_overrides(&data).map_err(io)?;
        let settings_file = if eff.scope == fleet_store::Scope::Channel {
            state::Profile::path(&data)
        } else {
            fleet_store::ship_names_path(self.dirs())
        };
        Ok(super::fleet::view(super::fleet::ViewParts {
            channel: &ch.name,
            catalog: cat,
            fleet: &fleet,
            effective: &eff,
            shared: &shared,
            overrides: &overrides,
            fleet_file: path_text(&fleet_store::fleet_path(self.dirs())),
            settings_file: path_text(&settings_file),
        }))
    }

    fn resolve(&self, r: &ChannelRef) -> CmdResult<Found> {
        let f = self.find(&r.install)?;
        f.channel(&r.channel)?;
        Ok(f)
    }
}

impl Backend for RealBackend {
    fn overview(&self) -> CmdResult<Overview> {
        let found = self.installs();
        let installs: Vec<InstallSummary> = found.iter().map(|f| self.install_summary(f)).collect();
        let first_run = !installs.iter().any(|i| i.channels.iter().any(|c| c.set_up));
        let active = installs
            .iter()
            .find(|i| i.saved && i.channels.iter().any(|c| c.set_up))
            .or_else(|| installs.iter().find(|i| i.saved))
            .map(|i| i.id.clone());
        Ok(Overview {
            contract: CONTRACT_VERSION,
            app_version: self.ctx().app_version.clone(),
            first_run,
            default_channel: self.config().default_channel,
            active_install: active,
            installs,
            job: self.mem().job.clone(),
        })
    }

    fn discover_installs(&self, _deep: bool) -> CmdResult<Vec<InstallSummary>> {
        Ok(self
            .installs()
            .iter()
            .map(|f| self.install_summary(f))
            .collect())
    }

    fn pick_install_folder(&self) -> CmdResult<Option<InstallSummary>> {
        let Some(d) = self.cfg.dialogs.get() else {
            return Ok(None);
        };
        let Some(dir) = d.pick_folder("Choose the Star Citizen install folder") else {
            return Ok(None);
        };
        if self.discover_folder(&dir).is_none() {
            return Err(err(
                ErrorCode::NotFound,
                format!(
                    "No Star Citizen channels (LIVE, PTU, ...) in {}. Pick the StarCitizen folder that holds them, or the Wine prefix.",
                    dir.display()
                ),
            ));
        }
        self.mem().manual.push(dir.clone());
        let found = self.installs();
        let f = self
            .discover_folder(&dir)
            .and_then(|i| {
                let key = fsutil::canonical_lenient(&i.root);
                found
                    .into_iter()
                    .find(|f| fsutil::canonical_lenient(&f.install.root) == key)
            })
            .ok_or_else(|| internal("picked install vanished"))?;
        Ok(Some(self.install_summary(&f)))
    }

    fn plan_setup(&self, req: SetupRequest) -> CmdResult<SetupPlan> {
        let mut f = self.find(&req.install)?;
        let mut channels = Vec::new();
        let mut warnings = Vec::new();
        let wanted: Vec<&ChannelConsent> = req.channels.iter().filter(|c| c.consent).collect();
        if !wanted.is_empty() {
            // The dry-run build reads game data into our cache, keyed by the install id.
            self.register(&mut f)?;
        }
        let all = patch::load_patches(self.dirs(), &[]);
        let disabled = if req.fix_known_bugs {
            Vec::new()
        } else {
            convert::builtin_patch_ids(&all)
        };
        let eprof =
            starorg_core::enhance::profile::Profile::preset(convert::preset_name(req.preset))
                .map_err(internal)?;
        for c in wanted {
            let ch = f.channel(&c.channel)?;
            let (lang, value) = Self::language(ch);
            if lang == Language::Other && !c.switch_language {
                warnings.push(Warning {
                    kind: "language".into(),
                    detail: format!(
                        "{}: the game language is {}; it will be skipped",
                        ch.name,
                        value.as_deref().unwrap_or("not English")
                    ),
                });
                continue;
            }
            let ini = apply::locate_ini(&ch.dir).map_err(internal)?;
            let cfg = usercfg::locate(&ch.dir).ok();
            let s = self.summary(&f, ch);
            // A channel that is already set up keeps its own profile (setup does not touch
            // it), so its counts come from that profile, not from the wizard's preset.
            let keep =
                self.is_set_up(&f, ch) && state::Profile::path(&self.data_dir(&f, ch)).is_file();
            let chosen = (!keep).then_some((&eprof, disabled.as_slice()));
            let counts = match self.build_table(&f, ch, chosen, true) {
                Ok(t) => t.counts.clone(),
                // Another process is reading: the wizard waits and asks again.
                Err(e) if e.code == ErrorCode::Busy => return Err(e),
                Err(e) => {
                    warnings.push(Warning {
                        kind: "game-data".into(),
                        detail: format!("{}: {}", ch.name, e.message),
                    });
                    convert::zero_counts()
                }
            };
            channels.push(PlannedChannel {
                channel: ch.name.clone(),
                target: path_text(&ini.path),
                target_is_link: ini.is_link,
                link_target: ini.is_link.then(|| path_text(&ini.real)),
                backup_dir: path_text(&backup::backups_dir(&self.data_dir(&f, ch))),
                user_cfg_line: (lang != Language::English).then(|| usercfg::OUR_LINE.to_string()),
                user_cfg_path: cfg.as_ref().map(|c| path_text(&c.path)),
                replaces_tool: match &s.status {
                    ApplyState::ChangedByAnotherTool { tool } => {
                        Some(tool.clone().unwrap_or_else(|| "another tool".into()))
                    }
                    _ => None,
                },
                counts,
            });
        }
        Ok(SetupPlan {
            install: req.install,
            channels,
            warnings,
        })
    }

    fn setup(&self, req: SetupRequest) -> CmdResult<SetupResult> {
        let mut f = self.find(&req.install)?;
        for c in req.channels.iter().filter(|c| c.consent) {
            let ch = f.channel(&c.channel)?;
            let (lang, value) = Self::language(ch);
            if lang == Language::Other && !c.switch_language {
                let mut e = err(
                    ErrorCode::NeedsDecision,
                    format!(
                        "{}: the game language is not English. Choose whether to switch it.",
                        ch.name
                    ),
                );
                e.detail = value;
                return Err(e);
            }
        }
        let id = self.register(&mut f)?;
        let all = patch::load_patches(self.dirs(), &[]);
        let mut set_up = Vec::new();
        let mut skipped = Vec::new();
        for c in &req.channels {
            let ch = f.channel(&c.channel)?;
            if !c.consent {
                skipped.push(ch.name.clone());
                continue;
            }
            let data = self.dirs().channel_dir(&id, &ch.name);
            let mut st = ChannelState::load(&data).map_err(io)?;
            let had_consent = st.consent;
            st.consent = true;
            if !had_consent || st.setup_time.is_none() {
                st.setup_time = Some(now());
            }
            st.switch_language = c.switch_language;
            // Running setup again (Settings, "Set up PTU") must not wipe a channel's
            // settings: an existing profile.toml is kept as it is.
            if state::Profile::path(&data).is_file() {
                st.save(&data).map_err(io)?;
                set_up.push(ch.name.clone());
                continue;
            }
            let mut prof = state::Profile::preset(convert::preset_name(req.preset))
                .ok_or_else(|| internal("unknown preset"))?;
            if !req.fix_known_bugs {
                let mut p = convert::profile_of(&prof)?;
                p.patches_disabled = convert::builtin_patch_ids(&all);
                convert::store_profile(&mut prof, &p)?;
            }
            st.save(&data).map_err(io)?;
            prof.save(&data).map_err(io)?;
            set_up.push(ch.name.clone());
        }
        let mut cfg = Config::load(self.dirs()).unwrap_or_default();
        if cfg.default_channel.is_none()
            && let Some(first) = set_up.first()
        {
            cfg.default_channel = Some(first.clone());
            cfg.save(self.dirs()).map_err(io)?;
        }
        // Published game data is not built yet (plan_setup warns); nothing is stored for it.
        self.log(
            LogLevel::Info,
            None,
            None,
            &format!(
                "Set up {} (skipped: {})",
                if set_up.is_empty() {
                    "nothing".into()
                } else {
                    set_up.join(", ")
                },
                if skipped.is_empty() {
                    "none".into()
                } else {
                    skipped.join(", ")
                }
            ),
        );
        Ok(SetupResult {
            install: id,
            set_up,
            skipped,
        })
    }

    fn channel_status(&self, req: ChannelRef) -> CmdResult<ChannelSummary> {
        let f = self.resolve(&req)?;
        let ch = f.channel(&req.channel)?;
        // "On the next run of any command for that channel" (spec 7.1, journal).
        let t = Target::new(&f.install, f.id.clone(), ch);
        if let Some(Ok(Some(m))) = apply::recover_if_idle(self.ctx(), &t) {
            self.log(LogLevel::Warn, Some(JobKind::Apply), Some(&ch.name), &m);
        }
        Ok(self.summary(&f, ch))
    }

    fn apply(&self, req: ApplyRequest, progress: Progress<'_>) -> CmdResult<ApplyReport> {
        let mut f = self.resolve(&req.target)?;
        let name = req.target.channel.clone();
        let mut job = self.job_start(JobKind::Apply, Some(&name), 3, "Checking the channel");
        progress(job.clone());
        let id = match self.register(&mut f) {
            Ok(id) => id,
            Err(e) => {
                self.job_end(job, JobPhase::Failed, &e.message, Some(progress));
                return Err(e);
            }
        };
        let ch = f.channel(&name)?;
        let src = match self.cfg.source.source(self.dirs()) {
            Ok(s) => s,
            Err(e) => {
                self.job_end(job, JobPhase::Failed, &e, Some(progress));
                return Err(internal(e));
            }
        };
        self.job_step(
            &mut job,
            2,
            if req.dry_run {
                "Reading game data and building (dry run)"
            } else {
                "Reading game data, building and writing"
            },
            Some(progress),
        );
        let t = Target::new(&f.install, Some(id.clone()), ch);
        let opts = ApplyOpts {
            dry_run: req.dry_run,
            obtain: self.obtain_provider(&f, true),
            ..ApplyOpts::default()
        };
        let r = {
            // Waits for a read the wizard or a screen started, then finds it in the cache.
            let _reading = self.reads.lock().unwrap_or_else(|e| e.into_inner());
            apply::apply(self.ctx(), &t, &opts, src.as_ref())
        };
        if let Some(o) = &r.obtain {
            self.log(
                LogLevel::Info,
                Some(JobKind::Apply),
                Some(&name),
                &format!(
                    "{name}: where to get: sources for {} items in {:.1} s",
                    o.items,
                    o.ms as f64 / 1000.0
                ),
            );
        }
        let data = t.data_dir(self.dirs());
        let out = convert::report(&r, Some(&backup::backups_dir(&data)));
        if out.result == ApplyResult::Applied
            && let Ok(Some(a)) =
                fsutil::read_json::<apply::Applied>(&data.join(state::APPLIED_JSON))
        {
            let last = LastApply {
                time: a.time,
                result: out.result,
                changed: out.counts.changed,
                warnings: out.warnings.len() as u32,
                dropped: out.counts.dropped,
                duration_ms: out.duration_ms,
            };
            let _ = fsutil::write_json(&data.join(LAST_APPLY_JSON), &last);
        }
        self.forget_tables(&f, ch);
        let level = match out.result {
            ApplyResult::Failed => LogLevel::Error,
            ApplyResult::Skipped | ApplyResult::NeedsDecision | ApplyResult::NeedsSetup => {
                LogLevel::Warn
            }
            _ if !out.warnings.is_empty() => LogLevel::Warn,
            _ => LogLevel::Info,
        };
        let mut msg = format!("{}: {}", name, kebab(&out.result));
        if let Some(why) = &out.reason {
            msg.push_str(&format!(" ({why})"));
        }
        // A warning row must say what the warning is, not only "applied" (Activity list).
        if let Some(w) = out.warnings.first() {
            let n = out.warnings.len();
            msg.push_str(&format!(
                " with {n} warning{}: {}",
                if n == 1 { "" } else { "s" },
                w.detail
            ));
            if n > 1 {
                msg.push_str(&format!(" (+{} more)", n - 1));
            }
        }
        self.log(level, Some(JobKind::Apply), Some(&name), &msg);
        let phase = if out.result == ApplyResult::Failed {
            JobPhase::Failed
        } else {
            JobPhase::Done
        };
        self.job_end(job, phase, &msg, Some(progress));
        Ok(out)
    }

    fn remove_changes(&self, req: RemoveRequest) -> CmdResult<ApplyReport> {
        let f = self.resolve(&req.target)?;
        let ch = f.channel(&req.target.channel)?;
        let job = self.job_start(JobKind::Remove, Some(&ch.name), 1, "Removing changes");
        let t = Target::new(&f.install, f.id.clone(), ch);
        let r = apply::remove(self.ctx(), &t, req.allow_foreign);
        let out = convert::report(&r, Some(&backup::backups_dir(&t.data_dir(self.dirs()))));
        self.forget_tables(&f, ch);
        self.log(
            if out.result == ApplyResult::Failed {
                LogLevel::Error
            } else {
                LogLevel::Info
            },
            Some(JobKind::Remove),
            Some(&ch.name),
            &format!("{}: {}", ch.name, kebab(&out.result)),
        );
        let phase = if out.result == ApplyResult::Failed {
            JobPhase::Failed
        } else {
            JobPhase::Done
        };
        self.job_end(job, phase, kebab(&out.result).as_str(), None);
        Ok(out)
    }

    fn undo_last_apply(&self, req: ChannelRef) -> CmdResult<ApplyReport> {
        let list = self.list_backups(req.clone())?;
        match list.offers.undo_last_apply {
            Some(b) => self.restore_backup(RestoreRequest {
                target: req,
                backup: b,
                allow_build_mismatch: false,
            }),
            None => {
                let f = self.resolve(&req)?;
                let ch = f.channel(&req.channel)?;
                Ok(convert::plain_report(
                    &ch.name,
                    convert::build(ch.build.as_ref()),
                    ApplyResult::NothingToDo,
                    "no earlier apply of this build to go back to",
                ))
            }
        }
    }

    fn restore_backup(&self, req: RestoreRequest) -> CmdResult<ApplyReport> {
        let f = self.resolve(&req.target)?;
        let ch = f.channel(&req.target.channel)?;
        let job = self.job_start(JobKind::Restore, Some(&ch.name), 1, "Restoring a backup");
        let t = Target::new(&f.install, f.id.clone(), ch);
        let r = apply::restore(self.ctx(), &t, &req.backup, req.allow_build_mismatch);
        let out = convert::report(&r, Some(&backup::backups_dir(&t.data_dir(self.dirs()))));
        self.forget_tables(&f, ch);
        self.log(
            if out.result == ApplyResult::Failed {
                LogLevel::Error
            } else {
                LogLevel::Info
            },
            Some(JobKind::Restore),
            Some(&ch.name),
            &format!("{}: {} {}", ch.name, kebab(&out.result), req.backup),
        );
        let phase = if out.result == ApplyResult::Failed {
            JobPhase::Failed
        } else {
            JobPhase::Done
        };
        self.job_end(job, phase, kebab(&out.result).as_str(), None);
        Ok(out)
    }

    fn list_backups(&self, req: ChannelRef) -> CmdResult<BackupList> {
        let f = self.resolve(&req)?;
        let ch = f.channel(&req.channel)?;
        Ok(self.backup_list(&f, ch))
    }

    fn backup_now(&self, req: ChannelRef) -> CmdResult<BackupView> {
        let mut f = self.resolve(&req)?;
        let id = self.register(&mut f)?;
        let ch = f.channel(&req.channel)?;
        let _l = self.channel_lock(&id, ch)?;
        let data = self.data_dir(&f, ch);
        let ini = apply::locate_ini(&ch.dir).map_err(internal)?;
        let bytes = if ini.exists {
            Some(std::fs::read(&ini.real).map_err(io)?)
        } else {
            None
        };
        let outputs = Outputs::load(self.dirs()).map_err(io)?;
        let (origin, tool) = match &bytes {
            Some(b) if !outputs.contains(&fsutil::sha256_hex(b)) => {
                (backup::FileOrigin::Foreign, apply::foreign_tool(b, &ch.dir))
            }
            Some(_) => (backup::FileOrigin::Ours, None),
            None => (backup::FileOrigin::None, None),
        };
        let cur = backup::Current {
            real_path: &ini.real,
            bytes: bytes.as_deref(),
            origin,
            foreign_tool: tool,
        };
        let made = backup::create(
            &data,
            backup::Reason::Manual,
            &self.ctx().app_version,
            ch.build.as_ref(),
            &cur,
        )
        .map_err(io)?;
        self.sizes_changed();
        let b = match made {
            Some(b) => b,
            // Same bytes as the newest backup: that one is the backup.
            None => backup::list(&data)
                .pop()
                .ok_or_else(|| internal("no backup made"))?,
        };
        self.log(
            LogLevel::Info,
            None,
            Some(&ch.name),
            &format!("Backed up {} ({})", ch.name, b.id),
        );
        Ok(convert::backup_view(&b))
    }

    fn delete_backup(&self, req: BackupRef) -> CmdResult<BackupList> {
        let f = self.resolve(&req.target)?;
        let ch = f.channel(&req.target.channel)?;
        let id =
            f.id.clone()
                .ok_or_else(|| err(ErrorCode::NotFound, "no backups for this install"))?;
        let _l = self.channel_lock(&id, ch)?;
        let data = self.data_dir(&f, ch);
        let b = backup::find(&data, &req.backup).ok_or_else(|| {
            err(
                ErrorCode::NotFound,
                format!("backup {} not found", req.backup),
            )
        })?;
        if b.pinned() {
            return Err(err(
                ErrorCode::Invalid,
                "The file from before StarOrg Codex is kept; it cannot be deleted.",
            ));
        }
        let gone = std::fs::remove_dir_all(&b.dir);
        self.sizes_changed();
        gone.map_err(io)?;
        self.log(
            LogLevel::Info,
            None,
            Some(&ch.name),
            &format!("Deleted backup {}", req.backup),
        );
        Ok(self.backup_list(&f, ch))
    }

    fn cancel_job(&self, job: String) -> CmdResult<()> {
        if let Some(r) = self.dev_cancel(&job) {
            return r;
        }
        if let Some(r) = self.asset_cancel(&job) {
            return r;
        }
        let running = self.mem().job.as_ref().is_some_and(|j| j.job == job);
        if !running {
            return Err(err(ErrorCode::NotFound, format!("no running job {job}")));
        }
        Err(err(
            ErrorCode::Invalid,
            "This job cannot be cancelled; it stops by itself before writing if anything is wrong.",
        ))
    }

    fn build_preview(&self, req: PreviewRequest) -> CmdResult<Vec<PreviewItem>> {
        let f = self.resolve(&req.target)?;
        let ch = f.channel(&req.target.channel)?;
        let profile = match (&req.profile, req.preset) {
            (Some(p), _) => Some(p.clone()),
            (None, Some(pre)) => Some(convert::preset(pre)),
            (None, None) => None,
        };
        let eprof = profile.as_ref().map(convert::profile_from);
        let disabled = profile
            .as_ref()
            .map(|p| p.patches_disabled.clone())
            .unwrap_or_default();
        let tbl = match self.build_table(
            &f,
            ch,
            eprof.as_ref().map(|p| (p, disabled.as_slice())),
            false,
        ) {
            Ok(t) => t,
            // No game data read yet: nothing to show (the wizard reads it at review).
            Err(e) if e.code == ErrorCode::NotFound || e.code == ErrorCode::NeedsDecision => {
                return Ok(Vec::new());
            }
            Err(e) => return Err(e),
        };
        Ok(diag::preview(&tbl))
    }

    fn read_game_data(&self, req: ReadGameDataRequest) -> CmdResult<ChannelSummary> {
        let f = self.resolve(&req.target)?;
        let ch = f.channel(&req.target.channel)?;
        let job = self.job_start(JobKind::Extract, Some(&ch.name), 3, "Reading game data");
        let r = (|| {
            let d = self.loaded(&f, ch, true)?;
            // Where to get: extract its datasets whatever the saved profile says, so the
            // preview of an unsaved "obtain on" has them too. A failure is only a warning.
            let mut job = job.clone();
            self.job_step(&mut job, 2, "Reading Where to get data", None);
            let prof = starorg_core::enhance::profile::Profile {
                obtain: true,
                ..Default::default()
            };
            let provider = self.obtain_provider(&f, true);
            let (_, warnings, _) =
                apply::obtain_hints(&d.game_data, &prof, provider.as_ref(), &ch.name);
            if let Some(w) = warnings.first() {
                self.log(
                    LogLevel::Warn,
                    Some(JobKind::Extract),
                    Some(&ch.name),
                    &format!("{}: {}", ch.name, w.detail),
                );
            }
            // Shops: the ship items item-tags lacks (ship-components), so the Shops screen
            // links ship guns, missiles and components. Same worker and memory limit as the
            // Where to get datasets; a failure is only a warning.
            self.job_step(&mut job, 3, "Reading ship item data for Shops", None);
            if let Err(e) = self.warm_shop_links(&f, &ch.name) {
                self.log(
                    LogLevel::Warn,
                    Some(JobKind::Extract),
                    Some(&ch.name),
                    &format!("{}: shop item links: {e}", ch.name),
                );
            }
            Ok::<_, AppError>(())
        })();
        match &r {
            Ok(()) => {
                let msg = format!("Read the game data of {}", ch.name);
                self.log(LogLevel::Info, Some(JobKind::Extract), Some(&ch.name), &msg);
                self.job_end(job, JobPhase::Done, &msg, None);
            }
            Err(e) => self.job_end(job, JobPhase::Failed, &e.message, None),
        }
        r?;
        Ok(self.summary(&f, ch))
    }

    fn profile_schema(&self) -> CmdResult<ProfileSchema> {
        Ok(schema::profile_schema())
    }

    fn obtain_status(&self) -> CmdResult<ObtainStatus> {
        // The folder `starorg_mcp::config_for` gives the Sources card's tools.
        Ok(self.obtain_status_now())
    }

    fn uex_token_status(&self) -> CmdResult<UexTokenStatus> {
        Ok(self.uex_token_view())
    }

    fn set_uex_token(&self, req: UexTokenRequest) -> CmdResult<UexTokenStatus> {
        self.save_uex_token(req)
    }

    fn fetch_uex(&self) -> CmdResult<ObtainStatus> {
        self.run_fetch_uex()
    }

    fn loot_list(&self, req: LootListRequest) -> CmdResult<LootList> {
        Ok(self.loot_list_of(req))
    }

    fn loot_detail(&self, req: LootDetailRequest) -> CmdResult<LootDetail> {
        Ok(self.loot_detail_of(req))
    }

    fn shop_inventory(&self, req: ShopInventoryRequest) -> CmdResult<ShopDirectory> {
        Ok(self.shop_directory(req))
    }

    fn shop_items(&self, req: ShopItemsRequest) -> CmdResult<ShopItems> {
        Ok(self.shop_items_of(req))
    }

    fn commodity_list(&self, req: CommodityListRequest) -> CmdResult<CommodityDirectory> {
        Ok(self.commodity_list_of(req))
    }

    fn commodity_detail(&self, req: CommodityDetailRequest) -> CmdResult<CommodityInfo> {
        Ok(self.commodity_detail_of(req))
    }

    fn get_profile(&self, req: ChannelRef) -> CmdResult<Profile> {
        let f = self.resolve(&req)?;
        let ch = f.channel(&req.channel)?;
        convert::profile_of(&self.profile_file(&f, ch)?)
    }

    fn preset_profile(&self, preset: Preset) -> CmdResult<Profile> {
        Ok(convert::preset(preset))
    }

    fn save_profile(&self, req: SaveProfileRequest) -> CmdResult<Profile> {
        let mut f = self.resolve(&req.target)?;
        self.register(&mut f)?;
        let ch = f.channel(&req.target.channel)?;
        check_profile(&req.profile)?;
        let mut file = self.profile_file(&f, ch)?;
        convert::store_profile(&mut file, &req.profile)?;
        file.save(&self.data_dir(&f, ch)).map_err(io)?;
        self.forget_tables(&f, ch);
        self.log(
            LogLevel::Info,
            None,
            Some(&ch.name),
            &format!("Saved the {} profile", ch.name),
        );
        convert::profile_of(&file)
    }

    fn copy_profile(&self, req: CopyProfileRequest) -> CmdResult<Profile> {
        let from = self.get_profile(req.from.clone())?;
        self.save_profile(SaveProfileRequest {
            target: req.to,
            profile: from,
        })
    }

    fn query_strings(&self, req: StringsQuery) -> CmdResult<StringsPage> {
        let f = self.resolve(&req.target)?;
        let ch = f.channel(&req.target.channel)?;
        let tbl = self.build_table(&f, ch, None, false)?;
        let meta = self.loaded(&f, ch, false)?.group_meta();
        Ok(strings_query::run(
            tbl.rows.iter().map(std::borrow::Cow::Borrowed),
            tbl.rows.len() as u32,
            &req,
            &meta,
        ))
    }

    fn read_string_groups(&self, req: ChannelRef) -> CmdResult<GroupState> {
        let f = self.resolve(&req)?;
        let ch = f.channel(&req.channel)?;
        let d = self.loaded(&f, ch, false)?;
        if d.groups.exact {
            return Ok(GroupState::Exact);
        }
        let t = Target::new(&f.install, f.id.clone(), ch);
        let src = self.cfg.source.source(self.dirs()).map_err(internal)?;
        let mut w = Vec::new();
        let _reading = self.reads.lock().unwrap_or_else(|e| e.into_inner());
        let l = apply::reload_channel(self.ctx(), &t, src.as_ref(), &Budget::unlimited(), &mut w)
            .map_err(|e| {
            let code = if e.contains("another extraction") {
                ErrorCode::Busy
            } else {
                ErrorCode::Internal
            };
            err(code, format!("reading game data: {e}"))
        })?;
        let fresh = Arc::new(LoadedData::new(d.key.clone(), l));
        let state = fresh.group_meta().state;
        let slot = (f.id.clone().unwrap_or_default(), ch.name.clone());
        let mut m = self.mem();
        m.data.insert(slot.clone(), fresh);
        m.tables.remove(&slot);
        m.previews.retain(|(s, _, _)| *s != slot);
        Ok(state)
    }

    fn string_detail(&self, req: KeyRequest) -> CmdResult<StringDetail> {
        let f = self.resolve(&req.target)?;
        let ch = f.channel(&req.target.channel)?;
        let tbl = self.build_table(&f, ch, None, false)?;
        let (i, _) = tbl
            .row(&req.key)
            .ok_or_else(|| err(ErrorCode::NotFound, format!("no key {}", req.key)))?;
        Ok(tbl.detail(i, Self::override_of(&self.data_dir(&f, ch), &req.key)))
    }

    fn set_override(&self, req: OverrideRequest) -> CmdResult<OverrideResult> {
        let mut f = self.resolve(&req.target)?;
        self.register(&mut f)?;
        let ch = f.channel(&req.target.channel)?;
        let d = self.loaded(&f, ch, false)?;
        let Some(stock) = d.stock.value(&req.key) else {
            return Ok(OverrideResult {
                accepted: false,
                rule: None,
                message: Some(format!(
                    "{} is not a key of this build (or it is spelled twice in the stock file).",
                    req.key
                )),
                detail: None,
            });
        };
        if req.value.contains(['\r', '\n']) {
            return Ok(OverrideResult {
                accepted: false,
                rule: Some(OverrideRule::ControlCharacter),
                message: Some("Use \\n for a new line.".into()),
                detail: None,
            });
        }
        if req.value != stock
            && let Err(rule) = validate::check_value(stock, &req.value, validate::Op::Override, &[])
        {
            let r = OverrideRule::ALL
                .iter()
                .copied()
                .find(|x| kebab(x) == rule.name());
            return Ok(OverrideResult {
                accepted: false,
                rule: r,
                message: Some(format!("Refused: {}", rule.name())),
                detail: None,
            });
        }
        let key = d
            .stock
            .find(&req.key)
            .ok()
            .and_then(|l| d.stock.key_at(l))
            .unwrap_or(&req.key)
            .to_string();
        let data = self.data_dir(&f, ch);
        Self::write_override(&data, &key, Some(&req.value))?;
        self.forget_tables(&f, ch);
        self.log(
            LogLevel::Info,
            None,
            Some(&ch.name),
            &format!("Override set for {key}"),
        );
        let detail = self.string_detail(KeyRequest {
            target: req.target,
            key,
        })?;
        Ok(OverrideResult {
            accepted: true,
            rule: None,
            message: None,
            detail: Some(detail),
        })
    }

    fn reset_override(&self, req: KeyRequest) -> CmdResult<StringDetail> {
        let f = self.resolve(&req.target)?;
        let ch = f.channel(&req.target.channel)?;
        let data = self.data_dir(&f, ch);
        let existing = state::parse_overrides(&Self::overrides_text(&data)?)
            .0
            .into_iter()
            .filter(|o| o.key.eq_ignore_ascii_case(&req.key))
            .map(|o| o.key)
            .collect::<Vec<_>>();
        for k in existing {
            Self::write_override(&data, &k, None)?;
        }
        self.forget_tables(&f, ch);
        self.log(
            LogLevel::Info,
            None,
            Some(&ch.name),
            &format!("Override reset for {}", req.key),
        );
        self.string_detail(req)
    }

    fn list_overrides(&self, req: ChannelRef) -> CmdResult<Vec<OverrideEntry>> {
        let f = self.resolve(&req)?;
        let ch = f.channel(&req.channel)?;
        let data = self.data_dir(&f, ch);
        let stock = self.loaded(&f, ch, false).ok();
        Ok(state::parse_overrides(&Self::overrides_text(&data)?)
            .0
            .into_iter()
            .map(|o| OverrideEntry {
                original: stock
                    .as_ref()
                    .and_then(|d| d.stock.value(&o.key).map(str::to_string))
                    .unwrap_or_default(),
                key: o.key,
                value: o.value,
            })
            .collect())
    }

    fn list_patches(&self, req: ChannelRef) -> CmdResult<Vec<PatchView>> {
        let f = self.resolve(&req)?;
        let ch = f.channel(&req.channel)?;
        let disabled = self.profile_file(&f, ch)?.disabled_patches();
        let set = patch::load_patches(self.dirs(), &disabled);
        let tbl = self.build_table(&f, ch, None, false).ok();
        Ok(set
            .patches
            .iter()
            .map(|p| diag::patch_view(p, tbl.as_deref()))
            .collect())
    }

    fn set_patch_enabled(&self, req: PatchToggle) -> CmdResult<PatchView> {
        let mut f = self.resolve(&req.target)?;
        self.register(&mut f)?;
        let ch = f.channel(&req.target.channel)?;
        // An unknown id is refused before anything is saved.
        if !patch::load_patches(self.dirs(), &[])
            .patches
            .iter()
            .any(|p| p.file.id == req.id)
        {
            return Err(err(
                ErrorCode::NotFound,
                format!("no patch {} for {}", req.id, ch.name),
            ));
        }
        let mut p = self.get_profile(req.target.clone())?;
        p.patches_disabled.retain(|x| *x != req.id);
        if !req.enabled {
            p.patches_disabled.push(req.id.clone());
        }
        self.save_profile(SaveProfileRequest {
            target: req.target.clone(),
            profile: p,
        })?;
        self.list_patches(req.target)?
            .into_iter()
            .find(|v| v.id == req.id)
            .ok_or_else(|| {
                err(
                    ErrorCode::NotFound,
                    format!("no patch {} for {}", req.id, ch.name),
                )
            })
    }

    fn patch_detail(&self, req: PatchRef) -> CmdResult<PatchDetail> {
        let f = self.resolve(&req.target)?;
        let ch = f.channel(&req.target.channel)?;
        let disabled = self.profile_file(&f, ch)?.disabled_patches();
        let set = patch::load_patches(self.dirs(), &disabled);
        let p = set
            .patches
            .iter()
            .find(|p| p.file.id == req.id)
            .ok_or_else(|| err(ErrorCode::NotFound, format!("no patch {}", req.id)))?;
        let tbl = self.build_table(&f, ch, None, false).ok();
        Ok(diag::patch_detail(p, tbl.as_deref()))
    }

    fn add_user_patch(&self, req: NewPatch) -> CmdResult<PatchView> {
        if req.title.trim().is_empty() || req.key.trim().is_empty() {
            return Err(err(ErrorCode::Invalid, "A patch needs a title and a key."));
        }
        if req.op == LocOp::Replace && req.find.as_deref().unwrap_or("").is_empty() {
            return Err(err(ErrorCode::Invalid, "Replace needs the text to find."));
        }
        let slug: String = req
            .title
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        let id = user_patch_id(&req.title, &slug);
        let path = patch::user_patch_path(self.dirs(), &id);
        let set = patch::load_patches(self.dirs(), &[]);
        if path.exists() || set.patches.iter().any(|p| p.file.id == id) {
            return Err(err(
                ErrorCode::Invalid,
                format!("A patch with the id {id} exists. Pick another title."),
            ));
        }
        let text = diag::patch_toml(&id, &req);
        let name = format!("{id}.patch.toml");
        patch::parse_patch(&name, &text)
            .map_err(|e| err(ErrorCode::Invalid, format!("patch refused: {e}")))?;
        std::fs::create_dir_all(self.dirs().user_patches()).map_err(io)?;
        fsutil::write_atomic(&path, text.as_bytes()).map_err(io)?;
        self.log(
            LogLevel::Info,
            None,
            Some(&req.target.channel),
            &format!("Added patch {}", req.title.trim()),
        );
        self.list_patches(req.target)?
            .into_iter()
            .find(|v| v.id == id)
            .ok_or_else(|| internal("the new patch was not loaded"))
    }

    fn import_user_patch(&self, req: ChannelRef) -> CmdResult<PatchView> {
        let d = self
            .cfg
            .dialogs
            .get()
            .ok_or_else(|| err(ErrorCode::Cancelled, "no file chosen"))?;
        let file = d
            .pick_file("Import a patch file (.patch.toml)", PATCH_FILTER)
            .ok_or_else(|| err(ErrorCode::Cancelled, "no file chosen"))?;
        let bytes = fsutil::read_limited(&file, patch::PATCH_LIMIT)
            .map_err(io)?
            .ok_or_else(|| err(ErrorCode::NotFound, format!("{} not found", file.display())))?;
        let text = String::from_utf8(bytes)
            .map_err(|_| err(ErrorCode::Invalid, "the patch file is not UTF-8"))?;
        let p = patch::parse_patch("import.patch.toml", &text)
            .map_err(|e| err(ErrorCode::Invalid, format!("patch refused: {e}")))?;
        if patch::load_patches(self.dirs(), &[])
            .patches
            .iter()
            .any(|x| x.file.id == p.id)
        {
            return Err(err(
                ErrorCode::Invalid,
                format!("A patch with the id {} exists already.", p.id),
            ));
        }
        std::fs::create_dir_all(self.dirs().user_patches()).map_err(io)?;
        fsutil::write_atomic(&patch::user_patch_path(self.dirs(), &p.id), text.as_bytes())
            .map_err(io)?;
        self.log(
            LogLevel::Info,
            None,
            Some(&req.channel),
            &format!("Imported patch {}", p.id),
        );
        self.list_patches(req)?
            .into_iter()
            .find(|v| v.id == p.id)
            .ok_or_else(|| internal("the imported patch was not loaded"))
    }

    fn export_user_patch(&self, req: PatchRef) -> CmdResult<PatchExport> {
        let set = patch::load_patches(self.dirs(), &[]);
        let p = set
            .patches
            .iter()
            .find(|p| p.file.id == req.id)
            .ok_or_else(|| err(ErrorCode::NotFound, format!("no patch {}", req.id)))?;
        let patch::PatchOrigin::User { path } = &p.origin else {
            return Err(err(
                ErrorCode::Invalid,
                "Built-in patches ship with the app; only your own patches can be exported.",
            ));
        };
        let text = std::fs::read_to_string(path).map_err(io)?;
        let file_name = format!("{}.patch.toml", p.file.id);
        if let Some(d) = self.cfg.dialogs.get()
            && let Some(to) = d.save_file("Export patch", &file_name, PATCH_FILTER)
        {
            fsutil::write_atomic(&to, text.as_bytes()).map_err(io)?;
            self.log(
                LogLevel::Info,
                None,
                Some(&req.target.channel),
                &format!("Exported patch {} to {}", p.file.id, to.display()),
            );
        }
        Ok(PatchExport { file_name, text })
    }

    fn delete_user_patch(&self, req: PatchRef) -> CmdResult<Vec<PatchView>> {
        let set = patch::load_patches(self.dirs(), &[]);
        let p = set
            .patches
            .iter()
            .find(|p| p.file.id == req.id)
            .ok_or_else(|| err(ErrorCode::NotFound, format!("no patch {}", req.id)))?;
        let patch::PatchOrigin::User { path } = &p.origin else {
            return Err(err(
                ErrorCode::Invalid,
                "Built-in patches can be switched off, not deleted.",
            ));
        };
        std::fs::remove_file(path).map_err(io)?;
        self.log(
            LogLevel::Info,
            None,
            Some(&req.target.channel),
            &format!("Deleted patch {}", req.id),
        );
        self.list_patches(req.target)
    }

    fn activity(&self, limit: u32) -> CmdResult<Vec<LogEntry>> {
        self.mcp_to_log();
        let m = self.mem();
        Ok(m.log.iter().rev().take(limit as usize).cloned().collect())
    }

    fn get_settings(&self) -> CmdResult<Settings> {
        Ok(self.settings())
    }

    fn save_settings(&self, req: Settings) -> CmdResult<Settings> {
        let mut a = self.app_file();
        a.appearance = Some(req.appearance.clone());
        // Every network switch is stored as given (check_updates: crate::update;
        // published_dumps: Developer mode's Diff only).
        a.network = Some(req.network.clone());
        a.safe_gpu = req.safe_gpu;
        let dev_off = a.dev.developer_mode && !req.advanced.developer_mode;
        a.dev.developer_mode = req.advanced.developer_mode;
        self.save_app_file(&a)?;
        if dev_off {
            // Off: the explorer's memory is returned at once.
            self.dev.stop();
            self.assets.stop();
        }
        let mut cfg = Config::load(self.dirs()).map_err(io)?;
        let prefs = starorg::prefs_of(&req.starorg);
        let old_env = starorg::contract_env(cfg.sync.environment);
        // Tokens are per environment: switching signs the old one out (the UI asks first),
        // before the new environment is saved, so a failure leaves the old one chosen.
        if old_env != req.starorg.environment {
            self.sign_out(old_env)?;
        }
        if cfg.default_channel != req.default_channel || cfg.sync != prefs {
            cfg.default_channel = req.default_channel.clone();
            cfg.sync = prefs;
            cfg.save(self.dirs()).map_err(io)?;
        }
        Ok(self.settings())
    }

    fn list_characters(&self, req: ChannelRef) -> CmdResult<CharactersView> {
        let f = self.resolve(&req)?;
        let ch = f.channel(&req.channel)?;
        {
            // Scan and memo under one hold: a finder read waiting on it then skips its scan.
            let _one = self.scans_lock();
            self.scan_held(&f, ch);
            if let Some(id) = &f.id {
                self.mem().listed =
                    Some(((id.clone(), ch.name.clone()), std::time::Instant::now()));
            }
        }
        self.characters_view(&f, ch)
    }

    fn set_active_character(&self, req: SetActiveCharacter) -> CmdResult<CharactersView> {
        let mut f = self.resolve(&req.target)?;
        self.register(&mut f)?;
        let ch = f.channel(&req.target.channel)?;
        let data = self.data_dir(&f, ch);
        let chars = blueprints::load_characters(&data).unwrap_or_default();
        let who = match req.mode {
            CharacterMode::Auto => "auto".to_string(),
            CharacterMode::Union => "any".to_string(),
            CharacterMode::Manual => req
                .character
                .clone()
                .ok_or_else(|| err(ErrorCode::Invalid, "pick a character"))?,
        };
        let value = match Choice::parse(Some(&who), &chars) {
            Ok(Choice::Auto) => None,
            Ok(Choice::Any) => Some("any".to_string()),
            Ok(Choice::Character(g)) => Some(g),
            Err(e) => return Err(err(ErrorCode::Invalid, e.to_string())),
        };
        let mut st = ChannelState::load(&data).map_err(io)?;
        st.active_character = value;
        st.save(&data).map_err(io)?;
        apply::forget_one_off_character(&data).map_err(io)?;
        self.forget_tables(&f, ch);
        self.characters_view(&f, ch)
    }

    fn owned_blueprints(&self, req: CharacterRequest) -> CmdResult<Vec<OwnedBlueprint>> {
        let f = self.resolve(&req.target)?;
        let ch = f.channel(&req.target.channel)?;
        if f.id.is_none() {
            return Ok(Vec::new());
        }
        let data = self.data_dir(&f, ch);
        let set =
            blueprints::load_set(&data, &req.character).map_err(|e| err(ErrorCode::Invalid, e))?;
        let mut out: Vec<OwnedBlueprint> = set
            .entries
            .into_values()
            .map(|e| OwnedBlueprint {
                acquired: e.sources.iter().filter_map(|s| s.time.clone()).min(),
                sources: e.sources.iter().map(|s| s.kind.clone()).collect(),
                name: e.name,
                ambiguous: e.ambiguous,
                favorite: e.favorite,
            })
            .collect();
        out.sort_by_key(|b| b.name.to_lowercase());
        Ok(out)
    }

    fn blueprint_finder(&self, req: BlueprintFinderRequest) -> CmdResult<BlueprintFinder> {
        let f = self.find(&req.install)?;
        let names: Vec<String> = f.install.channels.iter().map(|c| c.name.clone()).collect();
        // One channel asked for = the screen's channel; in "All" the one Blueprints lists.
        let single = req.channels.len() == 1;
        let inputs = crate::contract::finder::pick(&names, &req.channels)?
            .into_iter()
            .map(|n| {
                f.channel(n)
                    .map(|ch| self.finder_input(&f, ch, single || self.is_listed(&f, ch)))
            })
            .collect::<CmdResult<Vec<_>>>()?;
        Ok(crate::contract::finder::contract(bp_finder::build(&inputs)))
    }

    fn import_smart_citizen(&self, req: ImportRequest) -> CmdResult<ImportReport> {
        let file = match self.reuse_export(&req)? {
            Some(p) => p,
            None => {
                let d = self
                    .cfg
                    .dialogs
                    .get()
                    .ok_or_else(|| err(ErrorCode::Cancelled, "no file chosen"))?;
                // Downloads, or the folder of the last export we imported here.
                let start = self.last_export_dir(&req);
                d.pick_file_in(
                    "Owned-blueprints export (JSON)",
                    JSON_FILTER,
                    start.as_deref(),
                )
                .ok_or_else(|| err(ErrorCode::Cancelled, "no file chosen"))?
            }
        };
        if req.dry_run {
            self.mem().last_sc_pick = Some((
                (req.target.install.clone(), req.target.channel.clone()),
                file.clone(),
            ));
        }
        let bytes = fsutil::read_limited(&file, bp_import::EXPORT_LIMIT)
            .map_err(io)?
            .ok_or_else(|| err(ErrorCode::NotFound, format!("{} not found", file.display())))?;
        let export = bp_import::parse_export(&bytes).map_err(|e| err(ErrorCode::Invalid, e))?;
        let mut f = self.resolve(&req.target)?;
        if !req.dry_run {
            self.register(&mut f)?;
        }
        let ch = f.channel(&req.target.channel)?;
        let job = self.job_start(
            JobKind::Import,
            Some(&ch.name),
            1,
            "Reading blueprint names from game data",
        );
        let res = (|| {
            let loaded = self.loaded(&f, ch, true)?;
            let catalog = GameDataCatalog::new(&loaded.game_data, &loaded.stock);
            let data = self.data_dir(&f, ch);
            // Hold the lock from the scan to the write; a dry run writes nothing.
            let _lock = match (&f.id, req.dry_run) {
                (Some(id), false) => Some(self.blueprint_lock(id, ch)?),
                _ => None,
            };
            let mut store = if f.id.is_some() {
                Store::load(&data).map_err(io)?
            } else {
                Store::default()
            };
            logscan::scan_if(
                self.prefs().import_game_log,
                &ch.dir,
                &data,
                &mut store,
                None,
            );
            let geid = store
                .characters
                .resolve(&req.character)
                .map_err(|e| match e {
                    blueprints::ResolveError::Ambiguous(..) => {
                        err(ErrorCode::NeedsDecision, e.to_string())
                    }
                    _ => err(ErrorCode::NotFound, e.to_string()),
                })?;
            let set = store.set_mut(&data, &geid).map_err(io)?;
            let rep =
                bp_import::merge_with(&export, &catalog as &dyn BlueprintCatalog, set, req.replace);
            if !req.dry_run && !rep.empty_catalog {
                store.save(&data).map_err(io)?;
                self.after_merge(&ch.name, &store);
                self.mem().last_sc_pick = None;
                let abs = fsutil::canonicalize(&file).unwrap_or_else(|_| file.clone());
                if let Err(e) = bp_import::save_last_import(&data, &abs) {
                    self.log(
                        LogLevel::Warn,
                        Some(JobKind::Import),
                        Some(&ch.name),
                        &format!("could not remember the export for a re-import: {e}"),
                    );
                }
            }
            Ok::<_, AppError>(rep)
        })();
        let rep = match res {
            Ok(r) => r,
            Err(e) => {
                self.job_end(job, JobPhase::Failed, &e.message, None);
                return Err(e);
            }
        };
        self.forget_tables(&f, ch);
        self.log(
            LogLevel::Info,
            Some(JobKind::Import),
            Some(&ch.name),
            &format!(
                "Smart Citizen {}{}: {} new, {} known, {} removed, {} unmatched, {} ambiguous",
                if req.replace { "re-import" } else { "import" },
                if req.dry_run { " (dry run)" } else { "" },
                rep.imported.len(),
                rep.known.len(),
                rep.removed.len(),
                rep.unmatched.len(),
                rep.ambiguous.len()
            ),
        );
        self.job_end(job, JobPhase::Done, "Done", None);
        Ok(ImportReport {
            file: file
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            exported_at: rep.exported_at,
            dry_run: req.dry_run,
            total: rep.total as u32,
            not_completed: rep.not_completed as u32,
            duplicates: rep.duplicates as u32,
            imported: rep.imported,
            known: rep.known,
            unmatched: rep.unmatched,
            ambiguous: rep
                .ambiguous
                .into_iter()
                .map(|a| AmbiguousName {
                    name: a.name,
                    candidates: a.candidates,
                })
                .collect(),
            empty_catalog: rep.empty_catalog,
            removed: rep.removed,
        })
    }

    fn rescan_logs(&self, req: RescanRequest) -> CmdResult<RescanReport> {
        let mut f = self.resolve(&req.target)?;
        if !req.dry_run {
            self.register(&mut f)?;
        }
        let ch = f.channel(&req.target.channel)?;
        if !self.prefs().import_game_log {
            return Err(err(
                ErrorCode::Invalid,
                "Reading Game.log is off (Settings, Blueprints from Game.log); no log was read.",
            ));
        }
        let data = self.data_dir(&f, ch);
        let _lock = match (&f.id, req.dry_run) {
            (Some(id), false) => Some(self.blueprint_lock(id, ch)?),
            _ => None,
        };
        let mut store = if f.id.is_some() {
            Store::load(&data).map_err(io)?
        } else {
            Store::default()
        };
        let rep = bp_repair::rebuild_from_logs(&ch.dir, &data, &mut store, req.replace, None)
            .map_err(io)?;
        let refused = rep.replace_refusal();
        if !req.dry_run
            && let Some(why) = &refused
        {
            return Err(err(ErrorCode::Invalid, why.clone()));
        }
        if !req.dry_run && f.id.is_some() {
            store.save(&data).map_err(io)?;
            self.after_merge(&ch.name, &store);
            self.forget_tables(&f, ch);
        }
        let changes = rep
            .characters
            .iter()
            .map(|c| c.added.len() + c.removed.len())
            .sum::<usize>();
        self.log(
            LogLevel::Info,
            Some(JobKind::Import),
            Some(&ch.name),
            &format!(
                "Re-scan of the game logs{}: {} files read, {changes} blueprint changes, {} characters merged",
                if req.dry_run { " (dry run)" } else { "" },
                rep.scan.files_read,
                rep.scan.merged.len()
            ),
        );
        Ok(RescanReport {
            channel: ch.name.clone(),
            dry_run: req.dry_run,
            replace: req.replace,
            files_read: rep.scan.files_read as u32,
            merged: rep.scan.merged.len() as u32,
            warnings: rep.scan.warnings.clone(),
            replace_refused: refused,
            characters: rep
                .characters
                .into_iter()
                .map(|c| CharacterChange {
                    id: c.id,
                    name: c.name,
                    before: c.before as u32,
                    after: c.after as u32,
                    added: c.added,
                    removed: c.removed,
                })
                .collect(),
        })
    }

    fn forget_character(&self, req: CharacterRequest) -> CmdResult<CharactersView> {
        let f = self.resolve(&req.target)?;
        let ch = f.channel(&req.target.channel)?;
        let id =
            f.id.clone()
                .ok_or_else(|| err(ErrorCode::NotFound, "no characters for this install"))?;
        let _l = self.blueprint_lock(&id, ch)?;
        let data = self.data_dir(&f, ch);
        let mut chars = blueprints::load_characters(&data).map_err(io)?;
        if chars.characters.remove(&req.character).is_none() {
            return Err(err(ErrorCode::NotFound, "no such character"));
        }
        // Validates the id (alphanumeric) before it becomes a file name.
        blueprints::load_set(&data, &req.character).map_err(|e| err(ErrorCode::Invalid, e))?;
        let set = blueprints::dir(&data).join(format!("{}.json", req.character));
        match std::fs::remove_file(&set) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io(e)),
        }
        blueprints::save_characters(&data, &chars).map_err(io)?;
        let mut st = ChannelState::load(&data).map_err(io)?;
        if st.active_character.as_deref() == Some(req.character.as_str()) {
            st.active_character = None;
            st.save(&data).map_err(io)?;
        }
        self.forget_tables(&f, ch);
        self.log(
            LogLevel::Info,
            None,
            Some(&ch.name),
            "Forgot a character and its owned blueprints",
        );
        drop(_l);
        self.characters_view(&f, ch)
    }

    fn fleet_view(&self, req: FleetViewRequest) -> CmdResult<FleetView> {
        let mut f = self.resolve(&req.target)?;
        if req.read_game_data {
            self.register(&mut f)?;
        }
        let ch = f.channel(&req.target.channel)?;
        let cat = if req.read_game_data {
            let job = self.job_start(
                JobKind::Extract,
                Some(&ch.name),
                1,
                "Reading ship names from game data",
            );
            match self.hull_catalog(&f, ch, true) {
                Ok(c) => {
                    self.job_end(job, JobPhase::Done, "Done", None);
                    c
                }
                Err(e) => {
                    self.job_end(job, JobPhase::Failed, &e.message, None);
                    return Err(e);
                }
            }
        } else {
            self.hull_catalog(&f, ch, false)?
        };
        self.fleet_view_of(&f, ch, cat.as_ref())
    }

    fn fleet_import(&self, req: FleetImportRequest) -> CmdResult<FleetImportReport> {
        let (file, bytes) = match &req.source {
            FleetSource::Text { text } => (None, text.as_bytes().to_vec()),
            FleetSource::Last => {
                let (n, b) = self.mem().last_fleet.clone().ok_or_else(|| {
                    err(
                        ErrorCode::NotFound,
                        "Choose the file again: it is no longer in memory.",
                    )
                })?;
                (Some(n), b)
            }
            FleetSource::StarOrg => (
                Some("StarOrg Tools hangar".to_string()),
                self.starorg_hangar_bytes()?,
            ),
            FleetSource::File => {
                let d = self
                    .cfg
                    .dialogs
                    .get()
                    .ok_or_else(|| err(ErrorCode::Cancelled, "no file chosen"))?;
                let path = d
                    .pick_file(
                        "Fleet export (RSI+, FleetYards, FleetView) or a list of ships",
                        FLEET_FILTER,
                    )
                    .ok_or_else(|| err(ErrorCode::Cancelled, "no file chosen"))?;
                let b = fsutil::read_limited(&path, fleet_import::IMPORT_LIMIT)
                    .map_err(|e| err(ErrorCode::Invalid, e.to_string()))?
                    .ok_or_else(|| {
                        err(ErrorCode::NotFound, format!("{} not found", path.display()))
                    })?;
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.mem().last_fleet = Some((name.clone(), b.clone()));
                (Some(name), b)
            }
        };
        let parsed = super::fleet::parse(&bytes)?;
        let mut f = self.resolve(&req.target)?;
        if !req.dry_run {
            self.register(&mut f)?;
        }
        let ch = f.channel(&req.target.channel)?;
        let cat = self
            .hull_catalog(&f, ch, f.id.is_some())?
            .unwrap_or_default();
        let _lock = if req.dry_run {
            None
        } else {
            Some(self.fleet_lock()?)
        };
        let fleet = FleetFile::load(self.dirs()).map_err(io)?;
        let mut out = fleet_import::run(&parsed, &cat, &fleet, req.merge);
        super::fleet::check_prune(&req, &out)?;
        let mut appended = 0;
        let mut removed = None;
        if req.dry_run && req.prune && !out.report.empty_catalog {
            let mut s = fleet_store::load_shared(self.dirs()).map_err(io)?;
            removed = Some(super::fleet::prune_order(&mut s, &out, &cat));
        }
        if !req.dry_run && !out.report.empty_catalog {
            out.fleet.imported_at = Some(now());
            out.fleet.save(self.dirs()).map_err(io)?;
            if req.prune {
                removed = Some(self.prune_shared_order(&out, &cat)?);
            }
            appended = self.append_to_order(&out.added)?;
            self.forget_tables(&f, ch);
            if !req.dry_run {
                self.mem().last_fleet = None;
            }
        }
        let r = &out.report;
        self.log(
            LogLevel::Info,
            Some(JobKind::Import),
            Some(&ch.name),
            &format!(
                "Fleet import{}: {} hulls, {} unmatched, {} ambiguous",
                if req.dry_run { " (dry run)" } else { "" },
                r.hulls.len(),
                r.unmatched.len(),
                r.ambiguous.len()
            ),
        );
        Ok(super::fleet::report(
            &out,
            file,
            req.dry_run,
            appended,
            removed,
        ))
    }

    fn fleet_match(&self, req: FleetMatchRequest) -> CmdResult<FleetView> {
        let mut f = self.resolve(&req.target)?;
        self.register(&mut f)?;
        let ch = f.channel(&req.target.channel)?;
        let cat = self.hull_catalog(&f, ch, false)?.ok_or_else(|| {
            err(
                ErrorCode::NotFound,
                format!("Read the game data of {} first.", ch.name),
            )
        })?;
        let target = super::fleet::match_target(&cat, req.hull.as_deref())?;
        {
            let _lock = self.fleet_lock()?;
            let mut fleet = FleetFile::load(self.dirs()).map_err(io)?;
            let added = fleet_import::hand_match(&mut fleet, &req.name, target);
            fleet.save(self.dirs()).map_err(io)?;
            self.append_to_order(added.as_slice())?;
        }
        self.forget_tables(&f, ch);
        self.fleet_view_of(&f, ch, Some(&cat))
    }

    fn save_ship_names(&self, req: SaveShipNamesRequest) -> CmdResult<FleetView> {
        let mut f = self.resolve(&req.target)?;
        self.register(&mut f)?;
        let ch = f.channel(&req.target.channel)?;
        let new = super::fleet::to_core(&req.settings)?;
        let data = self.data_dir(&f, ch);
        let mut profile = self.profile_file(&f, ch)?;
        let before = profile.clone();
        let scope = super::fleet::core_scope(req.scope);
        {
            let _lock = self.fleet_lock()?;
            if scope == fleet_store::Scope::Shared {
                // An unreadable file is moved aside (`.bad-<stamp>`), never overwritten.
                let (mut shared, _backup) =
                    fleet_store::load_shared_for_save(self.dirs()).map_err(io)?;
                shared.merge_from(&new);
                fleet_store::save_shared(self.dirs(), &shared).map_err(io)?;
            }
            fleet_store::set_channel(&mut profile, scope, Some(&new));
            if profile != before {
                profile.save(&data).map_err(io)?;
            }
        }
        self.forget_tables(&f, ch);
        let cat = self.hull_catalog(&f, ch, false)?;
        self.fleet_view_of(&f, ch, cat.as_ref())
    }

    fn fleet_clear(&self, req: ChannelRef) -> CmdResult<FleetView> {
        let f = self.resolve(&req)?;
        let ch = f.channel(&req.channel)?;
        {
            let _lock = self.fleet_lock()?;
            let mut fleet = FleetFile::load(self.dirs()).map_err(io)?;
            fleet.hulls.clear();
            fleet.pending.clear();
            fleet.imported_at = None;
            fleet.format = None;
            fleet.save(self.dirs()).map_err(io)?;
        }
        let cat = self.hull_catalog(&f, ch, false)?;
        self.fleet_view_of(&f, ch, cat.as_ref())
    }

    fn starorg_status(&self, channel: String) -> CmdResult<StarOrgStatus> {
        self.starorg_view(&channel)
    }

    fn starorg_login(&self, channel: String) -> CmdResult<StarOrgStatus> {
        self.starorg_login_impl(&channel)
    }

    fn starorg_cancel_login(&self) -> CmdResult<()> {
        self.starorg_cancel_login_impl();
        Ok(())
    }

    fn starorg_logout(&self, channel: String) -> CmdResult<StarOrgStatus> {
        self.starorg_logout_impl(&channel)
    }

    fn set_starorg_guild(&self, req: StarOrgGuildRequest) -> CmdResult<StarOrgStatus> {
        self.set_starorg_guild_impl(&req)
    }

    fn set_sync_characters(&self, req: SyncCharactersRequest) -> CmdResult<StarOrgStatus> {
        let mut a = self.app_file();
        a.sync.insert(req.channel.clone(), req.ticked);
        self.save_app_file(&a)?;
        self.starorg_view(&req.channel)
    }

    fn starorg_pull(&self, req: StarOrgPullRequest) -> CmdResult<SyncReport> {
        self.starorg_pull_impl(&req)
    }

    fn starorg_push(&self, channel: String) -> CmdResult<SyncReport> {
        self.starorg_push_impl(&channel)
    }

    fn starorg_sync(&self, channel: String) -> CmdResult<SyncRun> {
        self.starorg_sync_impl(&channel)
    }

    fn hooks_status(&self) -> CmdResult<HooksStatus> {
        Ok(self.hooks_view())
    }

    fn hooks_install(&self) -> CmdResult<HooksStatus> {
        self.hooks_edit(true)
    }

    fn hooks_uninstall(&self) -> CmdResult<HooksStatus> {
        self.hooks_edit(false)
    }

    fn doctor(&self) -> CmdResult<DoctorReport> {
        Ok(diag::doctor(self))
    }

    fn export_diagnostics(&self) -> CmdResult<Diagnostics> {
        Ok(diag::diagnostics(self))
    }

    fn save_diagnostics(&self) -> CmdResult<Option<String>> {
        let d = diag::diagnostics(self);
        let Some(dialogs) = self.cfg.dialogs.get() else {
            return Ok(None);
        };
        let Some(to) = dialogs.save_file("Save diagnostics", &d.file_name, TEXT_FILTER) else {
            return Ok(None);
        };
        fsutil::write_atomic(&to, d.text.as_bytes()).map_err(io)?;
        let shown = diag::redact(self, &path_text(&to)).0;
        self.log(
            LogLevel::Info,
            None,
            None,
            &format!("Saved diagnostics to {shown}"),
        );
        Ok(Some(path_text(&to)))
    }

    fn update_status(&self) -> CmdResult<UpdateStatus> {
        self.update.status(self.updates_on())
    }

    fn check_for_updates(&self, req: UpdateCheckRequest) -> CmdResult<UpdateStatus> {
        let on = self.updates_on();
        let before = self.update.status(on)?.last_checked;
        let s = self.update.check(on, req.force)?;
        // Log only checks that made a request.
        if s.last_checked != before {
            let what = match &s.check {
                UpdateCheck::Available { release } => format!("{} is available", release.version),
                UpdateCheck::UpToDate => "up to date".into(),
                UpdateCheck::Failed { message, .. } => message.clone(),
                UpdateCheck::NotChecked => "not checked".into(),
            };
            self.log(LogLevel::Info, None, None, &format!("Update check: {what}"));
        }
        Ok(s)
    }

    fn set_update_prefs(&self, req: UpdatePrefs) -> CmdResult<UpdateStatus> {
        self.update.set_prefs(self.updates_on(), req)
    }

    fn skip_update(&self, version: String) -> CmdResult<UpdateStatus> {
        self.update.skip(self.updates_on(), version)
    }

    fn open_release_page(&self) -> CmdResult<()> {
        self.update.open_release_page()
    }

    fn open_link(&self, link: ExternalLink) -> CmdResult<()> {
        open::that_detached(link.url()).map_err(|e| {
            AppError::new(ErrorCode::Internal, format!("cannot open the browser: {e}"))
        })?;
        self.log(
            LogLevel::Info,
            None,
            None,
            &format!("Opened the {} link in the browser", link.label()),
        );
        Ok(())
    }

    fn install_update(&self) -> CmdResult<UpdateStatus> {
        self.log(LogLevel::Info, None, None, "Installing an update");
        self.update.install(self.updates_on())
    }

    fn mcp_status(&self) -> CmdResult<McpStatus> {
        self.mcp_to_log();
        Ok(self.mcp.status())
    }

    /// The first install the app shows; datasets are extracted by this binary started as
    /// `mcp-extract-worker` (`crate::worker_main`), into the app cache folder.
    fn set_mcp_enabled(&self, req: McpToggle) -> CmdResult<McpStatus> {
        let st = if req.enabled {
            self.mcp.enable(|| {
                let exe =
                    std::env::current_exe().map_err(|e| format!("cannot find own binary: {e}"))?;
                let installs = self.installs();
                Ok(starorg_mcp::config_for(
                    installs.first().map(|f| &f.install),
                    self.dirs(),
                    Arc::new(
                        starorg_mcp::worker::ProcessExtractor::new(exe)
                            .with_limit(self.mcp_limit()),
                    ),
                ))
            })
        } else {
            self.mcp.disable()
        };
        self.mcp_to_log();
        Ok(st)
    }

    fn memory_status(&self) -> CmdResult<MemoryStatus> {
        Ok(self.memory_view())
    }

    fn set_memory_limit(&self, req: MemoryLimitRequest) -> CmdResult<MemoryStatus> {
        self.save_memory_limit(req)
    }

    fn storage_status(&self) -> CmdResult<StorageStatus> {
        Ok(self.storage_view())
    }

    fn storage_measure(&self) -> CmdResult<StorageStatus> {
        Ok(self.storage_measure_now())
    }

    fn storage_clear_assets(&self) -> CmdResult<StorageStatus> {
        self.storage_clear_asset_cache()
    }
}

impl RealBackend {
    fn mcp_to_log(&self) {
        for (level, msg) in self.mcp.take_log() {
            self.log(level, Some(JobKind::Mcp), None, &msg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{check_profile, convert, migration_note, user_patch_id};
    use crate::contract::types::Preset;
    use starorg_core::state::Migration;

    #[test]
    fn migration_note_claims_a_change_only_for_changed_values() {
        let v = |k: &[&str]| k.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let both = Migration {
            from: 1,
            followed: v(&["enhance.mission.owned_em"]),
            unpinned: v(&["enhance.ships", "enhance.mission.max_list"]),
        };
        assert_eq!(
            migration_note(&both),
            "Profile updated: owned_em now follows the current default (it was saved at the old one); removed 2 saved defaults (no change); a backup is in profile.toml.bak-schema1"
        );
        let only = Migration {
            from: 2,
            followed: Vec::new(),
            unpinned: v(&["enhance.ships"]),
        };
        let s = migration_note(&only);
        assert!(s.contains("removed 1 saved default (no change)"), "{s}");
        assert!(s.ends_with("profile.toml.bak-schema2"), "{s}");
        assert!(!s.contains("now follow"), "{s}");
        let two = Migration {
            from: 1,
            followed: v(&["enhance.mission.owned_em", "enhance.mission.label_em"]),
            unpinned: Vec::new(),
        };
        assert!(
            migration_note(&two).contains("owned_em, label_em now follow the current defaults")
        );
    }

    #[test]
    fn presets_pass_the_profile_check_and_junk_does_not() {
        for p in [Preset::Recommended, Preset::Minimal, Preset::Everything] {
            check_profile(&convert::preset(p)).unwrap();
        }
        let base = convert::preset(Preset::Recommended);
        let mut p = base.clone();
        p.lines_off = vec!["bogus.line".into()];
        assert!(check_profile(&p).is_err());
        let mut p = base.clone();
        p.tags.components.elements.push("bogus".into());
        assert!(check_profile(&p).is_err());
        let mut p = base.clone();
        p.mission.max_list = u32::MAX;
        assert!(check_profile(&p).is_err());
        let mut p = base;
        p.header = "x".repeat(5000);
        assert!(check_profile(&p).is_err());
    }

    #[test]
    fn user_patch_ids_do_not_collapse() {
        assert_eq!(user_patch_id("Fix Rifle", "fix-rifle"), "user-fix-rifle");
        let a = user_patch_id("!!!", "");
        assert!(a.starts_with("user-") && a.len() == 13, "{a}");
        assert_ne!(a, user_patch_id("???", ""));
        let b = user_patch_id("Ünï çødé", "n-d");
        assert!(b.starts_with("user-n-d-"), "{b}");
        assert_ne!(b, user_patch_id("Ñnï çødé", "n-d"));
    }
}

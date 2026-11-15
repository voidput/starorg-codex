//! Apply, status, remove and restore (spec 7).
//!
//! The apply sequence (spec 7.1): lock -> recover -> guards -> load -> build -> validate ->
//! preflight -> back up -> `user.cfg` -> journal -> write -> verify -> record -> report.
//! Validation and preflight happen before anything is written, so a normal failure leaves
//! the game directory untouched; the verify step covers I/O faults only.
//!
//! **Journal.** `intent.json` closes the gap between writing the file and recording it. On
//! the next run of any command for the channel ([`recover`]):
//!
//! | Found | Action |
//! |---|---|
//! | Game file hash equals the intent's output hash | Write `applied.json` and `outputs.json`, delete the intent |
//! | Otherwise (temp file or nothing written) | Delete the temp file, undo the `user.cfg` change if one was made, delete the intent |

use crate::appdirs::AppDirs;
use crate::backup::{self, FileOrigin, Reason};
use crate::blueprints;
use crate::build::BuildInfo;
use crate::cache::{self, KeyInputs};
use crate::config::{LOOSE_INI, OutputEntry, Outputs};
use crate::discovery::{Channel, Fingerprint, Install};
use crate::fsutil::{self, read_json, write_json};
use crate::ini::StockIni;
use crate::lock::{self, LockError};
use crate::merge::{self, Counts};
use crate::patch::{self, EntryOutcome, PatchOutcome, PatchSet};
use crate::paths::resolve_ci;
use crate::running;
use crate::source::{Budget, GameSource, SourceError, Warning};
use crate::state::{self, APPLIED_JSON, ChannelState, INTENT_JSON, Profile};
use crate::usercfg::{self, CfgTarget, LanguageLine, Ledger, Plan, UndoOutcome};
use crate::validate;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Exit codes (spec 8). The CLI re-exports these.
pub mod exit {
    pub const OK: u8 = 0;
    pub const ERROR: u8 = 1;
    pub const USAGE: u8 = 2;
    pub const SKIPPED: u8 = 3;
    pub const NOT_FOUND: u8 = 4;
    pub const WARNINGS: u8 = 5;
    pub const NEEDS_DECISION: u8 = 6;

    /// Most serious first: 1, then 6, 4, 3, 5, 0 (spec 8, `--all`).
    pub fn worst(codes: impl IntoIterator<Item = u8>) -> u8 {
        const ORDER: [u8; 7] = [
            ERROR,
            USAGE,
            NEEDS_DECISION,
            NOT_FOUND,
            SKIPPED,
            WARNINGS,
            OK,
        ];
        let codes: Vec<u8> = codes.into_iter().collect();
        ORDER
            .iter()
            .copied()
            .find(|c| codes.contains(c))
            .unwrap_or(OK)
    }
}

/// Name of the temp file beside the real `global.ini`.
pub const INI_TMP: &str = "global.ini.starorg-tmp";

/// Settings shared by every command.
#[derive(Debug, Clone)]
pub struct Ctx {
    pub dirs: AppDirs,
    pub app_version: String,
    /// StarBreaker commit, part of the cache key.
    pub starbreaker_commit: String,
    /// `base.ini` must have at least this many keys (spec 4.2).
    pub min_stock_keys: usize,
    /// Gap between the two `Data.p4k` reads of the "being written" guard (500 ms).
    pub p4k_settle: Duration,
    /// Use the extraction cache.
    pub use_cache: bool,
    /// Test hook: sleep this long between the write and the record (to kill the process
    /// there).
    pub pause_after_write: Option<Duration>,
    /// Test hook: sleep this long before the `user.cfg` step (to let another channel change
    /// a shared `user.cfg` in between).
    pub pause_before_cfg: Option<Duration>,
}

impl Ctx {
    pub fn new(dirs: AppDirs, app_version: &str, starbreaker_commit: &str) -> Self {
        Self {
            dirs,
            app_version: app_version.to_string(),
            starbreaker_commit: starbreaker_commit.to_string(),
            min_stock_keys: cache::MIN_STOCK_KEYS,
            p4k_settle: Duration::from_millis(500),
            use_cache: true,
            pause_after_write: None,
            pause_before_cfg: None,
        }
    }
}

/// One channel of one install, with its id.
#[derive(Debug, Clone)]
pub struct Target<'a> {
    pub install: &'a Install,
    /// `None` for an install never registered (read-only commands).
    pub install_id: Option<String>,
    pub channel: &'a Channel,
    /// Wine prefix holding the install (from discovery, or found by walking up).
    pub prefix: Option<PathBuf>,
}

impl<'a> Target<'a> {
    pub fn new(install: &'a Install, install_id: Option<String>, channel: &'a Channel) -> Self {
        let prefix = install
            .prefix
            .clone()
            .or_else(|| running::enclosing_prefix(&install.root));
        Self {
            install,
            install_id,
            channel,
            prefix,
        }
    }

    pub fn name(&self) -> &str {
        &self.channel.name
    }

    fn id(&self) -> &str {
        self.install_id.as_deref().unwrap_or("unregistered")
    }

    pub fn data_dir(&self, dirs: &AppDirs) -> PathBuf {
        dirs.channel_dir(self.id(), &self.channel.name)
    }

    /// `<install-id>/<CHANNEL>`, as stored in `user.cfg` ledgers.
    pub fn channel_ref(&self) -> String {
        format!("{}/{}", self.id(), self.channel.name.to_ascii_uppercase())
    }

    pub fn build(&self) -> Option<&BuildInfo> {
        self.channel.build.as_ref()
    }
}

/// Fingerprint as stored in manifests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FpRecord {
    pub change: Option<u64>,
    pub p4k_size: u64,
    pub p4k_mtime_ns: u128,
}

impl From<&Fingerprint> for FpRecord {
    fn from(f: &Fingerprint) -> Self {
        Self {
            change: f.change,
            p4k_size: f.p4k_size,
            p4k_mtime_ns: f.p4k_mtime_ns,
        }
    }
}

/// `applied.json` (spec 7.5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Applied {
    pub branch: Option<String>,
    pub game_version: Option<String>,
    pub change: Option<u64>,
    pub version: Option<String>,
    pub fingerprint: FpRecord,
    pub cache_key: String,
    pub profile_hash: String,
    pub patch_set_hash: String,
    pub overrides_hash: String,
    /// Hash of the owned-blueprint names used (empty when none; older records lack it).
    #[serde(default)]
    pub owned_hash: String,
    pub app_version: String,
    pub output_sha256: String,
    pub time: String,
    pub backup: Option<String>,
    pub warnings: Vec<Warning>,
    pub real_target: PathBuf,
    pub user_cfg_changed: bool,
    /// `apply --character X` (one-off): the pick, and the channel's `active_character`
    /// setting at that time. While that setting is unchanged, later runs without
    /// `--character` (status, hooks) keep using the pick instead of flipping back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owned_choice: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owned_setting: Option<String>,
}

/// `intent.json`: the journal of an apply in progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Intent {
    output_sha256: String,
    real_target: PathBuf,
    temp: PathBuf,
    backup: Option<String>,
    /// Set when step 9 changed `user.cfg`.
    user_cfg: Option<CfgTarget>,
    applied: Applied,
    output: OutputEntry,
}

/// The inputs status compares with `applied.json`.
#[derive(Debug, Clone)]
pub struct Inputs {
    pub fingerprint: Option<Fingerprint>,
    pub profile: Profile,
    pub profile_hash: String,
    pub patches: PatchSet,
    pub overrides_bytes: Vec<u8>,
    pub overrides: Vec<state::Override>,
    pub overrides_bad: Vec<String>,
    pub overrides_hash: String,
    /// Owned blueprints for `[Owned]` markers (addendum §3).
    pub owned: blueprints::Selection,
    /// The one-off character pick in force (`--character`, or kept from the last apply).
    pub owned_choice: Option<String>,
    /// The channel's `active_character` setting.
    pub owned_setting: Option<String>,
}

/// Read the current inputs of a channel. `profile_file` replaces `profile.toml`; `character`
/// replaces the channel's `active_character` setting.
pub fn current_inputs(
    ctx: &Ctx,
    t: &Target<'_>,
    profile_file: Option<&Path>,
    character: Option<&str>,
) -> Result<Inputs, String> {
    let data = t.data_dir(&ctx.dirs);
    let profile_path = profile_file
        .map(Path::to_path_buf)
        .unwrap_or_else(|| Profile::path(&data));
    let profile = Profile::load_file(&profile_path).map_err(|e| e.to_string())?;
    let patches = patch::load_patches(&ctx.dirs, &profile.disabled_patches());
    let (overrides_bytes, overrides, overrides_bad) =
        state::load_overrides(&data).map_err(|e| e.to_string())?;
    let setting = ChannelState::load(&data)
        .map_err(|e| e.to_string())?
        .active_character;
    let one_off = match character {
        Some(c) => Some(c.to_string()),
        // A one-off pick from the last apply holds while the channel setting is unchanged.
        None => read_json::<Applied>(&data.join(APPLIED_JSON))
            .ok()
            .flatten()
            .filter(|a| a.owned_choice.is_some() && a.owned_setting == setting)
            .and_then(|a| a.owned_choice),
    };
    let choice = one_off.clone().or_else(|| setting.clone());
    let owned = blueprints::select(&data, choice.as_deref())?;
    Ok(Inputs {
        owned,
        owned_choice: one_off,
        owned_setting: setting,
        fingerprint: t.channel.fingerprint().ok(),
        profile_hash: profile.hash(),
        profile,
        patches,
        overrides_hash: fsutil::sha256_hex(&overrides_bytes),
        overrides_bytes,
        overrides,
        overrides_bad,
    })
}

/// Drop a one-off `apply --character` pick from `applied.json` (after `blueprints use`), so
/// status and the hooks go back to the channel setting.
pub fn forget_one_off_character(channel_data: &Path) -> Result<(), String> {
    let p = channel_data.join(APPLIED_JSON);
    match read_json::<Applied>(&p) {
        Ok(Some(mut a)) if a.owned_choice.is_some() => {
            a.owned_choice = None;
            a.owned_setting = None;
            write_json(&p, &a).map_err(|e| format!("{}: {e}", p.display()))
        }
        Ok(_) => Ok(()),
        Err(e) => Err(format!("{}: {e}", p.display())),
    }
}

/// Where the loose `global.ini` is.
#[derive(Debug, Clone, Serialize)]
pub struct IniTarget {
    /// In the channel directory, with on-disk casing (spec 10.3).
    pub path: PathBuf,
    pub exists: bool,
    /// After following links.
    pub real: PathBuf,
    /// The path leaves the channel folder through a link.
    pub is_link: bool,
}

pub fn locate_ini(channel_dir: &Path) -> Result<IniTarget, String> {
    let r = resolve_ci(channel_dir, LOOSE_INI).map_err(|e| e.to_string())?;
    let link_meta = std::fs::symlink_metadata(&r.path).ok();
    let exists = r.path.exists();
    let real = if exists {
        std::fs::canonicalize(&r.path).map_err(|e| format!("{}: {e}", r.path.display()))?
    } else if link_meta.is_some_and(|m| m.file_type().is_symlink()) {
        // A link to a missing file (as `remove` leaves a linked global.ini): follow it by
        // hand and write the file it names; the link itself is never replaced (spec 7.4).
        let target =
            std::fs::read_link(&r.path).map_err(|e| format!("{}: {e}", r.path.display()))?;
        let target = if target.is_absolute() {
            target
        } else {
            r.path.parent().unwrap_or(channel_dir).join(target)
        };
        let real = fsutil::canonical_lenient(&target);
        if !real.parent().is_some_and(Path::is_dir) {
            return Err(format!(
                "{} links to {}, whose folder is missing; refusing to replace the link",
                r.path.display(),
                real.display()
            ));
        }
        real
    } else {
        fsutil::canonical_lenient(&r.path)
    };
    let canon_dir =
        std::fs::canonicalize(channel_dir).unwrap_or_else(|_| channel_dir.to_path_buf());
    let rel = r.path.strip_prefix(channel_dir).unwrap_or(&r.path);
    let is_link = real != canon_dir.join(rel);
    Ok(IniTarget {
        path: r.path,
        exists,
        real,
        is_link,
    })
}

/// Name the tool that wrote a foreign file, when we can tell (spec 7.5).
pub fn foreign_tool(bytes: &[u8], channel_dir: &Path) -> Option<String> {
    let ini = StockIni::parse(bytes.to_vec());
    if let Ok(line) = ini.find("Frontend_PU_Version")
        && ini
            .value_at(line)
            .is_some_and(|v| v.to_ascii_lowercase().contains("smart citizen"))
    {
        return Some("Smart Citizen".into());
    }
    if matches!(resolve_ci(channel_dir, &[".starstrings_version"]), Ok(r) if r.exists()) {
        return Some("StarStrings".into());
    }
    None
}

// ---------------------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum State {
    Applied,
    Stale { reasons: Vec<String> },
    ChangedByAnotherTool { tool: Option<String> },
    NotApplied,
}

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub channel: String,
    pub set_up: bool,
    #[serde(flatten)]
    pub state: State,
    pub file: Option<IniTarget>,
    pub sha256: Option<String>,
    pub interrupted_apply: bool,
    pub applied: Option<Applied>,
}

fn stale_reasons(a: &Applied, inp: &Inputs, ctx: &Ctx) -> Vec<String> {
    let mut r = Vec::new();
    match &inp.fingerprint {
        Some(fp) if FpRecord::from(fp) == a.fingerprint => {}
        Some(fp) if fp.change != a.fingerprint.change => r.push("build".into()),
        _ => r.push("game data".into()),
    }
    if inp.profile_hash != a.profile_hash {
        r.push("profile".into());
    }
    if inp.patches.hash() != a.patch_set_hash {
        r.push("patches".into());
    }
    if inp.overrides_hash != a.overrides_hash {
        r.push("overrides".into());
    }
    if inp.owned.hash != a.owned_hash {
        r.push("owned blueprints".into());
    }
    if ctx.app_version != a.app_version {
        r.push("app version".into());
    }
    r
}

/// Status of a channel. Needs no extraction: hashes one file and reads small state files.
pub fn status(ctx: &Ctx, t: &Target<'_>, inputs: &Inputs) -> Result<Status, String> {
    let data = t.data_dir(&ctx.dirs);
    let set_up = t.install_id.is_some()
        && ChannelState::load(&data)
            .map(|s| s.consent)
            .unwrap_or(false);
    let applied: Option<Applied> =
        read_json(&data.join(APPLIED_JSON)).map_err(|e| e.to_string())?;
    let interrupted_apply = data.join(INTENT_JSON).exists();
    let ini = locate_ini(&t.channel.dir)?;
    if !ini.exists {
        return Ok(Status {
            channel: t.channel.name.clone(),
            set_up,
            state: State::NotApplied,
            file: Some(ini),
            sha256: None,
            interrupted_apply,
            applied,
        });
    }
    let bytes = std::fs::read(&ini.real).map_err(|e| format!("{}: {e}", ini.real.display()))?;
    let sha = fsutil::sha256_hex(&bytes);
    let outputs = Outputs::load(&ctx.dirs).map_err(|e| e.to_string())?;
    let state = match &applied {
        Some(a) if a.output_sha256 == sha => {
            let reasons = stale_reasons(a, inputs, ctx);
            if reasons.is_empty() {
                State::Applied
            } else {
                State::Stale { reasons }
            }
        }
        _ if outputs.contains(&sha) => State::Stale {
            reasons: vec!["not the last recorded output".into()],
        },
        _ => State::ChangedByAnotherTool {
            tool: foreign_tool(&bytes, &t.channel.dir),
        },
    };
    Ok(Status {
        channel: t.channel.name.clone(),
        set_up,
        state,
        file: Some(ini),
        sha256: Some(sha),
        interrupted_apply,
        applied,
    })
}

// ---------------------------------------------------------------------------------------
// Reports
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Applied,
    UpToDate,
    Skipped,
    Failed,
    DryRun,
    NeedsSetup,
    NeedsDecision,
    Removed,
    Restored,
    NothingToDo,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BuildJson {
    pub branch: Option<String>,
    pub game_version: Option<String>,
    pub change: Option<u64>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct UserCfgReport {
    pub changed: bool,
    pub path: Option<PathBuf>,
    pub link_target: Option<PathBuf>,
    /// Other channels (of any install) whose `user.cfg` is the same real file.
    pub shared_with: Vec<String>,
    pub action: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub channel: String,
    pub result: Outcome,
    pub reason: Option<String>,
    #[serde(skip)]
    pub code: u8,
    pub build: BuildJson,
    pub counts: serde_json::Value,
    pub patches: Vec<PatchOutcome>,
    pub patch_entries: Vec<EntryOutcome>,
    pub warnings: Vec<Warning>,
    pub user_cfg: UserCfgReport,
    pub backup: Option<String>,
    pub target: Option<PathBuf>,
    pub target_is_link: bool,
    pub output_sha256: Option<String>,
    pub recovered: Option<String>,
    /// Owned blueprints used for `[Owned]` markers (addendum §3).
    pub owned: Option<blueprints::Selection>,
    pub duration_ms: u64,
}

impl Report {
    fn new(t: &Target<'_>) -> Self {
        let b = t.build();
        Self {
            channel: t.channel.name.clone(),
            result: Outcome::Failed,
            reason: None,
            code: exit::ERROR,
            build: BuildJson {
                branch: b.map(|b| b.branch.clone()),
                game_version: b.and_then(|b| b.game_version).map(|v| v.to_string()),
                change: b.and_then(|b| b.change),
                version: b.map(|b| b.version.clone()),
            },
            counts: counts_json(&Counts::default(), &crate::enhance::Generated::default()),
            patches: Vec::new(),
            patch_entries: Vec::new(),
            warnings: Vec::new(),
            user_cfg: UserCfgReport::default(),
            backup: None,
            target: None,
            target_is_link: false,
            output_sha256: None,
            recovered: None,
            owned: None,
            duration_ms: 0,
        }
    }

    fn end(mut self, result: Outcome, code: u8, reason: Option<String>, started: Instant) -> Self {
        self.result = result;
        self.code = code;
        self.reason = reason;
        self.duration_ms = started.elapsed().as_millis() as u64;
        self
    }
}

/// `counts` in the shape of spec 8. Categories stay 0 until the M2 generators exist.
/// Per-category counts are what the generators offered; `changed`, `dropped` and the patch
/// and override counts are after the merge.
fn counts_json(c: &Counts, g: &crate::enhance::Generated) -> serde_json::Value {
    use crate::enhance::Category as C;
    let n = |cs: &[C]| cs.iter().map(|&x| g.count(x)).sum::<usize>();
    serde_json::json!({
        "changed": c.changed,
        "ships": n(&[C::Ships]),
        "components": n(&[C::Components, C::ComponentTags]),
        "ship_weapons": n(&[C::ShipWeapons, C::ShipWeaponTags]),
        "fps_weapons": n(&[C::FpsWeapons]),
        "missiles": n(&[C::Missiles, C::MissileTags]),
        "missions": n(&[C::MissionTitles, C::MissionDescriptions]),
        "generated": c.generated,
        "loc_patches": c.loc_patches,
        "overrides": c.overrides, "dropped": c.dropped,
    })
}

// ---------------------------------------------------------------------------------------
// Recovery
// ---------------------------------------------------------------------------------------

/// Finish or undo an interrupted apply (the journal table above). `Ok(None)` when there was
/// nothing to recover.
pub fn recover(ctx: &Ctx, t: &Target<'_>) -> Result<Option<String>, String> {
    let data = t.data_dir(&ctx.dirs);
    let intent_path = data.join(INTENT_JSON);
    let intent: Intent = match read_json(&intent_path) {
        Ok(Some(i)) => i,
        Ok(None) => return Ok(None),
        Err(e) => return Err(format!("unreadable journal {}: {e}", intent_path.display())),
    };
    let cur = fsutil::sha256_file(&intent.real_target).ok();
    let msg = if cur.as_deref() == Some(intent.output_sha256.as_str()) {
        write_json(&data.join(APPLIED_JSON), &intent.applied).map_err(|e| e.to_string())?;
        add_output(ctx, intent.output.clone())?;
        let _ = std::fs::remove_file(&intent.temp);
        "an interrupted apply had finished writing; recorded it".to_string()
    } else {
        let _ = std::fs::remove_file(&intent.temp);
        let mut m = "an interrupted apply had not written the file; undid it".to_string();
        if let Some(cfg) = &intent.user_cfg {
            let _l = wait_lock(&usercfg::lock_path(&ctx.dirs, &cfg.real), CFG_LOCK_WAIT)
                .map_err(|e| e.to_string())?;
            match usercfg::release(&ctx.dirs, cfg, &t.channel_ref()) {
                Ok(o) => m.push_str(&format!(" (user.cfg: {o:?})")),
                Err(e) => return Err(format!("undo user.cfg: {e}")),
            }
        }
        m
    };
    std::fs::remove_file(&intent_path).map_err(|e| e.to_string())?;
    Ok(Some(msg))
}

/// Run journal recovery for read-only commands (`status`, `doctor`): only for a registered
/// install, only when the channel lock is free. `None` when it did not run.
pub fn recover_if_idle(ctx: &Ctx, t: &Target<'_>) -> Option<Result<Option<String>, String>> {
    t.install_id.as_ref()?;
    if !t.data_dir(&ctx.dirs).join(INTENT_JSON).exists() {
        return None;
    }
    let _l = lock::try_lock(&ctx.dirs.channel_lock(t.id(), &t.channel.name)).ok()?;
    Some(recover(ctx, t))
}

fn add_output(ctx: &Ctx, entry: OutputEntry) -> Result<(), String> {
    // outputs.json is shared by all channels: serialise writers.
    let _l =
        wait_lock(&ctx.dirs.outputs_lock(), Duration::from_secs(5)).map_err(|e| e.to_string())?;
    let mut o = Outputs::load(&ctx.dirs).map_err(|e| e.to_string())?;
    o.add(entry);
    o.save(&ctx.dirs).map_err(|e| e.to_string())
}

/// How long a run waits for the shared `user.cfg` lock. The critical section is a read, a
/// small edit and a rename, so a short wait beats skipping the whole channel (exit 3).
pub const CFG_LOCK_WAIT: Duration = Duration::from_secs(10);

fn wait_lock(path: &Path, max: Duration) -> Result<lock::Lock, LockError> {
    let start = Instant::now();
    loop {
        match lock::try_lock(path) {
            Ok(l) => return Ok(l),
            Err(LockError::Held(_)) if start.elapsed() < max => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => return Err(e),
        }
    }
}

fn cfg_action(plan: &Plan) -> String {
    match plan {
        Plan::Nothing => "nothing (line already right)",
        Plan::Append => "append g_language = english",
        Plan::Replace { .. } => "replace the language line with english",
        Plan::NeedsDecision { .. } => "needs decision",
    }
    .into()
}

/// The real target must be ours to edit (or creatable) when the plan writes.
fn cfg_writable(cfg: &CfgTarget, plan: &Plan, channel_dir: &Path) -> Result<(), String> {
    if *plan == Plan::Nothing {
        return Ok(());
    }
    if cfg.exists {
        fsutil::owned_regular_file(&cfg.real)
    } else {
        write_probe(cfg.real.parent().unwrap_or(channel_dir))
    }
}

// ---------------------------------------------------------------------------------------
// Guards shared by apply, remove and restore
// ---------------------------------------------------------------------------------------

enum Guard {
    Pass(Vec<Warning>),
    Stop(Outcome, u8, String),
}

fn run_guards(ctx: &Ctx, t: &Target<'_>, check_p4k: bool) -> Guard {
    let mut warnings = Vec::new();
    if fsutil::is_elevated() {
        return Guard::Stop(
            Outcome::Failed,
            exit::ERROR,
            "refusing to run as root or elevated".into(),
        );
    }
    if let Some(seen) = running::game_running(t.prefix.as_deref(), &t.channel.name, &t.channel.dir)
    {
        return Guard::Stop(
            Outcome::Skipped,
            exit::SKIPPED,
            format!(
                "the game is running for {} (pid {}); try again after it exits",
                t.channel.name, seen.pid
            ),
        );
    }
    if running::launcher_running() {
        warnings.push(Warning::new(
            "launcher-running",
            "the RSI Launcher is running",
        ));
    }
    if check_p4k {
        let logs = t
            .prefix
            .as_deref()
            .map(crate::discovery::prefix_launcher_logs)
            .unwrap_or_default();
        if running::pipeline_running(&logs, &t.channel.name) {
            return Guard::Stop(
                Outcome::Skipped,
                exit::SKIPPED,
                format!("the launcher is installing {}", t.channel.name),
            );
        }
        if running::p4k_changing(&t.channel.p4k, ctx.p4k_settle) {
            return Guard::Stop(
                Outcome::Skipped,
                exit::SKIPPED,
                "Data.p4k is being written".into(),
            );
        }
    }
    let _ = ctx;
    Guard::Pass(warnings)
}

fn take_channel_lock(ctx: &Ctx, t: &Target<'_>) -> Result<lock::Lock, (Outcome, u8, String)> {
    let p = ctx.dirs.channel_lock(t.id(), &t.channel.name);
    lock::try_lock(&p).map_err(|e| match e {
        LockError::Held(_) => (
            Outcome::Skipped,
            exit::SKIPPED,
            format!(
                "another StarOrg Strings run is working on {}",
                t.channel.name
            ),
        ),
        other => (Outcome::Failed, exit::ERROR, other.to_string()),
    })
}

/// Create and delete a temp file in `dir` (or its deepest existing ancestor).
fn write_probe(dir: &Path) -> Result<(), String> {
    let mut d = dir;
    while !d.is_dir() {
        d = d
            .parent()
            .ok_or_else(|| format!("no existing folder above {}", dir.display()))?;
    }
    let probe = d.join(format!(".starorg-probe-{}", std::process::id()));
    std::fs::write(&probe, b"").map_err(|e| {
        format!(
            "cannot write in {}: {e}{}",
            d.display(),
            if cfg!(windows) {
                " (grant your user write access, e.g. icacls \"<folder>\" /grant \"%USERNAME%\":(OI)(CI)M)"
            } else {
                ""
            }
        )
    })?;
    std::fs::remove_file(&probe).map_err(|e| e.to_string())
}

/// Channels (other than `t`) whose `user.cfg` resolves to `real`.
fn sharing_channels(t: &Target<'_>, real: &Path) -> Vec<String> {
    t.install
        .channels
        .iter()
        .filter(|c| c.name != t.channel.name)
        .filter(|c| usercfg::locate(&c.dir).is_ok_and(|x| x.exists && x.real == real))
        .map(|c| c.name.clone())
        .collect()
}

// ---------------------------------------------------------------------------------------
// Apply
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct ApplyOpts {
    pub if_stale: bool,
    pub dry_run: bool,
    pub budget: Budget,
    pub profile_file: Option<PathBuf>,
    /// `--character <name|geid|auto|any>` for `[Owned]` markers.
    pub character: Option<String>,
}

enum LoadErr {
    Skip(String),
    Fail(String),
}

fn load_stock(
    ctx: &Ctx,
    t: &Target<'_>,
    inputs: Option<&KeyInputs>,
    source: &dyn GameSource,
    budget: &Budget,
    warnings: &mut Vec<Warning>,
) -> Result<crate::source::Loaded, LoadErr> {
    let cache_dir = ctx.dirs.channel_cache(t.id(), &t.channel.name);
    if ctx.use_cache
        && let Some(inp) = inputs
    {
        match cache::load(&cache_dir, inp, ctx.min_stock_keys) {
            Ok(l) => return Ok(l),
            Err(cache::Miss::Absent) => {}
            Err(cache::Miss::Rejected(why)) => {
                warnings.push(Warning::new(
                    "cache",
                    format!("cache entry rejected: {why}"),
                ));
            }
        }
    }
    // One extraction at a time on the machine (about 890 MB at peak).
    let _x = lock::try_lock(&ctx.dirs.extract_lock()).map_err(|e| match e {
        LockError::Held(_) => LoadErr::Skip("another extraction is running".into()),
        other => LoadErr::Fail(other.to_string()),
    })?;
    let loaded = source.load(t.channel, budget).map_err(|e| match e {
        SourceError::Timeout(d) => LoadErr::Skip(format!(
            "out of time after {:.1} s during extraction",
            d.as_secs_f64()
        )),
        other => LoadErr::Fail(other.to_string()),
    })?;
    if ctx.use_cache
        && let Some(inp) = inputs
        && let Err(e) = cache::store(&cache_dir, inp, &loaded, ctx.min_stock_keys)
    {
        warnings.push(Warning::new("cache", format!("not cached: {e}")));
    }
    Ok(loaded)
}

/// Stock text and `GameData` of a channel's current build: from the cache, or extracted
/// (one extraction at a time) and cached. With `ctx.use_cache` off it neither reads nor
/// writes the cache.
pub fn load_channel(
    ctx: &Ctx,
    t: &Target<'_>,
    source: &dyn GameSource,
    budget: &Budget,
    warnings: &mut Vec<Warning>,
) -> Result<crate::source::Loaded, String> {
    let key_inputs = t
        .channel
        .fingerprint()
        .ok()
        .map(|fp| KeyInputs::local(&fp, &ctx.starbreaker_commit));
    load_stock(ctx, t, key_inputs.as_ref(), source, budget, warnings).map_err(|e| match e {
        LoadErr::Skip(w) | LoadErr::Fail(w) => w,
    })
}

/// Apply one channel (spec 7.1).
pub fn apply(ctx: &Ctx, t: &Target<'_>, opts: &ApplyOpts, source: &dyn GameSource) -> Report {
    let started = Instant::now();
    let mut r = Report::new(t);
    let fail = |r: Report, why: String| r.end(Outcome::Failed, exit::ERROR, Some(why), started);

    if t.install_id.is_none() {
        return r.end(
            Outcome::NeedsSetup,
            exit::NEEDS_DECISION,
            Some(setup_hint(t)),
            started,
        );
    }
    // 1. Lock.
    let _lock = match take_channel_lock(ctx, t) {
        Ok(l) => l,
        Err((o, c, why)) => return r.end(o, c, Some(why), started),
    };
    // 2. Recover.
    match recover(ctx, t) {
        Ok(m) => r.recovered = m,
        Err(e) => return fail(r, e),
    }
    // 3. Guards: set-up and language first; they need a decision, not a retry.
    let data = t.data_dir(&ctx.dirs);
    let cstate = match ChannelState::load(&data) {
        Ok(s) => s,
        Err(e) => return fail(r, e.to_string()),
    };
    if !cstate.consent {
        return r.end(
            Outcome::NeedsSetup,
            exit::NEEDS_DECISION,
            Some(setup_hint(t)),
            started,
        );
    }
    let cfg = match usercfg::locate(&t.channel.dir) {
        Ok(c) => c,
        Err(e) => return fail(r, e.to_string()),
    };
    let cfg_text = match usercfg::read(&cfg) {
        Ok(t) => t,
        Err(e) => return fail(r, e.to_string()),
    };
    let lang = match &cfg_text {
        None => LanguageLine::NoFile,
        Some(text) => usercfg::inspect(text),
    };
    let plan = usercfg::plan(&lang, cstate.switch_language);
    if let Plan::NeedsDecision { value } = &plan {
        return r.end(
            Outcome::NeedsDecision,
            exit::NEEDS_DECISION,
            Some(format!(
                "the game language in {} is {value}; v1 writes English only. Run `starorg-strings setup --channel {} --switch-language --yes` to switch it",
                cfg.path.display(),
                t.channel.name
            )),
            started,
        );
    }
    let inputs = match current_inputs(
        ctx,
        t,
        opts.profile_file.as_deref(),
        opts.character.as_deref(),
    ) {
        Ok(i) => i,
        Err(e) => return fail(r, e),
    };
    r.owned = Some(inputs.owned.clone());
    if inputs.owned.ambiguous > 0 {
        r.warnings.push(Warning::new(
            "owned-ambiguous",
            format!(
                "{} owned blueprint name(s) match several blueprints; not marked [Owned] (`blueprints list` shows them)",
                inputs.owned.ambiguous
            ),
        ));
    }
    // --if-stale: nothing to do when Applied (no extraction, no Data.p4k open).
    if opts.if_stale {
        match status(ctx, t, &inputs) {
            Ok(s) if s.state == State::Applied => {
                r.output_sha256 = s.sha256;
                r.target = s.file.map(|f| f.real);
                return r.end(Outcome::UpToDate, exit::OK, None, started);
            }
            Ok(_) => {}
            Err(e) => return fail(r, e),
        }
    }
    match run_guards(ctx, t, true) {
        Guard::Pass(w) => r.warnings.extend(w),
        Guard::Stop(o, c, why) => return r.end(o, c, Some(why), started),
    }
    if opts.budget.expired() {
        return r.end(
            Outcome::Skipped,
            exit::SKIPPED,
            Some("out of time".into()),
            started,
        );
    }

    // 4. Load.
    let key_inputs = inputs
        .fingerprint
        .as_ref()
        .map(|fp| KeyInputs::local(fp, &ctx.starbreaker_commit));
    let loaded = match load_stock(
        ctx,
        t,
        key_inputs.as_ref(),
        source,
        &opts.budget,
        &mut r.warnings,
    ) {
        Ok(l) => l,
        Err(LoadErr::Skip(why)) => {
            return r.end(Outcome::Skipped, exit::SKIPPED, Some(why), started);
        }
        Err(LoadErr::Fail(why)) => return fail(r, why),
    };
    r.warnings.extend(loaded.warnings.iter().cloned());

    // 5. Build: data patches -> generators -> merge -> loc patches -> overrides.
    let stock = StockIni::parse(loaded.base_ini);
    for b in &inputs.overrides_bad {
        r.warnings.push(Warning::new("override-ignored", b.clone()));
    }
    for (k, lines) in stock.collisions() {
        r.warnings.push(Warning::new(
            "key-collision",
            format!("{k} folds to the same key on lines {lines:?}; left out"),
        ));
    }
    let enhance_profile = match inputs.profile.enhance() {
        Ok(p) => p,
        Err(e) => return fail(r, format!("profile.toml: {e}")),
    };
    // Data patches are an overlay for the generators; merge evaluates them on the original.
    let patched = patch::data_overlay(&inputs.patches, &loaded.game_data, t.build());
    let generated = crate::enhance::generate(
        patched.as_ref().unwrap_or(&loaded.game_data),
        &stock,
        &enhance_profile,
        &inputs.owned,
    );
    r.warnings.extend(generated.warnings.iter().cloned());
    let gen_edits: Vec<merge::GenEdit> =
        generated.edits.values().map(merge::GenEdit::from).collect();
    let merged = merge::merge(
        &stock,
        &loaded.game_data,
        t.build(),
        &gen_edits,
        &inputs.patches,
        &inputs.overrides,
    );
    r.counts = counts_json(&merged.counts, &generated);
    r.patches = patch::summarise(&inputs.patches, &merged.patch_outcomes);
    r.patch_entries = merged.patch_outcomes.clone();
    r.warnings.extend(merged.warnings.iter().cloned());

    // 6. Validate the whole file.
    let out = stock.render(&merged.edits);
    if let Err(e) = validate::check_file(&stock, &out) {
        return fail(r, format!("whole-file validation failed (writer bug): {e}"));
    }
    let out_sha = fsutil::sha256_hex(&out);
    r.output_sha256 = Some(out_sha.clone());

    // 7. Preflight.
    let ini = match locate_ini(&t.channel.dir) {
        Ok(i) => i,
        Err(e) => return fail(r, e),
    };
    r.target = Some(ini.real.clone());
    r.target_is_link = ini.is_link;
    if ini.exists
        && let Err(e) = fsutil::owned_regular_file(&ini.real)
    {
        return fail(r, e);
    }
    let real_dir = ini.real.parent().map(Path::to_path_buf).unwrap_or_default();
    if let Err(e) = write_probe(&real_dir) {
        return fail(r, e);
    }
    r.user_cfg.path = Some(cfg.path.clone());
    r.user_cfg.link_target = cfg.is_link.then(|| cfg.real.clone());
    r.user_cfg.shared_with = sharing_channels(t, &cfg.real);
    r.user_cfg.action = Some(cfg_action(&plan));
    if let Err(e) = cfg_writable(&cfg, &plan, &t.channel.dir) {
        return fail(r, e);
    }
    let cur_bytes = if ini.exists {
        match std::fs::read(&ini.real) {
            Ok(b) => Some(b),
            Err(e) => return fail(r, format!("{}: {e}", ini.real.display())),
        }
    } else {
        None
    };
    let outputs = match Outputs::load(&ctx.dirs) {
        Ok(o) => o,
        Err(e) => return fail(r, e.to_string()),
    };
    let cur_sha = cur_bytes.as_deref().map(fsutil::sha256_hex);
    let (origin, tool) = match (&cur_bytes, &cur_sha) {
        (Some(b), Some(s)) if !outputs.contains(s) => {
            (FileOrigin::Foreign, foreign_tool(b, &t.channel.dir))
        }
        (Some(_), _) => (FileOrigin::Ours, None),
        _ => (FileOrigin::None, None),
    };
    if origin == FileOrigin::Foreign {
        r.warnings.push(Warning::new(
            "foreign-file",
            format!(
                "replacing a global.ini from {} (kept as a backup); do not run two tools on one channel",
                tool.as_deref().unwrap_or("another tool")
            ),
        ));
    }
    if opts.dry_run {
        return finish_code(r, Outcome::DryRun, started);
    }
    if opts.budget.expired() {
        return r.end(
            Outcome::Skipped,
            exit::SKIPPED,
            Some("out of time before writing".into()),
            started,
        );
    }

    // 8. Back up.
    let all = backup::list(&data);
    let reason = if all.iter().any(|b| b.pinned()) {
        Reason::BeforeApply
    } else {
        Reason::PreExisting
    };
    let cur = backup::Current {
        real_path: &ini.real,
        bytes: cur_bytes.as_deref(),
        origin,
        foreign_tool: tool,
    };
    match backup::create(&data, reason, &ctx.app_version, t.build(), &cur) {
        Ok(Some(b)) => r.backup = Some(b.id),
        Ok(None) => r.backup = all.last().map(|b| b.id.clone()),
        Err(e) => return fail(r, format!("backup failed: {e}")),
    }
    if let Err(e) = backup::prune(&data) {
        r.warnings
            .push(Warning::new("backup", format!("retention: {e}")));
    }

    if let Some(p) = ctx.pause_before_cfg {
        eprintln!(
            "starorg-strings: test pause before user.cfg ({} ms)",
            p.as_millis()
        );
        std::thread::sleep(p);
    }

    // 9. user.cfg, before global.ini. The file may be shared with another channel whose run
    // changed it since step 3: re-read and re-plan under the lock, never write stale text.
    let _cfg_lock = match wait_lock(&usercfg::lock_path(&ctx.dirs, &cfg.real), CFG_LOCK_WAIT) {
        Ok(l) => l,
        Err(LockError::Held(_)) => {
            return r.end(
                Outcome::Skipped,
                exit::SKIPPED,
                Some(format!("{} is locked by another run", cfg.real.display())),
                started,
            );
        }
        Err(e) => return fail(r, e.to_string()),
    };
    let cfg = match usercfg::locate(&t.channel.dir) {
        Ok(c) => c,
        Err(e) => return fail(r, e.to_string()),
    };
    let cfg_text = match usercfg::read(&cfg) {
        Ok(t) => t,
        Err(e) => return fail(r, e.to_string()),
    };
    let plan = usercfg::plan(
        &match &cfg_text {
            None => LanguageLine::NoFile,
            Some(text) => usercfg::inspect(text),
        },
        cstate.switch_language,
    );
    if let Plan::NeedsDecision { value } = &plan {
        return r.end(
            Outcome::NeedsDecision,
            exit::NEEDS_DECISION,
            Some(format!(
                "the game language in {} changed to {value} while this run worked; nothing written",
                cfg.path.display()
            )),
            started,
        );
    }
    r.user_cfg.action = Some(cfg_action(&plan));
    if let Err(e) = cfg_writable(&cfg, &plan, &t.channel.dir) {
        return fail(r, e);
    }
    let cfg_changed = match update_user_cfg(ctx, t, &cfg, cfg_text.as_deref(), &plan) {
        Ok(c) => c,
        Err(e) => return fail(r, format!("user.cfg: {e}")),
    };
    r.user_cfg.changed = cfg_changed;
    let undo_cfg = |ctx: &Ctx| {
        if cfg_changed {
            let _ = usercfg::release(&ctx.dirs, &cfg, &t.channel_ref());
        }
    };

    // 10. Journal.
    let fp = inputs.fingerprint.clone().unwrap_or(Fingerprint {
        change: None,
        p4k_size: 0,
        p4k_mtime_ns: 0,
    });
    let now = fsutil::unix_now();
    let mut warnings_for_record = r.warnings.clone();
    warnings_for_record.dedup();
    let applied = Applied {
        branch: r.build.branch.clone(),
        game_version: r.build.game_version.clone(),
        change: r.build.change,
        version: r.build.version.clone(),
        fingerprint: (&fp).into(),
        cache_key: key_inputs.as_ref().map(KeyInputs::key).unwrap_or_default(),
        profile_hash: inputs.profile_hash.clone(),
        owned_hash: inputs.owned.hash.clone(),
        patch_set_hash: inputs.patches.hash(),
        overrides_hash: inputs.overrides_hash.clone(),
        app_version: ctx.app_version.clone(),
        output_sha256: out_sha.clone(),
        time: fsutil::rfc3339(now),
        backup: r.backup.clone(),
        warnings: warnings_for_record,
        real_target: ini.real.clone(),
        user_cfg_changed: cfg_changed,
        owned_choice: inputs.owned_choice.clone(),
        owned_setting: inputs
            .owned_choice
            .as_ref()
            .and(inputs.owned_setting.clone()),
    };
    let output = OutputEntry {
        sha256: out_sha.clone(),
        install_id: t.id().to_string(),
        channel: t.channel.name.to_ascii_uppercase(),
        change: r.build.change,
        time: fsutil::rfc3339(now),
    };
    let tmp = real_dir.join(INI_TMP);
    let intent = Intent {
        output_sha256: out_sha.clone(),
        real_target: ini.real.clone(),
        temp: tmp.clone(),
        backup: r.backup.clone(),
        user_cfg: cfg_changed.then(|| cfg.clone()),
        applied: applied.clone(),
        output: output.clone(),
    };
    let intent_path = data.join(INTENT_JSON);
    if let Err(e) = write_json(&intent_path, &intent) {
        undo_cfg(ctx);
        return fail(r, format!("journal: {e}"));
    }

    // 11. Write.
    let write =
        std::fs::create_dir_all(&real_dir).and_then(|()| fsutil::write_via(&tmp, &ini.real, &out));
    if let Err(e) = write {
        undo_cfg(ctx);
        let _ = std::fs::remove_file(&intent_path);
        return fail(r, format!("cannot write {}: {e}", ini.real.display()));
    }
    if let Some(p) = ctx.pause_after_write {
        eprintln!(
            "starorg-strings: test pause after write ({} ms)",
            p.as_millis()
        );
        std::thread::sleep(p);
    }

    // 12. Verify.
    if fsutil::sha256_file(&ini.real).ok().as_deref() != Some(out_sha.as_str()) {
        let restore = match &cur_bytes {
            Some(b) => fsutil::write_via(&tmp, &ini.real, b),
            None => std::fs::remove_file(&ini.real),
        };
        undo_cfg(ctx);
        let _ = std::fs::remove_file(&intent_path);
        return fail(
            r,
            format!(
                "read-back of {} differs from what was written; previous state {}",
                ini.real.display(),
                if restore.is_ok() {
                    "put back"
                } else {
                    "could NOT be put back"
                }
            ),
        );
    }

    // 13. Record.
    if let Err(e) = write_json(&data.join(APPLIED_JSON), &applied) {
        return fail(
            r,
            format!("record: {e} (the journal will finish this on the next run)"),
        );
    }
    if let Err(e) = add_output(ctx, output) {
        return fail(
            r,
            format!("record outputs.json: {e} (the journal will finish this on the next run)"),
        );
    }
    let _ = std::fs::remove_file(&intent_path);

    // 14. Report.
    finish_code(r, Outcome::Applied, started)
}

/// Warning kinds that are shown but do not make the result "applied with warnings" (exit 5):
/// they say nothing about missing text.
pub const INFO_KINDS: &[&str] = &[
    "launcher-running",
    "cache",
    "foreign-file",
    "link-kept",
    "owned-ambiguous",
];

fn finish_code(r: Report, outcome: Outcome, started: Instant) -> Report {
    let code = if r
        .warnings
        .iter()
        .any(|w| !INFO_KINDS.contains(&w.kind.as_str()))
    {
        exit::WARNINGS
    } else {
        exit::OK
    };
    r.end(outcome, code, None, started)
}

fn setup_hint(t: &Target<'_>) -> String {
    format!(
        "channel {} is not set up; run `starorg-strings setup --channel {} --preset recommended --yes`",
        t.channel.name, t.channel.name
    )
}

/// Step 9: change `user.cfg` if the plan says so and keep the ledger. Returns whether the
/// file was written.
fn update_user_cfg(
    ctx: &Ctx,
    t: &Target<'_>,
    cfg: &CfgTarget,
    text: Option<&str>,
    plan: &Plan,
) -> std::io::Result<bool> {
    let me = t.channel_ref();
    let mut ledger = usercfg::load_ledger(&ctx.dirs, &cfg.real)?;
    match plan {
        Plan::Nothing | Plan::NeedsDecision { .. } => {
            // The line may be ours from another channel sharing the file: rely on it too.
            if let Some(l) = ledger.as_mut()
                && text.is_some_and(|tx| tx.lines().any(|x| x.trim_end_matches('\r') == l.written))
                && !l.channels.contains(&me)
            {
                l.channels.push(me);
                usercfg::save_ledger(&ctx.dirs, l)?;
            }
            Ok(false)
        }
        Plan::Append | Plan::Replace { .. } => {
            let before = text.unwrap_or("");
            let (new_text, added_newline) = usercfg::edit(before, plan);
            let reference_copy = if text.is_some() {
                let p = ctx
                    .dirs
                    .cfg_dir()
                    .join(format!("{}.before.cfg", usercfg::path_hash(&cfg.real)));
                if !p.exists() {
                    fsutil::write_atomic(&p, before.as_bytes())?;
                }
                Some(p)
            } else {
                None
            };
            // Channels on an old ledger still rely on the line (someone removed it by hand):
            // keep them, so undo waits for all of them.
            let mut channels: Vec<String> = ledger.map(|l| l.channels).unwrap_or_default();
            if !channels.contains(&me) {
                channels.push(me);
            }
            // Ledger first: if we die after the file write, undo can still find it.
            let new_ledger = Ledger {
                real_path: cfg.real.clone(),
                previous: match plan {
                    Plan::Replace { previous } => Some(previous.clone()),
                    _ => None,
                },
                written: usercfg::OUR_LINE.to_string(),
                added_newline,
                channels,
                reference_copy,
                created: text.is_none(),
            };
            usercfg::save_ledger(&ctx.dirs, &new_ledger)?;
            if let Err(e) = usercfg::write(cfg, &new_text) {
                let _ = usercfg::delete_ledger(&ctx.dirs, &cfg.real);
                return Err(e);
            }
            Ok(true)
        }
    }
}

// ---------------------------------------------------------------------------------------
// Remove and restore (spec 7.6)
// ---------------------------------------------------------------------------------------

/// Remove StarOrg changes (back to stock).
pub fn remove(ctx: &Ctx, t: &Target<'_>, yes: bool) -> Report {
    let started = Instant::now();
    let mut r = Report::new(t);
    let fail = |r: Report, why: String| r.end(Outcome::Failed, exit::ERROR, Some(why), started);
    let _lock = match take_channel_lock(ctx, t) {
        Ok(l) => l,
        Err((o, c, why)) => return r.end(o, c, Some(why), started),
    };
    match recover(ctx, t) {
        Ok(m) => r.recovered = m,
        Err(e) => return fail(r, e),
    }
    match run_guards(ctx, t, false) {
        Guard::Pass(w) => r.warnings.extend(w),
        Guard::Stop(o, c, why) => return r.end(o, c, Some(why), started),
    }
    let data = t.data_dir(&ctx.dirs);
    let set_up = t.install_id.is_some()
        && ChannelState::load(&data)
            .map(|s| s.consent)
            .unwrap_or(false);
    let ini = match locate_ini(&t.channel.dir) {
        Ok(i) => i,
        Err(e) => return fail(r, e),
    };
    r.target = Some(ini.real.clone());
    r.target_is_link = ini.is_link;
    let mut removed_file = false;
    if ini.exists {
        let bytes = match std::fs::read(&ini.real) {
            Ok(b) => b,
            Err(e) => return fail(r, e.to_string()),
        };
        let sha = fsutil::sha256_hex(&bytes);
        let outputs = Outputs::load(&ctx.dirs).unwrap_or_default();
        let ours = outputs.contains(&sha);
        if !ours {
            if !set_up {
                return r.end(
                    Outcome::NeedsSetup,
                    exit::NEEDS_DECISION,
                    Some(format!(
                        "the loose global.ini is not ours and {} is not set up; nothing removed",
                        t.channel.name
                    )),
                    started,
                );
            }
            let tool = foreign_tool(&bytes, &t.channel.dir);
            if !yes {
                return r.end(
                    Outcome::NeedsDecision,
                    exit::NEEDS_DECISION,
                    Some(format!(
                        "the loose global.ini is from {}; pass --yes to back it up and remove it",
                        tool.as_deref().unwrap_or("another tool")
                    )),
                    started,
                );
            }
            let reason = if backup::list(&data).iter().any(|b| b.pinned()) {
                Reason::Manual
            } else {
                Reason::PreExisting
            };
            let cur = backup::Current {
                real_path: &ini.real,
                bytes: Some(&bytes),
                origin: FileOrigin::Foreign,
                foreign_tool: tool.clone(),
            };
            match backup::create(&data, reason, &ctx.app_version, t.build(), &cur) {
                Ok(b) => r.backup = b.map(|b| b.id),
                Err(e) => return fail(r, format!("backup failed: {e}")),
            }
            r.warnings.push(Warning::new(
                "foreign-file",
                format!(
                    "removed a global.ini from {} (backed up)",
                    tool.as_deref().unwrap_or("another tool")
                ),
            ));
        }
        if let Err(e) = std::fs::remove_file(&ini.real) {
            return fail(r, format!("cannot delete {}: {e}", ini.real.display()));
        }
        if ini.is_link {
            r.warnings.push(Warning::new(
                "link-kept",
                format!(
                    "{} is a link; deleted the file it points to ({}) and kept the link, so the game reads stock text. The next apply writes through it again",
                    ini.path.display(),
                    ini.real.display()
                ),
            ));
        }
        if let Some(d) = ini.real.parent() {
            let _ = fsutil::fsync_dir(d);
        }
        removed_file = true;
    }
    // user.cfg by the ledger rule.
    if t.install_id.is_some()
        && let Ok(cfg) = usercfg::locate(&t.channel.dir)
    {
        r.user_cfg.path = Some(cfg.path.clone());
        r.user_cfg.link_target = cfg.is_link.then(|| cfg.real.clone());
        match wait_lock(&usercfg::lock_path(&ctx.dirs, &cfg.real), CFG_LOCK_WAIT) {
            Ok(_l) => match usercfg::release(&ctx.dirs, &cfg, &t.channel_ref()) {
                Ok(o) => {
                    r.user_cfg.changed = o == UndoOutcome::Reverted;
                    if o == UndoOutcome::ChangedSince {
                        r.warnings.push(Warning::new(
                            "user-cfg",
                            "our g_language line was changed since; left alone",
                        ));
                    }
                    if let UndoOutcome::StillUsed { channels } = &o {
                        r.user_cfg.shared_with = channels.clone();
                    }
                    r.user_cfg.action = Some(o.kebab().to_string());
                }
                Err(e) => return fail(r, format!("user.cfg: {e}")),
            },
            Err(LockError::Held(_)) => {
                return r.end(
                    Outcome::Skipped,
                    exit::SKIPPED,
                    Some("user.cfg is locked by another run".into()),
                    started,
                );
            }
            Err(e) => return fail(r, e.to_string()),
        }
    }
    let _ = std::fs::remove_file(data.join(APPLIED_JSON));
    let outcome = if removed_file || r.user_cfg.changed {
        Outcome::Removed
    } else {
        Outcome::NothingToDo
    };
    finish_code(r, outcome, started)
}

/// Restore a backup.
pub fn restore(ctx: &Ctx, t: &Target<'_>, backup_id: &str, allow_build_mismatch: bool) -> Report {
    let started = Instant::now();
    let mut r = Report::new(t);
    let fail = |r: Report, why: String| r.end(Outcome::Failed, exit::ERROR, Some(why), started);
    if t.install_id.is_none() {
        return r.end(
            Outcome::NeedsSetup,
            exit::NEEDS_DECISION,
            Some(setup_hint(t)),
            started,
        );
    }
    let _lock = match take_channel_lock(ctx, t) {
        Ok(l) => l,
        Err((o, c, why)) => return r.end(o, c, Some(why), started),
    };
    match recover(ctx, t) {
        Ok(m) => r.recovered = m,
        Err(e) => return fail(r, e),
    }
    let data = t.data_dir(&ctx.dirs);
    if !ChannelState::load(&data)
        .map(|s| s.consent)
        .unwrap_or(false)
    {
        return r.end(
            Outcome::NeedsSetup,
            exit::NEEDS_DECISION,
            Some(setup_hint(t)),
            started,
        );
    }
    match run_guards(ctx, t, false) {
        Guard::Pass(w) => r.warnings.extend(w),
        Guard::Stop(o, c, why) => return r.end(o, c, Some(why), started),
    }
    let Some(b) = backup::find(&data, backup_id) else {
        return r.end(
            Outcome::Failed,
            exit::USAGE,
            Some(format!("no backup {backup_id} for {}", t.channel.name)),
            started,
        );
    };
    let now_change = t.build().and_then(|b| b.change);
    if b.manifest.build.change != now_change && !allow_build_mismatch {
        return r.end(
            Outcome::NeedsDecision,
            exit::NEEDS_DECISION,
            Some(format!(
                "backup {} was made for build {} and the game is now build {}; new text would show as raw @keys. Pass --allow-build-mismatch to restore anyway",
                b.id,
                b.manifest.build.change.map_or("unknown".into(), |c| c.to_string()),
                now_change.map_or("unknown".into(), |c| c.to_string()),
            )),
            started,
        );
    }
    let ini = match locate_ini(&t.channel.dir) {
        Ok(i) => i,
        Err(e) => return fail(r, e),
    };
    r.target = Some(ini.real.clone());
    let real_dir = ini.real.parent().map(Path::to_path_buf).unwrap_or_default();
    if let Err(e) = write_probe(&real_dir) {
        return fail(r, e);
    }
    let want: Option<Vec<u8>> = if b.manifest.file.existed {
        match std::fs::read(b.ini_path()) {
            Ok(bytes) if Some(fsutil::sha256_hex(&bytes)) == b.manifest.file.sha256 => Some(bytes),
            Ok(_) => return fail(r, format!("backup {} is damaged (hash mismatch)", b.id)),
            Err(e) => return fail(r, format!("backup {}: {e}", b.id)),
        }
    } else {
        None
    };
    // Keep what is there now.
    if ini.exists {
        let bytes = match std::fs::read(&ini.real) {
            Ok(b) => b,
            Err(e) => return fail(r, e.to_string()),
        };
        let outputs = Outputs::load(&ctx.dirs).unwrap_or_default();
        let origin = if outputs.contains(&fsutil::sha256_hex(&bytes)) {
            FileOrigin::Ours
        } else {
            FileOrigin::Foreign
        };
        let cur = backup::Current {
            real_path: &ini.real,
            bytes: Some(&bytes),
            foreign_tool: (origin == FileOrigin::Foreign)
                .then(|| foreign_tool(&bytes, &t.channel.dir))
                .flatten(),
            origin,
        };
        match backup::create(
            &data,
            Reason::BeforeRestore,
            &ctx.app_version,
            t.build(),
            &cur,
        ) {
            Ok(nb) => r.backup = nb.map(|x| x.id),
            Err(e) => return fail(r, format!("backup failed: {e}")),
        }
        let _ = backup::prune(&data);
    }
    let result = match &want {
        Some(bytes) => std::fs::create_dir_all(&real_dir)
            .and_then(|()| fsutil::write_via(&real_dir.join(INI_TMP), &ini.real, bytes)),
        None if ini.exists => std::fs::remove_file(&ini.real),
        None => Ok(()),
    };
    if let Err(e) = result {
        return fail(r, format!("cannot restore {}: {e}", ini.real.display()));
    }
    if let Some(bytes) = &want
        && fsutil::sha256_file(&ini.real).ok() != Some(fsutil::sha256_hex(bytes))
    {
        return fail(r, "read-back after restore differs".into());
    }
    // Status recomputes from the file; the old record no longer describes it.
    let _ = std::fs::remove_file(data.join(APPLIED_JSON));
    r.reason = Some(format!("restored backup {}", b.id));
    let reason = r.reason.clone();
    let mut rep = finish_code(r, Outcome::Restored, started);
    rep.reason = reason;
    rep
}

#[cfg(test)]
mod tests {
    use super::exit::worst;

    #[test]
    fn exit_code_severity() {
        assert_eq!(worst([0, 5, 3]), 3);
        assert_eq!(worst([0, 6, 1]), 1);
        assert_eq!(worst([5, 4, 6]), 6);
        assert_eq!(worst([5, 0]), 5);
        assert_eq!(worst([]), 0);
    }
}

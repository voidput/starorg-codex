//! Data shapes of the app <-> core command contract.
//!
//! Every type here is serialised to the webview as JSON and mirrored by hand in
//! `app/src/lib/contract/types.ts`. Two tests keep the mirror honest:
//!
//! - `contract::tests::samples_match_the_committed_file` writes one sample of every response
//!   (from the mock backend) to `app/src/lib/contract/samples.gen.json`; `svelte-check` then
//!   type-checks that file against the TS types (`samples.check.ts`).
//! - `contract::tests::ts_unions_list_every_variant` checks that each string union in the TS
//!   file lists exactly the serde names of the matching Rust enum.
//!
//! Field names follow the CLI's `--json` output where one exists (spec 8), so the real backend
//! can fill them from the same core structs (M1 `apply::Report`, `apply::Status`,
//! `backup::Manifest`, `blueprints::import::ImportReport`, `hooks::BlockState`).
//!
//! Rules: snake_case fields, kebab-case enum values, `Option` = `T | null` (never omitted),
//! counts and change numbers fit in a JS number. Paths are strings for display only; the
//! webview never sends a path back (spec 11).

use serde::{Deserialize, Serialize};

/// Bump on a breaking change; the TS client refuses a different major.
pub const CONTRACT_VERSION: u32 = 1;

/// Name of the progress event the backend emits while a job runs.
pub const JOB_EVENT: &str = "job-progress";

// ---------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    /// Bad arguments from the UI (a bug on our side).
    Invalid,
    NotFound,
    /// Channel not set up, language not English, restore across builds (CLI exit 6).
    NeedsDecision,
    /// Game running, lock held, Data.p4k being written (CLI exit 3).
    Busy,
    /// Network feature used while its switch is off.
    NetworkOff,
    Cancelled,
    /// The feature waits on something outside the app (StarOrg sync: SC-ORG-BOT#1659).
    NotImplemented,
    Internal,
    /// A Developer mode command while Settings > Advanced > "Developer mode" is off.
    DevDisabled,
}

/// Every command fails with this shape (`Result<T, AppError>`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    /// Extra lines for the error state (which rule, which file).
    pub detail: Option<String>,
}

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            detail: None,
        }
    }
}

pub type CmdResult<T> = Result<T, AppError>;

// ---------------------------------------------------------------------------------------
// Builds, installs, channels, status
// ---------------------------------------------------------------------------------------

/// Same fields as the CLI's `build` object (spec 8).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Build {
    pub branch: Option<String>,
    pub game_version: Option<String>,
    pub change: Option<u64>,
    pub version: Option<String>,
}

/// Channel status (spec 7.5), tagged like M1's `apply::State`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum ApplyState {
    Applied,
    Stale { reasons: Vec<String> },
    ChangedByAnotherTool { tool: Option<String> },
    NotApplied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Language {
    English,
    Other,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelSummary {
    pub channel: String,
    pub build: Build,
    pub build_error: Option<String>,
    pub set_up: bool,
    pub status: ApplyState,
    /// `g_language` in user.cfg (spec 7.4): the wizard asks instead of defaulting when Other.
    pub language: Language,
    pub language_value: Option<String>,
    /// A loose global.ini from another tool, named when we can tell (spec 7.5).
    pub foreign_tool: Option<String>,
    pub foreign_file: bool,
    pub game_running: bool,
    pub interrupted_apply: bool,
    /// `Data.p4k` modification time, RFC 3339, for the footer.
    pub p4k_time: Option<String>,
    pub cache: CacheState,
    /// Build the file in place was made for (M1 applied manifest); compare with `build`.
    pub applied_build: Option<Build>,
    pub last_apply: Option<LastApply>,
}

/// The last apply recorded in the applied manifest (spec 7.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastApply {
    pub time: String,
    pub result: ApplyResult,
    pub changed: u32,
    pub warnings: u32,
    pub dropped: u32,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CacheState {
    Fresh,
    Missing,
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FoundBy {
    Config,
    GameDir,
    Prefix,
    LugConfig,
    DefaultPrefix,
    Lutris,
    LauncherLog,
    CommonPath,
    Scan,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallSummary {
    /// Random install id once saved (spec 4.2); a discovery-time id before that.
    pub id: String,
    pub root: String,
    pub prefix: Option<String>,
    pub found_by: FoundBy,
    pub found_by_label: String,
    pub saved: bool,
    pub channels: Vec<ChannelSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Overview {
    pub contract: u32,
    pub app_version: String,
    /// No channel is set up yet: the UI shows the setup wizard.
    pub first_run: bool,
    pub installs: Vec<InstallSummary>,
    pub active_install: Option<String>,
    pub default_channel: Option<String>,
    pub job: Option<JobProgress>,
}

/// Address of one channel. The install id comes from `Overview`/`discover_installs`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChannelRef {
    pub install: String,
    pub channel: String,
}

// ---------------------------------------------------------------------------------------
// Jobs and progress events
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JobKind {
    Discover,
    Extract,
    Apply,
    Remove,
    Restore,
    Import,
    Sync,
    /// A LUG launch hook ran (`apply --all --if-stale` from sc-prelaunch / sc-postexit).
    Hook,
    /// Developer mode: worker start, record export, dataset diff (addendum 7).
    Dev,
    /// The experimental local MCP server (started, stopped, a dataset extracted for it).
    Mcp,
    /// UEX market data downloaded (Settings -> Network -> "Download UEX market data").
    Uex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JobPhase {
    Running,
    Done,
    Failed,
    Cancelled,
}

/// Payload of [`JOB_EVENT`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobProgress {
    pub job: String,
    pub kind: JobKind,
    pub channel: Option<String>,
    pub phase: JobPhase,
    /// 1-based step and total; the wizard's "Read game data" shows one row per step.
    pub step: u32,
    pub steps: u32,
    pub label: String,
    pub cancellable: bool,
}

// ---------------------------------------------------------------------------------------
// Setup (wizard)
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Preset {
    Recommended,
    Minimal,
    Everything,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelConsent {
    pub channel: String,
    pub consent: bool,
    /// Only meaningful when the channel's language is not English (spec 7.4).
    pub switch_language: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupRequest {
    pub install: String,
    pub channels: Vec<ChannelConsent>,
    // Game data is always read from Data.p4k on this machine: the old `data_source` field
    // (local | published-dumps) is gone since 2026-11-17 and ignored when an older
    // frontend still sends it.
    pub preset: Preset,
    pub fix_known_bugs: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupResult {
    pub install: String,
    pub set_up: Vec<String>,
    pub skipped: Vec<String>,
}

/// What setup + the first apply would do per channel: the wizard's review step (spec 9.4).
/// Real backend: path resolution (spec 10.3) and a dry-run build. Nothing is written in the
/// game folder; the app's own folders get the install registered and the game-data cache.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedChannel {
    pub channel: String,
    pub target: String,
    pub target_is_link: bool,
    pub link_target: Option<String>,
    pub backup_dir: String,
    /// The `user.cfg` line we add, or null for "no change needed".
    pub user_cfg_line: Option<String>,
    pub user_cfg_path: Option<String>,
    /// Another tool's file that will be backed up (pinned) and replaced.
    pub replaces_tool: Option<String>,
    pub counts: ApplyCounts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupPlan {
    pub install: String,
    pub channels: Vec<PlannedChannel>,
    pub warnings: Vec<Warning>,
}

// ---------------------------------------------------------------------------------------
// Apply, preview, remove, restore, backups
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApplyResult {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyCounts {
    pub changed: u32,
    pub ships: u32,
    pub components: u32,
    pub ship_weapons: u32,
    pub fps_weapons: u32,
    pub missiles: u32,
    pub missions: u32,
    /// Descriptions that got a WHERE TO GET block (spec obtain-hints).
    #[serde(default)]
    pub obtain_hints: u32,
    /// Name keys stock lacks that got a name, and description keys stock lacks (or holds as a
    /// placeholder) that got a sibling's description (spec missing-names).
    #[serde(default)]
    pub missing_names: u32,
    pub overrides: u32,
    pub dropped: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Warning {
    pub kind: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserCfgReport {
    pub changed: bool,
    pub path: Option<String>,
    pub link_target: Option<String>,
    pub shared_with: Vec<String>,
    /// The line we add or remove, or null for "no change needed".
    pub line: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchOutcome {
    pub id: String,
    pub state: PatchState,
}

/// What an apply used for `[Owned]` markers (M1 `blueprints::Selection`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedSelection {
    pub mode: OwnedMode,
    /// geid(s) whose sets were used.
    pub characters: Vec<String>,
    pub count: u32,
    /// Names matched to several blueprints: listed, never marked.
    pub ambiguous: u32,
}

/// M1 `Selection::mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OwnedMode {
    Auto,
    Any,
    Character,
}

/// The `apply` JSON entry of spec 8, plus what the review step shows.
///
/// A returned report is not a success: M1 reports guards (game running, Data.p4k being
/// written, lock held, timeout) as `skipped` + `reason`, and `failed`, `needs-setup`,
/// `needs-decision`, `up-to-date` the same way. The UI switches on `result`
/// (`app/src/lib/outcome.ts`); `Err(AppError)` is only for calls that could not run at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyReport {
    pub channel: String,
    pub result: ApplyResult,
    pub reason: Option<String>,
    pub build: Build,
    pub counts: ApplyCounts,
    pub patches: Vec<PatchOutcome>,
    pub warnings: Vec<Warning>,
    pub user_cfg: UserCfgReport,
    pub backup: Option<String>,
    pub backup_dir: Option<String>,
    pub target: Option<String>,
    pub target_is_link: bool,
    /// M1 journal recovery: an interrupted earlier apply was rolled back (shown to the user).
    pub recovered: Option<String>,
    pub owned: Option<OwnedSelection>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyRequest {
    pub target: ChannelRef,
    pub dry_run: bool,
}

/// M1 `remove(ctx, t, yes)`: without `allow_foreign`, a loose file that belongs to another
/// tool is not removed and the report comes back `needs-decision`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoveRequest {
    pub target: ChannelRef,
    pub allow_foreign: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackupReason {
    PreExisting,
    BeforeApply,
    BeforeRestore,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileOrigin {
    Ours,
    Foreign,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupView {
    pub id: String,
    pub created: String,
    pub reason: BackupReason,
    pub pinned: bool,
    pub build: Build,
    pub existed: bool,
    pub size: Option<u64>,
    pub origin: FileOrigin,
    pub foreign_tool: Option<String>,
}

/// The three actions of spec 7.6, with what the dialogs need to say.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreOffers {
    /// Remove StarOrg changes (back to stock). Always offered when a loose file exists.
    pub remove: bool,
    /// The loose file is not ours: remove backs it up first and says so.
    pub remove_foreign: bool,
    /// Undo the last apply: a `before-apply` backup under the current change number.
    pub undo_last_apply: Option<String>,
    /// Restore the file from before StarOrg Codex (pinned, origin foreign).
    pub restore_foreign: Option<ForeignRestore>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForeignRestore {
    pub backup: String,
    pub tool: Option<String>,
    pub made_for: Build,
    pub current: Build,
    /// Build differs: the dialog needs a second, explicit confirmation.
    pub build_mismatch: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupList {
    pub channel: String,
    pub folder: String,
    pub backups: Vec<BackupView>,
    pub offers: RestoreOffers,
    /// Retention (M1 `backup::KEEP_NEWEST`, `KEEP_BUILDS`): the newest N backups, plus the newest
    /// one of each of the last `keep_builds` game builds, plus the pinned pre-existing file.
    pub keep_newest: u32,
    pub keep_builds: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupRef {
    pub target: ChannelRef,
    pub backup: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreRequest {
    pub target: ChannelRef,
    pub backup: String,
    pub allow_build_mismatch: bool,
}

/// One rendered sample for the in-game preview (wizard step 4, Enhancements).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewItem {
    pub key: String,
    pub label: String,
    pub category: String,
    /// Raw game text: `\n` escapes and `<EMn>` tags, rendered by InGamePreview as text nodes.
    pub original: String,
    pub result: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewRequest {
    pub target: ChannelRef,
    /// The unsaved profile being edited; null = the saved one.
    pub profile: Option<Profile>,
    /// Or a preset (the wizard's preset cards).
    pub preset: Option<Preset>,
}

/// Read a set-up channel's game data into the cache (Enhancements' "Read game data"), so
/// the preview has examples without an apply. Writes nothing into the game folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadGameDataRequest {
    pub target: ChannelRef,
}

// ---------------------------------------------------------------------------------------
// Enhancements profile (spec 4.7, 5, 6)
// ---------------------------------------------------------------------------------------

/// Generator categories with an on/off switch (M2 `enhance::Profile` booleans).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CategoryId {
    Ships,
    Components,
    ShipWeapons,
    FpsWeapons,
    Missiles,
    Missions,
    /// WHERE TO GET (spec obtain-hints): `Profile::obtain`, lines `obtain.<line>`.
    Obtain,
}

/// Tag kinds of the Tag Builder (M2 `enhance::Tags` fields).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TagKind {
    Components,
    ShipWeapons,
    Missiles,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TagStyle {
    Short,
    Medium,
    Long,
}

/// M2 separators `-`, ``, ` `, `_`, `.`, `/`, `|`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TagSeparator {
    Dash,
    None,
    Space,
    Underscore,
    Dot,
    Slash,
    Pipe,
}

/// M2 enclosings `[]`, `()`, `{}`, `<>`, ``.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TagEnclosing {
    Square,
    Round,
    Curly,
    Angle,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TagPlacement {
    Prefix,
    Suffix,
}

/// One tag kind (M2 `enhance::TagStyle`). `elements` lists the parts that are on, in output
/// order; parts not listed are off. `styles` holds non-default styles (default `medium`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagKindSettings {
    pub enabled: bool,
    pub elements: Vec<String>,
    pub styles: std::collections::BTreeMap<String, TagStyle>,
    pub separator: TagSeparator,
    pub enclosing: TagEnclosing,
    pub placement: TagPlacement,
}

/// M2 `enhance::Tags`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagSettings {
    pub components: TagKindSettings,
    pub ship_weapons: TagKindSettings,
    pub missiles: TagKindSettings,
    /// Also tag the short-name key when the record has one (one switch for every kind).
    pub short_names: bool,
    /// Size-only tag on tractor beams, salvage heads, mining lasers and EMPs.
    pub tool_size_tag: bool,
}

/// M2 `enhance::Mission`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissionSettings {
    /// `[BP]`, or `[BP?]` when only some variants can reward one: one switch in core.
    pub title_blueprint: bool,
    pub title_reputation: bool,
    /// Wrap title tags in `<EM4>`.
    pub title_em: bool,
    pub blueprints: bool,
    pub item_rewards: bool,
    pub details: bool,
    /// Name tags on blueprint bullets.
    pub bullet_tags: bool,
    /// `[Owned]` on bullets of blueprints the active character owns.
    pub owned_marker: bool,
    /// Lists longer than this end with `- and N more`.
    pub max_list: u32,
    /// EM tag for section headers: 0 none, 1-5 (stock uses 3).
    pub header_em: u8,
    /// EM tag for detail labels: 0 none, 1-5.
    pub label_em: u8,
    /// EM tag of `[BP]` / `[BP?]` / `[BP Owned]` while `title_em` is on: 0 none, 1-5.
    pub title_bp_em: u8,
    /// EM tag of `[N Rep]` while `title_em` is on: 0 none, 1-5.
    pub title_rep_em: u8,
    /// `[BP Owned]` when the character owns every blueprint the contract can award.
    pub title_owned: bool,
    /// EM tag around `[Owned]`: 0 none, 1-5.
    pub owned_em: u8,
    /// Colour the whole owned line, not only `[Owned]`.
    pub owned_line: bool,
    /// `[Missing]` on blueprints the character does not own (needs an owned set).
    pub missing_marker: bool,
    /// EM tag around `[Missing]`: 0 none, 1-5.
    pub missing_em: u8,
    /// EM tag of variant sub-headers such as `[Easy]`: 0 none, 1-5.
    pub variant_em: u8,
    /// EM tag around item names under ITEM REWARDS: 0 none, 1-5.
    pub reward_em: u8,
}

/// The channel's `profile.toml` (spec 4.7): M2 `enhance::Profile` field for field, plus the
/// preset it was started from and M1's `[patches] disabled`. The real backend converts with
/// no defaults of its own, so every switch here changes the output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    /// The preset this profile equals, or null once anything differs.
    pub preset: Option<Preset>,
    pub ships: bool,
    pub components: bool,
    pub ship_weapons: bool,
    pub fps_weapons: bool,
    pub missiles: bool,
    pub missions: bool,
    /// `Used in: <contract> · <system>` on items a mission spawns or a contract asks for
    /// (real record references only, addendum §13).
    pub quest_hints: bool,
    /// WHERE TO GET in item and commodity descriptions (spec obtain-hints).
    pub obtain: bool,
    pub obtain_mode: ObtainMode,
    /// Shop names from the user's UEX files in the block (never prices).
    pub obtain_shops: bool,
    /// The `sell` line: the best commodity kiosks with a rounded price per SCU and the day
    /// the UEX prices were fetched. Off by default, on in Everything.
    pub obtain_prices: bool,
    /// Names for items whose name key stock lacks, and a sibling's description for items whose
    /// description key stock lacks (spec missing-names): adds keys; only placeholder stock
    /// lines are replaced.
    pub missing_names: bool,
    /// Stat lines switched off, as `<category>.<line>` (keys from [`ProfileSchema`]).
    pub lines_off: Vec<String>,
    pub stats_above: bool,
    /// Plain ASCII; no `%`, `<`, `~` or `\n` (core refuses them).
    pub header: String,
    pub tags: TagSettings,
    pub mission: MissionSettings,
    pub branding: bool,
    /// Colour test: appends `Colour test: EM1 … EM5` (each in its tag) to the Contracts
    /// Manager's "Please select a contract." prompt. Never in a preset.
    pub colour_test: bool,
    /// Patch ids switched off. "Fix known text bugs" off = every built-in id listed here.
    pub patches_disabled: Vec<String>,
}

/// How much the WHERE TO GET block says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ObtainMode {
    Short,
    Normal,
    Detailed,
}

/// One UEX file the WHERE TO GET shop line reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UexFile {
    /// `uex-item-prices.json`, `uex-commodity-prices.json`, `uex-terminals.json`.
    pub name: String,
    pub present: bool,
    /// Last modified (RFC 3339 UTC), when present.
    pub modified: Option<String>,
}

/// Where the shop names and sell prices of WHERE TO GET come from (spec obtain-hints): UEX
/// files in `uex_folder`, downloaded by `fetch_uex` while "Download UEX market data" is on
/// or put there by the user. Shops are names only; the sell line has rounded prices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObtainStatus {
    /// `<cache>/mcp/uex` (shared with the Sources card and the MCP server).
    pub uex_folder: String,
    pub files: Vec<UexFile>,
    /// Item prices and terminals are there: the shops line can say something.
    pub shops_ready: bool,
    /// Commodity prices and terminals are there: the sell line can say something.
    pub prices_ready: bool,
}

/// Whether a UEX API token is saved, and where. Never the token itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UexTokenStatus {
    pub set: bool,
    pub store: Option<TokenStoreKind>,
}

/// Save (`token`) or remove (`null` or empty) the UEX API token.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UexTokenRequest {
    pub token: Option<String>,
}

// The Loot screen (`loot_list`, `loot_detail`): every loot area with what drops there and
// every item with where, from the channel's cached loot data, typed in `starorg_mcp::loot`.
pub use starorg_mcp::loot::{
    LootDataState, LootDetail, LootDrop, LootDropKind, LootGroup, LootGroupBy, LootList,
    LootSummary, LootView,
};

/// Every loot area (`view: areas`) or every item that drops in one (`items`), summaries
/// only. `target`: the channel whose cached loot data is read (`None`: `missing`). Nothing
/// is extracted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LootListRequest {
    pub target: Option<ChannelRef>,
    pub view: LootView,
}

/// One area's items or one item's areas in groups; `id` is a [`LootSummary::id`] of that
/// view. `filter`: only the drops holding every word of it (name, path, container, rarity,
/// id), over every drop. `group_by`: `None` for the view's default (areas by container,
/// items by place). `groups`: the group keys whose rows to answer (opening a group); `None`
/// answers the default ones (the first and a few small ones, or the matches of a filter).
/// Every group's header covers all its drops; rows are capped per group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LootDetailRequest {
    pub target: Option<ChannelRef>,
    pub view: LootView,
    pub id: String,
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default)]
    pub group_by: Option<LootGroupBy>,
    #[serde(default)]
    pub groups: Option<Vec<String>>,
}

impl LootDetailRequest {
    /// What [`starorg_mcp::loot::LootScreen::detail`] takes.
    pub fn query(&self) -> starorg_mcp::loot::DetailQuery<'_> {
        starorg_mcp::loot::DetailQuery {
            filter: self.filter.as_deref(),
            group_by: self.group_by,
            groups: self.groups.as_deref(),
        }
    }
}

// The Shops screen (`shop_inventory`, `shop_items`): what each UEX terminal sells, from the
// same UEX files, typed in `starorg_mcp::shops` (the MCP `shop_inventory` tool reads them too).
pub use starorg_mcp::shops::{
    CategoryCount, ItemRecord, ShopCategory, ShopDataState, ShopDirectory, ShopItemMatch,
    ShopItemOffer, ShopItemRow, ShopItemSearch, ShopItems, ShopPlaceGroup, ShopSummary, ShopSystem,
};

/// Every shop. `target`: the channel whose cached item data names the rows and links them to
/// records (`None`, or a channel without that data: UEX names, no links). Nothing is
/// extracted or downloaded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShopInventoryRequest {
    pub target: Option<ChannelRef>,
}

/// One shop's items; `terminal` is a [`ShopSummary::id`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShopItemsRequest {
    pub target: Option<ChannelRef>,
    pub terminal: u64,
}

/// The Shops screen's item search: the items whose game or UEX name matches `query` (fuzzy,
/// best first, capped), and every shop (one offer each) of the item `key` names (a [`ShopItemMatch::key`]),
/// else of the best match. `target` as for [`ShopInventoryRequest`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShopItemSearchRequest {
    pub target: Option<ChannelRef>,
    pub query: String,
    #[serde(default)]
    pub key: Option<String>,
}

// The Commodities screen (`commodity_list`, `commodity_detail`): every tradable resource of
// the build's `game-commodities`, typed in `starorg_mcp::commodities`. The list is summaries
// only; one resource in full comes from a second call.
pub use starorg_mcp::commodities::{
    CargoDamage, CargoItem, CommodityDataState, CommodityDetail, CommodityDirectory, CommodityInfo,
    CommodityMarket, CommodityRef, CommoditySummary, CommodityVolatility, KioskPrice, MarketMeta,
};

/// Every tradable resource of `target`'s build, from its cached game data (nothing is
/// extracted; `not-read` until "Read game data" ran). `None`: `no-channel`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommodityListRequest {
    pub target: Option<ChannelRef>,
}

/// One resource; `id` is a [`CommoditySummary::id`] (`ResourceType.Gold`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommodityDetailRequest {
    pub target: Option<ChannelRef>,
    pub id: String,
}

// The Commodities screen's Mining tab (`mining_list`, `mining_detail`): every mineable
// resource with its places, quality per place, rock physics and the laser estimate, typed in
// `starorg_mcp::mining`. Same cached data and states as the commodity list.
pub use starorg_mcp::mining::{
    CurveSource as MiningCurveSource, Difficulty, ElementPhysics, Grade as MiningGrade, LaserClass,
    MineralDetail, MineralSummary, MiningDirectory, MiningInfo, MiningPlace, OverrideUse,
    PlaceQuality, Recommendation, ShipOption, Tool as MiningTool,
};

/// Every mineral of `target`'s build, from its cached game data. `None`: `no-channel`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MiningListRequest {
    pub target: Option<ChannelRef>,
}

/// One mineral; `id` is a [`MineralSummary::id`], a commodity id or an ore key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MiningDetailRequest {
    pub target: Option<ChannelRef>,
    pub id: String,
}

/// Never prints the token.
impl std::fmt::Debug for UexTokenRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UexTokenRequest")
            .field("token", &self.token.as_ref().map(|_| "[redacted]"))
            .finish()
    }
}

/// A stat line the user can switch off; `key` is what goes into `lines_off`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineSchema {
    pub key: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategorySchema {
    pub id: CategoryId,
    pub label: String,
    pub blurb: String,
    pub lines: Vec<LineSchema>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementSchema {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagKindSchema {
    pub kind: TagKind,
    pub label: String,
    /// Every part this kind can show, in default order.
    pub elements: Vec<ElementSchema>,
}

/// What the Enhancements screen can show: labels for the core's line and element ids, so the
/// UI never invents a switch the generators do not read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileSchema {
    pub categories: Vec<CategorySchema>,
    pub tag_kinds: Vec<TagKindSchema>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopyProfileRequest {
    pub from: ChannelRef,
    pub to: ChannelRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveProfileRequest {
    pub target: ChannelRef,
    pub profile: Profile,
}

// ---------------------------------------------------------------------------------------
// Strings table (spec 9.4 "Strings"): paging, filter and search run in Rust
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StringSource {
    Unmodified,
    Enhanced,
    Patched,
    Edited,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StringSort {
    Key,
    KeyDesc,
    Category,
    Source,
    /// Group order of the active `group_by` (hierarchy order, manufacturer name, size),
    /// then key.
    Group,
    /// Owner name A-Z (case-insensitive), then field, then key; keys without an owner last.
    Owner,
}

/// Which field of its owner a key is (Strings owners).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OwnerField {
    Name,
    ShortName,
    Description,
    Title,
    Career,
    Role,
}

/// What owns a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OwnerKind {
    Ship,
    GroundVehicle,
    Component,
    ShipWeapon,
    FpsWeapon,
    Missile,
    Mission,
    Reputation,
    Manufacturer,
    /// No record: the owner is the text of a sibling key (`item_Name*` for its `item_Desc*`)
    /// or the key's own text.
    Named,
    /// Nobody: `name` is the key made readable (`item desc · COOL_AEGS_S01_Bracer`).
    Key,
}

/// The thing a key belongs to, in words: `Bracer` + `description` (Ship component, Aegis
/// Dynamics). Built from the cached GameData and the stock file; no Developer mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyOwner {
    pub name: String,
    /// `None` for `kind: key`.
    pub field: Option<OwnerField>,
    pub kind: OwnerKind,
    /// Manufacturer display name, when the owner has one.
    pub detail: Option<String>,
}

/// What the Strings group tree shows (addendum 8.4).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GroupBy {
    /// No tree (`StringsPage::groups` is empty).
    #[default]
    None,
    /// The two-level type hierarchy.
    Type,
    Manufacturer,
    Size,
}

/// `exact`: groups come from every record that uses a key. `approximate`: the loc-reference
/// pass has not run for this build yet (a launch hook read it, or a published dump is in
/// use); only GameData links and key prefixes were used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GroupState {
    Exact,
    Approximate,
}

/// One node of the group tree for the active `group_by`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupNode {
    /// Type: a group id (`components/coolers`). Manufacturer: the code, or `none`. Size:
    /// `"1"`..`"12"`, or `none`.
    pub id: String,
    /// Type mode only: the parent group id.
    pub parent: Option<String>,
    pub label: String,
    /// Rows matching every filter except the selection of this dimension (facet rule).
    pub count: u32,
    pub enhanced: u32,
    pub edited: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StringsQuery {
    pub target: ChannelRef,
    /// Matches key, original, result and the owner's name and manufacturer,
    /// case-insensitive.
    pub search: String,
    pub sources: Vec<StringSource>,
    pub categories: Vec<String>,
    pub changed_only: bool,
    pub sort: StringSort,
    pub offset: u32,
    pub limit: u32,
    /// Group ids; a parent selects its children. Empty = all.
    #[serde(default)]
    pub groups: Vec<String>,
    /// Manufacturer codes; `none` = rows without one. Empty = all.
    #[serde(default)]
    pub manufacturers: Vec<String>,
    /// Sizes 1-12; 0 = rows without one. Empty = all.
    #[serde(default)]
    pub sizes: Vec<u32>,
    #[serde(default)]
    pub group_by: GroupBy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StringRow {
    pub key: String,
    pub original: String,
    pub result: String,
    pub source: StringSource,
    pub category: String,
    /// Primary group (a leaf id of the type hierarchy, `components/coolers`).
    pub group: String,
    pub group_label: String,
    /// Manufacturer code (`AEGS`), for keys of an item with a named manufacturer.
    pub manufacturer: Option<String>,
    pub size: Option<u32>,
    /// The primary owner. Always set on rows of a [`StringsPage`] (a `kind: key` fallback
    /// when nothing owns the key); boxed because most table rows have none.
    #[serde(default)]
    pub owner: Option<Box<KeyOwner>>,
    /// Distinct owners of the key (0 = none; above 1 the UI lists them).
    #[serde(default)]
    pub owners: u32,
    /// Backend only: the other owners' names and makers, lower case, for search.
    #[serde(skip)]
    pub other_owners: Option<Box<str>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FacetCount {
    pub id: String,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StringsPage {
    /// Rows matching the filter; `rows` is `offset..offset+limit` of them.
    pub total: u32,
    /// All keys in the table, unfiltered.
    pub all: u32,
    pub offset: u32,
    pub rows: Vec<StringRow>,
    /// `sources`, `categories` and `groups` come with the first page (`offset` 0) only and
    /// are empty on later pages of the same query.
    pub sources: Vec<FacetCount>,
    pub categories: Vec<FacetCount>,
    /// The tree of `group_by` (empty for `none`), in display order.
    pub groups: Vec<GroupNode>,
    pub group_state: GroupState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Layer {
    Stock,
    Generated,
    Patch,
    Override,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub layer: Layer,
    /// Generator, patch id or "overrides.ini".
    pub by: String,
    pub text: String,
    pub winner: bool,
    /// Namesake rule (spec 5.1): the records that competed for the key.
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StringDetail {
    pub row: StringRow,
    pub provenance: Vec<Provenance>,
    pub override_value: Option<String>,
    /// Labels of other groups whose records also use the key (Q-G3).
    pub also_used_by: Vec<String>,
    /// Every owner of the key, the primary one first (empty when nothing owns it).
    #[serde(default)]
    pub owners: Vec<KeyOwner>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyRequest {
    pub target: ChannelRef,
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverrideRequest {
    pub target: ChannelRef,
    pub key: String,
    pub value: String,
}

/// M1 `validate::Rule::name()`, one per rule an override can break (spec 7.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverrideRule {
    ControlCharacter,
    AddedEmUnbalanced,
    EmSignature,
    VariableTokens,
    FormatSpecifiers,
    FormatKey,
    PercentRule,
    TooLong,
    /// Generated text only (an override is never refused for it): a raw `@key`.
    RawLocKey,
    /// Generated text only: "PLACEHOLDER".
    PlaceholderText,
}

impl OverrideRule {
    pub const ALL: [OverrideRule; 10] = [
        OverrideRule::ControlCharacter,
        OverrideRule::AddedEmUnbalanced,
        OverrideRule::EmSignature,
        OverrideRule::VariableTokens,
        OverrideRule::FormatSpecifiers,
        OverrideRule::FormatKey,
        OverrideRule::PercentRule,
        OverrideRule::TooLong,
        OverrideRule::RawLocKey,
        OverrideRule::PlaceholderText,
    ];
}

/// An override that breaks a validation rule is refused with the rule (spec 4.4, 7.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverrideResult {
    pub accepted: bool,
    pub rule: Option<OverrideRule>,
    pub message: Option<String>,
    pub detail: Option<StringDetail>,
}

/// One line of the channel's `overrides.ini`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverrideEntry {
    pub key: String,
    pub value: String,
    pub original: String,
}

// ---------------------------------------------------------------------------------------
// Patches (spec 4.5)
// ---------------------------------------------------------------------------------------

/// M1's `patch::EntryState` names plus `retired`, the per-patch summary M1's
/// `PatchOutcome::state` gives when every entry is `not-needed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PatchState {
    Applies,
    NotNeeded,
    TargetChanged,
    NothingToAdd,
    Missing,
    OutOfRange,
    Conflict,
    Disabled,
    NoData,
    Retired,
    /// Not worked out yet (the list came before the table): `patch_states` has it.
    Checking,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PatchOrigin {
    BuiltIn,
    User,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchView {
    pub id: String,
    pub title: String,
    pub description: String,
    pub keys: u32,
    pub origin: PatchOrigin,
    pub state: PatchState,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchRef {
    pub target: ChannelRef,
    pub id: String,
}

/// One key a patch touches, as stock and as patched text (spec 9.4 Patches detail).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchExample {
    pub key: String,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchDetail {
    pub patch: PatchView,
    /// `loc` (output string) or `data` (a GameData field), spec 4.5.
    pub kind: String,
    /// The build range the patch is written for, as text ("4.9.0 - 4.10.x"), or null when open.
    pub range: Option<String>,
    /// Why the state is what it is, in words.
    pub reason: String,
    pub examples: Vec<PatchExample>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchToggle {
    pub target: ChannelRef,
    pub id: String,
    pub enabled: bool,
}

/// Loc ops a user patch made in the app can use (spec 4.5; `append_from` needs a file).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LocOp {
    Set,
    Append,
    Prepend,
    Replace,
}

/// "Add patch": a one-entry loc patch written to `data/patches/<id>.patch.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewPatch {
    pub target: ChannelRef,
    pub title: String,
    pub description: String,
    pub key: String,
    pub op: LocOp,
    /// Only for `replace`: the exact text to swap (must match once).
    pub find: Option<String>,
    pub value: String,
}

/// A user patch as its TOML file. The real backend also offers a save dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchExport {
    pub file_name: String,
    pub text: String,
}

// ---------------------------------------------------------------------------------------
// Activity
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    pub id: u64,
    pub time: String,
    pub level: LogLevel,
    pub job: Option<JobKind>,
    pub channel: Option<String>,
    pub message: String,
}

// ---------------------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    System,
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Accent {
    Cyan,
    Green,
    Amber,
    Orange,
    Rose,
    Violet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Palette {
    Void,
    Black,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Appearance {
    pub theme: Theme,
    pub accent: Accent,
    pub palette: Palette,
    pub reduce_effects: bool,
}

/// All off by default (spec 11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkSettings {
    pub check_updates: bool,
    /// "Download published LIVE builds for Diff": Developer mode's Diff may download past
    /// LIVE builds from voidput/sc-gamedata-dumps (addendum §1). Never used by apply.
    pub published_dumps: bool,
    pub starorg: bool,
    /// "Download UEX market data": "Fetch now" may download the two UEX files WHERE TO GET's
    /// shop line reads from api.uexcorp.space. Missing in older saved settings = off.
    #[serde(default)]
    pub uex: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub appearance: Appearance,
    pub network: NetworkSettings,
    pub default_channel: Option<String>,
    pub safe_gpu: bool,
    /// Missing in older saved settings = all off.
    #[serde(default)]
    pub advanced: AdvancedSettings,
    /// Settings -> StarOrg Tools: environment and "What to sync". Stored in `config.toml`
    /// (`[sync]`), so the CLI and the launch hooks follow the same switches.
    #[serde(default)]
    pub starorg: StarOrgSettings,
}

/// StarOrg Tools environment and what to sync (addendum §5). Defaults: production, Game.log
/// import on (as before), everything that talks to StarOrg Tools off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StarOrgSettings {
    pub environment: StarOrgEnv,
    /// Read Game.log for new blueprints (app, CLI, post-exit hook). Off: no log is read.
    pub import_game_log: bool,
    /// Push owned blueprints of the ticked characters to StarOrg Tools.
    pub push_blueprints: bool,
    /// Pull owned blueprints from StarOrg Tools into a character.
    pub pull_blueprints: bool,
    /// Pull the StarOrg Tools hangar into the fleet (Ships).
    pub pull_fleet: bool,
    /// Sync by itself after the game exits; off = only when asked.
    pub auto_sync: bool,
}

impl Default for StarOrgSettings {
    fn default() -> Self {
        Self {
            environment: StarOrgEnv::Production,
            import_game_log: true,
            push_blueprints: false,
            pull_blueprints: false,
            pull_fleet: false,
            auto_sync: false,
        }
    }
}

/// Settings > Advanced (addendum 7.7).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvancedSettings {
    /// Explorer, Datasets and Diff screens. Off by default.
    #[serde(default)]
    pub developer_mode: bool,
}

// ---------------------------------------------------------------------------------------
// Characters and owned blueprints (addendum §3)
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CharacterMode {
    /// The character of the newest session in this channel's logs.
    Auto,
    Manual,
    /// Any of my characters.
    Union,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterView {
    /// geid: the stable key. Personal data: shown, never uploaded, redacted in diagnostics.
    pub id: String,
    pub name: String,
    pub last_seen: Option<String>,
    pub owned: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveCharacter {
    pub mode: CharacterMode,
    /// Manual pick (mode = manual).
    pub character: Option<String>,
    /// Who `auto` resolves to right now: the newest session's character, or, when that one
    /// owns nothing and another does, the most recently seen one that owns something.
    pub resolved: Option<String>,
    /// `auto` passed over the newest character (it owns nothing): its geid, for a hint.
    #[serde(default)]
    pub fallback_from: Option<String>,
    /// `auto`: another character (not merged with the one used) owns far more; "Any of my
    /// characters" may be what the user wants. Its geid, for a hint.
    #[serde(default)]
    pub larger: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharactersView {
    pub channel: String,
    pub characters: Vec<CharacterView>,
    pub active: ActiveCharacter,
    /// Blueprints seen before any login line in their log file.
    pub unattributed: u32,
    pub last_scan: Option<String>,
    /// Characters of different accounts share a name: kept apart (not merged), so the
    /// user picks. Characters of one account with the same name (a reset) are merged.
    #[serde(default)]
    pub notice: Option<String>,
    /// File name of the last Smart Citizen export imported on this channel (a re-import
    /// reads it again without the picker).
    #[serde(default)]
    pub last_import: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetActiveCharacter {
    pub target: ChannelRef,
    pub mode: CharacterMode,
    pub character: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedBlueprint {
    pub name: String,
    pub ambiguous: bool,
    pub favorite: bool,
    /// `log`, `import:smart-citizen`, `starorg:<guild>`.
    pub sources: Vec<String>,
    pub acquired: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterRequest {
    pub target: ChannelRef,
    pub character: String,
}

// ---------------------------------------------------------------------------------------
// Blueprint finder (addendum §7.19): the build's catalogue with every channel's owners
// ---------------------------------------------------------------------------------------

/// What a blueprint makes (record path and category).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BlueprintKind {
    ShipComponent,
    ShipWeapon,
    FpsWeapon,
    Armor,
    FpsAmmo,
    MissionItem,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlueprintFinderRequest {
    pub install: String,
    /// Channels to merge; empty = every channel of the install.
    pub channels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlueprintFinderChannel {
    pub channel: String,
    /// Blueprints in this channel's build; `None` = game data not read yet (apply reads it).
    pub catalog: Option<u32>,
    pub characters: u32,
    /// Owned entries over every character of the channel (and the unattributed set).
    pub owned: u32,
    /// Set files that could not be read or placed: not shown (logged).
    pub skipped: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlueprintFinderCharacter {
    /// geid (personal data: shown, never uploaded), or `unattributed` for blueprints seen
    /// before any login line.
    pub id: String,
    pub name: String,
    pub last_seen: Option<String>,
    /// Channels whose logs know the character (same geid = same character, own sets).
    pub channels: Vec<String>,
}

/// One character's ownership of a blueprint on one channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlueprintOwner {
    pub channel: String,
    pub character: String,
    /// `log`, `import:smart-citizen`, `starorg:<guild>`.
    pub sources: Vec<String>,
    pub acquired: Option<String>,
    pub favorite: bool,
    /// The name could be several blueprints (an import that matched several, or a name
    /// several blueprints have): listed, never marked.
    pub ambiguous: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlueprintRow {
    /// Blueprint id (lower case), or `name:<normalised name>` for an owned name that matched
    /// no single blueprint of the read builds.
    pub key: String,
    pub name: String,
    /// Record short name (`BP_CRAFT_…`); `None` for a name-only row.
    pub record: Option<String>,
    pub kind: BlueprintKind,
    pub type_label: Option<String>,
    /// Armor weight: Light, Medium, Heavy.
    pub class: Option<String>,
    pub manufacturer: Option<String>,
    pub manufacturer_name: Option<String>,
    pub size: Option<u32>,
    /// A..D (ship components).
    pub grade: Option<String>,
    /// Channels whose build has the blueprint (name rows: their candidates' channels).
    pub channels: Vec<String>,
    /// Name rows: keys of the blueprints the name could be; empty = in no read build.
    pub candidates: Vec<String>,
    pub owners: Vec<BlueprintOwner>,
    /// Owners of an ambiguous name that could be this blueprint ("maybe owned").
    pub maybe: Vec<BlueprintOwner>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlueprintFinder {
    pub channels: Vec<BlueprintFinderChannel>,
    pub characters: Vec<BlueprintFinderCharacter>,
    /// Sorted by name; the app sorts, filters and counts facets.
    pub rows: Vec<BlueprintRow>,
}

// ---------------------------------------------------------------------------------------
// Smart Citizen import (addendum §4)
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportRequest {
    pub target: ChannelRef,
    pub character: String,
    pub dry_run: bool,
    /// Read the last export imported on this channel (or the file the dry run just read)
    /// instead of opening the picker.
    #[serde(default)]
    pub reuse_last: bool,
    /// Re-import: drop what an earlier Smart Citizen import added and this export no
    /// longer has. Entries from the logs or StarOrg Tools are never removed.
    #[serde(default)]
    pub replace: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmbiguousName {
    pub name: String,
    pub candidates: Vec<String>,
}

/// Same fields as M1's `blueprints::import::ImportReport`, plus the file shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportReport {
    pub file: String,
    pub exported_at: Option<String>,
    pub dry_run: bool,
    pub total: u32,
    pub not_completed: u32,
    pub duplicates: u32,
    pub imported: Vec<String>,
    pub known: Vec<String>,
    pub unmatched: Vec<String>,
    pub ambiguous: Vec<AmbiguousName>,
    /// Game data was not read for this channel, so nothing could match (M1).
    pub empty_catalog: bool,
    /// `replace`: entries of an earlier import this export no longer has.
    #[serde(default)]
    pub removed: Vec<String>,
}

/// Re-read every log of a channel from the start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RescanRequest {
    pub target: ChannelRef,
    pub dry_run: bool,
    /// Start every set empty: only what the logs show is kept (imported and StarOrg entries
    /// go too).
    #[serde(default)]
    pub replace: bool,
}

/// One character's change in a rescan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterChange {
    /// geid; `None` for blueprints seen before any login.
    pub id: Option<String>,
    pub name: String,
    pub before: u32,
    pub after: u32,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RescanReport {
    pub channel: String,
    pub dry_run: bool,
    pub replace: bool,
    pub files_read: u32,
    /// Characters merged into another (same account and name, new geid).
    pub merged: u32,
    /// A log skipped (too large) or not readable, and similar: shown with the report.
    #[serde(default)]
    pub warnings: Vec<String>,
    /// `replace` with a log that could not be read: the confirm is refused with this reason.
    #[serde(default)]
    pub replace_refused: Option<String>,
    pub characters: Vec<CharacterChange>,
}

// ---------------------------------------------------------------------------------------
// Fleet import and ship names (addendum §6)
// ---------------------------------------------------------------------------------------

/// `ship-names.toml` `style`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NameStyle {
    None,
    Numeric,
    GroupLetter,
    Symbol,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RestMode {
    Stock,
    Numbered,
}

/// `profile.toml` `[ship_names] use`: the shared list, this channel's own, or off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShipNamesScope {
    Shared,
    Channel,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FleetFormat {
    XplorJson,
    XplorCsv,
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShipGroup {
    pub id: String,
    pub name: String,
}

/// One `[[ship]]`: file order is the order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShipEntry {
    pub hull: String,
    pub group: Option<String>,
    /// Rename of the long name.
    pub name: Option<String>,
    /// Rename of the short name; defaults to `name`.
    pub short: Option<String>,
}

/// Same fields as the core's `fleet::store::ShipNames` (minus `schema` and unknown keys).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShipNamesSettings {
    pub enabled: bool,
    pub style: NameStyle,
    /// `" "`, `"-"` or `". "`.
    pub separator: String,
    /// One of `! # * + - .`.
    pub symbol: String,
    pub rest: RestMode,
    pub prefix_short: bool,
    pub groups: Vec<ShipGroup>,
    pub ships: Vec<ShipEntry>,
}

/// A player-flyable hull of the channel's build.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FleetHull {
    /// Hull id = the long name key (`vehicle_NameAEGS_Gladius`).
    pub hull: String,
    /// Stock long name.
    pub name: String,
    /// Stock short name, when the build has the `_short` key.
    pub short: Option<String>,
    /// Game maker code (`aegs`).
    pub maker: Option<String>,
    pub ground: bool,
    /// Other records (AI, PU, unmanned variants) that show the same name.
    pub npc_shared: u32,
    /// Cargo grid SCU of the stock loadout; `None` when the game data has none or the
    /// records disagree.
    pub cargo_scu: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedHull {
    pub hull: String,
    pub count: u32,
    /// Custom names from the export: rename suggestions.
    pub names: Vec<String>,
}

/// An import row that matched no hull, or several (`candidates`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingShip {
    pub name: String,
    pub count: u32,
    pub maker: Option<String>,
    pub names: Vec<String>,
    pub candidates: Vec<String>,
}

/// User overrides on a hull's name keys: they win with their whole value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HullOverride {
    pub hull: String,
    pub long: Option<String>,
    pub short: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetViewRequest {
    pub target: ChannelRef,
    /// Read the game data when it is not cached for this build (slow; a job).
    pub read_game_data: bool,
}

/// Everything the Ships screen needs for one channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FleetView {
    pub channel: String,
    /// The build's game data has been read: `catalog` is the real list.
    pub catalog_ready: bool,
    /// Player-flyable hulls, sorted by stock name.
    pub catalog: Vec<FleetHull>,
    pub owned: Vec<OwnedHull>,
    pub pending: Vec<PendingShip>,
    /// Row names marked "Not in the game".
    pub not_in_game: Vec<String>,
    pub imported_at: Option<String>,
    pub format: Option<FleetFormat>,
    pub scope: ShipNamesScope,
    /// The settings in use for this channel (for `off`: the shared ones, not written).
    pub settings: ShipNamesSettings,
    /// The shared `ship-names.toml` (what "All channels" switches back to).
    pub shared_settings: ShipNamesSettings,
    pub overrides: Vec<HullOverride>,
    pub warnings: Vec<String>,
    /// The settings table could not be used: nothing is written until it is fixed or saved.
    pub invalid: bool,
    /// Display only.
    pub fleet_file: String,
    pub settings_file: String,
}

/// Where the import reads from. The real backend opens the file picker for `file` and keeps
/// the file in memory, so the confirm step can use `last` without asking again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum FleetSource {
    File,
    Text {
        text: String,
    },
    Last,
    /// The StarOrg Tools hangar (`GET /api/fleet/hangar`); needs "Pull fleet/hangar" on and
    /// a sign-in.
    StarOrg,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetImportRequest {
    pub target: ChannelRef,
    pub source: FleetSource,
    pub dry_run: bool,
    /// Add to the fleet instead of replacing it.
    pub merge: bool,
    /// "Remove all ships first": replace the fleet and drop the ships of the shared My
    /// ships list that the import does not have. Ships that stay keep their group, rename
    /// and place. Not with `merge`.
    #[serde(default)]
    pub prune: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedHull {
    pub hull: String,
    pub name: String,
    pub count: u32,
    pub names: Vec<String>,
    /// Not in the fleet before.
    pub new: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetImportReport {
    /// File name (no folder), `null` for pasted text.
    pub file: Option<String>,
    pub format: FleetFormat,
    pub dry_run: bool,
    pub merge: bool,
    pub total: u32,
    /// Rows that are not ships.
    pub skipped: u32,
    pub hulls: Vec<ImportedHull>,
    pub unmatched: Vec<PendingShip>,
    pub ambiguous: Vec<PendingShip>,
    pub not_in_game: u32,
    pub empty_catalog: bool,
    /// Hulls appended to the shared order list (0 for a dry run).
    pub added_to_order: u32,
    pub prune: bool,
    /// With `prune`: ships dropped from the shared order list (would be, on a dry run).
    pub removed_from_order: Vec<RemovedShip>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemovedShip {
    pub hull: String,
    /// Catalogue name; the hull id when the build does not have it.
    pub name: String,
    /// Its rename in My ships, lost with it.
    pub rename: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetMatchRequest {
    pub target: ChannelRef,
    /// The pending row's name.
    pub name: String,
    /// A hull id, or `null` for "Not in the game".
    pub hull: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveShipNamesRequest {
    pub target: ChannelRef,
    pub scope: ShipNamesScope,
    /// Written to the shared file (`shared`) or the channel's profile (`channel`); ignored
    /// for `off`.
    pub settings: ShipNamesSettings,
}

// ---------------------------------------------------------------------------------------
// StarOrg Tools account (addendum §5, Q22 = push the union of ticked characters)
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StarOrgEnv {
    Production,
    Dev,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncCharacter {
    pub id: String,
    pub name: String,
    pub ticked: bool,
    pub owned: u32,
}

/// One org from the account card (`/api/auth/me` `guilds[]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StarOrgGuild {
    /// Discord id of the guild (the blueprint routes' `{guild_id}`).
    pub id: String,
    pub name: String,
    /// The org has StarOrg's industrial feature (blueprint tracking).
    pub blueprints: bool,
}

/// Where the sign-in token is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TokenStoreKind {
    /// Windows Credential Manager / Secret Service.
    Keyring,
    /// An owner-only file in the app's config folder (no keyring available).
    File,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StarOrgStatus {
    /// The network switch (Settings -> Network). Off: login is refused.
    pub enabled: bool,
    pub connected: bool,
    pub environment: StarOrgEnv,
    /// `starorg.tools` or `dev.starorg.tools`.
    pub host: String,
    pub user: Option<String>,
    /// Name of the org the blueprint routes go through.
    pub guild: Option<String>,
    pub guild_id: Option<String>,
    /// The account's orgs (empty until signed in).
    pub guilds: Vec<StarOrgGuild>,
    pub expires: Option<String>,
    /// Signed in before, but the token expired or was revoked: sign in again.
    pub expired: bool,
    pub token_store: Option<TokenStoreKind>,
    pub last_sync: Option<String>,
    /// Characters whose owned sets are pushed as one union. Pull writes into one character.
    pub sync_characters: Vec<SyncCharacter>,
    /// Size of the union that a push would send.
    pub push_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StarOrgGuildRequest {
    pub channel: String,
    /// A `StarOrgGuild::id` of the signed-in account.
    pub guild: String,
}

/// "Sync now" and the automatic run after the game exits: what the switches allow, in order
/// pull, push, fleet. A step that fails is listed in `skipped`; the others still run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncRun {
    pub channel: String,
    pub reports: Vec<SyncReport>,
    pub fleet: Option<FleetImportReport>,
    /// Steps not run, with the reason.
    pub skipped: Vec<String>,
    pub time: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncCharactersRequest {
    pub channel: String,
    pub ticked: Vec<String>,
}

/// Pull writes into one chosen character (union, never delete; addendum §5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StarOrgPullRequest {
    pub channel: String,
    pub character: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SyncDirection {
    Pull,
    Push,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncReport {
    pub direction: SyncDirection,
    pub channel: String,
    pub guild: Option<String>,
    /// Pull: the character written into. Push: null (the union of ticked characters).
    pub character: Option<String>,
    /// Pull: names added locally. Push: names the server did not have yet.
    pub added: Vec<String>,
    pub already: u32,
    /// Pull: StarOrg entries with no blueprint record in this build's game data. Push: local
    /// names StarOrg Tools has no single blueprint for (unknown or ambiguous; not sent).
    pub unmatched: Vec<String>,
    /// Push: size of the union sent.
    pub sent: u32,
    pub time: String,
}

// ---------------------------------------------------------------------------------------
// Launch hooks (addendum §2) and doctor (spec 10.5)
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HookKind {
    Prelaunch,
    Postexit,
}

/// M1's `hooks::BlockState`, plus `missing` when the script itself is not there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum HookState {
    Absent,
    Current,
    Outdated,
    Broken { why: String },
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookScript {
    pub kind: HookKind,
    pub path: Option<String>,
    pub is_link: bool,
    pub status: HookState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HooksStatus {
    /// LUG Helper hooks exist on Linux only; Windows shows "Check now" instead.
    pub supported: bool,
    pub cli_on_path: bool,
    pub scripts: Vec<HookScript>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CheckLevel {
    Ok,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorCheck {
    pub level: CheckLevel,
    pub label: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorSection {
    pub title: String,
    pub checks: Vec<DoctorCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorReport {
    pub sections: Vec<DoctorSection>,
}

/// Doctor and recent log as one text, with paths, user names and character ids replaced
/// (spec 9.4 Activity, addendum §3 privacy). The UI shows it before saving or copying.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostics {
    pub file_name: String,
    pub text: String,
    /// How many values were replaced.
    pub redacted: u32,
}

// ---------------------------------------------------------------------------------------
// Updates (spec 11, 12; docs/updates.md). Only used when `network.check_updates` is on.
// ---------------------------------------------------------------------------------------

/// Where releases are looked up, and whether pre-releases count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdatePrefs {
    /// `owner/name` on github.com. Default `voidput/starorg-codex`; a public mirror works.
    pub repo: String,
    pub include_prereleases: bool,
}

/// How this copy of the app can be updated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateInstall {
    /// AppImage, NSIS or MSI with a real updater key: download, verify the signature, install.
    InApp,
    /// deb, rpm, portable or a build without a key: the app only links to the release.
    DownloadOnly,
    /// The Microsoft Store build: the Store updates it. No release check, no install.
    Store,
}

/// Why a check did not give an answer. Not alarming: the app keeps working either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateProblem {
    /// HTTP 404 / 401 / 403 without a rate limit: the repo is private, renamed or has no
    /// public releases (spec Q1).
    NotVisible,
    /// GitHub's limit for requests without sign-in.
    RateLimited,
    /// No HTTP answer at all.
    Offline,
    /// An answer we could not use (5xx, not JSON, too large).
    BadResponse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NoteKind {
    Heading,
    Item,
    Text,
}

/// One block of release notes, as plain text: Markdown markers, HTML, links and images are
/// removed in Rust, and the UI shows `text` as text (never as HTML).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteBlock {
    pub kind: NoteKind,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseView {
    /// Semver without the `v`.
    pub version: String,
    pub tag: String,
    pub title: String,
    /// `https://github.com/<repo>/releases/tag/<tag>`, built by the app (never taken from the
    /// response).
    pub url: String,
    pub published: Option<String>,
    pub prerelease: bool,
    pub notes: Vec<NoteBlock>,
    /// Notes were cut to the first blocks; the release page has the rest.
    pub notes_truncated: bool,
}

/// Result of the last check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum UpdateCheck {
    NotChecked,
    UpToDate,
    Available {
        release: ReleaseView,
    },
    Failed {
        problem: UpdateProblem,
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateStatus {
    /// `network.check_updates`. Off: nothing is requested, the last result is kept.
    pub enabled: bool,
    pub current_version: String,
    pub prefs: UpdatePrefs,
    pub last_checked: Option<String>,
    pub check: UpdateCheck,
    /// "Skip this version": no banner for exactly this version (a newer one shows again).
    pub skipped_version: Option<String>,
    pub install: UpdateInstall,
    /// Why `install` is what it is, in one sentence for Settings.
    pub install_note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateCheckRequest {
    /// `false`: the start-up check, at most once per 24 h. `true`: "Check now".
    pub force: bool,
}

/// A link the webview may ask to open. An allowlist: the webview never sends a URL, so it
/// cannot open anything else (it has no opener/shell permission of its own).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExternalLink {
    /// The Discord server.
    Support,
    /// Buy me a coffee.
    Donate,
    /// StarOrg Tools on Patreon.
    Patreon,
    /// UEX Corp, where a UEX API token is made (My Apps).
    Uex,
}

impl ExternalLink {
    pub const fn url(self) -> &'static str {
        match self {
            Self::Support => "https://discord.gg/GCkYhbFYNq",
            Self::Donate => "https://buymeacoffee.com/zedwil",
            Self::Patreon => "https://www.patreon.com/cw/StarOrgTools",
            Self::Uex => "https://uexcorp.space",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Support => "Support",
            Self::Donate => "Buy me a coffee",
            Self::Patreon => "Patreon",
            Self::Uex => "UEX Corp",
        }
    }
}

// ---------------------------------------------------------------------------------------
// Experimental: local MCP server (docs/mcp.md)
// ---------------------------------------------------------------------------------------

/// One HTTP request the local MCP server answered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpRequest {
    pub time: String,
    /// JSON-RPC method (`initialize`, `tools/call`, ...), when the body had one.
    pub method: Option<String>,
    /// `tools/call`: the tool.
    pub tool: Option<String>,
    /// HTTP status (401 = no or wrong token).
    pub status: u16,
    pub ok: bool,
    pub millis: u64,
}

/// Settings -> Experimental -> "Local MCP server". Off at every start of the app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpStatus {
    pub running: bool,
    /// `http://127.0.0.1:<port>/mcp` while running.
    pub url: Option<String>,
    /// Bearer token of this run (new at every start). Shown to the user, never logged.
    pub token: Option<String>,
    pub started: Option<String>,
    /// Why the last start failed.
    pub error: Option<String>,
    /// Channels the tools answer for.
    pub channels: Vec<String>,
    /// `claude mcp add ...` for Claude Code, while running.
    pub claude_code: Option<String>,
    /// `claude_desktop_config.json` entry (through `mcp-remote`), while running.
    pub claude_desktop: Option<String>,
    /// The same tools over stdio from the CLI: `starorg-codex mcp`.
    pub stdio: String,
    /// Newest first, at most 100.
    pub requests: Vec<McpRequest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpToggle {
    pub enabled: bool,
}

// ---------------------------------------------------------------------------------------
// Memory limit of the Developer mode and MCP workers (addendum 7.18)
// ---------------------------------------------------------------------------------------

/// How the cap is held on this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MemoryEnforcement {
    /// Linux with `STARORG_MEMORY_CGROUP=1`: cgroup v2 `memory.max` (a `systemd-run --user
    /// --scope` per worker), above the 90% watchdog.
    Cgroup,
    /// Linux (default): `RLIMIT_DATA` in each worker, above the 90% watchdog.
    Rlimit,
    /// Windows: each worker stops itself at 90% of the limit (no hard cap).
    Watchdog,
}

/// One measured peak and whether it fits under the limit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryEstimate {
    pub id: String,
    pub label: String,
    pub bytes: u64,
    pub fits: bool,
}

/// Settings > Advanced > Memory limit, and the numbers of the warning shown before
/// Developer mode or the MCP server is turned on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryStatus {
    /// This machine's RAM (`None` when it cannot be read).
    pub total_bytes: Option<u64>,
    /// Free now (Linux `MemAvailable`, Windows available physical memory).
    pub available_bytes: Option<u64>,
    /// The limit in GB: the setting, else `default_gb`.
    pub limit_gb: f64,
    /// No setting saved: the default for this machine applies.
    pub is_default: bool,
    pub min_gb: f64,
    pub max_gb: f64,
    pub step_gb: f64,
    pub default_gb: f64,
    pub enforcement: MemoryEnforcement,
    /// A worker is stopped at this share of the limit (percent); `fits` is measured against
    /// it, not the limit itself.
    pub stop_percent: u32,
    /// Smallest first.
    pub estimates: Vec<MemoryEstimate>,
    /// Set on machines with too little RAM for a comfortable limit.
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryLimitRequest {
    /// GB, rounded to the step and kept inside the slider; `None` = back to the default.
    pub gb: Option<f64>,
}

impl MemoryEnforcement {
    /// What the workers on this machine get.
    pub fn here() -> Self {
        if cfg!(windows) {
            MemoryEnforcement::Watchdog
        } else if starorg_core::memlimit::systemd_run().is_some() {
            MemoryEnforcement::Cgroup
        } else {
            MemoryEnforcement::Rlimit
        }
    }
}

impl MemoryStatus {
    /// The view for a saved limit (`configured`, GB) on a machine with `info`.
    pub fn build(
        configured: Option<f64>,
        info: starorg_core::memlimit::MemInfo,
        enforcement: MemoryEnforcement,
    ) -> Self {
        use starorg_core::memlimit as m;
        let b = m::bounds(info.total);
        let limit_gb = configured.unwrap_or(b.default_gb);
        let limit = m::gb_bytes(limit_gb);
        MemoryStatus {
            total_bytes: info.total,
            available_bytes: info.available,
            limit_gb,
            is_default: configured.is_none(),
            min_gb: b.min_gb,
            max_gb: b.max_gb,
            step_gb: b.step_gb,
            default_gb: b.default_gb,
            enforcement,
            stop_percent: m::STOP_PERCENT as u32,
            estimates: m::estimates()
                .into_iter()
                .map(|e| MemoryEstimate {
                    id: e.id.into(),
                    label: e.label.into(),
                    bytes: e.bytes,
                    fits: m::fits(e.bytes, limit),
                })
                .collect(),
            note: m::low_ram_note(info.total),
        }
    }
}

/// One area of what the app keeps on this PC (Settings > Storage).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StorageAreaId {
    /// Extracted game text and data per channel (`<cache>/<install>/<CHANNEL>/`).
    GameData,
    /// WHERE TO GET / MCP datasets (`<cache>/mcp/<CHANNEL>/`).
    WhereToGet,
    /// UEX market data (`<cache>/mcp/uex/`).
    Uex,
    /// Developer mode asset exports (the asset cache folder).
    AssetExports,
    /// Extraction temp folders (`<cache>/work/`).
    Temporary,
    /// Live telemetry (experimental): the schema read from each build's `StarCitizen.exe`
    /// (`<cache>/telemetry/`).
    Telemetry,
    /// Live telemetry (experimental): recorded sessions (`<data>/telemetry/recordings/`; not
    /// in the cache, bounded by their writer).
    TelemetryRecordings,
    /// Live telemetry (experimental): the server and shard join history from Game.log
    /// (`<data>/telemetry/sessions/`; not in the cache, bounded by its writer).
    TelemetrySessions,
    /// `global.ini` backups (app data folder; not in the cache, not under its cap).
    Backups,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageArea {
    pub id: StorageAreaId,
    pub label: String,
    /// The folder shown for it.
    pub path: String,
    pub bytes: u64,
    pub files: u64,
    /// Counted toward the cache folder's cap.
    pub in_cache: bool,
}

/// Settings > Storage: what the app keeps on this PC and the cache folder's cap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageStatus {
    /// The app's cache folder.
    pub cache_path: String,
    /// Everything in it now.
    pub cache_bytes: u64,
    /// Its cap (`[storage] cache_max_mb`, default 1 GiB): least recently used goes first.
    pub cap_bytes: u64,
    /// Cache folder + backups (+ the asset cache when it is in a folder of its own).
    pub total_bytes: u64,
    /// In a fixed order: game data, WHERE TO GET, UEX, asset exports, temporary, live
    /// telemetry schemas, live telemetry recordings, live telemetry session history, backups.
    pub areas: Vec<StorageArea>,
    /// The asset cache's own cap.
    pub asset_cap_bytes: u64,
    /// Backups kept per channel (plus the newest of each of the last `backup_keep_builds`
    /// game builds and the one from before the app).
    pub backup_keep_newest: u32,
    pub backup_keep_builds: u32,
    /// Why "Clear asset cache" is refused right now (an asset job is running), else `None`.
    pub asset_busy: Option<String>,
}

impl StorageStatus {
    /// From a measured report (`starorg_core::storage`).
    pub fn build(
        r: &starorg_core::storage::Report,
        cap: u64,
        asset_cap: u64,
        assets_inside_cache: bool,
        asset_busy: Option<String>,
    ) -> Self {
        use starorg_core::storage::Area;
        let areas = r
            .areas
            .iter()
            .map(|a| {
                let (id, label) = match a.area {
                    Area::GameData => (StorageAreaId::GameData, "Game data cache"),
                    Area::WhereToGet => (StorageAreaId::WhereToGet, "WHERE TO GET datasets"),
                    Area::Uex => (StorageAreaId::Uex, "UEX market data"),
                    Area::AssetExports => (StorageAreaId::AssetExports, "Asset exports"),
                    Area::Temporary => (StorageAreaId::Temporary, "Temporary extraction folders"),
                    Area::Telemetry => (StorageAreaId::Telemetry, "Live telemetry schemas"),
                    Area::TelemetryRecordings => (
                        StorageAreaId::TelemetryRecordings,
                        "Live telemetry recordings",
                    ),
                    Area::TelemetrySessions => (
                        StorageAreaId::TelemetrySessions,
                        "Live telemetry session history",
                    ),
                    Area::Backups => (StorageAreaId::Backups, "global.ini backups"),
                };
                StorageArea {
                    id,
                    label: label.into(),
                    path: a.path.display().to_string(),
                    bytes: a.usage.bytes,
                    files: a.usage.files,
                    in_cache: match a.area {
                        Area::Backups | Area::TelemetryRecordings | Area::TelemetrySessions => {
                            false
                        }
                        Area::AssetExports => assets_inside_cache,
                        _ => true,
                    },
                }
            })
            .collect();
        StorageStatus {
            cache_path: r.cache_path.display().to_string(),
            cache_bytes: r.cache.bytes,
            cap_bytes: cap,
            total_bytes: r.total(assets_inside_cache).bytes,
            areas,
            asset_cap_bytes: asset_cap,
            backup_keep_newest: starorg_core::backup::KEEP_NEWEST as u32,
            backup_keep_builds: starorg_core::backup::KEEP_BUILDS as u32,
            asset_busy,
        }
    }
}

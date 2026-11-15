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
    Internal,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DataSource {
    /// Read Data.p4k on this machine (default, no network).
    Local,
    /// Published dumps when one exists for the exact build (network opt-in, addendum §1).
    PublishedDumps,
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
    pub data_source: DataSource,
    pub preset: Preset,
    pub fix_known_bugs: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupResult {
    pub install: String,
    pub set_up: Vec<String>,
    pub skipped: Vec<String>,
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

/// The `apply` JSON entry of spec 8, plus what the review step shows.
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
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyRequest {
    pub target: ChannelRef,
    pub dry_run: bool,
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
    /// Restore the file from before StarOrg Strings (pinned, origin foreign).
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
    pub keep_newest: u32,
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

// ---------------------------------------------------------------------------------------
// Enhancements profile (spec 4.7, 5, 6)
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CategoryId {
    Ships,
    Components,
    ShipWeapons,
    Missiles,
    FpsWeapons,
    NameTags,
    Missions,
    Blueprints,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatLine {
    pub id: String,
    pub label: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategorySettings {
    pub id: CategoryId,
    pub label: String,
    pub enabled: bool,
    pub lines: Vec<StatLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StatsPlacement {
    BelowProse,
    AboveProse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormatOptions {
    pub placement: StatsPlacement,
    pub header: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TagStyle {
    Short,
    Medium,
    Long,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagElement {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    pub style: TagStyle,
}

/// Tag builder settings for one tag kind (spec 5.1 "Name tags"). `elements` is in output order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagSettings {
    pub kind: String,
    pub label: String,
    pub elements: Vec<TagElement>,
    pub separator: TagSeparator,
    pub enclosing: TagEnclosing,
    pub placement: TagPlacement,
    pub short_name_too: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissionOptions {
    pub tag_bp: bool,
    pub tag_bp_maybe: bool,
    pub tag_rep: bool,
    /// Wrap title tags in `<EM4>` (M2 in-game check decides the default).
    pub em4_title_tags: bool,
    pub body_sections: Vec<StatLine>,
    pub mark_owned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub preset: Option<Preset>,
    pub categories: Vec<CategorySettings>,
    pub format: FormatOptions,
    pub tags: Vec<TagSettings>,
    pub missions: MissionOptions,
    pub branding: bool,
    pub fix_known_bugs: bool,
    pub patches_disabled: Vec<String>,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StringsQuery {
    pub target: ChannelRef,
    /// Matches key, original and result, case-insensitive.
    pub search: String,
    pub sources: Vec<StringSource>,
    pub categories: Vec<String>,
    pub changed_only: bool,
    pub sort: StringSort,
    pub offset: u32,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StringRow {
    pub key: String,
    pub original: String,
    pub result: String,
    pub source: StringSource,
    pub category: String,
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
    pub sources: Vec<FacetCount>,
    pub categories: Vec<FacetCount>,
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

/// An override that breaks a validation rule is refused with the rule (spec 4.4, 7.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverrideResult {
    pub accepted: bool,
    pub rule: Option<String>,
    pub message: Option<String>,
    pub detail: Option<StringDetail>,
}

// ---------------------------------------------------------------------------------------
// Patches (spec 4.5)
// ---------------------------------------------------------------------------------------

/// Same names as M1's `patch::EntryState` plus `retired` (all entries not needed).
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
    Retired,
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
pub struct PatchToggle {
    pub target: ChannelRef,
    pub id: String,
    pub enabled: bool,
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
    pub published_dumps: bool,
    pub starorg: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub appearance: Appearance,
    pub network: NetworkSettings,
    pub default_channel: Option<String>,
    pub safe_gpu: bool,
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
    /// Who `auto` resolves to right now.
    pub resolved: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharactersView {
    pub channel: String,
    pub characters: Vec<CharacterView>,
    pub active: ActiveCharacter,
    /// Blueprints seen before any login line in their log file.
    pub unattributed: u32,
    pub last_scan: Option<String>,
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
// Smart Citizen import (addendum §4)
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportRequest {
    pub target: ChannelRef,
    pub character: String,
    pub dry_run: bool,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StarOrgStatus {
    /// The network switch (Settings -> Network). Off: login is refused.
    pub enabled: bool,
    pub connected: bool,
    pub environment: StarOrgEnv,
    pub user: Option<String>,
    pub guild: Option<String>,
    pub expires: Option<String>,
    pub last_sync: Option<String>,
    /// Characters whose owned sets are pushed as one union. Pull writes into one character.
    pub sync_characters: Vec<SyncCharacter>,
    /// Size of the union that a push would send.
    pub push_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncCharactersRequest {
    pub channel: String,
    pub ticked: Vec<String>,
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

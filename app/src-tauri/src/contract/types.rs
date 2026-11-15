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
    /// EM tag for section headers, 3 or 4.
    pub header_em: u8,
    /// EM tag for detail labels, 3 or 4.
    pub label_em: u8,
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
    /// Stat lines switched off, as `<category>.<line>` (keys from [`ProfileSchema`]).
    pub lines_off: Vec<String>,
    pub stats_above: bool,
    /// Plain ASCII; no `%`, `<`, `~` or `\n` (core refuses them).
    pub header: String,
    pub tags: TagSettings,
    pub mission: MissionSettings,
    pub branding: bool,
    /// Patch ids switched off. "Fix known text bugs" off = every built-in id listed here.
    pub patches_disabled: Vec<String>,
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
    /// Matches key, original and result, case-insensitive.
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
}

impl OverrideRule {
    pub const ALL: [OverrideRule; 8] = [
        OverrideRule::ControlCharacter,
        OverrideRule::AddedEmUnbalanced,
        OverrideRule::EmSignature,
        OverrideRule::VariableTokens,
        OverrideRule::FormatSpecifiers,
        OverrideRule::FormatKey,
        OverrideRule::PercentRule,
        OverrideRule::TooLong,
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
    /// Game data was not read for this channel, so nothing could match (M1).
    pub empty_catalog: bool,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    Text { text: String },
    Last,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetImportRequest {
    pub target: ChannelRef,
    pub source: FleetSource,
    pub dry_run: bool,
    /// Add to the fleet instead of replacing it.
    pub merge: bool,
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
    /// Pull: StarOrg entries with no blueprint record in this build's game data.
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
    /// `owner/name` on github.com. Default `voidput/starorg-strings`; a public mirror works.
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

/**
 * App <-> core command contract: TypeScript mirror of app/src-tauri/src/contract/types.rs.
 *
 * Kept in step by two Rust tests (contract::tests): every string union below must list exactly
 * the serde names of its Rust enum, and samples.gen.json (real mock output) must type-check
 * against these interfaces (samples.check.ts, run by `npm run check`).
 * Rules: snake_case fields, kebab-case values, `T | null` for Option (never omitted).
 */

export const CONTRACT_VERSION = 1;
export const JOB_EVENT = 'job-progress';

// Errors --------------------------------------------------------------------------------

export type ErrorCode = 'invalid' | 'not-found' | 'needs-decision' | 'busy' | 'network-off' | 'cancelled' | 'not-implemented' | 'internal' | 'dev-disabled';

export interface AppError {
  code: ErrorCode;
  message: string;
  detail: string | null;
}

// Builds, installs, channels ------------------------------------------------------------

export interface Build {
  branch: string | null;
  game_version: string | null;
  change: number | null;
  version: string | null;
}

export type ApplyStateName = 'applied' | 'stale' | 'changed-by-another-tool' | 'not-applied';

export type ApplyState =
  | { state: 'applied' }
  | { state: 'stale'; reasons: string[] }
  | { state: 'changed-by-another-tool'; tool: string | null }
  | { state: 'not-applied' };

export type Language = 'english' | 'other' | 'unknown';
export type CacheState = 'fresh' | 'missing' | 'stale';

export interface ChannelSummary {
  channel: string;
  build: Build;
  build_error: string | null;
  set_up: boolean;
  status: ApplyState;
  language: Language;
  language_value: string | null;
  foreign_tool: string | null;
  foreign_file: boolean;
  game_running: boolean;
  interrupted_apply: boolean;
  p4k_time: string | null;
  cache: CacheState;
  /** Build the file in place was made for (applied manifest); compare with `build`. */
  applied_build: Build | null;
  last_apply: LastApply | null;
}

export interface LastApply {
  time: string;
  result: ApplyResult;
  changed: number;
  warnings: number;
  dropped: number;
  duration_ms: number;
}

export type FoundBy = 'config' | 'game-dir' | 'prefix' | 'lug-config' | 'default-prefix' | 'lutris' | 'launcher-log' | 'common-path' | 'scan' | 'manual';

export interface InstallSummary {
  id: string;
  root: string;
  prefix: string | null;
  found_by: FoundBy;
  found_by_label: string;
  saved: boolean;
  channels: ChannelSummary[];
}

export interface Overview {
  contract: number;
  app_version: string;
  first_run: boolean;
  installs: InstallSummary[];
  active_install: string | null;
  default_channel: string | null;
  job: JobProgress | null;
}

export interface ChannelRef {
  install: string;
  channel: string;
}

// Jobs ----------------------------------------------------------------------------------

export type JobKind = 'discover' | 'extract' | 'apply' | 'remove' | 'restore' | 'import' | 'sync' | 'hook' | 'dev' | 'mcp';
export type JobPhase = 'running' | 'done' | 'failed' | 'cancelled';

export interface JobProgress {
  job: string;
  kind: JobKind;
  channel: string | null;
  phase: JobPhase;
  step: number;
  steps: number;
  label: string;
  cancellable: boolean;
}

// Setup ---------------------------------------------------------------------------------

export type Preset = 'recommended' | 'minimal' | 'everything';
export type DataSource = 'local' | 'published-dumps';

export interface ChannelConsent {
  channel: string;
  consent: boolean;
  switch_language: boolean;
}

export interface SetupRequest {
  install: string;
  channels: ChannelConsent[];
  data_source: DataSource;
  preset: Preset;
  fix_known_bugs: boolean;
}

export interface SetupResult {
  install: string;
  set_up: string[];
  skipped: string[];
}

export interface PlannedChannel {
  channel: string;
  target: string;
  target_is_link: boolean;
  link_target: string | null;
  backup_dir: string;
  user_cfg_line: string | null;
  user_cfg_path: string | null;
  replaces_tool: string | null;
  counts: ApplyCounts;
}

export interface SetupPlan {
  install: string;
  channels: PlannedChannel[];
  warnings: Warning[];
}

// Apply, backups ------------------------------------------------------------------------

export type ApplyResult = 'applied' | 'up-to-date' | 'skipped' | 'failed' | 'dry-run' | 'needs-setup' | 'needs-decision' | 'removed' | 'restored' | 'nothing-to-do';

export interface ApplyCounts {
  changed: number;
  ships: number;
  components: number;
  ship_weapons: number;
  fps_weapons: number;
  missiles: number;
  missions: number;
  overrides: number;
  dropped: number;
}

export interface Warning {
  kind: string;
  detail: string;
}

export interface UserCfgReport {
  changed: boolean;
  path: string | null;
  link_target: string | null;
  shared_with: string[];
  line: string | null;
}

export interface PatchOutcome {
  id: string;
  state: PatchState;
}

export type OwnedMode = 'auto' | 'any' | 'character';

/** What an apply used for `[Owned]` markers (M1 `blueprints::Selection`). */
export interface OwnedSelection {
  mode: OwnedMode;
  characters: string[];
  count: number;
  ambiguous: number;
}

/**
 * A returned report is not a success: M1 reports guards (game running, Data.p4k being written,
 * lock held, timeout) as `skipped` + `reason`, and failed / needs-setup / needs-decision /
 * up-to-date the same way. The UI switches on `result` (lib/outcome.ts).
 */
export interface ApplyReport {
  channel: string;
  result: ApplyResult;
  reason: string | null;
  build: Build;
  counts: ApplyCounts;
  patches: PatchOutcome[];
  warnings: Warning[];
  user_cfg: UserCfgReport;
  backup: string | null;
  backup_dir: string | null;
  target: string | null;
  target_is_link: boolean;
  /** Journal recovery: an interrupted earlier apply was rolled back first. */
  recovered: string | null;
  owned: OwnedSelection | null;
  duration_ms: number;
}

export interface ApplyRequest {
  target: ChannelRef;
  dry_run: boolean;
}

/** Without `allow_foreign`, another tool's loose file is kept and the report is needs-decision. */
export interface RemoveRequest {
  target: ChannelRef;
  allow_foreign: boolean;
}

export type BackupReason = 'pre-existing' | 'before-apply' | 'before-restore' | 'manual';
export type FileOrigin = 'ours' | 'foreign' | 'none';

export interface BackupView {
  id: string;
  created: string;
  reason: BackupReason;
  pinned: boolean;
  build: Build;
  existed: boolean;
  size: number | null;
  origin: FileOrigin;
  foreign_tool: string | null;
}

export interface ForeignRestore {
  backup: string;
  tool: string | null;
  made_for: Build;
  current: Build;
  build_mismatch: boolean;
}

export interface RestoreOffers {
  remove: boolean;
  remove_foreign: boolean;
  undo_last_apply: string | null;
  restore_foreign: ForeignRestore | null;
}

export interface BackupList {
  channel: string;
  folder: string;
  backups: BackupView[];
  offers: RestoreOffers;
  /** Retention: newest N, plus the newest of each of the last `keep_builds` builds, plus the pinned file. */
  keep_newest: number;
  keep_builds: number;
}

export interface BackupRef {
  target: ChannelRef;
  backup: string;
}

export interface RestoreRequest {
  target: ChannelRef;
  backup: string;
  allow_build_mismatch: boolean;
}

export interface PreviewItem {
  key: string;
  label: string;
  category: string;
  original: string;
  result: string;
}

export interface PreviewRequest {
  target: ChannelRef;
  profile: Profile | null;
  preset: Preset | null;
}

// Enhancements profile (M2 enhance::Profile, field for field) ---------------------------

export type CategoryId = 'ships' | 'components' | 'ship-weapons' | 'fps-weapons' | 'missiles' | 'missions';
export type TagKind = 'components' | 'ship-weapons' | 'missiles';
export type TagStyle = 'short' | 'medium' | 'long';
export type TagSeparator = 'dash' | 'none' | 'space' | 'underscore' | 'dot' | 'slash' | 'pipe';
export type TagEnclosing = 'square' | 'round' | 'curly' | 'angle' | 'none';
export type TagPlacement = 'prefix' | 'suffix';

/** One tag kind. `elements`: parts that are on, in output order. `styles`: non-default (medium) styles. */
export interface TagKindSettings {
  enabled: boolean;
  elements: string[];
  styles: Record<string, TagStyle>;
  separator: TagSeparator;
  enclosing: TagEnclosing;
  placement: TagPlacement;
}

export interface TagSettings {
  components: TagKindSettings;
  ship_weapons: TagKindSettings;
  missiles: TagKindSettings;
  /** One switch for every kind. */
  short_names: boolean;
  tool_size_tag: boolean;
}

export interface MissionSettings {
  /** [BP], or [BP?] when only some variants can reward one. */
  title_blueprint: boolean;
  title_reputation: boolean;
  title_em: boolean;
  blueprints: boolean;
  item_rewards: boolean;
  details: boolean;
  bullet_tags: boolean;
  owned_marker: boolean;
  max_list: number;
  header_em: number;
  label_em: number;
}

export interface Profile {
  preset: Preset | null;
  ships: boolean;
  components: boolean;
  ship_weapons: boolean;
  fps_weapons: boolean;
  missiles: boolean;
  missions: boolean;
  /** `<category>.<line>` keys from ProfileSchema. */
  lines_off: string[];
  stats_above: boolean;
  /** Plain ASCII; no %, <, ~ or \n. */
  header: string;
  tags: TagSettings;
  mission: MissionSettings;
  branding: boolean;
  /** "Fix known text bugs" off = every built-in patch id here. */
  patches_disabled: string[];
}

export interface LineSchema {
  key: string;
  label: string;
}

export interface CategorySchema {
  id: CategoryId;
  label: string;
  blurb: string;
  lines: LineSchema[];
}

export interface ElementSchema {
  id: string;
  label: string;
}

export interface TagKindSchema {
  kind: TagKind;
  label: string;
  elements: ElementSchema[];
}

export interface ProfileSchema {
  categories: CategorySchema[];
  tag_kinds: TagKindSchema[];
}

export interface CopyProfileRequest {
  from: ChannelRef;
  to: ChannelRef;
}

export interface SaveProfileRequest {
  target: ChannelRef;
  profile: Profile;
}

// Strings -------------------------------------------------------------------------------

export type StringSource = 'unmodified' | 'enhanced' | 'patched' | 'edited';
export type StringSort = 'key' | 'key-desc' | 'category' | 'source' | 'group';
/** What the Strings group tree shows (addendum 8.4). */
export type GroupBy = 'none' | 'type' | 'manufacturer' | 'size';
/** `approximate`: the loc-reference pass has not run for this build yet. */
export type GroupState = 'exact' | 'approximate';

export interface StringsQuery {
  target: ChannelRef;
  search: string;
  sources: StringSource[];
  categories: string[];
  changed_only: boolean;
  sort: StringSort;
  offset: number;
  limit: number;
  /** Group ids; a parent selects its children. Empty = all. */
  groups?: string[];
  /** Manufacturer codes; `none` = rows without one. */
  manufacturers?: string[];
  /** Sizes 1-12; 0 = rows without one. */
  sizes?: number[];
  group_by?: GroupBy;
}

export interface StringRow {
  key: string;
  original: string;
  result: string;
  source: StringSource;
  category: string;
  /** Leaf id of the type hierarchy, e.g. `components/coolers`. */
  group: string;
  group_label: string;
  manufacturer: string | null;
  size: number | null;
}

/** A node of the group tree. Type: group id; manufacturer: code or `none`; size: `"1"`..`"12"` or `none`. */
export interface GroupNode {
  id: string;
  parent: string | null;
  label: string;
  count: number;
  enhanced: number;
  edited: number;
}

export interface FacetCount {
  id: string;
  count: number;
}

export interface StringsPage {
  total: number;
  all: number;
  offset: number;
  rows: StringRow[];
  sources: FacetCount[];
  categories: FacetCount[];
  groups: GroupNode[];
  group_state: GroupState;
}

export type Layer = 'stock' | 'generated' | 'patch' | 'override';

export interface Provenance {
  layer: Layer;
  by: string;
  text: string;
  winner: boolean;
  note: string | null;
}

export interface StringDetail {
  row: StringRow;
  provenance: Provenance[];
  override_value: string | null;
  /** Labels of other groups whose records also use the key. */
  also_used_by: string[];
}

export interface KeyRequest {
  target: ChannelRef;
  key: string;
}

export interface OverrideRequest {
  target: ChannelRef;
  key: string;
  value: string;
}

/** M1 `validate::Rule::name()`. */
export type OverrideRule = 'control-character' | 'added-em-unbalanced' | 'em-signature' | 'variable-tokens' | 'format-specifiers' | 'format-key' | 'percent-rule' | 'too-long';

export interface OverrideResult {
  accepted: boolean;
  rule: OverrideRule | null;
  message: string | null;
  detail: StringDetail | null;
}

export interface OverrideEntry {
  key: string;
  value: string;
  original: string;
}

// Patches -------------------------------------------------------------------------------

export type PatchState = 'applies' | 'not-needed' | 'target-changed' | 'nothing-to-add' | 'missing' | 'out-of-range' | 'conflict' | 'disabled' | 'no-data' | 'retired';
export type PatchOrigin = 'built-in' | 'user';

export interface PatchView {
  id: string;
  title: string;
  description: string;
  keys: number;
  origin: PatchOrigin;
  state: PatchState;
  enabled: boolean;
}

export interface PatchRef {
  target: ChannelRef;
  id: string;
}

export interface PatchExample {
  key: string;
  before: string;
  after: string;
}

export interface PatchDetail {
  patch: PatchView;
  kind: string;
  range: string | null;
  reason: string;
  examples: PatchExample[];
}

export interface PatchToggle {
  target: ChannelRef;
  id: string;
  enabled: boolean;
}

export type LocOp = 'set' | 'append' | 'prepend' | 'replace';

export interface NewPatch {
  target: ChannelRef;
  title: string;
  description: string;
  key: string;
  op: LocOp;
  find: string | null;
  value: string;
}

export interface PatchExport {
  file_name: string;
  text: string;
}

// Activity ------------------------------------------------------------------------------

export type LogLevel = 'info' | 'warn' | 'error';

export interface LogEntry {
  id: number;
  time: string;
  level: LogLevel;
  job: JobKind | null;
  channel: string | null;
  message: string;
}

// Settings ------------------------------------------------------------------------------

export type Theme = 'system' | 'dark' | 'light';
export type Accent = 'cyan' | 'green' | 'amber' | 'orange' | 'rose' | 'violet';
export type Palette = 'void' | 'black';

export interface Appearance {
  theme: Theme;
  accent: Accent;
  palette: Palette;
  reduce_effects: boolean;
}

export interface NetworkSettings {
  check_updates: boolean;
  published_dumps: boolean;
  starorg: boolean;
}

export interface Settings {
  appearance: Appearance;
  network: NetworkSettings;
  default_channel: string | null;
  safe_gpu: boolean;
  advanced: AdvancedSettings;
  /** Settings -> StarOrg Tools; stored in config.toml [sync] (shared with the CLI and hooks). */
  starorg: StarOrgSettings;
}

/** Environment and "What to sync". Defaults: production, Game.log import on, the rest off. */
export interface StarOrgSettings {
  environment: StarOrgEnv;
  /** Read Game.log for new blueprints (app, CLI, post-exit hook). Off: no log is read. */
  import_game_log: boolean;
  push_blueprints: boolean;
  pull_blueprints: boolean;
  pull_fleet: boolean;
  /** Sync by itself after the game exits; off = only when asked. */
  auto_sync: boolean;
}

/** Settings > Advanced (addendum 7.7). */
export interface AdvancedSettings {
  /** Explorer, Datasets and Diff screens. Off by default. */
  developer_mode: boolean;
}

// Characters and blueprints -------------------------------------------------------------

export type CharacterMode = 'auto' | 'manual' | 'union';

export interface CharacterView {
  id: string;
  name: string;
  last_seen: string | null;
  owned: number;
}

export interface ActiveCharacter {
  mode: CharacterMode;
  character: string | null;
  resolved: string | null;
}

export interface CharactersView {
  channel: string;
  characters: CharacterView[];
  active: ActiveCharacter;
  unattributed: number;
  last_scan: string | null;
}

export interface SetActiveCharacter {
  target: ChannelRef;
  mode: CharacterMode;
  character: string | null;
}

export interface OwnedBlueprint {
  name: string;
  ambiguous: boolean;
  favorite: boolean;
  sources: string[];
  acquired: string | null;
}

export interface CharacterRequest {
  target: ChannelRef;
  character: string;
}

export interface ImportRequest {
  target: ChannelRef;
  character: string;
  dry_run: boolean;
}

export interface AmbiguousName {
  name: string;
  candidates: string[];
}

export interface ImportReport {
  file: string;
  exported_at: string | null;
  dry_run: boolean;
  total: number;
  not_completed: number;
  duplicates: number;
  imported: string[];
  known: string[];
  unmatched: string[];
  ambiguous: AmbiguousName[];
  /** Game data was not read for this channel, so nothing could match. */
  empty_catalog: boolean;
}

// Fleet import and ship names (addendum §6) ---------------------------------------------

export type NameStyle = 'none' | 'numeric' | 'group-letter' | 'symbol';
export type RestMode = 'stock' | 'numbered';
/** profile.toml `[ship_names] use`: the shared list, this channel's own, or off. */
export type ShipNamesScope = 'shared' | 'channel' | 'off';
export type FleetFormat = 'xplor-json' | 'xplor-csv' | 'text';

export interface ShipGroup {
  id: string;
  name: string;
}

/** One `[[ship]]`: list order is the order. */
export interface ShipEntry {
  hull: string;
  group: string | null;
  /** Rename of the long name. */
  name: string | null;
  /** Rename of the short name; defaults to `name`. */
  short: string | null;
}

export interface ShipNamesSettings {
  enabled: boolean;
  style: NameStyle;
  /** ' ', '-' or '. '. */
  separator: string;
  /** One of ! # * + - . */
  symbol: string;
  rest: RestMode;
  prefix_short: boolean;
  groups: ShipGroup[];
  ships: ShipEntry[];
}

export interface FleetHull {
  /** Hull id = the long name key. */
  hull: string;
  name: string;
  short: string | null;
  maker: string | null;
  ground: boolean;
  /** Other records (AI, PU variants) that show the same name. */
  npc_shared: number;
}

export interface OwnedHull {
  hull: string;
  count: number;
  /** Custom names from the export: rename suggestions. */
  names: string[];
}

export interface PendingShip {
  name: string;
  count: number;
  maker: string | null;
  names: string[];
  /** Set for ambiguous rows. */
  candidates: string[];
}

export interface HullOverride {
  hull: string;
  long: string | null;
  short: string | null;
}

export interface FleetViewRequest {
  target: ChannelRef;
  /** Read the game data when it is not cached for this build (slow). */
  read_game_data: boolean;
}

export interface FleetView {
  channel: string;
  catalog_ready: boolean;
  catalog: FleetHull[];
  owned: OwnedHull[];
  pending: PendingShip[];
  not_in_game: string[];
  imported_at: string | null;
  format: FleetFormat | null;
  scope: ShipNamesScope;
  settings: ShipNamesSettings;
  /** The shared ship-names.toml (what "All channels" switches back to). */
  shared_settings: ShipNamesSettings;
  overrides: HullOverride[];
  warnings: string[];
  invalid: boolean;
  fleet_file: string;
  settings_file: string;
}

export type FleetSourceKind = 'file' | 'text' | 'last' | 'star-org';
/** `star-org`: the StarOrg Tools hangar (needs "Pull fleet/hangar" and a sign-in). */
export type FleetSource = { kind: 'file' } | { kind: 'text'; text: string } | { kind: 'last' } | { kind: 'star-org' };

export interface FleetImportRequest {
  target: ChannelRef;
  source: FleetSource;
  dry_run: boolean;
  merge: boolean;
}

export interface ImportedHull {
  hull: string;
  name: string;
  count: number;
  names: string[];
  new: boolean;
}

export interface FleetImportReport {
  file: string | null;
  format: FleetFormat;
  dry_run: boolean;
  merge: boolean;
  total: number;
  skipped: number;
  hulls: ImportedHull[];
  unmatched: PendingShip[];
  ambiguous: PendingShip[];
  not_in_game: number;
  empty_catalog: boolean;
  added_to_order: number;
}

export interface FleetMatchRequest {
  target: ChannelRef;
  name: string;
  /** null = "Not in the game". */
  hull: string | null;
}

export interface SaveShipNamesRequest {
  target: ChannelRef;
  scope: ShipNamesScope;
  settings: ShipNamesSettings;
}

// StarOrg Tools -------------------------------------------------------------------------

export type StarOrgEnv = 'production' | 'dev';

export interface SyncCharacter {
  id: string;
  name: string;
  ticked: boolean;
  owned: number;
}

/** One org from the account card. `blueprints`: the org has blueprint tracking. */
export interface StarOrgGuild {
  id: string;
  name: string;
  blueprints: boolean;
}

export type TokenStoreKind = 'keyring' | 'file';

export interface StarOrgStatus {
  enabled: boolean;
  connected: boolean;
  environment: StarOrgEnv;
  /** starorg.tools or dev.starorg.tools */
  host: string;
  user: string | null;
  guild: string | null;
  guild_id: string | null;
  guilds: StarOrgGuild[];
  expires: string | null;
  /** Signed in before, but the token expired or was revoked. */
  expired: boolean;
  token_store: TokenStoreKind | null;
  last_sync: string | null;
  sync_characters: SyncCharacter[];
  push_count: number;
}

export interface StarOrgGuildRequest {
  channel: string;
  guild: string;
}

/** "Sync now" / the run after game exit: pull, push, fleet as the switches allow. */
export interface SyncRun {
  channel: string;
  reports: SyncReport[];
  fleet: FleetImportReport | null;
  skipped: string[];
  time: string;
}

export interface SyncCharactersRequest {
  channel: string;
  ticked: string[];
}

export interface StarOrgPullRequest {
  channel: string;
  character: string;
}

export type SyncDirection = 'pull' | 'push';

export interface SyncReport {
  direction: SyncDirection;
  channel: string;
  guild: string | null;
  character: string | null;
  added: string[];
  already: number;
  unmatched: string[];
  sent: number;
  time: string;
}

// Hooks and doctor ----------------------------------------------------------------------

export type HookKind = 'prelaunch' | 'postexit';
export type HookStateName = 'absent' | 'current' | 'outdated' | 'broken' | 'missing';

export type HookState =
  | { state: 'absent' }
  | { state: 'current' }
  | { state: 'outdated' }
  | { state: 'broken'; why: string }
  | { state: 'missing' };

export interface HookScript {
  kind: HookKind;
  path: string | null;
  is_link: boolean;
  status: HookState;
}

export interface HooksStatus {
  supported: boolean;
  cli_on_path: boolean;
  scripts: HookScript[];
}

export type CheckLevel = 'ok' | 'info' | 'warn' | 'error';

export interface DoctorCheck {
  level: CheckLevel;
  label: string;
  detail: string | null;
}

export interface DoctorSection {
  title: string;
  checks: DoctorCheck[];
}

export interface DoctorReport {
  sections: DoctorSection[];
}

/** Doctor + recent log with paths, user names and character ids replaced. */
export interface Diagnostics {
  file_name: string;
  text: string;
  redacted: number;
}

// Updates (spec 11, 12; docs/updates.md). Only used when network.check_updates is on --------

export interface UpdatePrefs {
  /** owner/name on github.com. */
  repo: string;
  include_prereleases: boolean;
}

/** `store`: the Microsoft Store build; the Store updates it, the app never checks or installs. */
export type UpdateInstall = 'in-app' | 'download-only' | 'store';

export type UpdateProblem = 'not-visible' | 'rate-limited' | 'offline' | 'bad-response';

export type NoteKind = 'heading' | 'item' | 'text';

/** Plain text (sanitised in Rust); render as text, never as HTML. */
export interface NoteBlock {
  kind: NoteKind;
  text: string;
}

export interface ReleaseView {
  version: string;
  tag: string;
  title: string;
  url: string;
  published: string | null;
  prerelease: boolean;
  notes: NoteBlock[];
  notes_truncated: boolean;
}

export type UpdateCheck =
  | { state: 'not-checked' }
  | { state: 'up-to-date' }
  | { state: 'available'; release: ReleaseView }
  | { state: 'failed'; problem: UpdateProblem; message: string };

export type UpdateCheckState = 'not-checked' | 'up-to-date' | 'available' | 'failed';

export interface UpdateStatus {
  enabled: boolean;
  current_version: string;
  prefs: UpdatePrefs;
  last_checked: string | null;
  check: UpdateCheck;
  skipped_version: string | null;
  install: UpdateInstall;
  install_note: string;
}

export interface UpdateCheckRequest {
  force: boolean;
}

/** Footer / About links; the URL for each lives in Rust (`ExternalLink::url`). */
export type ExternalLink = 'support' | 'donate';

// Developer mode (addendum 7): dev.ts ---------------------------------------------------
export type * from './dev';
export type * from './assets';
// Experimental: local MCP server (docs/mcp.md) --------------------------------------------

/** One HTTP request the local MCP server answered. */
export interface McpRequest {
  time: string;
  method: string | null;
  tool: string | null;
  /** 401 = no or wrong token. */
  status: number;
  ok: boolean;
  millis: number;
}

/** Settings -> Experimental -> "Local MCP server". Off at every start of the app. */
export interface McpStatus {
  running: boolean;
  url: string | null;
  /** Bearer token of this run (new at every start). */
  token: string | null;
  started: string | null;
  error: string | null;
  channels: string[];
  claude_code: string | null;
  claude_desktop: string | null;
  stdio: string;
  /** Newest first, at most 100. */
  requests: McpRequest[];
}

export interface McpToggle {
  enabled: boolean;
}

/** How the memory cap is held on this machine (addendum 7.18). */
export type MemoryEnforcement = 'cgroup' | 'rlimit' | 'watchdog';

export interface MemoryEstimate {
  id: string;
  label: string;
  bytes: number;
  fits: boolean;
}

/** Settings > Advanced > Memory limit of the Developer mode and MCP workers. */
export interface MemoryStatus {
  total_bytes: number | null;
  available_bytes: number | null;
  limit_gb: number;
  is_default: boolean;
  min_gb: number;
  max_gb: number;
  step_gb: number;
  default_gb: number;
  enforcement: MemoryEnforcement;
  /** A worker is stopped at this share of the limit; `fits` is measured against it. */
  stop_percent: number;
  /** Smallest first. */
  estimates: MemoryEstimate[];
  /** Set on machines with too little RAM for a comfortable limit. */
  note: string | null;
}

export interface MemoryLimitRequest {
  /** null = back to the default. */
  gb: number | null;
}

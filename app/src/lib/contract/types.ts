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

export type ErrorCode = 'invalid' | 'not-found' | 'needs-decision' | 'busy' | 'network-off' | 'cancelled' | 'internal';

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

export type JobKind = 'discover' | 'extract' | 'apply' | 'remove' | 'restore' | 'import' | 'sync';
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
  duration_ms: number;
}

export interface ApplyRequest {
  target: ChannelRef;
  dry_run: boolean;
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
  keep_newest: number;
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

// Enhancements profile ------------------------------------------------------------------

export type CategoryId = 'ships' | 'components' | 'ship-weapons' | 'missiles' | 'fps-weapons' | 'name-tags' | 'missions' | 'blueprints';

export interface StatLine {
  id: string;
  label: string;
  enabled: boolean;
}

export interface CategorySettings {
  id: CategoryId;
  label: string;
  enabled: boolean;
  lines: StatLine[];
}

export type StatsPlacement = 'below-prose' | 'above-prose';

export interface FormatOptions {
  placement: StatsPlacement;
  header: string;
}

export type TagStyle = 'short' | 'medium' | 'long';
export type TagSeparator = 'dash' | 'none' | 'space' | 'underscore' | 'dot' | 'slash' | 'pipe';
export type TagEnclosing = 'square' | 'round' | 'curly' | 'angle' | 'none';
export type TagPlacement = 'prefix' | 'suffix';

export interface TagElement {
  id: string;
  label: string;
  enabled: boolean;
  style: TagStyle;
}

export interface TagSettings {
  kind: string;
  label: string;
  elements: TagElement[];
  separator: TagSeparator;
  enclosing: TagEnclosing;
  placement: TagPlacement;
  short_name_too: boolean;
}

export interface MissionOptions {
  tag_bp: boolean;
  tag_bp_maybe: boolean;
  tag_rep: boolean;
  em4_title_tags: boolean;
  body_sections: StatLine[];
  mark_owned: boolean;
}

export interface Profile {
  preset: Preset | null;
  categories: CategorySettings[];
  format: FormatOptions;
  tags: TagSettings[];
  missions: MissionOptions;
  branding: boolean;
  fix_known_bugs: boolean;
  patches_disabled: string[];
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
export type StringSort = 'key' | 'key-desc' | 'category' | 'source';

export interface StringsQuery {
  target: ChannelRef;
  search: string;
  sources: StringSource[];
  categories: string[];
  changed_only: boolean;
  sort: StringSort;
  offset: number;
  limit: number;
}

export interface StringRow {
  key: string;
  original: string;
  result: string;
  source: StringSource;
  category: string;
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

export interface OverrideResult {
  accepted: boolean;
  rule: string | null;
  message: string | null;
  detail: StringDetail | null;
}

// Patches -------------------------------------------------------------------------------

export type PatchState = 'applies' | 'not-needed' | 'target-changed' | 'nothing-to-add' | 'missing' | 'out-of-range' | 'conflict' | 'disabled' | 'retired';
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

export interface PatchToggle {
  target: ChannelRef;
  id: string;
  enabled: boolean;
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
}

// StarOrg Tools -------------------------------------------------------------------------

export type StarOrgEnv = 'production' | 'dev';

export interface SyncCharacter {
  id: string;
  name: string;
  ticked: boolean;
  owned: number;
}

export interface StarOrgStatus {
  enabled: boolean;
  connected: boolean;
  environment: StarOrgEnv;
  user: string | null;
  guild: string | null;
  expires: string | null;
  last_sync: string | null;
  sync_characters: SyncCharacter[];
  push_count: number;
}

export interface SyncCharactersRequest {
  channel: string;
  ticked: string[];
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

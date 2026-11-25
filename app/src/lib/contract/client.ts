/**
 * Typed client for the command contract (app/src-tauri/src/contract/mod.rs).
 *
 * Two transports behind one interface:
 * - `tauri`: `invoke(name, args)` + the `job-progress` event, inside the app window.
 * - `mock`: the in-browser mock (mock.ts), for `npm run dev` / headless renders without Tauri.
 * Selection is at runtime: outside Tauri, or with `?backend=mock`, the mock is used.
 */
import type * as T from './types';
import type { DevCommandMap } from './dev';
import type { AssetCommandMap } from './assets';
import type { TelemetryCommandMap, TelemetryEvent } from './telemetry';
import { CONTRACT_VERSION, JOB_EVENT } from './types';

/** Event name of live telemetry (telemetry.ts has it too; kept here so the transport needs no runtime import of it). */
const TELEMETRY_EVENT = 'telemetry';

/** Command name -> [arguments object, result]. Argument names match the Rust parameters. */
export interface CommandMap extends DevCommandMap, AssetCommandMap, TelemetryCommandMap {
  overview: [Record<string, never>, T.Overview];
  discover_installs: [{ deep: boolean }, T.InstallSummary[]];
  pick_install_folder: [Record<string, never>, T.InstallSummary | null];
  plan_setup: [{ req: T.SetupRequest }, T.SetupPlan];
  setup: [{ req: T.SetupRequest }, T.SetupResult];
  channel_status: [{ req: T.ChannelRef }, T.ChannelSummary];
  apply: [{ req: T.ApplyRequest }, T.ApplyReport];
  remove_changes: [{ req: T.RemoveRequest }, T.ApplyReport];
  undo_last_apply: [{ req: T.ChannelRef }, T.ApplyReport];
  restore_backup: [{ req: T.RestoreRequest }, T.ApplyReport];
  list_backups: [{ req: T.ChannelRef }, T.BackupList];
  backup_now: [{ req: T.ChannelRef }, T.BackupView];
  delete_backup: [{ req: T.BackupRef }, T.BackupList];
  cancel_job: [{ job: string }, null];
  build_preview: [{ req: T.PreviewRequest }, T.PreviewItem[]];
  read_game_data: [{ req: T.ReadGameDataRequest }, T.ChannelSummary];
  profile_schema: [Record<string, never>, T.ProfileSchema];
  obtain_status: [Record<string, never>, T.ObtainStatus];
  uex_token_status: [Record<string, never>, T.UexTokenStatus];
  set_uex_token: [{ req: T.UexTokenRequest }, T.UexTokenStatus];
  fetch_uex: [Record<string, never>, T.ObtainStatus];
  loot_list: [{ req: T.LootListRequest }, T.LootList];
  loot_detail: [{ req: T.LootDetailRequest }, T.LootDetail];
  shop_inventory: [{ req: T.ShopInventoryRequest }, T.ShopDirectory];
  shop_items: [{ req: T.ShopItemsRequest }, T.ShopItems];
  shop_item_search: [{ req: T.ShopItemSearchRequest }, T.ShopItemSearch];
  commodity_list: [{ req: T.CommodityListRequest }, T.CommodityDirectory];
  commodity_detail: [{ req: T.CommodityDetailRequest }, T.CommodityInfo];
  mining_list: [{ req: T.MiningListRequest }, T.MiningDirectory];
  mining_detail: [{ req: T.MiningDetailRequest }, T.MiningInfo];
  get_profile: [{ req: T.ChannelRef }, T.Profile];
  preset_profile: [{ preset: T.Preset }, T.Profile];
  save_profile: [{ req: T.SaveProfileRequest }, T.Profile];
  copy_profile: [{ req: T.CopyProfileRequest }, T.Profile];
  query_strings: [{ req: T.StringsQuery }, T.StringsPage];
  read_string_groups: [{ req: T.ChannelRef }, T.GroupState];
  string_detail: [{ req: T.KeyRequest }, T.StringDetail];
  set_override: [{ req: T.OverrideRequest }, T.OverrideResult];
  reset_override: [{ req: T.KeyRequest }, T.StringDetail];
  list_overrides: [{ req: T.ChannelRef }, T.OverrideEntry[]];
  list_patches: [{ req: T.ChannelRef }, T.PatchView[]];
  patch_states: [{ req: T.ChannelRef }, T.PatchView[]];
  set_patch_enabled: [{ req: T.PatchToggle }, T.PatchView];
  patch_detail: [{ req: T.PatchRef }, T.PatchDetail];
  add_user_patch: [{ req: T.NewPatch }, T.PatchView];
  import_user_patch: [{ req: T.ChannelRef }, T.PatchView];
  export_user_patch: [{ req: T.PatchRef }, T.PatchExport];
  delete_user_patch: [{ req: T.PatchRef }, T.PatchView[]];
  activity: [{ limit: number }, T.LogEntry[]];
  get_settings: [Record<string, never>, T.Settings];
  save_settings: [{ req: T.Settings }, T.Settings];
  list_characters: [{ req: T.ChannelRef }, T.CharactersView];
  set_active_character: [{ req: T.SetActiveCharacter }, T.CharactersView];
  owned_blueprints: [{ req: T.CharacterRequest }, T.OwnedBlueprint[]];
  blueprint_finder: [{ req: T.BlueprintFinderRequest }, T.BlueprintFinder];
  import_smart_citizen: [{ req: T.ImportRequest }, T.ImportReport];
  rescan_logs: [{ req: T.RescanRequest }, T.RescanReport];
  forget_character: [{ req: T.CharacterRequest }, T.CharactersView];
  fleet_view: [{ req: T.FleetViewRequest }, T.FleetView];
  fleet_import: [{ req: T.FleetImportRequest }, T.FleetImportReport];
  fleet_match: [{ req: T.FleetMatchRequest }, T.FleetView];
  save_ship_names: [{ req: T.SaveShipNamesRequest }, T.FleetView];
  fleet_clear: [{ req: T.ChannelRef }, T.FleetView];
  starorg_status: [{ channel: string }, T.StarOrgStatus];
  starorg_login: [{ channel: string }, T.StarOrgStatus];
  starorg_cancel_login: [Record<string, never>, null];
  starorg_logout: [{ channel: string }, T.StarOrgStatus];
  set_starorg_guild: [{ req: T.StarOrgGuildRequest }, T.StarOrgStatus];
  set_sync_characters: [{ req: T.SyncCharactersRequest }, T.StarOrgStatus];
  starorg_pull: [{ req: T.StarOrgPullRequest }, T.SyncReport];
  starorg_push: [{ channel: string }, T.SyncReport];
  starorg_sync: [{ channel: string }, T.SyncRun];
  hooks_status: [Record<string, never>, T.HooksStatus];
  hooks_install: [Record<string, never>, T.HooksStatus];
  hooks_uninstall: [Record<string, never>, T.HooksStatus];
  doctor: [Record<string, never>, T.DoctorReport];
  export_diagnostics: [Record<string, never>, T.Diagnostics];
  save_diagnostics: [Record<string, never>, string | null];
  update_status: [Record<string, never>, T.UpdateStatus];
  check_for_updates: [{ req: T.UpdateCheckRequest }, T.UpdateStatus];
  set_update_prefs: [{ req: T.UpdatePrefs }, T.UpdateStatus];
  skip_update: [{ version: string }, T.UpdateStatus];
  open_release_page: [Record<string, never>, null];
  install_update: [Record<string, never>, T.UpdateStatus];
  open_link: [{ link: T.ExternalLink }, null];
  mcp_status: [Record<string, never>, T.McpStatus];
  set_mcp_enabled: [{ req: T.McpToggle }, T.McpStatus];
  memory_status: [Record<string, never>, T.MemoryStatus];
  set_memory_limit: [{ req: T.MemoryLimitRequest }, T.MemoryStatus];
  storage_status: [Record<string, never>, T.StorageStatus];
  storage_measure: [Record<string, never>, T.StorageStatus];
  storage_clear_assets: [Record<string, never>, T.StorageStatus];
}

export type Command = keyof CommandMap;
export type Args<K extends Command> = CommandMap[K][0];
export type Result<K extends Command> = CommandMap[K][1];

/** Same order as `COMMAND_NAMES` in contract/mod.rs (a Rust test compares them). */
export const COMMANDS = [
  'overview',
  'discover_installs',
  'pick_install_folder',
  'plan_setup',
  'setup',
  'channel_status',
  'apply',
  'remove_changes',
  'undo_last_apply',
  'restore_backup',
  'list_backups',
  'backup_now',
  'delete_backup',
  'cancel_job',
  'build_preview',
  'read_game_data',
  'profile_schema',
  'obtain_status',
  'uex_token_status',
  'set_uex_token',
  'fetch_uex',
  'loot_list',
  'loot_detail',
  'shop_inventory',
  'shop_items',
  'shop_item_search',
  'commodity_list',
  'commodity_detail',
  'mining_list',
  'mining_detail',
  'get_profile',
  'preset_profile',
  'save_profile',
  'copy_profile',
  'query_strings',
  'read_string_groups',
  'string_detail',
  'set_override',
  'reset_override',
  'list_overrides',
  'list_patches',
  'patch_states',
  'set_patch_enabled',
  'patch_detail',
  'add_user_patch',
  'import_user_patch',
  'export_user_patch',
  'delete_user_patch',
  'activity',
  'get_settings',
  'save_settings',
  'list_characters',
  'set_active_character',
  'owned_blueprints',
  'blueprint_finder',
  'import_smart_citizen',
  'rescan_logs',
  'forget_character',
  'fleet_view',
  'fleet_import',
  'fleet_match',
  'save_ship_names',
  'fleet_clear',
  'starorg_status',
  'starorg_login',
  'starorg_cancel_login',
  'starorg_logout',
  'set_starorg_guild',
  'set_sync_characters',
  'starorg_pull',
  'starorg_push',
  'starorg_sync',
  'hooks_status',
  'hooks_install',
  'hooks_uninstall',
  'doctor',
  'export_diagnostics',
  'save_diagnostics',
  'update_status',
  'check_for_updates',
  'set_update_prefs',
  'skip_update',
  'open_release_page',
  'install_update',
  'open_link',
  'dev_status',
  'dev_open',
  'dev_close',
  'dev_types',
  'dev_tree',
  'dev_list',
  'dev_owner',
  'dev_compare',
  'dev_search',
  'dev_record',
  'dev_record_combat',
  'dev_record_sources',
  'dev_export_record',
  'dev_export_type',
  'dev_sources',
  'dev_add_source',
  'dev_remove_source',
  'dev_dump_releases',
  'dev_refresh_dump_releases',
  'dev_download_dump',
  'dev_dataset',
  'dev_table',
  'dev_rows',
  'dev_row',
  'dev_groups',
  'dev_diff',
  'dev_diff_summary',
  'dev_diff_records',
  'dev_diff_groups',
  'dev_diff_record',
  'dev_diff_search',
  'asset_status',
  'asset_close',
  'asset_list',
  'asset_search',
  'asset_preview',
  'asset_extract',
  'asset_sounds',
  'asset_sound_search',
  'asset_trigger',
  'asset_sound',
  'asset_export_sounds',
  'asset_export_model',
  'asset_model_info',
  'asset_model_paints',
  'asset_model_view',
  'asset_model_chunk',
  'asset_cache_status',
  'asset_cache_pick',
  'asset_cache_reset',
  'asset_cache_clear',
  'asset_cache_move',
  'mcp_status',
  'set_mcp_enabled',
  'memory_status',
  'set_memory_limit',
  'storage_status',
  'storage_measure',
  'storage_clear_assets',
  'telemetry_status',
  'telemetry_configure',
  'telemetry_start',
  'telemetry_stop',
  'telemetry_actions',
  'telemetry_trigger',
  'telemetry_history',
  'telemetry_record',
  'telemetry_recordings',
  'telemetry_recording_delete',
  'telemetry_replay_start',
  'telemetry_replay_control',
  'telemetry_alerts',
  'telemetry_alerts_configure',
  'telemetry_alert_test',
  'telemetry_gamelog',
  'telemetry_people_clear',
  'telemetry_sessions',
  'telemetry_sessions_clear',
] as const satisfies readonly Command[];

// Compile-time: COMMANDS covers every key of CommandMap.
type _Missing = Exclude<Command, (typeof COMMANDS)[number]>;
const _allListed: _Missing extends never ? true : _Missing = true;
void _allListed;

export interface Transport {
  readonly kind: 'tauri' | 'mock';
  call<K extends Command>(cmd: K, args: Args<K>): Promise<Result<K>>;
  /** Subscribe to job progress; returns the unsubscribe function. */
  onJob(cb: (p: T.JobProgress) => void): Promise<() => void>;
  /** Subscribe to live telemetry events (status changes and frames); returns the unsubscribe function. */
  onTelemetry(cb: (e: TelemetryEvent) => void): Promise<() => void>;
}

/** Thrown by `call` for every failure, so the UI can switch on `code`. */
export class ContractError extends Error implements T.AppError {
  code: T.ErrorCode;
  detail: string | null;
  constructor(e: T.AppError) {
    super(e.message);
    this.name = 'ContractError';
    this.code = e.code;
    this.detail = e.detail;
  }
}

export function toContractError(e: unknown): ContractError {
  if (e instanceof ContractError) return e;
  if (e && typeof e === 'object' && 'code' in e && 'message' in e) {
    const o = e as Partial<T.AppError>;
    return new ContractError({ code: (o.code as T.ErrorCode) ?? 'internal', message: String(o.message), detail: o.detail ?? null });
  }
  return new ContractError({ code: 'internal', message: typeof e === 'string' ? e : e instanceof Error ? e.message : String(e), detail: null });
}

async function tauriTransport(): Promise<Transport> {
  const { invoke } = await import('@tauri-apps/api/core');
  const { listen } = await import('@tauri-apps/api/event');
  return {
    kind: 'tauri',
    async call(cmd, args) {
      try {
        return await invoke(cmd, args as Record<string, unknown>);
      } catch (e) {
        throw toContractError(e);
      }
    },
    async onJob(cb) {
      return listen<T.JobProgress>(JOB_EVENT, (ev) => cb(ev.payload));
    },
    async onTelemetry(cb) {
      return listen<TelemetryEvent>(TELEMETRY_EVENT, (ev) => cb(ev.payload));
    },
  };
}

export function inTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

function wantMock(): boolean {
  if (!inTauri()) return true;
  try {
    return new URLSearchParams(window.location.search).get('backend') === 'mock';
  } catch {
    return false;
  }
}

let transport: Promise<Transport> | null = null;

/** The transport for this window, chosen once. */
export function getTransport(): Promise<Transport> {
  transport ??= wantMock() ? import('./mock').then((m) => m.createMockTransport()) : tauriTransport();
  return transport;
}

/** Swap the transport (tests, or a "use mock data" switch). */
export function setTransport(t: Transport): void {
  transport = Promise.resolve(t);
}

export async function call<K extends Command>(cmd: K, args: Args<K>): Promise<Result<K>> {
  const t = await getTransport();
  return t.call(cmd, args);
}

const none = {} as Record<string, never>;

/** One function per command. */
export const api = {
  overview: async () => {
    const o = await call('overview', none);
    if (o.contract !== CONTRACT_VERSION) {
      throw new ContractError({ code: 'internal', message: `App and core disagree on the contract (core ${o.contract}, app ${CONTRACT_VERSION}).`, detail: null });
    }
    return o;
  },
  discoverInstalls: (deep = false) => call('discover_installs', { deep }),
  pickInstallFolder: () => call('pick_install_folder', none),
  planSetup: (req: T.SetupRequest) => call('plan_setup', { req }),
  setup: (req: T.SetupRequest) => call('setup', { req }),
  channelStatus: (req: T.ChannelRef) => call('channel_status', { req }),
  apply: (req: T.ApplyRequest) => call('apply', { req }),
  removeChanges: (req: T.RemoveRequest) => call('remove_changes', { req }),
  undoLastApply: (req: T.ChannelRef) => call('undo_last_apply', { req }),
  restoreBackup: (req: T.RestoreRequest) => call('restore_backup', { req }),
  listBackups: (req: T.ChannelRef) => call('list_backups', { req }),
  backupNow: (req: T.ChannelRef) => call('backup_now', { req }),
  deleteBackup: (req: T.BackupRef) => call('delete_backup', { req }),
  cancelJob: (job: string) => call('cancel_job', { job }),
  buildPreview: (req: T.PreviewRequest) => call('build_preview', { req }),
  /** Extract the channel's game data (and Where to get datasets) so the preview has examples; a job. */
  readGameData: (target: T.ChannelRef) => call('read_game_data', { req: { target } }),
  profileSchema: () => call('profile_schema', none),
  /** The UEX folder and files the WHERE TO GET shop line reads (metadata only). */
  obtainStatus: () => call('obtain_status', none),
  /** Whether a UEX token is saved (keyring or file); never the token. */
  uexTokenStatus: () => call('uex_token_status', none),
  /** Save the UEX token; null or empty removes it. */
  setUexToken: (token: string | null) => call('set_uex_token', { req: { token } }),
  /** "Fetch now": download both UEX files (network-off unless "Download UEX market data" is on). */
  fetchUex: () => call('fetch_uex', none),
  /** The Loot screen's list: every loot area or every item that drops in one, summaries only (`state: missing` until the game data is read). */
  lootList: (target: T.ChannelRef | null, view: T.LootView) => call('loot_list', { req: { target, view } }),
  /** One area's items or one item's areas, likeliest first (rough odds per spawned box). */
  lootDetail: (target: T.ChannelRef | null, view: T.LootView, id: string, filter: string | null = null, group_by: T.LootGroupBy | null = null, groups: string[] | null = null) =>
    call('loot_detail', { req: { target, view, id, filter, group_by, groups } }),
  /** The Shops screen: every UEX item terminal by system and place (`state: missing` without the files). */
  shopInventory: (target: T.ChannelRef | null) => call('shop_inventory', { req: { target } }),
  /** One shop's items by category, linked to records through the channel's cached item data. */
  shopItems: (target: T.ChannelRef | null, terminal: number) => call('shop_items', { req: { target, terminal } }),
  /** Every shop that sells or buys an item: the items a name matches, and one item's shops cheapest first. */
  shopItemSearch: (target: T.ChannelRef | null, query: string, key: string | null = null) => call('shop_item_search', { req: { target, query, key } }),
  /** The Commodities screen: every tradable resource of the channel's build, list rows only (`not-read` until "Read game data"). */
  commodityList: (target: T.ChannelRef | null) => call('commodity_list', { req: { target } }),
  /** One resource in full: forms, quality, volatility, cargo, where mined, UEX kiosk prices. */
  commodityDetail: (target: T.ChannelRef | null, id: string) => call('commodity_detail', { req: { target, id } }),
  /** The Commodities screen's Mining tab: every mineable resource, list rows only (same states as the commodity list). */
  miningList: (target: T.ChannelRef | null) => call('mining_list', { req: { target } }),
  /** One mineral in full: places with quality, rock profile, difficulty and the laser estimate. */
  miningDetail: (target: T.ChannelRef | null, id: string) => call('mining_detail', { req: { target, id } }),
  getProfile: (req: T.ChannelRef) => call('get_profile', { req }),
  presetProfile: (preset: T.Preset) => call('preset_profile', { preset }),
  saveProfile: (req: T.SaveProfileRequest) => call('save_profile', { req }),
  copyProfile: (req: T.CopyProfileRequest) => call('copy_profile', { req }),
  queryStrings: (req: T.StringsQuery) => call('query_strings', { req }),
  readStringGroups: (req: T.ChannelRef) => call('read_string_groups', { req }),
  stringDetail: (req: T.KeyRequest) => call('string_detail', { req }),
  setOverride: (req: T.OverrideRequest) => call('set_override', { req }),
  resetOverride: (req: T.KeyRequest) => call('reset_override', { req }),
  listOverrides: (req: T.ChannelRef) => call('list_overrides', { req }),
  listPatches: (req: T.ChannelRef) => call('list_patches', { req }),
  patchStates: (req: T.ChannelRef) => call('patch_states', { req }),
  setPatchEnabled: (req: T.PatchToggle) => call('set_patch_enabled', { req }),
  patchDetail: (req: T.PatchRef) => call('patch_detail', { req }),
  addUserPatch: (req: T.NewPatch) => call('add_user_patch', { req }),
  importUserPatch: (req: T.ChannelRef) => call('import_user_patch', { req }),
  exportUserPatch: (req: T.PatchRef) => call('export_user_patch', { req }),
  deleteUserPatch: (req: T.PatchRef) => call('delete_user_patch', { req }),
  activity: (limit = 200) => call('activity', { limit }),
  getSettings: () => call('get_settings', none),
  saveSettings: (req: T.Settings) => call('save_settings', { req }),
  listCharacters: (req: T.ChannelRef) => call('list_characters', { req }),
  setActiveCharacter: (req: T.SetActiveCharacter) => call('set_active_character', { req }),
  ownedBlueprints: (req: T.CharacterRequest) => call('owned_blueprints', { req }),
  blueprintFinder: (req: T.BlueprintFinderRequest) => call('blueprint_finder', { req }),
  importSmartCitizen: (req: T.ImportRequest) => call('import_smart_citizen', { req }),
  rescanLogs: (req: T.RescanRequest) => call('rescan_logs', { req }),
  forgetCharacter: (req: T.CharacterRequest) => call('forget_character', { req }),
  fleetView: (req: T.FleetViewRequest) => call('fleet_view', { req }),
  fleetImport: (req: T.FleetImportRequest) => call('fleet_import', { req }),
  fleetMatch: (req: T.FleetMatchRequest) => call('fleet_match', { req }),
  saveShipNames: (req: T.SaveShipNamesRequest) => call('save_ship_names', { req }),
  fleetClear: (req: T.ChannelRef) => call('fleet_clear', { req }),
  starorgStatus: (channel: string) => call('starorg_status', { channel }),
  /** Opens the system browser and waits for it (or starorgCancelLogin). */
  starorgLogin: (channel: string) => call('starorg_login', { channel }),
  starorgCancelLogin: () => call('starorg_cancel_login', none),
  starorgLogout: (channel: string) => call('starorg_logout', { channel }),
  setStarorgGuild: (req: T.StarOrgGuildRequest) => call('set_starorg_guild', { req }),
  setSyncCharacters: (req: T.SyncCharactersRequest) => call('set_sync_characters', { req }),
  starorgPull: (req: T.StarOrgPullRequest) => call('starorg_pull', { req }),
  starorgPush: (channel: string) => call('starorg_push', { channel }),
  starorgSync: (channel: string) => call('starorg_sync', { channel }),
  hooksStatus: () => call('hooks_status', none),
  hooksInstall: () => call('hooks_install', none),
  hooksUninstall: () => call('hooks_uninstall', none),
  doctor: () => call('doctor', none),
  exportDiagnostics: () => call('export_diagnostics', none),
  /** Native save dialog (Tauri only); the path comes back for display, null = cancelled. */
  saveDiagnostics: () => call('save_diagnostics', none),
  updateStatus: () => call('update_status', none),
  /** force=false: the start-up check (switch on, at most once per 24 h); true: "Check now". */
  checkForUpdates: (force: boolean) => call('check_for_updates', { req: { force } }),
  setUpdatePrefs: (req: T.UpdatePrefs) => call('set_update_prefs', { req }),
  /** "Skip this version"; '' clears it. */
  skipUpdate: (version: string) => call('skip_update', { version }),
  /** Opens the release page in the system browser (Rust side, github.com only). */
  openReleasePage: () => call('open_release_page', none),
  installUpdate: () => call('install_update', none),
  /** Support / Buy me a coffee in the system browser. An allowlist: the URLs live in Rust. */
  openLink: (link: T.ExternalLink) => call('open_link', { link }),
  /** Experimental local MCP server: off at every start, 127.0.0.1 only. */
  mcpStatus: () => call('mcp_status', none),
  setMcpEnabled: (enabled: boolean) => call('set_mcp_enabled', { req: { enabled } }),
  /** Memory limit of the Developer mode and MCP workers (Settings > Advanced). */
  memoryStatus: () => call('memory_status', none),
  /** null resets to the default. */
  setMemoryLimit: (gb: number | null) => call('set_memory_limit', { req: { gb } }),
  /** Settings > Storage: sizes per area and the cache folder's cap. */
  storageStatus: () => call('storage_status', none),
  /** "Measure again": walks the folders now instead of the sizes kept from earlier. */
  storageMeasure: () => call('storage_measure', none),
  /** "Clear asset cache"; `busy` while an asset job runs. */
  storageClearAssets: () => call('storage_clear_assets', none),
  onJob: async (cb: (p: T.JobProgress) => void) => (await getTransport()).onJob(cb),
  /** Live telemetry events of the running stream (Settings > Experimental). */
  onTelemetry: async (cb: (e: TelemetryEvent) => void) => (await getTransport()).onTelemetry(cb),
};

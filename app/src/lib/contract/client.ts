/**
 * Typed client for the command contract (app/src-tauri/src/contract/mod.rs).
 *
 * Two transports behind one interface:
 * - `tauri`: `invoke(name, args)` + the `job-progress` event, inside the app window.
 * - `mock`: the in-browser mock (mock.ts), for `npm run dev` / headless renders without Tauri.
 * Selection is at runtime: outside Tauri, or with `?backend=mock`, the mock is used.
 */
import type * as T from './types';
import { CONTRACT_VERSION, JOB_EVENT } from './types';

/** Command name -> [arguments object, result]. Argument names match the Rust parameters. */
export interface CommandMap {
  overview: [Record<string, never>, T.Overview];
  discover_installs: [{ deep: boolean }, T.InstallSummary[]];
  pick_install_folder: [Record<string, never>, T.InstallSummary | null];
  setup: [{ req: T.SetupRequest }, T.SetupResult];
  channel_status: [{ req: T.ChannelRef }, T.ChannelSummary];
  apply: [{ req: T.ApplyRequest }, T.ApplyReport];
  remove_changes: [{ req: T.ChannelRef }, T.ApplyReport];
  undo_last_apply: [{ req: T.ChannelRef }, T.ApplyReport];
  restore_backup: [{ req: T.RestoreRequest }, T.ApplyReport];
  list_backups: [{ req: T.ChannelRef }, T.BackupList];
  backup_now: [{ req: T.ChannelRef }, T.BackupView];
  delete_backup: [{ req: T.BackupRef }, T.BackupList];
  cancel_job: [{ job: string }, null];
  build_preview: [{ req: T.PreviewRequest }, T.PreviewItem[]];
  get_profile: [{ req: T.ChannelRef }, T.Profile];
  preset_profile: [{ preset: T.Preset }, T.Profile];
  save_profile: [{ req: T.SaveProfileRequest }, T.Profile];
  copy_profile: [{ req: T.CopyProfileRequest }, T.Profile];
  query_strings: [{ req: T.StringsQuery }, T.StringsPage];
  string_detail: [{ req: T.KeyRequest }, T.StringDetail];
  set_override: [{ req: T.OverrideRequest }, T.OverrideResult];
  reset_override: [{ req: T.KeyRequest }, T.StringDetail];
  list_patches: [{ req: T.ChannelRef }, T.PatchView[]];
  set_patch_enabled: [{ req: T.PatchToggle }, T.PatchView];
  activity: [{ limit: number }, T.LogEntry[]];
  get_settings: [Record<string, never>, T.Settings];
  save_settings: [{ req: T.Settings }, T.Settings];
  list_characters: [{ req: T.ChannelRef }, T.CharactersView];
  set_active_character: [{ req: T.SetActiveCharacter }, T.CharactersView];
  owned_blueprints: [{ req: T.CharacterRequest }, T.OwnedBlueprint[]];
  import_smart_citizen: [{ req: T.ImportRequest }, T.ImportReport];
  starorg_status: [{ channel: string }, T.StarOrgStatus];
  starorg_login: [{ channel: string }, T.StarOrgStatus];
  starorg_logout: [{ channel: string }, T.StarOrgStatus];
  set_sync_characters: [{ req: T.SyncCharactersRequest }, T.StarOrgStatus];
  hooks_status: [Record<string, never>, T.HooksStatus];
  hooks_install: [Record<string, never>, T.HooksStatus];
  hooks_uninstall: [Record<string, never>, T.HooksStatus];
  doctor: [Record<string, never>, T.DoctorReport];
}

export type Command = keyof CommandMap;
export type Args<K extends Command> = CommandMap[K][0];
export type Result<K extends Command> = CommandMap[K][1];

/** Same order as `COMMAND_NAMES` in contract/mod.rs (a Rust test compares them). */
export const COMMANDS = [
  'overview',
  'discover_installs',
  'pick_install_folder',
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
  'get_profile',
  'preset_profile',
  'save_profile',
  'copy_profile',
  'query_strings',
  'string_detail',
  'set_override',
  'reset_override',
  'list_patches',
  'set_patch_enabled',
  'activity',
  'get_settings',
  'save_settings',
  'list_characters',
  'set_active_character',
  'owned_blueprints',
  'import_smart_citizen',
  'starorg_status',
  'starorg_login',
  'starorg_logout',
  'set_sync_characters',
  'hooks_status',
  'hooks_install',
  'hooks_uninstall',
  'doctor',
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
  setup: (req: T.SetupRequest) => call('setup', { req }),
  channelStatus: (req: T.ChannelRef) => call('channel_status', { req }),
  apply: (req: T.ApplyRequest) => call('apply', { req }),
  removeChanges: (req: T.ChannelRef) => call('remove_changes', { req }),
  undoLastApply: (req: T.ChannelRef) => call('undo_last_apply', { req }),
  restoreBackup: (req: T.RestoreRequest) => call('restore_backup', { req }),
  listBackups: (req: T.ChannelRef) => call('list_backups', { req }),
  backupNow: (req: T.ChannelRef) => call('backup_now', { req }),
  deleteBackup: (req: T.BackupRef) => call('delete_backup', { req }),
  cancelJob: (job: string) => call('cancel_job', { job }),
  buildPreview: (req: T.PreviewRequest) => call('build_preview', { req }),
  getProfile: (req: T.ChannelRef) => call('get_profile', { req }),
  presetProfile: (preset: T.Preset) => call('preset_profile', { preset }),
  saveProfile: (req: T.SaveProfileRequest) => call('save_profile', { req }),
  copyProfile: (req: T.CopyProfileRequest) => call('copy_profile', { req }),
  queryStrings: (req: T.StringsQuery) => call('query_strings', { req }),
  stringDetail: (req: T.KeyRequest) => call('string_detail', { req }),
  setOverride: (req: T.OverrideRequest) => call('set_override', { req }),
  resetOverride: (req: T.KeyRequest) => call('reset_override', { req }),
  listPatches: (req: T.ChannelRef) => call('list_patches', { req }),
  setPatchEnabled: (req: T.PatchToggle) => call('set_patch_enabled', { req }),
  activity: (limit = 200) => call('activity', { limit }),
  getSettings: () => call('get_settings', none),
  saveSettings: (req: T.Settings) => call('save_settings', { req }),
  listCharacters: (req: T.ChannelRef) => call('list_characters', { req }),
  setActiveCharacter: (req: T.SetActiveCharacter) => call('set_active_character', { req }),
  ownedBlueprints: (req: T.CharacterRequest) => call('owned_blueprints', { req }),
  importSmartCitizen: (req: T.ImportRequest) => call('import_smart_citizen', { req }),
  starorgStatus: (channel: string) => call('starorg_status', { channel }),
  starorgLogin: (channel: string) => call('starorg_login', { channel }),
  starorgLogout: (channel: string) => call('starorg_logout', { channel }),
  setSyncCharacters: (req: T.SyncCharactersRequest) => call('set_sync_characters', { req }),
  hooksStatus: () => call('hooks_status', none),
  hooksInstall: () => call('hooks_install', none),
  hooksUninstall: () => call('hooks_uninstall', none),
  doctor: () => call('doctor', none),
  onJob: async (cb: (p: T.JobProgress) => void) => (await getTransport()).onJob(cb),
};

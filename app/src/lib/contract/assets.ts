/**
 * Developer mode assets (addendum 7.16): types mirrored from app/src-tauri/src/contract/assets.rs
 * (and the starorg-data shapes it re-exports) plus the client calls. Re-exported by types.ts.
 * Everything but the cache calls fails with `dev-disabled` while Developer mode is off. Nothing
 * is read until the Files browser, a Sounds panel or a model export asks; cached previews,
 * sounds and models are answered without starting the asset worker.
 */
import { call } from './client';
import type { ChannelRef } from './types';
import type { WorkerState } from './dev';
export { AudioLru } from '../audiolru';

/** Shown next to every export button. */
export const PERSONAL_USE = "For personal use: these are Cloud Imperium's assets; don't redistribute.";

export interface AssetStatus {
  state: WorkerState;
  channel: string | null;
  entries: number;
  datacore: boolean;
  audio: boolean;
  memory: number | null;
  error: string | null;
}

export interface FileEntry {
  name: string;
  /** Archive path, backslash separated. */
  path: string;
  dir: boolean;
  size: number;
  packed: number;
}

export interface DirPage {
  dir: string;
  parent: string | null;
  entries: FileEntry[];
  offset: number;
  total: number;
}

export interface FileSearch {
  query: string;
  hits: FileEntry[];
  total: number;
}

export type PreviewKind = 'image' | 'text' | 'audio' | 'none';

export interface AssetPreview {
  path: string;
  size: number;
  kind: PreviewKind;
  format: string;
  text: string | null;
  truncated: boolean;
  width: number | null;
  height: number | null;
  /** `data:` URL. */
  image: string | null;
  note: string | null;
  cached: boolean;
}

export interface AssetListRequest {
  target: ChannelRef;
  dir: string;
  offset: number;
  limit: number;
}

export interface AssetSearchRequest {
  target: ChannelRef;
  query: string;
  limit: number;
}

export interface AssetPathRequest {
  target: ChannelRef;
  path: string;
}

export interface AssetExtractRequest {
  target: ChannelRef;
  paths: string[];
  convert: boolean;
}

export interface AssetExportResult {
  cancelled: boolean;
  path: string | null;
  files: number;
  bytes: number;
  cached: boolean;
  failed: string[];
  note: string | null;
}

export type AudioSource = 'embedded' | 'media-file' | 'external' | 'unavailable';

export interface SoundRef {
  media_id: number;
  source: AudioSource;
  bank: string;
  media_path: string | null;
}

export interface SoundInfo extends SoundRef {
  label: string;
  playable: boolean;
  detail: string;
}

/** One ATL trigger found by name (7.17). */
export interface TriggerHit {
  name: string;
  event: string;
  bank: string;
  duration_type: string | null;
}

export interface TriggerSearch {
  hits: TriggerHit[];
  total: number;
}

export interface AssetSoundSearchRequest {
  target: ChannelRef;
  query: string;
  limit: number;
}

export interface AssetTriggerRequest {
  target: ChannelRef;
  name: string;
}

export interface TriggerInfo {
  name: string;
  property: string;
  from: string;
  bank: string | null;
  event: string | null;
  duration_type: string | null;
  sounds: SoundInfo[];
}

export interface RecordSounds {
  record: string;
  name: string;
  triggers: TriggerInfo[];
  sounds: number;
  playable: number;
  loadout: number;
}

export interface AssetRecordRequest {
  target: ChannelRef;
  what: string;
}

export interface AssetSoundRequest {
  target: ChannelRef;
  sound: SoundRef;
}

export interface AssetAudio {
  key: string;
  /** `audio/ogg` (.ogg, .opus) or `audio/wav`; '' when the sound stays a .wem (`note` says why). */
  mime: string;
  /** Base64; '' when not playable. */
  data: string;
  bytes: number;
  duration: number | null;
  cached: boolean;
  note: string | null;
  /** The codec and the file it became, e.g. `Vorbis (.ogg)`. */
  format: string;
  /** What this system lacks when the webview refuses to play it (Linux: gst-plugins-good). */
  hint: string | null;
}

export interface AssetSoundPick {
  trigger: string;
  sound: SoundRef;
}

export interface AssetExportSoundsRequest {
  target: ChannelRef;
  what: string;
  /** [] = every playable sound. */
  sounds: AssetSoundPick[];
  wem: boolean;
}

export type ModelPreset = 'full' | 'print-shell';

export interface AssetExportModelRequest {
  target: ChannelRef;
  what: string;
  name: string;
  preset: ModelPreset;
  lod: number;
  stl: boolean;
  /** Export even with less free memory than the preset may need (after `needs-decision`). */
  force: boolean;
}

export interface AssetCachePrevious {
  path: string;
  bytes: number;
  files: number;
}

export interface AssetCacheStatus {
  path: string;
  default_path: string;
  is_default: boolean;
  bytes: number;
  files: number;
  max_bytes: number;
  other_drive: boolean;
  previous: AssetCachePrevious | null;
  /** Why the folder cannot be used; every cache use is refused until it is changed or reset. */
  problem: string | null;
}

/** LOD the export dialog starts with per preset; 0-4 offered. */
export const DEFAULT_LOD: Record<ModelPreset, number> = { full: 1, 'print-shell': 0 };
export const MAX_LOD = 4;

export interface AssetCommandMap {
  asset_status: [{ req: ChannelRef }, AssetStatus];
  asset_close: [{ req: ChannelRef }, AssetStatus];
  asset_list: [{ req: AssetListRequest }, DirPage];
  asset_search: [{ req: AssetSearchRequest }, FileSearch];
  asset_preview: [{ req: AssetPathRequest }, AssetPreview];
  asset_extract: [{ req: AssetExtractRequest }, AssetExportResult];
  asset_sounds: [{ req: AssetRecordRequest }, RecordSounds];
  asset_sound_search: [{ req: AssetSoundSearchRequest }, TriggerSearch];
  asset_trigger: [{ req: AssetTriggerRequest }, TriggerInfo];
  asset_sound: [{ req: AssetSoundRequest }, AssetAudio];
  asset_export_sounds: [{ req: AssetExportSoundsRequest }, AssetExportResult];
  asset_export_model: [{ req: AssetExportModelRequest }, AssetExportResult];
  asset_cache_status: [Record<string, never>, AssetCacheStatus];
  asset_cache_pick: [Record<string, never>, AssetCacheStatus];
  asset_cache_reset: [Record<string, never>, AssetCacheStatus];
  asset_cache_clear: [Record<string, never>, AssetCacheStatus];
  asset_cache_move: [Record<string, never>, AssetCacheStatus];
}

/** Same order as `ASSET_COMMAND_NAMES` in contract/assets.rs. */
export const ASSET_COMMANDS = [
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
  'asset_cache_status',
  'asset_cache_pick',
  'asset_cache_reset',
  'asset_cache_clear',
  'asset_cache_move',
] as const satisfies readonly (keyof AssetCommandMap)[];

const none = {} as Record<string, never>;

export const assetApi = {
  status: (req: ChannelRef) => call('asset_status', { req }),
  close: (req: ChannelRef) => call('asset_close', { req }),
  list: (req: AssetListRequest) => call('asset_list', { req }),
  search: (req: AssetSearchRequest) => call('asset_search', { req }),
  preview: (req: AssetPathRequest) => call('asset_preview', { req }),
  /** Native folder dialog (Rust side). */
  extract: (req: AssetExtractRequest) => call('asset_extract', { req }),
  sounds: (req: AssetRecordRequest) => call('asset_sounds', { req }),
  /** Triggers by name, event or bank (starts the asset worker; reads the audio index only). */
  soundSearch: (req: AssetSoundSearchRequest) => call('asset_sound_search', { req }),
  trigger: (req: AssetTriggerRequest) => call('asset_trigger', { req }),
  sound: (req: AssetSoundRequest) => call('asset_sound', { req }),
  /** Native folder dialog (Rust side). */
  exportSounds: (req: AssetExportSoundsRequest) => call('asset_export_sounds', { req }),
  /** Native save dialog (Rust side). */
  exportModel: (req: AssetExportModelRequest) => call('asset_export_model', { req }),
  cacheStatus: () => call('asset_cache_status', none),
  /** Native folder dialog (Rust side). */
  cachePick: () => call('asset_cache_pick', none),
  cacheReset: () => call('asset_cache_reset', none),
  cacheClear: () => call('asset_cache_clear', none),
  cacheMove: () => call('asset_cache_move', none),
};

/** The webview's key of a sound (its LRU): the media id, or `x:` + the external path. */
export function soundId(s: SoundRef): string {
  return s.media_path ? `x:${s.media_path.toLowerCase()}` : String(s.media_id);
}

/** Bytes as `1.2 MB`. */
export function bytesText(n: number): string {
  if (n < 1024) return `${n} B`;
  const u = ['KB', 'MB', 'GB', 'TB'];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < u.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v < 10 ? v.toFixed(1) : Math.round(v)} ${u[i]}`;
}

/** Record types with a model to export: entities (and the mock's synthetic ships and items). */
export const hasModel = (type: string): boolean => type === 'EntityClassDefinition' || type === 'SynShip' || type === 'SynComponent';

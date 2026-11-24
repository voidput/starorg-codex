/**
 * Settings > Storage wording (StorageCard.svelte): sizes, the cache folder's cap, what each
 * area is. Pure, so `npm test` covers it.
 */
import type { StorageArea, StorageAreaId, StorageStatus } from './contract/types';

/** "1.5 GB", "120 MB", "980 KB", "12 B" (binary units, as the Rust side counts). */
export function sizeText(n: number): string {
  if (n < 1024) return `${n} B`;
  const u = ['KB', 'MB', 'GB', 'TB'];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < u.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v < 10 ? Number(v.toFixed(1)) : Math.round(v)} ${u[i]}`;
}

/** Share of the cap in use, 0..100 (over the cap reads 100). */
export function capPercent(s: Pick<StorageStatus, 'cache_bytes' | 'cap_bytes'>): number {
  if (s.cap_bytes <= 0) return 0;
  return Math.min(100, Math.round((s.cache_bytes / s.cap_bytes) * 100));
}

/** "312 MB of 1 GB" (+ " (over the cap; the oldest go at the next write)" when over). */
export function capText(s: Pick<StorageStatus, 'cache_bytes' | 'cap_bytes'>): string {
  const t = `${sizeText(s.cache_bytes)} of ${sizeText(s.cap_bytes)}`;
  return s.cache_bytes > s.cap_bytes ? `${t} (over the cap: the least recently used go at the next write)` : t;
}

/** What each area holds and how it is bounded. */
export const AREA_HINTS: Record<StorageAreaId, string> = {
  'game-data': 'Extracted game text and data, one entry per channel, compressed. Read again after a game patch.',
  'where-to-get': 'Datasets behind WHERE TO GET and the MCP server, the last 2 builds per channel.',
  uex: 'UEX market prices, replaced on each download.',
  'asset-exports': 'Developer mode: decoded sounds, exported models and previews.',
  temporary: 'Extraction folders, removed after each read; leftovers go after an hour.',
  telemetry: 'Live telemetry (experimental): the game service layout read from StarCitizen.exe, the last 3 builds.',
  backups: 'Copies of global.ini, compressed. Not part of the cache and never removed by its cap.',
};

/** "5 newest + newest of the last 3 builds + the original" */
export function backupRule(s: Pick<StorageStatus, 'backup_keep_newest' | 'backup_keep_builds'>): string {
  return `Kept per channel: the ${s.backup_keep_newest} newest, the newest of each of the last ${s.backup_keep_builds} game builds, and the file from before StarOrg Codex.`;
}

/** The area row's size line: "48 MB · 3 files". */
export function areaText(a: Pick<StorageArea, 'bytes' | 'files'>): string {
  return `${sizeText(a.bytes)} · ${a.files} ${a.files === 1 ? 'file' : 'files'}`;
}

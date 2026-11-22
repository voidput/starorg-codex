/**
 * The Explorer record's Sources card, as pure functions: which records get one, percent
 * and price text, place names and the card's storage key. The data comes from
 * `dev_record_sources` (shops and commodity kiosks from UEX files the user supplied, loot
 * containers from the loot tables, mining places from the mining spawns).
 */
import type { MinedBelt, MinedSpot, QualityRange, RecordSources } from './contract/dev';

/**
 * Closed unless this viewer opened it ([`loadOpen`](./cardopen.ts) default false): the first
 * open per build extracts the loot and mining data (a minute or two), so it never starts by
 * itself.
 */
export const SOURCES_OPEN_KEY = 'starorg.dev.sourcesOpen';

/** Record types that can be loot or a mined ore. */
const TYPES = new Set(['EntityClassDefinition', 'ResourceType', 'MineableElement', 'MineableComposition']);

export const hasSources = (type: string): boolean => TYPES.has(type);

/** A 0..1 share as "21 %", "0.84 %", "0.012 %", "< 0.001 %"; a dash when missing. */
export function pctText(v: number | null | undefined): string {
  if (v === null || v === undefined || !Number.isFinite(v)) return '–';
  const p = v * 100;
  if (p === 0) return '0 %';
  if (p < 0.001) return '< 0.001 %';
  const digits = p >= 10 ? 0 : p >= 1 ? 1 : p >= 0.1 ? 2 : 3;
  return `${Number(p.toFixed(digits))} %`;
}

/** "Hurston · Stanton", else the provider key ("Pyro5a"). */
export function beltPlace(b: MinedBelt): string {
  const name = b.body || b.location;
  return b.system && b.system.toLowerCase() !== name.toLowerCase() ? `${name} · ${b.system}` : name;
}

/** The known place, else the readable area. */
export const spotPlace = (s: MinedSpot): string => s.place ?? s.area;

/** Belt weights as bar widths (0..1) against the heaviest row of the same kind. */
export function beltBars(belts: MinedBelt[]): number[] {
  const max = new Map<string, number>();
  for (const b of belts) max.set(b.kind ?? '', Math.max(max.get(b.kind ?? '') ?? 0, b.spawn_share ?? 0));
  return belts.map((b) => {
    const m = max.get(b.kind ?? '') ?? 0;
    return m > 0 ? (b.spawn_share ?? 0) / m : 0;
  });
}

/** The header's summary: "2 shops · 5 kiosks buy it · 12 loot areas · 3 mining places", or null when empty. */
export function sourcesSummary(s: RecordSources | null): string | null {
  if (!s) return null;
  const parts: string[] = [];
  const b = s.buy?.total ?? 0;
  if (b) parts.push(`${b} shop${b === 1 ? '' : 's'}`);
  const k = s.sell?.total ?? 0;
  if (k) parts.push(`${k} kiosk${k === 1 ? '' : 's'} buy${k === 1 ? 's' : ''} it`);
  const c = s.loot?.container_total ?? 0;
  if (c) parts.push(`${c} loot area${c === 1 ? '' : 's'}`);
  const m = Array.isArray(s.mined) ? s.mined.reduce((n, r) => n + r.belts.length + r.spots.length, 0) : 0;
  if (m) parts.push(`${m} mining place${m === 1 ? '' : 's'}`);
  return parts.length ? parts.join(' · ') : null;
}

/** Nothing to show: no shop or kiosk row, loot row or mining place. Missing UEX files still show a note. */
export const isEmpty = (s: RecordSources): boolean =>
  !s.buy?.offers.length &&
  s.buy?.loaded !== false &&
  !s.sell?.offers.length &&
  s.sell?.loaded !== false &&
  !s.loot?.containers.length &&
  !s.loot?.tables.length &&
  !(Array.isArray(s.mined) && s.mined.length) &&
  !(Array.isArray(s.quality) && s.quality.length);

/** "6,900 aUEC". */
export const priceText = (v: number): string => `${Math.round(v).toLocaleString('en-US')} aUEC`;

/** "9,612 aUEC/SCU": a commodity kiosk's price. */
export const scuPriceText = (v: number): string => `${priceText(v)}/SCU`;

/** "fetched 21 Nov 2026" for the UEX file's `_fetched`; null when unknown. */
export function fetchedText(iso: string | null | undefined): string | null {
  const t = iso ? Date.parse(iso) : NaN;
  if (Number.isNaN(t)) return null;
  return `fetched ${new Date(t).toLocaleDateString('en-GB', { day: 'numeric', month: 'short', year: 'numeric', timeZone: 'UTC' })}`;
}

/** "201–1000, most near 201 (± 298)"; a dash when the curve is unknown. */
export function qualityText(r: QualityRange): string {
  const span = r.min !== null && r.max !== null ? `${r.min}–${r.max}` : null;
  // A curve cut at its minimum has its mean below it (501–1000, mean 500): keep it inside.
  const mean = r.mean === null ? null : Math.min(Math.max(r.mean, r.min ?? r.mean), r.max ?? r.mean);
  const peak = mean !== null ? `most near ${Math.round(mean)}${r.stddev ? ` (± ${Math.round(r.stddev)})` : ''}` : null;
  return [span, peak].filter(Boolean).join(', ') || '–';
}

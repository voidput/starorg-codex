/**
 * The Commodities screen's Mining tab, as pure functions: the list search, kind filter and
 * sort, the place sort ("most common" or "best quality first"), and the labels the detail
 * shows. The data comes from `mining_list` (summaries) and `mining_detail` (one mineral),
 * typed in `starorg_mcp::mining`.
 */
import type { Difficulty, MineralSummary, MiningGrade, MiningPlace, MiningTool, PlaceQuality, QualityRange } from './contract/types';
import { matchesWords } from './shops.ts';

/** The kind chips: every kind of mining, then the three the rocks name. */
export type MiningKind = '' | 'hand' | 'vehicle' | 'ship';
export const KINDS: { id: MiningKind; label: string }[] = [
  { id: '', label: 'All' },
  { id: 'hand', label: 'Hand' },
  { id: 'vehicle', label: 'ROC' },
  { id: 'ship', label: 'Ship' },
];

/** "hand" -> "Hand", "vehicle" -> "ROC", "ship" -> "Ship"; a dash when unknown. */
export function kindLabel(kind: string | null | undefined): string {
  return KINDS.find((k) => k.id && k.id === kind)?.label ?? '–';
}

export type MineralSort = 'name' | 'quality';
export type PlaceSort = 'common' | 'quality';

/** Rows of `kind` (all when empty) whose name, id or refined form has every word of `query`. */
export function filterMinerals(rows: MineralSummary[] | null | undefined, query: string, kind: MiningKind): MineralSummary[] {
  return (Array.isArray(rows) ? rows : []).filter((r) => (!kind || r.kinds.includes(kind)) && matchesWords(`${r.name} ${r.id} ${r.refined ?? ''}`, query));
}

/**
 * By name (the list's order), or the best chance of a high-quality roll first: minerals whose
 * best share is uncertain (a quality scale below 1, or a curve not known) after the others,
 * unknown last.
 */
export function sortMinerals(rows: MineralSummary[], by: MineralSort): MineralSummary[] {
  if (by === 'name') return rows;
  return [...rows].sort((a, b) => Number(a.best_high_uncertain) - Number(b.best_high_uncertain) || (b.best_high ?? -1) - (a.best_high ?? -1) || a.name.localeCompare(b.name));
}

/** A known curve and a full quality scale: the share compares with the others'. */
export const trusted = (q: PlaceQuality): boolean => q.full_scale && q.source !== 'unknown';

/**
 * Places most common first (the backend's order: by kind, then spawn weight), or best quality
 * first: places with a full quality scale and a known curve before the others (a scale below
 * 1 may roll lower; how much is not in the data), then the high-roll chance, the top two
 * steps' share and the curve's mean; places with nothing known last, in their common order.
 */
export function sortPlaces(places: MiningPlace[] | null | undefined, by: PlaceSort): MiningPlace[] {
  const list = Array.isArray(places) ? places : [];
  if (by === 'common') return list;
  const key = (q: PlaceQuality) => [q.high ?? -1, q.top_share ?? -1, q.range.mean ?? -1] as const;
  return list
    .map((p, i) => ({ p, i }))
    .sort((a, b) => {
      const x = key(a.p.quality);
      const y = key(b.p.quality);
      return Number(trusted(b.p.quality)) - Number(trusted(a.p.quality)) || y[0] - x[0] || y[1] - x[1] || y[2] - x[2] || a.i - b.i;
    })
    .map((x) => x.p);
}

/** "Hurston · Stanton", "Lagrange A" (system not known). */
export function placeName(p: Pick<MiningPlace, 'place' | 'location' | 'system'>): string {
  const name = p.place || p.location;
  return p.system && p.system.toLowerCase() !== name.toLowerCase() ? `${name} · ${p.system}` : name;
}

/** "501–1000" of a curve; a dash when unknown. */
export function rangeText(r: QualityRange | null | undefined): string {
  return r && r.min !== null && r.max !== null ? `${r.min}–${r.max}` : '–';
}

/** Where the place's curve comes from: "Pyro System override", "same as base", or not known. */
export const curveSource = (q: PlaceQuality): string =>
  q.source === 'override' && q.override_location ? `${q.override_location} override` : q.source === 'unknown' ? 'not known: system not known, an override may apply' : 'same as base';

/** "quality scale ×0.49: effect not known, share may be lower"; null at full scale. */
export function scaleNote(p: Pick<MiningPlace, 'quality_scales' | 'quality'>): string | null {
  if (p.quality.full_scale) return null;
  const below = p.quality_scales.filter((s) => s !== 1);
  return `quality scale ${below.map((s) => `×${s}`).join(', ')}: effect not known, share may be lower`;
}

export const GRADE_LABEL: Record<MiningGrade, string> = { easy: 'Easy', medium: 'Medium', hard: 'Hard' };

/** Tailwind text colour of a grade. */
export const gradeTone = (g: MiningGrade | null | undefined): string => (g === 'hard' ? 'text-destructive' : g === 'medium' ? 'text-warning' : g === 'easy' ? 'text-success' : 'text-muted-foreground');

/** "resistance 0.95 · instability 1,000 · window thinness 2.3 → 6 of 6 points" (missing inputs said). */
export function difficultyInputs(d: Difficulty): string {
  const n = (v: number) => Number(v.toFixed(3)).toLocaleString('en-US');
  const parts = [d.resistance !== null ? `resistance ${n(d.resistance)}` : null, d.instability !== null ? `instability ${n(d.instability)}` : null, d.window_thinness !== null ? `window thinness ${n(d.window_thinness)}` : null].filter(Boolean);
  const missing = d.missing.length ? ` (not known: ${d.missing.join(', ')})` : '';
  return `${parts.join(' · ')} → ${d.points} of ${d.max_points} points${missing}`;
}

export const TOOL_LABEL: Record<MiningTool, string> = { hand: 'Hand', roc: 'ROC', ship: 'Ship', unknown: 'Not known' };

/** "1,234" power figures; a dash when unknown. */
export function powerText(v: number | null | undefined): string {
  return v === null || v === undefined || !Number.isFinite(v) ? '–' : Math.round(v).toLocaleString('en-US');
}

/** "20–45 %" of a rock's ore share; a dash when unknown. */
export function shareText(min: number | null, max: number | null): string {
  if (min === null && max === null) return '–';
  const f = (v: number | null) => (v === null ? '?' : Number(v.toFixed(2)).toString());
  return `${f(min)}–${f(max)} %`;
}

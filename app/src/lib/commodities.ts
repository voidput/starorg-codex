/**
 * The Commodities screen's pure helpers: the list search and group chips, the flat row model
 * the windowed list renders (group headers and rows, one fixed height), the visible window,
 * and small text helpers. The data comes from `commodity_list` (summaries only) and
 * `commodity_detail` (one resource, on selection), typed in `starorg_mcp::commodities`.
 */
import type { CargoDamage, CommoditySummary } from './contract/types';
import { matchesWords } from './shops.ts';

/** "Metal", "Metal > Unrefined Ores". */
export const groupPath = (c: Pick<CommoditySummary, 'group' | 'subgroup'>): string => (c.subgroup ? `${c.group} > ${c.subgroup}` : c.group);

/** Top groups in list order, with how many rows each has (before the text search). */
export function groupChips(rows: CommoditySummary[] | null | undefined): { group: string; count: number }[] {
  const out: { group: string; count: number }[] = [];
  for (const r of Array.isArray(rows) ? rows : []) {
    const hit = out.find((g) => g.group === r.group);
    if (hit) hit.count++;
    else out.push({ group: r.group, count: 1 });
  }
  return out;
}

/** Rows of `group` (all when empty) whose name, id or group path has every word of `query`. */
export function filterCommodities(rows: CommoditySummary[] | null | undefined, query: string, group: string): CommoditySummary[] {
  return (Array.isArray(rows) ? rows : []).filter((r) => (!group || r.group === group) && matchesWords(`${r.name} ${r.id} ${groupPath(r)}`, query));
}

export type ListRow = { kind: 'header'; key: string; label: string; count: number } | { kind: 'item'; key: string; item: CommoditySummary };

/** Rows with a header before each group path (the list is sorted by group, then name). */
export function flatRows(rows: CommoditySummary[]): ListRow[] {
  const out: ListRow[] = [];
  let header = null as Extract<ListRow, { kind: 'header' }> | null;
  for (const r of rows) {
    const label = groupPath(r);
    if (header?.label !== label) {
      header = { kind: 'header', key: `h:${label}`, label, count: 0 };
      out.push(header);
    }
    header.count++;
    out.push({ kind: 'item', key: r.id, item: r });
  }
  return out;
}

/** First and last row index to render (inclusive) for a scroll position; `last < first` when empty. */
export function visibleRange(scrollTop: number, viewHeight: number, rowHeight: number, total: number, overscan = 6): { first: number; last: number } {
  if (total <= 0 || rowHeight <= 0) return { first: 0, last: -1 };
  const first = Math.max(0, Math.floor(Math.max(0, scrollTop) / rowHeight) - overscan);
  const last = Math.min(total - 1, Math.ceil((Math.max(0, scrollTop) + Math.max(0, viewHeight)) / rowHeight) + overscan);
  return { first: Math.min(first, total - 1), last };
}

/** The picked row while it is listed, else the first listed one. */
export function currentId(rows: CommoditySummary[], picked: string | null): string | null {
  if (picked && rows.some((r) => r.id === picked)) return picked;
  return rows[0]?.id ?? null;
}

/** The next or previous listed row (arrow keys), staying on the ends. */
export function stepId(rows: CommoditySummary[], id: string | null, by: number): string | null {
  if (!rows.length) return null;
  const i = rows.findIndex((r) => r.id === id);
  if (i < 0) return rows[0]!.id;
  return rows[Math.min(rows.length - 1, Math.max(0, i + by))]!.id;
}

/** "0.01 SCU", "1 SCU", "32 SCU". */
export function scuText(v: number | null | undefined): string {
  if (v === null || v === undefined || !Number.isFinite(v)) return '–';
  return `${Number(v.toPrecision(3)).toLocaleString('en-US')} SCU`;
}

/** A plain number with at most `digits` decimals ("0.0675", "2.1"); a dash when missing. */
export function numText(v: number | null | undefined, digits = 4): string {
  if (v === null || v === undefined || !Number.isFinite(v)) return '–';
  return Number(v.toFixed(digits)).toLocaleString('en-US', { maximumFractionDigits: digits });
}

/** "375 damage, 0.1–2.1 m": one damage row's explosion; null when the row has none. */
export function explosionText(d: CargoDamage): string | null {
  const parts: string[] = [];
  if (d.explosion_damage !== null) parts.push(`${numText(d.explosion_damage, 1)} damage`);
  if (d.explosion_min_radius !== null || d.explosion_max_radius !== null) parts.push(`${numText(d.explosion_min_radius, 2)}–${numText(d.explosion_max_radius, 2)} m`);
  return parts.length ? parts.join(', ') : null;
}

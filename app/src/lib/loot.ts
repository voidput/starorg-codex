/**
 * The Loot screen's pure helpers: rough-odds text, the list search and group filter, the
 * grouped areas list as flat rows (headers and areas, for one windowed list), the visible
 * window of a long list, and labels for how an item is there.
 */
import type { LootDrop, LootDropKind, LootSummary, LootView } from './contract/types';

/** Rough odds per spawned box as a percentage, as the WHERE TO GET line writes them (`pct` in where_.rs). */
export function oddsText(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return '0%';
  const p = n * 100;
  if (n >= 0.1) return `${p.toFixed(1)}%`;
  if (n >= 0.001) return `${p.toFixed(2)}%`;
  return `${Number(p.toPrecision(2))}%`;
}

/** Lower-case words of a query. */
export const queryWords = (q: string): string[] => q.toLowerCase().split(/\s+/).filter(Boolean);

/** Rows whose name, group or id hold every word of `query`, in `group` when set. */
export function filterSummaries(rows: LootSummary[] | null | undefined, query: string, group: string): LootSummary[] {
  if (!Array.isArray(rows)) return [];
  const words = queryWords(query);
  return rows.filter((r) => {
    if (group && r.group !== group) return false;
    if (!words.length) return true;
    const hay = `${r.name} ${r.group} ${r.id}`.toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}

/** The groups of a list with their sizes, by name with `Other` last (the filter's options). */
export function groupsOf(rows: LootSummary[] | null | undefined): { group: string; count: number }[] {
  const out: { group: string; count: number }[] = [];
  if (!Array.isArray(rows)) return out;
  const at = new Map<string, number>();
  for (const r of rows) {
    const i = at.get(r.group);
    if (i === undefined) {
      at.set(r.group, out.length);
      out.push({ group: r.group, count: 1 });
    } else out[i]!.count++;
  }
  return out.sort((a, b) => Number(a.group === 'Other') - Number(b.group === 'Other') || a.group.localeCompare(b.group));
}

export type ListEntry = { kind: 'group'; group: string; count: number } | { kind: 'row'; row: LootSummary };

/** One windowed list: the areas view gets a header before each group (rows arrive grouped); items are flat. */
export function listEntries(rows: LootSummary[], view: LootView): ListEntry[] {
  if (view === 'items') return rows.map((row) => ({ kind: 'row', row }));
  const out: ListEntry[] = [];
  let head: { kind: 'group'; group: string; count: number } | null = null;
  for (const row of rows) {
    if (!head || head.group !== row.group) {
      head = { kind: 'group', group: row.group, count: 0 };
      out.push(head);
    }
    head.count++;
    out.push({ kind: 'row', row });
  }
  return out;
}

/** The rows a windowed list draws: `[first, end)` around the visible part, `overscan` rows either side. */
export function windowRange(scrollTop: number, viewHeight: number, rowHeight: number, total: number, overscan = 6): { first: number; end: number } {
  if (total <= 0 || rowHeight <= 0) return { first: 0, end: 0 };
  const top = Math.max(0, scrollTop);
  const first = Math.max(0, Math.floor(top / rowHeight) - overscan);
  const end = Math.min(total, Math.ceil((top + Math.max(0, viewHeight)) / rowHeight) + overscan);
  return { first: Math.min(first, end), end };
}

/** How the item is there, beside the container: a rack always holds it (a loot roll or a loose spot, whose container says "Lying loose", needs no word). */
export const KIND_LABEL: Record<LootDropKind, string> = { loot: '', fixed: 'always in it', loose: '' };

/** The container line of a drop: `Large weapons crate · Rare · always in it`. */
export function dropWhere(d: Pick<LootDrop, 'container' | 'rarity' | 'kind'>): string {
  return [d.container, d.rarity, KIND_LABEL[d.kind]].filter(Boolean).join(' · ');
}

/** The id still listed, else null (a pick hidden by the filter or gone after a refresh). */
export const listed = (rows: LootSummary[], id: string | null): string | null => (id !== null && rows.some((r) => r.id === id) ? id : null);

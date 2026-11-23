/**
 * The Loot screen's pure helpers: rough-odds text, the list search and group filter, and
 * labels for how an item is there. The lists' trees are in loottree.ts.
 */
import type { LootDrop, LootDropKind, LootSummary } from './contract/types';

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

/** Rows whose name, tree path, group or id hold every word of `query`, in `group` when set. */
export function filterSummaries(rows: readonly LootSummary[] | null | undefined, query: string, group: string): LootSummary[] {
  if (!Array.isArray(rows)) return [];
  const words = queryWords(query);
  return rows.filter((r) => {
    if (group && r.group !== group) return false;
    if (!words.length) return true;
    const path = Array.isArray(r.path) ? r.path.join(' ') : '';
    const hay = `${r.name} ${path} ${r.group} ${r.id}`.toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}

/** The groups of a list with their sizes, by name with `Other` last (the filter's options). */
export function groupsOf(rows: readonly LootSummary[] | null | undefined): { group: string; count: number }[] {
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

/** How the item is there, beside the container: a rack always holds it (a loot roll or a loose spot, whose container says "Lying loose", needs no word). */
export const KIND_LABEL: Record<LootDropKind, string> = { loot: '', fixed: 'always in it', loose: '' };

/** The container line of a drop: `Large weapons crate · Rare · always in it`. */
export function dropWhere(d: Pick<LootDrop, 'container' | 'rarity' | 'kind'>): string {
  return [d.container, d.rarity, KIND_LABEL[d.kind]].filter(Boolean).join(' · ');
}

/** The id still listed, else null (a pick hidden by the filter or gone after a refresh). */
export const listed = (rows: readonly LootSummary[], id: string | null): string | null => (id !== null && rows.some((r) => r.id === id) ? id : null);

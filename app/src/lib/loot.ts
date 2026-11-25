/**
 * The Loot screen's pure helpers: rough-odds text, the list search and group filter, and
 * labels for how an item is there. The lists' trees are in loottree.ts.
 */
import type { LootDrop, LootDropKind, LootGroupBy, LootSummary, LootView } from './contract/types';

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

/** Id prefix of an NPC row of the areas list (`NPC_PREFIX` in loot_tree.rs): `npc:` and its actor loot table. */
export const NPC_PREFIX = 'npc:';
/** The kind of place NPC rows sit under (`NPC_KIND`). */
export const NPC_KIND = 'NPCs (body loot)';
/** Whether an areas-list id is an NPC body's. */
export const isNpc = (id: string | null | undefined): boolean => typeof id === 'string' && id.startsWith(NPC_PREFIX);
/** The line at the top of an NPC: what its list covers. */
export const NPC_NOTE = 'Body loot only: what the NPC carries in its pockets. The gear it wears (suit, armour, weapon) is not in this list yet. The data gives no roll count; these odds may be a floor.';
/** What one roll of odds is per: one body of an NPC, one spawned box of an area. */
export const perWhat = (id: string | null | undefined): string => (isNpc(id) ? 'per body' : 'per box');

/** What a row's odds are per: an area's by `perWhat`; an item's per box, per body (NPCs only) or both. */
export function rowPer(view: LootView, r: Pick<LootSummary, 'id' | 'count' | 'npcs'>): string {
  if (view === 'areas') return perWhat(r.id);
  const npcs = r.npcs ?? 0;
  if (npcs <= 0) return 'per box';
  return npcs >= r.count ? 'per body' : 'per box or body';
}

/** Whether a branch (its tree id, `p:` and its labels) is the NPC kind or below it. */
export const npcBranch = (branchId: string): boolean => branchId.slice(2).split('\u001f')[0] === NPC_KIND;
/** What a branch's odds are per: below `NPCs (body loot)` per body; an item branch per box or body. */
export const branchPer =(view: LootView, branchId: string): string => (view === 'items' ? 'per box or body' : npcBranch(branchId) ? 'per body' : 'per box');

/** What a detail group's odds are per: an area's drops as the area; an item's by its places, an NPC group per body. */
export function groupPer(view: LootView, entry: Pick<LootSummary, 'id' | 'count' | 'npcs'> | null | undefined, groupBy: LootGroupBy, label: string): string {
  if (!entry) return 'per box';
  if (view === 'areas') return perWhat(entry.id);
  if (groupBy === 'place') return label === NPC_KIND ? 'per body' : 'per box';
  return rowPer(view, entry);
}

/** Where an item drops, places and NPCs counted apart: `3 areas`, `2 NPCs`, `3 areas, 1 NPC`. */
export function placesText(r: Pick<LootSummary, 'count' | 'npcs'>): string {
  const npcs = Math.max(0, Math.min(r.npcs ?? 0, r.count));
  const areas = r.count - npcs;
  const plural = (n: number, one: string, many: string) => `${n.toLocaleString('en-US')} ${n === 1 ? one : many}`;
  const parts = [];
  if (areas > 0 || npcs === 0) parts.push(plural(areas, 'area', 'areas'));
  if (npcs > 0) parts.push(plural(npcs, 'NPC', 'NPCs'));
  return parts.join(', ');
}

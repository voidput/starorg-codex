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

/** How the item is there, beside the container: a rack always holds it, an NPC can spawn wearing it (a loot roll or a loose spot, whose container says "Lying loose", needs no word). */
export const KIND_LABEL: Record<LootDropKind, string> = { loot: '', fixed: 'always in it', loose: '', worn: 'can spawn wearing' };

/** The container line of a drop: `Large weapons crate · Rare · always in it`, `Helmet · can spawn wearing · if AI/NpcClass/Heavy`. */
export function dropWhere(d: Pick<LootDrop, 'container' | 'rarity' | 'kind'> & { note?: string | null }): string {
  return [d.container, d.rarity, KIND_LABEL[d.kind], d.note].filter(Boolean).join(' · ');
}

/** The id still listed, else null (a pick hidden by the filter or gone after a refresh). */
export const listed = (rows: readonly LootSummary[], id: string | null): string | null => (id !== null && rows.some((r) => r.id === id) ? id : null);

/** Id prefix of an NPC row of the areas list (`NPC_PREFIX` in loot_tree.rs): `npc:` and its actor loot table. */
export const NPC_PREFIX = 'npc:';
/** The kind of place NPC rows sit under (`NPC_KIND`). */
export const NPC_KIND = 'NPCs (body loot)';
/** Whether an areas-list id is an NPC body's. */
export const isNpc = (id: string | null | undefined): boolean => typeof id === 'string' && id.startsWith(NPC_PREFIX);
/** The line at the top of an NPC's body loot: what its list covers. */
export const NPC_NOTE = 'Body loot: what the NPC carries in its pockets. What it wears and wields (suit, armour, weapon) is its worn-gear row, under NPCs (worn gear). The data gives no roll count; these odds may be a floor.';
/** Id prefix of an NPC's worn-gear row (`GEAR_PREFIX` in loot_tree.rs): `gear:` and an actor class. */
export const GEAR_PREFIX = 'gear:';
/** The kind of place worn-gear rows sit under (`GEAR_KIND`). */
export const GEAR_KIND = 'NPCs (worn gear)';
/** Whether an areas-list id is an NPC's worn gear. */
export const isGear = (id: string | null | undefined): boolean => typeof id === 'string' && id.startsWith(GEAR_PREFIX);
/** The line at the top of an NPC's worn gear. */
export const GEAR_NOTE = 'What this NPC type can spawn wearing, from its loadout records. Missions can change outfits, and the pick between options is assumed even (the data gives no weights).';
/** What worn-gear odds are per: one NPC of the type, an estimate. */
export const PER_NPC = 'per NPC (est.)';
/** What one roll of odds is per: one body of an NPC, one NPC of a type for worn gear, one spawned box of an area. */
export const perWhat = (id: string | null | undefined): string => (isNpc(id) ? 'per body' : isGear(id) ? PER_NPC : 'per box');

/** `per box`, `per body`, `per NPC (est.)`, or the words joined (`per box or body`, `per box, body or NPC`). */
function perOf(box: boolean, body: boolean, npc: boolean): string {
  const words = [box && 'box', body && 'body', npc && 'NPC'].filter((w): w is string => !!w);
  if (words.length === 0) return 'per box';
  if (words.length === 1) return words[0] === 'NPC' ? PER_NPC : `per ${words[0]}`;
  return `per ${words.slice(0, -1).join(', ')} or ${words[words.length - 1]}`;
}

/** What a row's odds are per: an area's by `perWhat`; an item's per box, per body (NPC bodies), per NPC (worn gear) or a mix. */
export function rowPer(view: LootView, r: Pick<LootSummary, 'id' | 'count' | 'npcs'> & { worn?: number }): string {
  if (view === 'areas') return perWhat(r.id);
  const npcs = Math.max(0, r.npcs ?? 0);
  const worn = Math.max(0, r.worn ?? 0);
  return perOf(r.count - npcs - worn > 0, npcs > 0, worn > 0);
}

/** The kind (top label) of a branch from its tree id (`p:` and its labels). */
const branchKind = (branchId: string): string => branchId.slice(2).split('\u001f')[0] ?? '';
/** Whether a branch (its tree id, `p:` and its labels) is the NPC kind or below it. */
export const npcBranch = (branchId: string): boolean => branchKind(branchId) === NPC_KIND;
/** Whether a branch is the worn-gear kind or below it. */
export const gearBranch = (branchId: string): boolean => branchKind(branchId) === GEAR_KIND;
/** What a branch's odds are per: below `NPCs (body loot)` per body, below `NPCs (worn gear)` per NPC; an item branch any of them. */
export const branchPer = (view: LootView, branchId: string): string =>
  view === 'items' ? 'per box, body or NPC' : npcBranch(branchId) ? 'per body' : gearBranch(branchId) ? PER_NPC : 'per box';

/** What a detail group's odds are per: an area's drops as the area; an item's by its places, an NPC group per body, a worn-gear group per NPC. */
export function groupPer(view: LootView, entry: (Pick<LootSummary, 'id' | 'count' | 'npcs'> & { worn?: number }) | null | undefined, groupBy: LootGroupBy, label: string): string {
  if (!entry) return 'per box';
  if (view === 'areas') return perWhat(entry.id);
  if (groupBy === 'place') return label === NPC_KIND ? 'per body' : label === GEAR_KIND ? PER_NPC : 'per box';
  return rowPer(view, entry);
}

/** Where an item is found, places, NPC bodies and NPC types wearing it counted apart: `3 areas`, `2 NPCs`, `3 areas, 1 NPC, worn by 2 NPC types`. */
export function placesText(r: Pick<LootSummary, 'count' | 'npcs'> & { worn?: number }): string {
  const worn = Math.max(0, Math.min(r.worn ?? 0, r.count));
  const npcs = Math.max(0, Math.min(r.npcs ?? 0, r.count - worn));
  const areas = r.count - npcs - worn;
  const plural = (n: number, one: string, many: string) => `${n.toLocaleString('en-US')} ${n === 1 ? one : many}`;
  const parts = [];
  if (areas > 0 || (npcs === 0 && worn === 0)) parts.push(plural(areas, 'area', 'areas'));
  if (npcs > 0) parts.push(plural(npcs, 'NPC', 'NPCs'));
  if (worn > 0) parts.push(`worn by ${plural(worn, 'NPC type', 'NPC types')}`);
  return parts.join(', ');
}

/** The gear slots in body order (`SLOTS` in npc_loadouts.rs): a worn-gear row's groups come in this order. */
export const GEAR_SLOTS = ['Undersuit', 'Helmet', 'Torso', 'Arms', 'Legs', 'Backpack', 'Clothing', 'Primary weapon', 'Sidearm', 'Utility', 'Optics', 'Weapon attachments', 'Magazines', 'Grenades', 'Medical', 'Carried'] as const;
/** A slot's place in body order (unknown ones last). */
export function slotRank(slot: string): number {
  const i = (GEAR_SLOTS as readonly string[]).indexOf(slot);
  return i < 0 ? GEAR_SLOTS.length : i;
}
/** The loadout's `WearRange` as the data gives it (`Wear range 0–0.5`); `Condition not known` when the loadout sets none. */
export function conditionText(w: { min: number; max: number } | null | undefined): string {
  if (!w || !Number.isFinite(w.min) || !Number.isFinite(w.max)) return 'Condition not known';
  const f = (n: number) => String(Math.round(n * 100) / 100);
  const lo = Math.min(w.min, w.max);
  const hi = Math.max(w.min, w.max);
  return lo === hi ? `Wear range ${f(hi)}` : `Wear range ${f(lo)}–${f(hi)}`;
}

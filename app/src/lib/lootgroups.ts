/**
 * The Loot screen's drops in groups: the "Group by" choices per view (remembered per viewer),
 * the rows a windowed grouped table draws (group headers, drops, "N more" and loading rows),
 * which names repeat (their class names then show), and the grouping itself as
 * `LootScreen::detail` does it (the in-browser mock). Plain TS, unit-tested in
 * tests/lootgroups.test.ts.
 */
import type { LootDrop, LootGroup, LootGroupBy, LootView } from './contract/types';

/** Rows kept per group (`MAX_GROUP_ROWS` in loot.rs) and in the one group of an ungrouped detail (`MAX_DROPS`). */
export const MAX_GROUP_ROWS = 500;
export const MAX_DROPS = 2000;
/** Without a filter, the first group opens, then the next while the opened ones hold at most this many drops (`OPEN_BUDGET`). */
export const OPEN_BUDGET = 40;
/** Key of the one group of an ungrouped detail (`ALL_GROUP`). */
export const ALL_GROUP = 'all';
/** Separator of a drop's tree path (`PATH_SEP`). */
export const PATH_SEP = ' › ';

/** The "Group by" choices of a view, the default first. */
export function groupByOptions(view: LootView): { id: LootGroupBy; label: string; hint: string }[] {
  const where = { id: 'container' as const, label: 'Where in it', hint: 'By the spot: weapons rack, lying loose, crate kind' };
  const none = { id: 'none' as const, label: 'None', hint: 'One list, likeliest first' };
  if (view === 'areas') return [where, none];
  return [{ id: 'place', label: 'Place', hint: 'By kind of place: caves, outposts, stations' }, where, none];
}

/** The view's default grouping: areas by container, items by place. */
export const defaultGroupBy = (view: LootView): LootGroupBy => (view === 'areas' ? 'container' : 'place');

/** The storage key of a view's grouping (per viewer, in localStorage). */
export const groupByKey = (view: LootView): string => `starorg.loot.groupby.${view}`;

/** A stored grouping, else the view's default (anything unknown, or not offered in that view). */
export function parseGroupBy(view: LootView, raw: string | null): LootGroupBy {
  return groupByOptions(view).find((o) => o.id === raw)?.id ?? defaultGroupBy(view);
}

/** The view's stored grouping (storage blocked or empty: the default). */
export function loadGroupBy(view: LootView, storage: Pick<Storage, 'getItem'> | null = globalThis.localStorage ?? null): LootGroupBy {
  try {
    return parseGroupBy(view, storage?.getItem(groupByKey(view)) ?? null);
  } catch {
    return defaultGroupBy(view);
  }
}

/** Remember a view's grouping (storage blocked: it still applies for this session). */
export function saveGroupBy(view: LootView, g: LootGroupBy, storage: Pick<Storage, 'setItem'> | null = globalThis.localStorage ?? null): void {
  try {
    storage?.setItem(groupByKey(view), g);
  } catch {
    /* storage blocked */
  }
}

/** Names that more than one row carries (`Standard Container` x7): those rows also show their id. */
export function dupNames(rows: Iterable<Pick<LootDrop, 'name'>>): Set<string> {
  const seen = new Set<string>();
  const dup = new Set<string>();
  for (const r of rows) {
    if (seen.has(r.name)) dup.add(r.name);
    else seen.add(r.name);
  }
  return dup;
}

/** One drawn row of the grouped drops table. */
export type DropRow =
  | { kind: 'head'; id: string; group: LootGroup; open: boolean }
  | { kind: 'drop'; id: string; group: string; drop: LootDrop }
  | { kind: 'more'; id: string; group: LootGroup; shown: number }
  | { kind: 'loading'; id: string; group: string };

/**
 * The rows to draw: each group's header, then, when it is open, its loaded rows (or a loading
 * row while they come) and a "N more" row when the group holds more than were sent. An
 * ungrouped detail (`flat`) draws no header: its one group's rows (its "more" is the footer).
 */
export function dropRows(groups: readonly LootGroup[] | null | undefined, open: ReadonlySet<string>, loaded: Readonly<Record<string, readonly LootDrop[]>>, flat: boolean): DropRow[] {
  const out: DropRow[] = [];
  if (!Array.isArray(groups)) return out;
  for (const g of groups) {
    const on = flat || open.has(g.key);
    if (!flat) out.push({ kind: 'head', id: `h:${g.key}`, group: g, open: on });
    if (!on) continue;
    const rows = loaded[g.key];
    if (!Array.isArray(rows)) {
      out.push({ kind: 'loading', id: `l:${g.key}`, group: g.key });
      continue;
    }
    rows.forEach((d, i) => out.push({ kind: 'drop', id: `d:${g.key}/${i}/${d.id}`, group: g.key, drop: d }));
    if (!flat && g.count > rows.length) out.push({ kind: 'more', id: `m:${g.key}`, group: g, shown: rows.length });
  }
  return out;
}

/** The rows an answer sent, by group key (groups whose rows were not asked for are left out). */
export function loadedRows(groups: readonly LootGroup[] | null | undefined): Record<string, LootDrop[]> {
  const out: Record<string, LootDrop[]> = {};
  if (Array.isArray(groups)) for (const g of groups) if (Array.isArray(g.rows)) out[g.key] = g.rows;
  return out;
}

/** The keys of open groups without rows yet and not loading, sorted (what to ask for next). */
export function missingGroups(open: ReadonlySet<string>, loaded: Readonly<Record<string, unknown>>, loading: ReadonlySet<string>): string[] {
  return [...open].filter((k) => !(k in loaded) && !loading.has(k)).sort();
}

/** Names as `cmp_name` orders them: case-insensitive, then exact. */
function cmpName(a: string, b: string): number {
  const x = a.toLowerCase();
  const y = b.toLowerCase();
  if (x !== y) return x < y ? -1 : 1;
  return a < b ? -1 : a > b ? 1 : 0;
}

/** The groups opened when none are asked for (`default_open` in loot.rs). */
export function defaultOpen(counts: readonly number[], cap: number, filtered: boolean): boolean[] {
  const budget = filtered ? MAX_DROPS : OPEN_BUDGET;
  let used = 0;
  let stop = false;
  return counts.map((c, i) => {
    const n = Math.min(c, cap);
    if (stop || (i > 0 && used + n > budget)) {
      stop = true;
      return false;
    }
    used += n;
    return true;
  });
}

/** A drop's group key: its container, the top of its tree path, or the one group. */
export function groupKeyOf(d: Pick<LootDrop, 'container' | 'group'>, by: LootGroupBy): string {
  if (by === 'container') return d.container;
  if (by === 'place') return d.group.split(PATH_SEP)[0] ?? d.group;
  return ALL_GROUP;
}

/**
 * Likeliest-first drops in groups as `LootScreen::detail` makes them: best odds first, then by
 * name; rows for the `groups` asked for, else the default ones; each capped.
 */
export function groupDrops(rows: readonly LootDrop[], by: LootGroupBy, groups: readonly string[] | null, filtered: boolean): LootGroup[] {
  const flat = by === 'none';
  const at = new Map<string, LootDrop[]>();
  for (const d of rows) {
    const k = groupKeyOf(d, by);
    const list = at.get(k);
    if (list) list.push(d);
    else at.set(k, [d]);
  }
  const acc = [...at.entries()].map(([key, list]) => ({ key, list, best: list[0]?.odds ?? 0 }));
  acc.sort((a, b) => b.best - a.best || cmpName(a.key, b.key));
  const cap = flat ? MAX_DROPS : MAX_GROUP_ROWS;
  const open = groups ? acc.map((g) => groups.includes(g.key)) : defaultOpen(acc.map((g) => g.list.length), cap, filtered);
  return acc.map((g, i) => ({
    key: g.key,
    label: flat ? 'All drops' : g.key,
    count: g.list.length,
    best: g.best,
    rows: open[i] ? g.list.slice(0, cap) : null,
  }));
}

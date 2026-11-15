/**
 * Datasets and Diff group trees (addendum 7.15): the visible items, keyboard moves and the
 * remembered group-by. The backend sends only the visible nodes (roots and the children of
 * open nodes, depth first); this adds the "All" item and the keyboard model of the Strings
 * tree (`grouptree.ts`), single selection. Pure, so `npm test` covers it without a browser.
 */
import type { DataGroup, DiffCounts, DiffGroupBy, GroupLevel, GroupPreset } from './contract/dev';

/** The "All" item at the top of the tree. */
export const ALL = '';
/** The group-by id that turns the tree off. */
export const NONE = 'none';

export interface DataTreeItem {
  id: string;
  label: string;
  /** 1 = top level ("All" too). */
  depth: number;
  parent: string | null;
  hasChildren: boolean;
  expanded: boolean;
  count: number;
  /** Children not listed under this open node. */
  more: number;
  diff: DiffCounts | null;
}

/** "All" then the nodes as sent. */
export function dataItems(nodes: readonly DataGroup[], open: ReadonlySet<string>, total: number, allLabel = 'All'): DataTreeItem[] {
  const out: DataTreeItem[] = [{ id: ALL, label: allLabel, depth: 1, parent: null, hasChildren: false, expanded: false, count: total, more: 0, diff: null }];
  for (const n of Array.isArray(nodes) ? nodes : []) {
    out.push({ id: n.id, label: n.label, depth: n.depth, parent: n.parent, hasChildren: n.children > 0, expanded: n.children > 0 && open.has(n.id), count: n.count, more: n.more, diff: n.diff });
  }
  return out;
}

export type DataTreeAction = { kind: 'focus'; id: string } | { kind: 'expand'; id: string } | { kind: 'collapse'; id: string } | { kind: 'select'; id: string } | { kind: 'clear' } | { kind: 'none' };

/** The action for a key on the focused item; `typed` is the typeahead buffer with this key. */
export function dataKeyAction(items: readonly DataTreeItem[], focused: string, key: string, typed = ''): DataTreeAction {
  const i = Math.max(0, items.findIndex((x) => x.id === focused));
  const cur = items[i];
  if (!cur) return { kind: 'none' };
  switch (key) {
    case 'ArrowDown':
      return { kind: 'focus', id: items[Math.min(items.length - 1, i + 1)]!.id };
    case 'ArrowUp':
      return { kind: 'focus', id: items[Math.max(0, i - 1)]!.id };
    case 'Home':
      return { kind: 'focus', id: items[0]!.id };
    case 'End':
      return { kind: 'focus', id: items[items.length - 1]!.id };
    case 'ArrowRight':
      if (cur.hasChildren && !cur.expanded) return { kind: 'expand', id: cur.id };
      if (cur.expanded && items[i + 1]?.parent === cur.id) return { kind: 'focus', id: items[i + 1]!.id };
      return { kind: 'none' };
    case 'ArrowLeft':
      if (cur.expanded) return { kind: 'collapse', id: cur.id };
      if (cur.parent !== null) return { kind: 'focus', id: cur.parent };
      return { kind: 'none' };
    case 'Enter':
    case ' ':
      return cur.id === ALL ? { kind: 'clear' } : { kind: 'select', id: cur.id };
    case 'Escape':
      return { kind: 'clear' };
  }
  if (key.length === 1 && typed) {
    const t = typed.toLowerCase();
    const start = typed.length > 1 ? 0 : 1;
    for (let k = start; k < items.length + start; k++) {
      const x = items[(i + k) % items.length]!;
      if (x.id !== ALL && x.label.toLowerCase().startsWith(t)) return { kind: 'focus', id: x.id };
    }
  }
  return { kind: 'none' };
}

/** Open ids after collapsing one: its open descendants close too. */
export function collapse(open: readonly string[], id: string): string[] {
  const p = keys(id);
  return open.filter((o) => {
    if (o === id) return false;
    const q = keys(o);
    return !(p && q && q.length > p.length && p.every((k, i) => q[i] === k));
  });
}

/** The key path of a node id (the JSON array of keys from the root), or null. */
export function keys(id: string): (string | null)[] | null {
  try {
    const v = JSON.parse(id) as unknown;
    return Array.isArray(v) && v.every((x) => x === null || typeof x === 'string') ? (v as (string | null)[]) : null;
  } catch {
    return null;
  }
}

/** Accessible name: label and counts. */
export function dataItemName(x: DataTreeItem, unit = 'rows'): string {
  // "1 row", "1 record": the unit is a plural noun ending in s.
  const one = x.count === 1 && unit.endsWith('s') ? unit.slice(0, -1) : unit;
  const parts = [x.label, `${x.count.toLocaleString('en-US')} ${one}`];
  if (x.diff) {
    if (x.diff.added) parts.push(`${x.diff.added.toLocaleString('en-US')} added`);
    if (x.diff.removed) parts.push(`${x.diff.removed.toLocaleString('en-US')} removed`);
    if (x.diff.changed) parts.push(`${x.diff.changed.toLocaleString('en-US')} changed`);
  }
  return parts.join(', ');
}

/** Storage key of a table's group-by (per viewer; the same table in every source). */
export const groupByKey = (dataset: string, table: string) => `starorg.dev.groupby.${dataset}|${table}`;

/** The remembered group-by of a table: a preset id it still offers, "none", else the default. */
export function pickGroupBy(raw: string | null, presets: readonly GroupPreset[]): string {
  if (raw === NONE) return NONE;
  if (raw && presets.some((p) => p.id === raw)) return raw;
  return presets[0]?.id ?? NONE;
}

/** The levels of a group-by id. */
export function levelsOf(id: string, presets: readonly GroupPreset[]): GroupLevel[] {
  return presets.find((p) => p.id === id)?.levels.map((l) => ({ ...l })) ?? [];
}

/** The remembered Diff group-by. */
export function pickDiffBy(raw: string | null): DiffGroupBy {
  return raw === 'tables' || raw === 'folders' ? raw : 'auto';
}

/** Read and write per-viewer storage; blocked storage reads as empty. */
export function readPref(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}
export function writePref(key: string, v: string) {
  try {
    localStorage.setItem(key, v);
  } catch {
    /* storage blocked */
  }
}

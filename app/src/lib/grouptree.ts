/**
 * Strings group tree (addendum 8.5): which rows are visible, keyboard moves and selection.
 * Pure functions so `npm test` covers them without a browser; `GroupTree.svelte` renders them.
 */
import type { GroupBy, GroupNode } from './contract/types';

/** The "All groups" item at the top of every tree. */
export const ALL = '';

export interface TreeItem {
  id: string;
  label: string;
  level: 1 | 2;
  parent: string | null;
  hasChildren: boolean;
  expanded: boolean;
  count: number;
  enhanced: number;
  edited: number;
}

/** Visible items in order: "All", then each top-level node, then its children when expanded. */
export function visibleItems(nodes: GroupNode[], expanded: ReadonlySet<string>, total: number, allLabel = 'All groups'): TreeItem[] {
  const kids = new Map<string, GroupNode[]>();
  for (const n of nodes) if (n.parent) kids.set(n.parent, [...(kids.get(n.parent) ?? []), n]);
  const out: TreeItem[] = [{ id: ALL, label: allLabel, level: 1, parent: null, hasChildren: false, expanded: false, count: total, enhanced: 0, edited: 0 }];
  for (const n of nodes) {
    if (n.parent) continue;
    const ch = kids.get(n.id) ?? [];
    const open = ch.length > 0 && expanded.has(n.id);
    out.push({ ...n, level: 1, parent: null, hasChildren: ch.length > 0, expanded: open });
    if (open) for (const c of ch) out.push({ ...c, level: 2, parent: n.id, hasChildren: false, expanded: false });
  }
  return out;
}

export type TreeAction =
  | { kind: 'focus'; id: string }
  | { kind: 'expand'; id: string }
  | { kind: 'collapse'; id: string }
  | { kind: 'select-only'; id: string }
  | { kind: 'toggle'; id: string }
  | { kind: 'clear' }
  | { kind: 'none' };

/**
 * The action for a key on the focused item. `typed` is the typeahead buffer including this
 * key (the component resets it after a pause).
 */
export function keyAction(items: TreeItem[], focused: string, key: string, ctrl: boolean, typed = ''): TreeAction {
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
      if (cur.hasChildren && cur.expanded) return { kind: 'focus', id: items[i + 1]!.id };
      return { kind: 'none' };
    case 'ArrowLeft':
      if (cur.hasChildren && cur.expanded) return { kind: 'collapse', id: cur.id };
      if (cur.parent) return { kind: 'focus', id: cur.parent };
      return { kind: 'none' };
    case 'Enter':
    case ' ':
      if (cur.id === ALL) return { kind: 'clear' };
      return ctrl ? { kind: 'toggle', id: cur.id } : { kind: 'select-only', id: cur.id };
    case 'Escape':
      return { kind: 'clear' };
  }
  if (key.length === 1 && !ctrl && typed) {
    // One letter: the next match after the current item. More letters: keep the current item
    // while it still matches. Wraps; "All groups" is skipped.
    const t = typed.toLowerCase();
    const start = typed.length > 1 ? 0 : 1;
    for (let k = start; k < items.length + start; k++) {
      const x = items[(i + k) % items.length]!;
      if (x.id !== ALL && x.label.toLowerCase().startsWith(t)) return { kind: 'focus', id: x.id };
    }
  }
  return { kind: 'none' };
}

/** Selection after an action (ids of the tree's dimension). */
export function applySelection(selected: readonly string[], a: TreeAction): string[] {
  switch (a.kind) {
    case 'select-only':
      return selected.length === 1 && selected[0] === a.id ? [] : [a.id];
    case 'toggle':
      return selected.includes(a.id) ? selected.filter((x) => x !== a.id) : [...selected, a.id];
    case 'clear':
      return [];
    default:
      return [...selected];
  }
}

/** Whether an item shows as selected: itself, or its parent is selected. */
export function isSelected(item: Pick<TreeItem, 'id' | 'parent'>, selected: readonly string[]): boolean {
  if (item.id === ALL) return selected.length === 0;
  return selected.includes(item.id) || (item.parent !== null && selected.includes(item.parent));
}

/** Accessible name of a tree item, with its counts. */
export function itemName(x: TreeItem): string {
  const parts = [x.label, `${x.count.toLocaleString('en-US')} keys`];
  if (x.enhanced) parts.push(`${x.enhanced.toLocaleString('en-US')} enhanced`);
  if (x.edited) parts.push(`${x.edited.toLocaleString('en-US')} edited`);
  return parts.join(', ');
}

/** The remembered state of the group UI for one channel. */
export interface GroupPrefs {
  groupBy: GroupBy;
  groups: string[];
  manufacturers: string[];
  sizes: number[];
  open: string[];
}

export const DEFAULT_PREFS: GroupPrefs = { groupBy: 'type', groups: [], manufacturers: [], sizes: [], open: [] };

/** Parse stored prefs, dropping anything malformed (storage is per viewer and may be stale). */
export function parsePrefs(raw: string | null): GroupPrefs {
  try {
    const v = JSON.parse(raw ?? 'null') as Partial<GroupPrefs> | null;
    if (!v || typeof v !== 'object') return { ...DEFAULT_PREFS };
    const strs = (x: unknown) => (Array.isArray(x) ? x.filter((s): s is string => typeof s === 'string') : []);
    const by = ['none', 'type', 'manufacturer', 'size'].includes(v.groupBy as string) ? (v.groupBy as GroupBy) : 'type';
    return {
      groupBy: by,
      groups: strs(v.groups),
      manufacturers: strs(v.manufacturers),
      sizes: Array.isArray(v.sizes) ? v.sizes.filter((n): n is number => Number.isInteger(n) && n >= 0 && n <= 12) : [],
      open: strs(v.open),
    };
  } catch {
    return { ...DEFAULT_PREFS };
  }
}

/** A removable filter chip. */
export interface Chip {
  dim: 'groups' | 'manufacturers' | 'sizes';
  id: string;
  label: string;
}

/**
 * Chips for every selected group, manufacturer and size. Labels come from the tree nodes when
 * that dimension is shown, else from `known` (labels remembered from earlier pages).
 */
export function chips(p: GroupPrefs, known: ReadonlyMap<string, string>): Chip[] {
  const label = (dim: string, id: string, fallback: string) => known.get(`${dim}:${id}`) ?? fallback;
  return [
    ...p.groups.map((id) => ({ dim: 'groups' as const, id, label: label('groups', id, idLabel(id)) })),
    ...p.manufacturers.map((id) => ({ dim: 'manufacturers' as const, id, label: label('manufacturers', id, id === 'none' ? 'No manufacturer' : id) })),
    ...p.sizes.map((n) => ({ dim: 'sizes' as const, id: String(n), label: n === 0 ? 'No size' : `Size ${n}` })),
  ];
}

/** A readable label from a group id when the tree has not shown it yet: `components/power-plants` -> "Power plants". */
export function idLabel(id: string): string {
  const last = (id.split('/').pop() ?? id).replace(/-/g, ' ');
  return last.charAt(0).toUpperCase() + last.slice(1);
}

/** Compact count for the small tree badges: 1,251 -> "1.3k" (the accessible name keeps the exact number). */
export function compact(n: number): string {
  if (n < 1000) return String(n);
  if (n < 10_000) return `${(n / 1000).toFixed(1).replace(/\.0$/, '')}k`;
  return `${Math.round(n / 1000)}k`;
}

/** Map a tree node id of a flat mode onto the query value. */
export function sizeOf(id: string): number {
  return id === 'none' ? 0 : Number(id);
}
export function sizeId(n: number): string {
  return n === 0 ? 'none' : String(n);
}

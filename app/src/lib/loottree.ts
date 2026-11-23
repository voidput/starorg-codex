/**
 * The Loot screen's trees: built client-side from the list rows' `path` (kind of place or
 * category -> a middle level -> the row), flattened to the rows a windowed list draws, with
 * the search, the category filter and the per-viewer expanded state. Plain TS, unit-tested
 * in tests/loottree.test.ts; keyboard moves come from `keyAction` in grouptree.ts.
 */
import type { LootSummary, LootView } from './contract/types';
import { filterSummaries, queryWords } from './loot.ts';

export interface LootBranch {
  kind: 'branch';
  /** `p:` and the labels of the path down to it. */
  id: string;
  label: string;
  children: LootNode[];
  /** Rows below it, all levels. */
  count: number;
  /** Best rough odds of the rows below it. */
  best: number;
}

export interface LootLeaf {
  kind: 'leaf';
  /** `r:` and the row's id. */
  id: string;
  label: string;
  row: LootSummary;
  /** Another row under the same parent has the same name (its id then shows beside it). */
  clash: boolean;
}

export type LootNode = LootBranch | LootLeaf;

/** Id of the branch at a path (labels top first). */
export const branchId = (path: readonly string[]): string => `p:${path.join('\u001f')}`;
/** Id of a row's leaf. */
export const leafId = (rowId: string): string => `r:${rowId}`;

/** The general rows of a level first (`General`, `General (v3)`), then by name. */
function leafOrder(a: string, b: string): number {
  return Number(!a.startsWith('General')) - Number(!b.startsWith('General')) || a.localeCompare(b);
}

function sortLevel(nodes: LootNode[], top: boolean): void {
  nodes.sort((a, b) => {
    if (a.kind !== b.kind) return a.kind === 'branch' ? -1 : 1;
    if (a.kind === 'branch') {
      // `Other` last among the kinds of place and categories.
      if (top) {
        const d = Number(a.label === 'Other') - Number(b.label === 'Other');
        if (d) return d;
      }
      return a.label.localeCompare(b.label);
    }
    return leafOrder(a.label, b.label);
  });
  const seen = new Map<string, number>();
  for (const n of nodes) if (n.kind === 'leaf') seen.set(n.label, (seen.get(n.label) ?? 0) + 1);
  for (const n of nodes) {
    if (n.kind === 'branch') sortLevel(n.children, false);
    else n.clash = (seen.get(n.label) ?? 0) > 1;
  }
}

/** The tree of the rows: branches first then rows at each level, by name (`Other` last); same-named sibling rows are marked (`clash`). */
export function buildTree(rows: readonly LootSummary[] | null | undefined): LootNode[] {
  const root: LootNode[] = [];
  if (!Array.isArray(rows)) return root;
  const branches = new Map<string, LootBranch>();
  for (const row of rows) {
    let level = root;
    const path = Array.isArray(row.path) ? row.path : [];
    const chain: LootBranch[] = [];
    for (let d = 0; d < path.length; d++) {
      const id = branchId(path.slice(0, d + 1));
      let b = branches.get(id);
      if (!b) {
        b = { kind: 'branch', id, label: path[d]!, children: [], count: 0, best: 0 };
        branches.set(id, b);
        level.push(b);
      }
      chain.push(b);
      level = b.children;
    }
    for (const b of chain) {
      b.count++;
      if (row.best > b.best) b.best = row.best;
    }
    level.push({ kind: 'leaf', id: leafId(row.id), label: row.name, row, clash: false });
  }
  sortLevel(root, true);
  return root;
}

/** One drawn row of a tree (the `aria-*` of a flattened, windowed tree). */
export interface TreeRow {
  id: string;
  label: string;
  /** 1 at the top. */
  level: number;
  parent: string | null;
  hasChildren: boolean;
  expanded: boolean;
  /** Position among its siblings, from 1, and how many there are. */
  pos: number;
  size: number;
  node: LootNode;
}

/**
 * The visible rows in order: every top node, and the children of each expanded branch
 * (`'all'`: every branch but those in `closed`).
 */
export function flatten(nodes: readonly LootNode[], expanded: ReadonlySet<string> | 'all', closed: ReadonlySet<string> = new Set()): TreeRow[] {
  const out: TreeRow[] = [];
  const walk = (level: readonly LootNode[], depth: number, parent: string | null) => {
    level.forEach((n, i) => {
      const branch = n.kind === 'branch';
      const open = branch && (expanded === 'all' ? !closed.has(n.id) : expanded.has(n.id));
      out.push({ id: n.id, label: n.label, level: depth, parent, hasChildren: branch, expanded: open, pos: i + 1, size: level.length, node: n });
      if (open) walk(n.children, depth + 1, n.id);
    });
  };
  walk(nodes, 1, null);
  return out;
}

/** Whether a query searches (holds a word): the tree then opens every branch. */
export const searching = (query: string): boolean => queryWords(query).length > 0;

/**
 * What the list draws: the matching rows as a tree, collapsed but for `expanded`; while
 * searching every branch is open (a match never hides in a closed branch) but those closed
 * during this search (`closedInSearch`). `matched`: rows that pass the search and filter.
 */
export function visibleTree(
  rows: readonly LootSummary[] | null | undefined,
  query: string,
  group: string,
  expanded: ReadonlySet<string>,
  closedInSearch: ReadonlySet<string> = new Set(),
): { rows: TreeRow[]; matched: number } {
  const hits = filterSummaries(rows, query, group);
  const tree = buildTree(hits);
  return { rows: searching(query) ? flatten(tree, 'all', closedInSearch) : flatten(tree, expanded), matched: hits.length };
}

/** The branches above a row (top first), to open so it shows. */
export function ancestorsOf(row: Pick<LootSummary, 'path'>): string[] {
  const path = Array.isArray(row.path) ? row.path : [];
  return path.map((_, d) => branchId(path.slice(0, d + 1)));
}

/** Where a row sits, as a line: `Caves › Occupied`, else its group. */
export function whereText(row: Pick<LootSummary, 'path' | 'group'>): string {
  return Array.isArray(row.path) && row.path.length ? row.path.join(' › ') : row.group;
}

/** The storage key of a view's expanded branches (per viewer, in localStorage). */
export const openKey = (view: LootView): string => `starorg.loot.open.${view}`;

/** Parse stored expanded branch ids, dropping anything malformed. */
export function parseOpen(raw: string | null): string[] {
  try {
    const v = JSON.parse(raw ?? '[]') as unknown;
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string' && x.startsWith('p:')).slice(0, 500) : [];
  } catch {
    return [];
  }
}

/** The expanded set with `id` opened or closed. */
export function toggled(open: ReadonlySet<string>, id: string, on: boolean): Set<string> {
  const next = new Set(open);
  if (on) next.add(id);
  else next.delete(id);
  return next;
}

/**
 * Explorer navigation helpers (addendum 7.4, 7.8), pure: the back/forward history of visited
 * records, and the lazily loaded type/path tree's visible rows and keys.
 */
import type { TreeNode } from './contract/dev';

export const HISTORY = 50;

/** Back/forward stack of visited records (GUIDs), 50 entries, per session. */
export class History {
  back: string[] = [];
  forward: string[] = [];
  current: string | null = null;

  /** A new visit: the forward stack is dropped (as in a browser). */
  visit(x: string): void {
    if (x === this.current) return;
    if (this.current !== null) this.back.push(this.current);
    if (this.back.length > HISTORY - 1) this.back.splice(0, this.back.length - (HISTORY - 1));
    this.forward = [];
    this.current = x;
  }
  goBack(): string | null {
    const x = this.back.pop();
    if (x === undefined) return null;
    if (this.current !== null) this.forward.push(this.current);
    this.current = x;
    return x;
  }
  goForward(): string | null {
    const x = this.forward.pop();
    if (x === undefined) return null;
    if (this.current !== null) this.back.push(this.current);
    this.current = x;
    return x;
  }
  get canBack() {
    return this.back.length > 0;
  }
  get canForward() {
    return this.forward.length > 0;
  }
}

/** Children of a node as loaded so far: `total` may exceed `nodes.length` ("Show 500 more"). */
export interface Loaded {
  nodes: TreeNode[];
  total: number;
}

export interface TreeRow {
  /** Node id, or `more:<parent>` for the "Show more" row. */
  id: string;
  node: TreeNode | null;
  level: number;
  parent: string;
  expanded: boolean;
  /** The "Show more" row: how many children are not loaded yet. */
  more: number;
  /** Position among siblings (1-based) and their count, for aria-posinset/setsize. */
  pos: number;
  size: number;
}

const expandable = (n: TreeNode) => n.kind !== 'record';

/** Rows shown for the root (`''`) and every open node whose children are loaded. */
export function treeRows(loaded: Map<string, Loaded>, open: Set<string>): TreeRow[] {
  const out: TreeRow[] = [];
  const go = (parent: string, level: number) => {
    const l = loaded.get(parent);
    if (!l) return;
    const size = l.total;
    l.nodes.forEach((n, i) => {
      const exp = expandable(n) && open.has(n.id);
      out.push({ id: n.id, node: n, level, parent, expanded: exp, more: 0, pos: i + 1, size });
      if (exp) go(n.id, level + 1);
    });
    if (l.total > l.nodes.length) out.push({ id: `more:${parent}`, node: null, level, parent, expanded: false, more: l.total - l.nodes.length, pos: l.nodes.length + 1, size });
  };
  go('', 1);
  return out;
}

export type TreeAction =
  | { kind: 'focus'; id: string }
  | { kind: 'open'; id: string }
  | { kind: 'close'; id: string }
  | { kind: 'activate'; id: string }
  | { kind: 'more'; parent: string }
  | null;

/** Tree keys (7.8): Up/Down, Right opens or goes to the first child, Left closes or goes to the parent, Home/End, Enter opens. */
export function treeKey(rows: TreeRow[], current: string, key: string): TreeAction {
  const i = rows.findIndex((r) => r.id === current);
  if (i < 0) return rows[0] ? { kind: 'focus', id: rows[0].id } : null;
  const r = rows[i]!;
  switch (key) {
    case 'ArrowDown':
      return rows[i + 1] ? { kind: 'focus', id: rows[i + 1]!.id } : null;
    case 'ArrowUp':
      return i > 0 ? { kind: 'focus', id: rows[i - 1]!.id } : null;
    case 'Home':
      return { kind: 'focus', id: rows[0]!.id };
    case 'End':
      return { kind: 'focus', id: rows[rows.length - 1]!.id };
    case 'ArrowRight':
      if (!r.node || !expandable(r.node)) return null;
      if (!r.expanded) return { kind: 'open', id: r.id };
      return rows[i + 1] && rows[i + 1]!.parent === r.id ? { kind: 'focus', id: rows[i + 1]!.id } : null;
    case 'ArrowLeft':
      if (r.node && r.expanded) return { kind: 'close', id: r.id };
      return r.parent ? { kind: 'focus', id: r.parent } : null;
    case 'Enter':
    case ' ':
      if (!r.node) return { kind: 'more', parent: r.parent };
      return expandable(r.node) ? (r.expanded ? { kind: 'close', id: r.id } : { kind: 'open', id: r.id }) : { kind: 'activate', id: r.id };
    default:
      return null;
  }
}

/** Typeahead: the next row after `current` whose label starts with `prefix`. */
export function typeahead(rows: TreeRow[], current: string, prefix: string): string | null {
  const p = prefix.toLowerCase();
  if (!p) return null;
  const i = rows.findIndex((r) => r.id === current);
  for (let k = 1; k <= rows.length; k++) {
    const r = rows[(i + k + rows.length) % rows.length]!;
    if (r.node && r.node.label.toLowerCase().startsWith(p)) return r.id;
  }
  return null;
}

/** `r:<guid>` -> guid. */
export const guidOf = (id: string) => (id.startsWith('r:') ? id.slice(2) : null);

export const isGuidLike = (s: string) => /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(s.trim());

/** Human size: 1.5 MB, 812 KB, 440 B. */
export function bytes(n: number): string {
  if (n >= 1e9) return `${(n / 1e9).toFixed(1)} GB`;
  if (n >= 1e6) return `${(n / 1e6).toFixed(1)} MB`;
  if (n >= 1e3) return `${Math.round(n / 1e3)} KB`;
  return `${n} B`;
}

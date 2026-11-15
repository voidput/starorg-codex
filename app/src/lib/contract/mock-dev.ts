/**
 * Developer mode in the in-browser mock: the same behaviour as the Rust mock
 * (app/src-tauri/src/contract/mock_dev.rs) over the same synthetic data (`mock-dev.gen.json`,
 * written by the Rust contract tests). The explorer index, record cutting, shape inference,
 * rows and diff are ports of `starorg_core::dev`; `tests/devmock.test.ts` checks the answers
 * against the Rust samples. No JSON import here, so node can load it in tests.
 */
import type * as T from './types';
import type * as D from './dev';

type Json = null | boolean | number | string | Json[] | { [k: string]: Json };
type Obj = { [k: string]: Json };

export interface DevMockData {
  entries: { guid: string; name: string; type: string; path: string; main: boolean; tag: string | null }[];
  values: Json[];
  huge: { guid: string; ports: number };
  builds: Record<string, Json>;
  /** The records the labels know (7.14), manufacturer names by code, enhanced text by key. */
  known: KnownRecord[];
  makers: Record<string, string>;
  enhanced: Record<string, string>;
  folder: { label: string; path: string; files: { file: string; value: Json }[] };
}

export interface DevMockCtx {
  devOn(): boolean;
  dumpsOn(): boolean;
  hasChannel(r: T.ChannelRef): boolean;
  channelNames(install: string): string[];
  /** Stock text of a key (case-insensitive), or null. */
  stockText(key: string): string | null;
  emit(p: T.JobProgress): void;
  sleep(ms: number): Promise<void>;
  delay: number;
  nextJob(): string;
  fail(code: T.ErrorCode, message: string): never;
}

const RECORDS_ROOT = 'libs/foundry/records/';
const MAX_PAGE = 500;
const MAX_REPLY = 1 << 20;
const CUT_DEPTH = 3;
const MAX_LOC = 500;
const MAX_CELL = 200;
const MAX_DISTINCT = 50;
const DEFAULT_COLUMNS = 8;
const LEADING = ['name', 'recordName', 'id'];
const IDENTITY = ['id', 'recordName', 'className', 'name', 'key', 'code'];
const GAMEDATA = 'gamedata.json';

/** Byte-order comparison, as Rust's `str::cmp` (ASCII data). */
const cmp = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);
const isObj = (v: unknown): v is Obj => typeof v === 'object' && v !== null && !Array.isArray(v);
const stringify = (v: Json) => JSON.stringify(v);

// ------------------------------------------------------------------------------------------
// Explorer index (starorg_core::dev::explorer)
// ------------------------------------------------------------------------------------------

export function relativePath(path: string): string {
  const p = path.replace(/^[/\\]+/, '');
  return p.slice(0, RECORDS_ROOT.length).toLowerCase() === RECORDS_ROOT ? p.slice(RECORDS_ROOT.length) : p;
}
const norm = (s: string) => s.replace(/\\/g, '/').toLowerCase();
export const guidLike = (s: string) => s.length >= 4 && /^[0-9a-fA-F-]+$/.test(s);
export const isGuid = (s: string) => /^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$/.test(s);
function subsequence(needle: string, hay: string): boolean {
  let i = 0;
  for (const c of needle) {
    const j = hay.indexOf(c, i);
    if (j < 0) return false;
    i = j + 1;
  }
  return true;
}

export class Index {
  readonly entries: DevMockData['entries'];
  private lname: string[];
  private lrel: string[];
  private byGuidMap = new Map<string, number>();
  private byType = new Map<string, number[]>();
  private byPath: number[];

  constructor(entries: DevMockData['entries']) {
    this.entries = entries;
    this.lname = entries.map((e) => e.name.toLowerCase());
    this.lrel = entries.map((e) => norm(relativePath(e.path)));
    entries.forEach((e, i) => {
      this.byGuidMap.set(e.guid.toLowerCase(), i);
      const v = this.byType.get(e.type) ?? [];
      v.push(i);
      this.byType.set(e.type, v);
    });
    for (const v of this.byType.values()) v.sort((a, b) => cmp(this.lname[a]!, this.lname[b]!) || a - b);
    this.byPath = entries.map((_, i) => i).sort((a, b) => cmp(this.lrel[a]!, this.lrel[b]!) || cmp(this.lname[a]!, this.lname[b]!) || a - b);
  }

  get length() {
    return this.entries.length;
  }
  mainCount() {
    return this.entries.filter((e) => e.main).length;
  }
  types(): D.TypeCount[] {
    return [...this.byType.keys()].sort(cmp).map((name) => {
      const v = this.byType.get(name)!;
      return { name, records: v.length, main: v.filter((i) => this.entries[i]!.main).length };
    });
  }
  ofType(name: string, mainOnly: boolean): number[] {
    return (this.byType.get(name) ?? []).filter((i) => !mainOnly || this.entries[i]!.main);
  }
  byGuid(g: string): number | null {
    return this.byGuidMap.get(g.trim().toLowerCase()) ?? null;
  }
  findName(name: string): number | null {
    const ln = name.trim().toLowerCase();
    if (!ln) return null;
    let hit: number | null = null;
    for (let i = 0; i < this.lname.length; i++) {
      if (this.lname[i] === ln) {
        if (this.entries[i]!.main) return i;
        hit ??= i;
      }
    }
    return hit;
  }
  find(what: string): number | null {
    const w = what.trim();
    if (!w) return null;
    const g = this.byGuid(w);
    if (g !== null) return g;
    const strip = (s: string) => s.replace(/\.(xml|json)$/, '');
    const lw = norm(relativePath(w));
    const best = (pred: (i: number) => boolean) => {
      let hit: number | null = null;
      for (let i = 0; i < this.entries.length; i++) {
        if (pred(i)) {
          if (this.entries[i]!.main) return i;
          hit ??= i;
        }
      }
      return hit;
    };
    if (lw.includes('/')) {
      const stem = strip(lw);
      const i = best((i) => strip(this.lrel[i]!) === stem);
      if (i !== null) return i;
    }
    const ln = w.toLowerCase();
    return best((i) => this.lname[i] === ln);
  }
  private recordNode(i: number): D.TreeNode {
    const e = this.entries[i]!;
    return { id: `r:${e.guid}`, label: e.name, kind: 'record', count: 0, main: e.main };
  }
  tree(root: D.TreeRoot, node: string, offset: number, limit: number, mainOnly: boolean): D.TreePage {
    const all = this.children(root, node, mainOnly);
    return { nodes: all.slice(offset, offset + Math.min(limit, MAX_PAGE)), offset, total: all.length };
  }
  private children(root: D.TreeRoot, node: string, mainOnly: boolean): D.TreeNode[] {
    if (root === 'type') {
      if (!node) {
        return this.types()
          .map((t) => [t.name, mainOnly ? t.main : t.records] as const)
          .filter(([, n]) => n > 0)
          .map(([name, count]) => ({ id: `t:${name}`, label: name, kind: 'type' as const, count, main: false }));
      }
      if (!node.startsWith('t:')) return [];
      return this.ofType(node.slice(2), mainOnly).map((i) => this.recordNode(i));
    }
    let prefix = '';
    if (node) {
      if (!node.startsWith('p:')) return [];
      prefix = node.slice(2).toLowerCase();
    }
    const folders: [string, string, number][] = [];
    const records: number[] = [];
    for (const i of this.byPath) {
      const l = this.lrel[i]!;
      if (!l.startsWith(prefix)) continue;
      if (mainOnly && !this.entries[i]!.main) continue;
      const rest = l.slice(prefix.length);
      const cut = rest.indexOf('/');
      if (cut >= 0) {
        const seg = rest.slice(0, cut);
        const last = folders.at(-1);
        if (last && last[0] === seg) last[2]++;
        else {
          const orig = relativePath(this.entries[i]!.path).replace(/\\/g, '/');
          folders.push([seg, orig.slice(prefix.length, prefix.length + cut) || seg, 1]);
        }
      } else records.push(i);
    }
    records.sort((a, b) => cmp(this.lname[a]!, this.lname[b]!) || a - b);
    return [
      ...folders.map(([lower, label, count]) => ({ id: `p:${prefix}${lower}/`, label, kind: 'folder' as const, count, main: false })),
      ...records.map((i) => this.recordNode(i)),
    ];
  }
  search(query: string, limit: number): D.SearchPage {
    let typeFilter: string | null = null;
    const tokens: string[] = [];
    for (const t of query.split(/\s+/).filter(Boolean)) {
      if (t.slice(0, 5).toLowerCase() === 'type:') typeFilter = t.slice(5).toLowerCase();
      else tokens.push(t.toLowerCase());
    }
    const joined = tokens.join(' ');
    const compact = tokens.join('');
    const guidQ = tokens.length === 1 && guidLike(tokens[0]!) ? tokens[0]! : null;
    const ranked: [number, number][] = [];
    this.entries.forEach((e, i) => {
      if (typeFilter !== null && typeFilter !== '' && e.type.toLowerCase() !== typeFilter) return;
      if (!tokens.length) {
        if (typeFilter !== null) ranked.push([5, i]);
        return;
      }
      const name = this.lname[i]!;
      if (guidQ !== null && e.guid.startsWith(guidQ)) {
        ranked.push([0, i]);
        return;
      }
      let rank: number;
      if (name === joined) rank = 1;
      else if (name.startsWith(joined)) rank = 2;
      else if (name.includes(joined)) rank = 3;
      else if (tokens.every((t) => name.includes(t) || this.lrel[i]!.includes(t))) rank = 4;
      else if (compact.length >= 3 && subsequence(compact, name)) rank = 5;
      else return;
      ranked.push([rank, i]);
    });
    ranked.sort((a, b) => {
      const ea = this.entries[a[1]]!;
      const eb = this.entries[b[1]]!;
      return a[0] - b[0] || Number(eb.main) - Number(ea.main) || this.lname[a[1]]!.length - this.lname[b[1]]!.length || cmp(this.lname[a[1]]!, this.lname[b[1]]!) || a[1] - b[1];
    });
    return {
      hits: ranked.slice(0, Math.min(limit, MAX_PAGE)).map(([, i]) => {
        const e = this.entries[i]!;
        return { guid: e.guid, name: e.name, type: e.type, path: e.path, main: e.main };
      }),
      total: ranked.length,
    };
  }
}

// ------------------------------------------------------------------------------------------
// Record replies (starorg_core::dev::record)
// ------------------------------------------------------------------------------------------

const isRef = (v: Json) => isObj(v) && typeof v['_ref'] === 'string';

export function cut(v: Json, depth: number): [Json, boolean] {
  let any = false;
  const go = (v: Json, d: number): Json => {
    if (isObj(v) && isRef(v)) return { ...v };
    if (isObj(v) && d === 0 && Object.keys(v).length) {
      any = true;
      return { _cut: { kind: 'object', len: Object.keys(v).length } };
    }
    if (Array.isArray(v) && d === 0 && v.length) {
      any = true;
      return { _cut: { kind: 'array', len: v.length } };
    }
    if (isObj(v)) return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, go(x, d - 1)]));
    if (Array.isArray(v)) return v.map((x) => go(x, d - 1));
    return v;
  };
  const out = go(v, depth);
  return [out, any];
}

/** RFC 6901 pointer lookup. */
export function pointer(v: Json, p: string): Json | undefined {
  if (p === '') return v;
  if (!p.startsWith('/')) return undefined;
  let cur: Json | undefined = v;
  for (const raw of p.slice(1).split('/')) {
    const k = raw.replace(/~1/g, '/').replace(/~0/g, '~');
    if (Array.isArray(cur)) {
      if (!/^(0|[1-9][0-9]*)$/.test(k)) return undefined;
      cur = cur[Number(k)];
    } else if (isObj(cur)) cur = Object.prototype.hasOwnProperty.call(cur, k) ? cur[k] : undefined;
    else return undefined;
    if (cur === undefined) return undefined;
  }
  return cur;
}

export function viewJson(root: Json, ptr: string, depth: number | null): [string, boolean] | null {
  const node = pointer(root, ptr);
  if (node === undefined) return null;
  if (depth === null) {
    const s = stringify(node);
    if (s.length <= MAX_REPLY) return [s, false];
  }
  let d = Math.max(depth ?? CUT_DEPTH, 1);
  for (;;) {
    const [v, any] = cut(node, d);
    const s = stringify(v);
    if (s.length <= MAX_REPLY || d === 1) return [s, any];
    d--;
  }
}

export function locKeys(v: Json): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  const go = (v: Json) => {
    if (out.length >= MAX_LOC) return;
    if (typeof v === 'string') {
      if (v.length > 1 && v.startsWith('@') && !seen.has(v)) {
        seen.add(v);
        out.push(v);
      }
    } else if (Array.isArray(v)) v.forEach(go);
    else if (isObj(v)) Object.values(v).forEach(go);
  };
  go(v);
  return out;
}

// ------------------------------------------------------------------------------------------
// Datasets (starorg_core::dev::dataset)
// ------------------------------------------------------------------------------------------

interface Table {
  name: string;
  kind: D.TableKind;
  rows: Obj[];
  columns: D.Column[];
}
interface Dataset {
  info: D.DatasetInfo;
  tables: Table[];
}

const scalarText = (v: Json | undefined): string | null =>
  typeof v === 'string' ? v : typeof v === 'number' ? String(v) : typeof v === 'boolean' ? String(v) : null;

export function cellText(v: Json | undefined): string | null {
  if (v === undefined || v === null) return null;
  if (Array.isArray(v)) return `[${v.length}]`;
  if (isObj(v)) return Object.keys(v).length ? '{…}' : '{}';
  const s = scalarText(v)!;
  const chars = Array.from(s);
  return chars.length > MAX_CELL ? chars.slice(0, MAX_CELL).join('') + '…' : s;
}

const kindOf = (v: Json): D.ColType | null =>
  v === null ? null : typeof v === 'string' ? 'text' : typeof v === 'number' ? 'number' : typeof v === 'boolean' ? 'bool' : Array.isArray(v) ? 'list' : 'object';
const scalarKind = (k: D.ColType) => k === 'text' || k === 'number' || k === 'bool';

export function columns(rows: Obj[]): D.Column[] {
  const order: string[] = [];
  const acc = new Map<string, { kind: D.ColType | null; filled: number; distinct: Map<string, number>; tooMany: boolean; min: number | null; max: number | null }>();
  for (const r of rows) {
    for (const [k, v] of Object.entries(r)) {
      let a = acc.get(k);
      if (!a) {
        order.push(k);
        a = { kind: null, filled: 0, distinct: new Map(), tooMany: false, min: null, max: null };
        acc.set(k, a);
      }
      const kind = kindOf(v);
      if (kind === null) continue;
      a.filled++;
      a.kind = a.kind === null ? kind : a.kind === kind ? kind : 'mixed';
      if (typeof v === 'string' && !a.tooMany) {
        a.distinct.set(v, (a.distinct.get(v) ?? 0) + 1);
        if (a.distinct.size > MAX_DISTINCT) {
          a.tooMany = true;
          a.distinct.clear();
        }
      }
      if (typeof v === 'number') {
        a.min = a.min === null ? v : Math.min(a.min, v);
        a.max = a.max === null ? v : Math.max(a.max, v);
      }
    }
  }
  return order.map((name) => {
    const a = acc.get(name)!;
    const kind = a.kind ?? 'empty';
    const distinct = [...a.distinct.entries()].sort((x, y) => cmp(x[0], y[0])).map(([value, count]) => ({ value, count }));
    distinct.sort((x, y) => y.count - x.count || cmp(x.value, y.value));
    return {
      name,
      kind,
      filled: a.filled,
      distinct: kind === 'text' && !a.tooMany && distinct.length ? distinct : null,
      min: kind === 'number' ? a.min : null,
      max: kind === 'number' ? a.max : null,
    };
  });
}

export function defaultColumns(cols: D.Column[]): string[] {
  const out = LEADING.filter((l) => cols.some((c) => c.name === l));
  for (const c of cols) {
    if (out.length >= DEFAULT_COLUMNS) break;
    if (scalarKind(c.kind) && !out.includes(c.name)) out.push(c.name);
  }
  if (!out.length) out.push(...cols.slice(0, DEFAULT_COLUMNS).map((c) => c.name));
  return out;
}

const objectArray = (v: Json) => Array.isArray(v) && v.length > 0 && v.every(isObj);
const table = (name: string, kind: D.TableKind, rows: Obj[]): Table => ({ name, kind, rows, columns: columns(rows) });
const factText = (v: Json) => scalarText(v) ?? stringify(v);
const fileStem = (f: string) => (f.split(/[/\\]/).at(-1) ?? f).replace(/\.json$/, '');

export function infer(file: string, bytes: number, root: Json): Dataset {
  const meta: D.Fact[] = [];
  const facts: D.Fact[] = [];
  const tables: Table[] = [];
  if (Array.isArray(root)) {
    tables.push(table(fileStem(file), 'main', root.every(isObj) ? (root as Obj[]) : root.map((v) => ({ value: v }))));
  } else if (isObj(root)) {
    const cols = root['columns'];
    const names = Array.isArray(cols) && cols.every((c) => typeof c === 'string') ? (cols as string[]) : null;
    const withColumns = (v: Json) => names !== null && Array.isArray(v) && v.length > 0 && v.every((r) => Array.isArray(r) && r.length === names.length);
    const hasArrayTables = Object.entries(root).some(([k, v]) => !k.startsWith('_') && (objectArray(v) || withColumns(v)));
    const deferred: Table[] = [];
    for (const [k, v] of Object.entries(root)) {
      if (k.startsWith('_')) {
        meta.push({ key: k, value: factText(v) });
        continue;
      }
      if (k === 'summary' && isObj(v)) {
        for (const [fk, fv] of Object.entries(v)) facts.push({ key: fk, value: factText(fv) });
        continue;
      }
      if (objectArray(v)) tables.push(table(k, 'main', v as Obj[]));
      else if (withColumns(v)) tables.push(table(k, 'main', (v as Json[][]).map((r) => Object.fromEntries(names!.map((n, i) => [n, r[i]!])))));
      else if (isObj(v) && Object.keys(v).length) {
        const allScalar = Object.values(v).every((x) => !isObj(x) && !Array.isArray(x));
        deferred.push(table(k, allScalar && !hasArrayTables ? 'main' : 'index', Object.entries(v).map(([key, value]) => ({ key, value }))));
      } else if (k !== 'columns') deferred.push(table(k, 'tree', [{ key: k, value: v }]));
    }
    for (const kind of ['main', 'index', 'tree'] as const) tables.push(...deferred.filter((t) => t.kind === kind));
  } else {
    tables.push(table(fileStem(file), 'tree', [{ value: root }]));
  }
  return {
    info: { file, bytes, meta, facts, tables: tables.map((t) => ({ name: t.name, kind: t.kind, rows: t.rows.length })) },
    tables,
  };
}

const identityOf = (row: Obj, col: string): string | null => {
  const s = scalarText(row[col]);
  return s ? s : null;
};
function uniqueIn(rows: Obj[], col: string): boolean {
  const seen = new Set<string>();
  return (
    rows.length > 0 &&
    rows.every((r) => {
      const k = identityOf(r, col);
      if (k === null || seen.has(k)) return false;
      seen.add(k);
      return true;
    })
  );
}
const identityColumn = (rows: Obj[]) => IDENTITY.find((c) => uniqueIn(rows, c)) ?? null;

function tableInfo(t: Table): D.TableInfo {
  return { name: t.name, kind: t.kind, rows: t.rows.length, columns: t.columns, identity: identityColumn(t.rows), default_columns: defaultColumns(t.columns), group_presets: presets(t) };
}

function filterOk(row: Obj, f: D.RowFilter): boolean {
  const raw = row[f.column];
  const v = raw === undefined || raw === null ? undefined : raw;
  if (f.has !== null && (v !== undefined) !== f.has) return false;
  if (f.values.length) {
    const t = scalarText(v);
    if (t === null || !f.values.includes(t)) return false;
  }
  if (f.min !== null || f.max !== null) {
    if (typeof v !== 'number') return false;
    if ((f.min !== null && v < f.min) || (f.max !== null && v > f.max)) return false;
  }
  return true;
}

function cmpCells(a: Json | undefined, b: Json | undefined): number {
  const x = a === null ? undefined : a;
  const y = b === null ? undefined : b;
  if (x === undefined && y === undefined) return 0;
  if (x === undefined) return 1;
  if (y === undefined) return -1;
  if (typeof x === 'number' && typeof y === 'number') return x < y ? -1 : x > y ? 1 : 0;
  return cmp((scalarText(x) ?? '').toLowerCase(), (scalarText(y) ?? '').toLowerCase());
}

export function rowMatches(r: Obj, cols: string[], needle: string, filters: D.RowFilter[], l: Lookup | null = null, identity: string | null = null): boolean {
  const has = (s: string) => s.toLowerCase().includes(needle);
  if (!filters.every((f) => filterOk(r, f))) return false;
  if (!needle) return true;
  const cell = cols.some((c) => {
    const s = scalarText(r[c]);
    if (s === null) return false;
    if (has(s)) return true;
    const x = l ? label(l, s, c) : null;
    return x !== null && (has(x.text) || (x.enhanced !== null && has(x.enhanced)));
  });
  if (cell) return true;
  const title = l ? rowTitle(l, r, identity) : null;
  return title !== null && has(title);
}

export function queryRows(t: Table, q: Omit<D.DevRowsRequest, 'target' | 'source' | 'dataset' | 'table'>, l: Lookup | null = null): D.RowsPage {
  const cols = q.columns.length ? q.columns : defaultColumns(t.columns);
  const needle = q.search.trim().toLowerCase();
  const identity = identityColumn(t.rows);
  const g = q.group ?? null;
  let hits = t.rows
    .map((_, i) => i)
    .filter((i) => rowMatches(t.rows[i]!, cols, needle, q.filters, l, identity) && (g === null || rowIn(t.rows[i]!, g)));
  if (q.sort) {
    const s = q.sort;
    hits = hits.sort((a, b) => {
      const o = cmpCells(t.rows[a]![s.column], t.rows[b]![s.column]);
      return (s.desc ? -o : o) || a - b;
    });
  }
  const limit = Math.min(Math.max(q.limit, 1), MAX_PAGE);
  return {
    columns: cols,
    rows: hits.slice(q.offset, q.offset + limit).map((i) => {
      const r = t.rows[i]!;
      return {
        id: i,
        cells: cols.map((c) => cellText(r[c])),
        labels: cols.map((c) => {
          const s = scalarText(r[c]);
          return s === null || !l ? null : label(l, s, c);
        }),
        title: l ? rowTitle(l, r, identity) : null,
      };
    }),
    offset: q.offset,
    total: hits.length,
    all: t.rows.length,
  };
}

// ------------------------------------------------------------------------------------------
// Group trees (starorg_core::dev::group)
// ------------------------------------------------------------------------------------------

const MAX_CHILDREN = 500;
const MAX_GROUP_VALUES = 200;
const MAX_PRESETS = 12;
const CHAIN = [['type', 'category', 'kind', 'classification', 'itemType'], ['subType', 'subCategory', 'subKind', 'subtype'], ['size']];
const col = (c: string): D.GroupLevel => ({ column: c, folders: false });

function groupable(t: Table, column: string): boolean {
  const c = t.columns.find((x) => x.name === column);
  if (!c || !scalarKind(c.kind)) return false;
  const seen = new Set<string>();
  let filled = 0;
  for (const r of t.rows) {
    const v = r[column];
    const s = v === null || v === undefined ? null : scalarText(v);
    if (s === null) continue;
    filled++;
    if (seen.size <= MAX_GROUP_VALUES) seen.add(s);
  }
  return seen.size >= 2 && seen.size <= MAX_GROUP_VALUES && seen.size < filled && filled * 2 >= t.rows.length;
}

function pathLike(t: Table, column: string): boolean {
  const lower = column.toLowerCase();
  const named = ['path', 'file', 'filepath', 'filename', 'folder'].includes(lower) || lower.endsWith('path');
  if (!named || t.columns.find((c) => c.name === column)?.kind !== 'text') return false;
  let withSlash = 0;
  let filled = 0;
  for (const r of t.rows) {
    const v = r[column];
    if (typeof v !== 'string') continue;
    filled++;
    if (/[/\\]/.test(v)) withSlash++;
  }
  return filled > 0 && withSlash * 2 >= filled;
}

export function presets(t: Table): D.GroupPreset[] {
  if (t.kind === 'tree' || t.rows.length < 2) return [];
  const out: D.GroupPreset[] = [];
  const chain: [number, string][] = [];
  CHAIN.forEach((names, tier) => {
    const c = names.find((n) => groupable(t, n));
    if (c !== undefined) chain.push([tier, c]);
  });
  if (chain.length >= 2 || chain[0]?.[0] === 0) out.push({ id: 'chain', label: chain.map((c) => c[1]).join(' › '), levels: chain.map((c) => col(c[1])) });
  const singles = t.columns.filter((c) => c.name.toLowerCase().includes('manufacturer') && groupable(t, c.name)).map((c) => c.name);
  for (const c of singles) out.push({ id: `col:${c}`, label: c, levels: [col(c)] });
  for (const c of t.columns) if (pathLike(t, c.name)) out.push({ id: `folders:${c.name}`, label: `${c.name} folders`, levels: [{ column: c.name, folders: true }] });
  for (const c of t.columns) {
    if (out.length >= MAX_PRESETS) break;
    if (!singles.includes(c.name) && groupable(t, c.name)) out.push({ id: `col:${c.name}`, label: c.name, levels: [col(c.name)] });
  }
  return out.slice(0, MAX_PRESETS);
}

type SegKind = 'fixed' | 'folder' | 'number' | 'text';
interface Seg {
  key: string | null;
  column: string;
  kind: SegKind;
}

function segments(row: Obj, by: D.GroupLevel[]): Seg[] {
  const out: Seg[] = [];
  for (const l of by) {
    const raw = row[l.column];
    const v = raw === null || raw === undefined ? undefined : raw;
    const t = v === undefined ? null : scalarText(v);
    const text = t ? t : null;
    if (l.folders) {
      if (text === null) out.push({ key: null, column: l.column, kind: 'folder' });
      else {
        const parts = text.split(/[/\\]/).filter((p) => p !== '');
        parts.pop();
        for (const p of parts) out.push({ key: p, column: l.column, kind: 'folder' });
      }
      break;
    }
    out.push({ key: text, column: l.column, kind: typeof v === 'number' || typeof v === 'boolean' ? 'number' : 'text' });
  }
  return out;
}

export const rowPath = (row: Obj, by: D.GroupLevel[]) => segments(row, by).map((s) => s.key);

export function nodePath(id: string): (string | null)[] | null {
  try {
    const v = JSON.parse(id) as unknown;
    return Array.isArray(v) && v.every((x) => x === null || typeof x === 'string') ? (v as (string | null)[]) : null;
  } catch {
    return null;
  }
}

const inNode = (path: (string | null)[], node: (string | null)[]) => path.length >= node.length && node.every((k, i) => path[i] === k);

export function rowIn(row: Obj, pick: D.GroupPick): boolean {
  const node = nodePath(pick.node);
  return node === null || inNode(rowPath(row, pick.by), node);
}

function segLabel(s: Seg, named: (column: string, value: string) => string | null): string {
  if (s.key === null) return `No ${s.column}`;
  if (s.kind === 'number') return `${s.column.charAt(0).toUpperCase()}${s.column.slice(1)} ${s.key}`;
  if (s.kind === 'text') return named(s.column, s.key) ?? s.key;
  return s.key;
}

const plainNumber = (s: string) => (/^-?\d+(\.\d+)?$/.test(s) ? Number(s) : null);

interface GNode {
  keys: (string | null)[];
  label: string;
  kind: SegKind;
  kids: GNode[];
  index: Map<string | null, GNode>;
  count: number;
  ends: number;
  diff: D.DiffCounts;
}
const gnode = (keys: (string | null)[], label: string, kind: SegKind): GNode => ({ keys, label, kind, kids: [], index: new Map(), count: 0, ends: 0, diff: { added: 0, removed: 0, changed: 0 } });

type LabelFn = (column: string, value: string) => string | null;
/** Node labels from a lookup (starorg_core::dev::group::lookup_label). */
const lookupLabel =
  (l: Lookup | null): LabelFn =>
  (c, v) =>
    l ? (label(l, v, c)?.text ?? null) : null;

function treeAdd(root: GNode, segs: Seg[], hit: boolean, state: D.RecordState | null, named: LabelFn) {
  const bump = (n: GNode) => {
    if (hit) n.count++;
    if (state) n.diff[state]++;
  };
  let at = root;
  bump(at);
  for (const s of segs) {
    let next = at.index.get(s.key);
    if (!next) {
      next = gnode([s.key], segLabel(s, named), s.kind);
      at.index.set(s.key, next);
      at.kids.push(next);
    }
    at = next;
    bump(at);
  }
  at.ends++;
}

function treeFinish(n: GNode) {
  for (;;) {
    const k = n.kids[0];
    if (n.kind !== 'folder' || n.kids.length !== 1 || n.ends !== 0 || n.keys[0] === null || !k || k.kind !== 'folder' || k.keys[0] === null) break;
    n.keys = [...n.keys, ...k.keys];
    n.label = `${n.label}/${k.label}`;
    n.kids = k.kids;
    n.index = k.index;
    n.ends = k.ends;
  }
  n.kids.sort((a, b) => {
    const ka = a.keys.at(-1) ?? null;
    const kb = b.keys.at(-1) ?? null;
    if (ka === null && kb === null) return 0;
    if (ka === null) return 1;
    if (kb === null) return -1;
    // A total order, as sibling_order in core: numbers first by value, then text by label.
    const na = plainNumber(ka);
    const nb = plainNumber(kb);
    if (na !== null && nb === null) return -1;
    if (na === null && nb !== null) return 1;
    if (na !== null && nb !== null && na !== nb) return na < nb ? -1 : 1;
    return cmp(a.label.toLowerCase(), b.label.toLowerCase()) || cmp(a.label, b.label) || cmp(ka, kb);
  });
  for (const k of n.kids) treeFinish(k);
}

function treeEmit(root: GNode, open: Set<string>, skipSingle: boolean, diff: boolean): D.DataGroup[] {
  const out: D.DataGroup[] = [];
  const walk = (n: GNode, path: (string | null)[], parent: string | null, depth: number) => {
    for (const k of n.kids.slice(0, MAX_CHILDREN)) {
      const p = [...path, ...k.keys];
      const id = JSON.stringify(p);
      const isOpen = open.has(id);
      out.push({ id, parent, label: k.label, depth, count: k.count, children: k.kids.length, more: isOpen ? Math.max(0, k.kids.length - MAX_CHILDREN) : 0, diff: diff ? { ...k.diff } : null });
      if (isOpen && k.kids.length) walk(k, p, id, depth + 1);
    }
  };
  const only = root.kids.length === 1 && skipSingle ? root.kids[0]! : null;
  if (only) walk(only, [...only.keys], null, 1);
  else walk(root, [], null, 1);
  return out;
}

export function groupRows(t: Table, q: Omit<D.DevGroupsRequest, 'target' | 'source' | 'dataset' | 'table'>, l: Lookup | null = null): D.GroupPage {
  const cols = q.columns.length ? q.columns : defaultColumns(t.columns);
  const needle = q.search.trim().toLowerCase();
  const identity = identityColumn(t.rows);
  const named = lookupLabel(l);
  const root = gnode([], '', 'fixed');
  let total = 0;
  for (const r of t.rows) {
    const hit = rowMatches(r, cols, needle, q.filters, l, identity);
    if (hit) total++;
    if (q.by.length) treeAdd(root, segments(r, q.by), hit, null, named);
  }
  treeFinish(root);
  return { nodes: treeEmit(root, new Set(q.open), false, false), total, all: t.rows.length };
}

export function diffLevels(by: D.DiffGroupBy, a: Table | undefined, b: Table | undefined): D.GroupLevel[] {
  const t = b && b.rows.length ? b : a;
  if (!t || by === 'tables') return [];
  const p = presets(t);
  return (by === 'auto' ? p[0] : p.find((x) => x.id.startsWith('folders:')))?.levels ?? [];
}

function diffRow(r: D.DiffRecord, a: Table | undefined, b: Table | undefined): Obj | undefined {
  return (r.b !== null ? b?.rows[r.b] : undefined) ?? (r.a !== null ? a?.rows[r.a] : undefined);
}

const sideTable = (side: [string, Dataset][], dataset: string, table: string) => side.find((x) => x[0] === dataset)?.[1].tables.find((t) => t.name === table);

export function diffTree(tables: TableDiff[], a: [string, Dataset][], b: [string, Dataset][], req: D.DevDiffGroupsRequest, l: Lookup | null = null): D.GroupPage {
  const named = lookupLabel(l);
  const root = gnode([], '', 'fixed');
  let all = 0;
  for (const d of tables) {
    if (!d.records.length) continue;
    const ta = sideTable(a, d.summary.dataset, d.summary.table);
    const tb = sideTable(b, d.summary.dataset, d.summary.table);
    const levels = diffLevels(req.by, ta, tb);
    const head: Seg[] = [
      { key: d.summary.dataset, column: 'dataset', kind: 'fixed' },
      { key: d.summary.table, column: 'table', kind: 'fixed' },
    ];
    for (const r of d.records) {
      all++;
      const row = diffRow(r, ta, tb);
      treeAdd(root, row ? [...head, ...segments(row, levels)] : head, true, r.state, named);
    }
  }
  treeFinish(root);
  return { nodes: treeEmit(root, new Set(req.open), true, true), total: all, all };
}

export function diffGroupFilter(req: D.DevDiffRecordsRequest, a: [string, Dataset][], b: [string, Dataset][]): (r: D.DiffRecord) => boolean {
  const g = req.group ?? null;
  if (g === null) return () => true;
  const p = nodePath(g.node);
  const ds = p?.[0] ?? null;
  if (p === null || ds === null) return () => true;
  const table = p[1] ?? null;
  if (ds !== req.dataset || (table !== null && table !== req.table)) return () => false;
  const ta = sideTable(a, req.dataset, req.table);
  const tb = sideTable(b, req.dataset, req.table);
  const levels = diffLevels(g.by, ta, tb);
  const rest = p.slice(2);
  return (r) => {
    if (!rest.length) return true;
    const row = diffRow(r, ta, tb);
    return row !== undefined && inNode(rowPath(row, levels), rest);
  };
}

// ------------------------------------------------------------------------------------------
// Diff (starorg_core::dev::diff)
// ------------------------------------------------------------------------------------------

interface TableDiff {
  summary: D.DiffTable;
  records: D.DiffRecord[];
}
const esc = (s: string) => s.replace(/~/g, '~0').replace(/\//g, '~1');

export function flatten(v: Json): Map<string, Json> {
  const out = new Map<string, Json>();
  const go = (v: Json, path: string) => {
    if (isObj(v) && Object.keys(v).length) {
      for (const [k, x] of Object.entries(v)) go(x, `${path}/${esc(k)}`);
    } else if (Array.isArray(v) && v.length) {
      const id = v.every(isObj) ? identityColumn(v as Obj[]) : null;
      v.forEach((x, i) => {
        const val = id !== null && isObj(x) ? identityOf(x, id) : null;
        go(x, `${path}/${val !== null ? `[${id}=${esc(val)}]` : i}`);
      });
    } else out.set(path, v);
  };
  go(v, '');
  return out;
}

const jsonEq = (a: Json | undefined, b: Json | undefined) => (a === undefined || b === undefined ? a === b : stringify(a) === stringify(b));

export function fieldChanges(a: Json, b: Json): D.FieldChange[] {
  const fa = flatten(a);
  const fb = flatten(b);
  const keys = [...new Set([...fa.keys(), ...fb.keys()])].sort(cmp);
  const out: D.FieldChange[] = [];
  for (const k of keys) {
    const x = fa.get(k);
    const y = fb.get(k);
    if (jsonEq(x, y)) continue;
    let delta: number | null = null;
    let percent: number | null = null;
    if (typeof x === 'number' && typeof y === 'number') {
      delta = y - x;
      percent = x !== 0 ? ((y - x) / Math.abs(x)) * 100 : null;
    }
    const numeric = typeof (x ?? y) === 'number';
    out.push({ path: k, old: x ?? null, new: y ?? null, delta, percent, path_label: null, group: null, unit: numeric ? unitOf(k) : null, old_label: null, new_label: null });
  }
  return out;
}

function pickIdentity(a: Obj[], b: Obj[], over: string | null): string | null {
  const ok = (rows: Obj[], c: string) => {
    const seen = new Set<string>();
    return rows.every((r) => {
      const k = identityOf(r, c);
      if (k === null || seen.has(k)) return false;
      seen.add(k);
      return true;
    });
  };
  if (over !== null) return (a.length || b.length) && ok(a, over) && ok(b, over) ? over : null;
  if (!a.length && !b.length) return null;
  return IDENTITY.find((c) => ok(a, c) && ok(b, c)) ?? null;
}

export function diffTable(dataset: string, name: string, a: Table | undefined, b: Table | undefined, over: string | null): TableDiff {
  const ra = a?.rows ?? [];
  const rb = b?.rows ?? [];
  const presence: D.Presence = a && b ? 'both' : a ? 'only-a' : 'only-b';
  const identity = pickIdentity(ra, rb, over);
  const records: D.DiffRecord[] = [];
  let same = 0;
  const changed = (x: Obj, y: Obj) => fieldChanges(x, y).map((c) => c.path);
  if (identity !== null) {
    const ib = new Map<string, number>();
    rb.forEach((r, i) => {
      const k = identityOf(r, identity);
      if (k !== null) ib.set(k, i);
    });
    const seenB = rb.map(() => false);
    ra.forEach((r, i) => {
      const id = identityOf(r, identity) ?? '';
      const j = ib.get(id);
      if (j !== undefined) {
        seenB[j] = true;
        if (jsonEq(r, rb[j]!)) same++;
        else {
          const fields = changed(r, rb[j]!);
          if (!fields.length) same++;
          else records.push({ id, state: 'changed', fields, a: i, b: j, label: null });
        }
      } else records.push({ id, state: 'removed', fields: [], a: i, b: null, label: null });
    });
    rb.forEach((r, j) => {
      if (!seenB[j]) records.push({ id: identityOf(r, identity) ?? '', state: 'added', fields: [], a: null, b: j, label: null });
    });
  } else {
    for (let i = 0; i < Math.max(ra.length, rb.length); i++) {
      const id = `#${i}`;
      const x = ra[i];
      const y = rb[i];
      if (x && y && jsonEq(x, y)) same++;
      else if (x && y) records.push({ id, state: 'changed', fields: changed(x, y), a: i, b: i, label: null });
      else if (x) records.push({ id, state: 'removed', fields: [], a: i, b: null, label: null });
      else if (y) records.push({ id, state: 'added', fields: [], a: null, b: i, label: null });
    }
  }
  const rank = (s: D.RecordState) => (s === 'changed' ? 0 : s === 'added' ? 1 : 2);
  records.sort((x, y) => rank(x.state) - rank(y.state) || y.fields.length - x.fields.length || cmp(x.id, y.id));
  const count = (s: D.RecordState) => records.filter((r) => r.state === s).length;
  return { summary: { dataset, table: name, presence, added: count('added'), removed: count('removed'), changed: count('changed'), same, identity }, records };
}

const weight = (t: D.DiffTable) => t.added + t.removed + t.changed;

export function diffSources(a: [string, Dataset][], b: [string, Dataset][]): TableDiff[] {
  const names = [...new Set([...a.map((x) => x[0]), ...b.map((x) => x[0])])].sort(cmp);
  const out: TableDiff[] = [];
  for (const n of names) {
    const da = a.find((x) => x[0] === n)?.[1];
    const db = b.find((x) => x[0] === n)?.[1];
    const tables = [...new Set([...(da?.tables ?? []), ...(db?.tables ?? [])].map((t) => t.name))];
    for (const t of tables) out.push(diffTable(n, t, da?.tables.find((x) => x.name === t), db?.tables.find((x) => x.name === t), null));
  }
  out.sort((x, y) => weight(y.summary) - weight(x.summary) || cmp(x.summary.dataset, y.summary.dataset) || cmp(x.summary.table, y.summary.table));
  return out;
}

export function fieldCounts(records: D.DiffRecord[]): D.ValueCount[] {
  const m = new Map<string, number>();
  for (const r of records) {
    const seen = new Set<string>();
    for (const f of r.fields) {
      const head = f.replace(/^\/+/, '').split('/')[0] ?? '';
      if (!seen.has(head)) {
        seen.add(head);
        m.set(head, (m.get(head) ?? 0) + 1);
      }
    }
  }
  return [...m.entries()].map(([value, count]) => ({ value, count })).sort((a, b) => b.count - a.count || cmp(a.value, b.value));
}

function pageRecords(d: TableDiff, req: D.DevDiffRecordsRequest, identity_options: string[], inGroup: (r: D.DiffRecord) => boolean, title: (r: D.DiffRecord) => string | null): D.DevDiffRecords {
  const q = (req.query ?? '').trim().toLowerCase();
  const hits = d.records
    .filter((r) => req.state === null || r.state === req.state)
    .filter(inGroup)
    .filter((r) => {
      if (req.field === null) return true;
      const f = `/${req.field.replace(/^\/+/, '')}`;
      return r.fields.some((p) => p === f || p.startsWith(`${f}/`));
    })
    .filter((r) => {
      if (!q) return true;
      const t = title(r);
      return r.id.toLowerCase().includes(q) || (t !== null && t.toLowerCase().includes(q));
    });
  const limit = Math.min(Math.max(req.limit, 1), MAX_PAGE);
  return {
    summary: { ...d.summary },
    records: hits.slice(req.offset, req.offset + limit).map((r) => ({ ...r, fields: r.fields.slice(0, 20), label: title(r) })),
    offset: req.offset,
    total: hits.length,
    fields: fieldCounts(d.records),
    identity_options,
  };
}

export function explorerLinks(row: Json, isRecord: (s: string) => string | null): D.ExplorerLink[] {
  const out: D.ExplorerLink[] = [];
  const go = (v: Json, path: string, key: string | null) => {
    if (out.length >= 200) return;
    if (typeof v === 'string') {
      if (isGuid(v) || key === 'recordName' || key === 'className') {
        const what = isRecord(v);
        if (what !== null) out.push({ pointer: path, value: v, what });
      }
    } else if (Array.isArray(v)) v.forEach((x, i) => go(x, `${path}/${i}`, null));
    else if (isObj(v)) for (const [k, x] of Object.entries(v)) go(x, `${path}/${esc(k)}`, k);
  };
  go(row, '', null);
  return out;
}

export function rowLoc(row: Json, textOf: (k: string) => string | null): D.LocText[] {
  const out: D.LocText[] = [];
  const go = (v: Json) => {
    if (out.length >= MAX_LOC) return;
    if (typeof v === 'string') {
      if (!v || v.length >= 200) return;
      const bare = v.startsWith('@') ? v.slice(1) : v;
      if (!bare || bare.includes(' ') || out.some((l) => l.key === v)) return;
      const text = textOf(bare);
      if (text !== null) out.push({ key: v, text });
    } else if (Array.isArray(v)) v.forEach(go);
    else if (isObj(v)) Object.values(v).forEach(go);
  };
  go(row);
  return out;
}

// ------------------------------------------------------------------------------------------
// Readable labels (starorg_core::dev::readable, addendum 7.14)
// ------------------------------------------------------------------------------------------

export interface KnownRecord {
  guid: string | null;
  name: string;
  kind: D.LabelKind;
  display: string | null;
}

export interface Lookup {
  byGuid(g: string): KnownRecord | null;
  byName(n: string): KnownRecord | null;
  stock(k: string): string | null;
  enhanced(k: string): string | null;
  bare: boolean;
}

const MAX_RAW = 256;
const MAX_LABELS = 2_000;
const shortName = (n: string) => {
  const i = n.indexOf('.');
  return i < 0 ? n : n.slice(i + 1);
};

/** MapLookup: first record of a GUID or name wins. */
export function mapLookup(known: KnownRecord[], stock: (k: string) => string | null, enhanced: (k: string) => string | null, bare: boolean): Lookup {
  const byGuid = new Map<string, KnownRecord>();
  const byName = new Map<string, KnownRecord>();
  for (const r of known) {
    if (r.guid !== null && !byGuid.has(r.guid.toLowerCase())) byGuid.set(r.guid.toLowerCase(), r);
    for (const n of [r.name.toLowerCase(), shortName(r.name).toLowerCase()]) if (n && !byName.has(n)) byName.set(n, r);
  }
  return { byGuid: (g) => byGuid.get(g) ?? null, byName: (n) => byName.get(n.toLowerCase()) ?? null, stock, enhanced, bare };
}

export const strict = (l: Lookup): Lookup => ({ ...l, bare: false });
export const isMakerKey = (k: string) => k.toLowerCase().includes('manufacturer') || k.toLowerCase() === 'maker';
const isCode = (s: string) => s.length >= 2 && s.length <= 6 && /^[0-9A-Za-z]+$/.test(s);
const shown = (t: string) => t.trim() !== '' && !t.includes('PLACEHOLDER');
const hasSpace = (s: string) => /\s/.test(s);

function loc(l: Lookup, key: string): D.Readable | null {
  if (!key || hasSpace(key)) return null;
  const text = l.stock(key);
  if (text === null || !shown(text)) return null;
  const e = l.enhanced(key);
  return { kind: 'loc', text, detail: null, enhanced: e !== null && e !== text ? e : null };
}

export function recordLabel(l: Lookup, r: KnownRecord): D.Readable {
  const short = shortName(r.name);
  let display: [string, string | null] | null = null;
  const d = r.display;
  if (d !== null) {
    if (d.startsWith('@')) {
      const x = loc(l, d.slice(1));
      display = x ? [x.text, x.enhanced] : null;
    } else if (r.kind === 'tag') display = shown(d) ? [d, null] : null;
    else if (l.bare) {
      const x = loc(l, d);
      display = x ? [x.text, x.enhanced] : shown(d) ? [d, null] : null;
    } else display = shown(d) ? [d, null] : null;
  }
  if (display) return { kind: r.kind, text: display[0], detail: r.kind === 'record' && short !== display[0] ? short : null, enhanced: display[1] };
  return { kind: r.kind, text: short, detail: null, enhanced: null };
}

export function label(l: Lookup, raw: string, key: string | null): D.Readable | null {
  const s = raw.trim();
  if (!s || s.length > MAX_RAW) return null;
  if (s.startsWith('@')) return loc(l, s.slice(1));
  if (isGuid(s)) {
    const r = l.byGuid(s.toLowerCase());
    return r ? recordLabel(l, r) : null;
  }
  if (hasSpace(s)) return null;
  if (key !== null && isMakerKey(key) && isCode(s)) {
    const text = l.stock(`manufacturer_Name${s.toUpperCase()}`);
    if (text !== null && shown(text)) return { kind: 'manufacturer', text, detail: null, enhanced: null };
  }
  if (s.length >= 4 && (s.includes('_') || s.includes('.'))) {
    const r = l.byName(s);
    if (r) return recordLabel(l, r);
  }
  if (l.bare && s.includes('_')) return loc(l, s);
  return null;
}

export function labelsIn(l: Lookup, v: Json): Record<string, D.Readable> {
  const out: Record<string, D.Readable> = {};
  let n = 0;
  const seen = new Set<string>();
  const go = (v: Json, key: string | null) => {
    if (n >= MAX_LABELS) return;
    if (typeof v === 'string') {
      if (seen.has(v)) return;
      seen.add(v);
      const r = label(l, v, key);
      if (r) {
        out[v] = r;
        n++;
      }
    } else if (Array.isArray(v)) v.forEach((x) => go(x, key));
    else if (isObj(v)) {
      if (isRef(v)) go(v['_ref']!, null);
      else for (const [k, x] of Object.entries(v)) go(x, k);
    }
  };
  go(v, null);
  return out;
}

const TITLE_KEYS = ['/loc/name', '/Localization/Name', '/displayName', '/nameKey', '/name'];

export function rowTitle(l: Lookup, row: Json, identity: string | null): string | null {
  for (const p of TITLE_KEYS) {
    const s = pointer(row, p);
    if (typeof s === 'string') {
      const r = label(l, s, 'name');
      if (r && r.kind === 'loc') return r.text;
    }
  }
  if (!isObj(row)) return null;
  const ids = [...(identity !== null ? [identity] : []), 'recordName', 'className', 'id', 'record', 'item'];
  for (const c of ids) {
    const s = row[c];
    if (typeof s === 'string') {
      const r = label(l, s, c);
      if (r && r.detail !== null) return r.text;
    }
  }
  const n = row['name'];
  if (typeof n === 'string' && identity !== 'name' && shown(n) && !n.startsWith('@')) return n;
  for (const c of ids) {
    const s = row[c];
    if (typeof s === 'string') {
      const r = label(l, s, c);
      if (r && r.text !== s) return r.text;
    }
  }
  return null;
}

const unescape = (seg: string) => seg.replace(/~1/g, '/').replace(/~0/g, '~');
function keyed(seg: string): [string, string] | null {
  if (!seg.startsWith('[') || !seg.endsWith(']')) return null;
  const inner = seg.slice(1, -1);
  const i = inner.indexOf('=');
  return i < 0 ? null : [inner.slice(0, i), inner.slice(i + 1)];
}
function segText(l: Lookup, seg: string): [string, boolean] {
  const k = keyed(seg);
  if (!k) return [unescape(seg), false];
  const v = unescape(k[1]);
  const r = label(l, v, k[0]);
  return r ? [r.text, true] : [v, false];
}

export function pathLabel(l: Lookup, path: string): string | null {
  let any = false;
  const parts = path
    .replace(/^\/+/, '')
    .split('/')
    .map((s) => {
      const [t, hit] = segText(l, s);
      any ||= hit;
      return t;
    });
  return any ? parts.join(' › ') : null;
}

export function groupOf(l: Lookup, path: string): string | null {
  const segs = path.replace(/^\/+/, '').split('/');
  if (segs.length < 2) return null;
  const at = segs.findIndex((s) => keyed(s) !== null);
  const end = at < 0 ? 1 : at + 1;
  return segs
    .slice(0, end)
    .map((s) => segText(l, s)[0])
    .join(' › ');
}

export function unitOf(path: string): string | null {
  const last = (path.split('/').at(-1) ?? path).toLowerCase();
  const fixed: Record<string, string> = { mass: 'kg', dps: 'dmg/s', weapondps: 'dmg/s', rpm: 'rpm', firerate: 'rpm', health: 'HP', armorhealth: 'HP', shieldhealth: 'HP', scm: 'm/s', duration: 's', cooldown: 's' };
  if (fixed[last]) return fixed[last]!;
  if (last.includes('speed')) return 'm/s';
  if (last.endsWith('range') || last.endsWith('distance')) return 'm';
  if (last.endsWith('seconds')) return 's';
  if (last.includes('scu')) return 'SCU';
  return null;
}

export function annotateChanges(changes: D.FieldChange[], a: Lookup, b: Lookup): void {
  const keyOf = (p: string) => {
    const last = p.split('/').at(-1) ?? p;
    return /^\[([^=]*)=/.exec(last)?.[1] ?? last;
  };
  const read = (l: Lookup, v: unknown, key: string) => (typeof v === 'string' ? label(l, v, key) : null);
  for (const c of changes) {
    c.path_label = pathLabel(b, c.path) ?? pathLabel(a, c.path);
    c.group = groupOf(b, c.path);
    const key = keyOf(c.path);
    c.old_label = read(a, c.old, key);
    c.new_label = read(b, c.new, key);
  }
}

export function recordTitle(r: D.DiffRecord, identity: string | null, a: Table | undefined, b: Table | undefined, la: Lookup, lb: Lookup): string | null {
  const rb = r.b !== null ? b?.rows[r.b] : undefined;
  const ra = r.a !== null ? a?.rows[r.a] : undefined;
  return (rb ? rowTitle(lb, rb, identity) : null) ?? (ra ? rowTitle(la, ra, identity) : null);
}

function identityOptions(a: Table | undefined, b: Table | undefined): string[] {
  const cols = (t: Table | undefined) => (t ? t.columns.filter((c) => scalarKind(c.kind)).map((c) => c.name) : []);
  const ca = cols(a);
  const cb = cols(b);
  if (a && b) return ca.filter((c) => cb.includes(c));
  return a ? ca : cb;
}

// ------------------------------------------------------------------------------------------
// The handlers
// ------------------------------------------------------------------------------------------

type DevHandlers = { [K in keyof D.DevCommandMap]: (args: D.DevCommandMap[K][0]) => D.DevCommandMap[K][1] | Promise<D.DevCommandMap[K][1]> };

export interface DevMock {
  handlers: DevHandlers;
  index: Index;
  /** Render/demo switch: the sample folder is already added. */
  addFolder(): void;
}

/** The huge ship's loadout, rebuilt by the Rust mock's rule. */
function hugeLoadout(ports: number): Json[] {
  return Array.from({ length: ports }, (_, i) => ({ Port: `hardpoint_syn_${String(i).padStart(5, '0')}`, Size: (i % 5) + 1, Note: 'synthetic loadout padding '.repeat(16) }));
}

export function createDevMock(data: DevMockData, ctx: DevMockCtx): DevMock {
  const index = new Index(data.entries);
  const values: Json[] = data.values.map((v, i) => {
    if (data.entries[i]!.guid !== data.huge.guid || !isObj(v)) return v;
    const rv = { ...(v['_RecordValue_'] as Obj), Loadout: hugeLoadout(data.huge.ports) };
    return { ...v, _RecordValue_: rv };
  });
  let open: string | null = null;
  let folder = false;
  let diff: { summary: D.DevDiffSummary; tables: TableDiff[]; a: [string, Dataset][]; b: [string, Dataset][] } | null = null;
  const key = (r: T.ChannelRef) => `${r.install}/${r.channel}`;
  const stockOf = (k: string): string | null => {
    const lk = k.toLowerCase();
    if (lk.startsWith('manufacturer_name')) {
      const code = lk.slice('manufacturer_name'.length);
      return Object.entries(data.makers).find(([c]) => c.toLowerCase() === code)?.[1] ?? null;
    }
    return ctx.stockText(k);
  };
  const lookup = mapLookup(data.known, stockOf, (k) => data.enhanced[k.toLowerCase()] ?? null, true);
  const toDataset = (file: string, v: Json) => infer(file, stringify(v).length, v);

  const on = (r: T.ChannelRef) => {
    if (!ctx.devOn()) ctx.fail('dev-disabled', 'Developer mode is off (Settings > Advanced).');
    if (!ctx.hasChannel(r)) ctx.fail('not-found', `No channel ${r.channel} in install ${r.install}`);
  };
  const explorer = (r: T.ChannelRef) => {
    on(r);
    open = key(r);
    return index;
  };
  const status = (r: T.ChannelRef): D.DevStatus => {
    const isOpen = open === key(r);
    return {
      state: isOpen ? 'ready' : 'stopped',
      channel: r.channel,
      build: '4.10.0-mock',
      records: isOpen ? index.length : 0,
      main: isOpen ? index.mainCount() : 0,
      types: isOpen ? index.types().length : 0,
      started: isOpen ? '2026-11-14T21:30:05Z' : null,
      start_ms: isOpen ? 840 : null,
      memory: isOpen ? 412 * 1024 * 1024 : null,
      error: null,
    };
  };
  const summaryOf = (file: string, d: Dataset): D.DevDatasetSummary => ({ file, bytes: d.info.bytes, tables: d.tables.length, rows: d.tables.reduce((n, t) => n + t.rows.length, 0) });
  const sources = (r: T.ChannelRef): D.DevSource[] => {
    const chans = [...ctx.channelNames(r.install)].sort((a, b) => Number(a !== r.channel) - Number(b !== r.channel));
    const out: D.DevSource[] = chans.map((c) => ({
      id: `build:${c}`,
      kind: 'build',
      label: `${c} build`,
      build: '4.10.0-mock',
      state: 'ready',
      detail: null,
      datasets: [summaryOf(GAMEDATA, toDataset(GAMEDATA, data.builds[c] ?? null))],
    }));
    if (folder) {
      out.push({ id: 'folder:0', kind: 'folder', label: data.folder.label, build: null, state: 'ready', detail: data.folder.path, datasets: data.folder.files.map((f) => summaryOf(f.file, toDataset(f.file, f.value))) });
    }
    out.push({ id: `dump:${r.channel}`, kind: 'dump', label: 'Published dump', build: null, state: 'off', detail: ctx.dumpsOn() ? 'Follow-up: not in this build' : 'Off (Settings > Network)', datasets: [] });
    return out;
  };
  const dumpRefusal = (on: boolean): never =>
    on
      ? ctx.fail('not-implemented', 'Published dumps as a dataset source are a follow-up; use This build or a local folder.')
      : ctx.fail('network-off', 'Turn on Settings > Network > "Use published game data" first.');
  const datasetsOf = (r: T.ChannelRef, source: string): [string, Dataset][] => {
    if (source.startsWith('build:')) {
      const c = source.slice(6);
      if (!ctx.channelNames(r.install).includes(c)) ctx.fail('not-found', `no source ${source}`);
      return [[GAMEDATA, toDataset(GAMEDATA, data.builds[c] ?? null)]];
    }
    if (source === 'folder:0' && folder) return data.folder.files.map((f) => [f.file, toDataset(f.file, f.value)]);
    if (source.startsWith('dump:')) dumpRefusal(false);
    return ctx.fail('not-found', `no source ${source}`);
  };
  const datasetOf = (r: T.ChannelRef, source: string, name: string): Dataset => {
    on(r);
    return datasetsOf(r, source).find((d) => d[0] === name)?.[1] ?? ctx.fail('not-found', `no dataset ${name}`);
  };
  const tableOf = (d: Dataset, name: string) => d.tables.find((t) => t.name === name) ?? ctx.fail('not-found', `no table ${name}`);
  const job = async (label: string, steps: number, channel: string, run: (p: T.JobProgress) => Promise<void>) => {
    const p: T.JobProgress = { job: ctx.nextJob(), kind: 'dev', channel, phase: 'running', step: 1, steps, label, cancellable: false };
    ctx.emit({ ...p });
    await run(p);
    ctx.emit({ ...p, phase: 'done', step: steps });
  };
  const noDiff = () => ctx.fail('not-found', 'No comparison yet: pick A and B.');
  const cancelledExport: D.DevExportResult = { cancelled: true, path: null, records: 0, bytes: 0 };

  const handlers: DevHandlers = {
    dev_status: ({ req }) => (on(req), status(req)),
    dev_open: async ({ req }) => {
      on(req);
      await job('Opening the game data', 2, req.channel, async (p) => {
        await ctx.sleep(ctx.delay);
        ctx.emit({ ...p, step: 2, label: 'Indexing records' });
        await ctx.sleep(ctx.delay);
        p.label = 'Explorer ready';
      });
      explorer(req);
      return status(req);
    },
    dev_close: ({ req }) => {
      on(req);
      open = null;
      return status(req);
    },
    dev_types: ({ req }) => explorer(req).types(),
    dev_tree: ({ req }) => explorer(req.target).tree(req.root, req.node, req.offset, Math.min(req.limit, MAX_PAGE), req.main_only),
    dev_search: ({ req }) => explorer(req.target).search(req.query, req.limit),
    dev_record: ({ req }) => {
      const ix = explorer(req.target);
      const i = ix.find(req.what) ?? ctx.fail('not-found', `no record "${req.what}"`);
      const v = values[i]!;
      const view = viewJson(v, req.pointer, req.depth) ?? ctx.fail('not-found', `no node at "${req.pointer}"`);
      const node = pointer(v, req.pointer) ?? v;
      const loc = locKeys(node).flatMap((k) => {
        const text = ctx.stockText(k.replace(/^@+/, ''));
        return text === null ? [] : [{ key: k, text }];
      });
      const e = ix.entries[i]!;
      return {
        header: { guid: e.guid, name: e.name, type: e.type, path: e.path, tag: e.tag, main: e.main, bytes: stringify(v).length },
        json: view[0],
        pointer: req.pointer,
        truncated: view[1],
        loc,
        labels: labelsIn(strict(lookup), JSON.parse(view[0]) as Json),
      };
    },
    dev_export_record: ({ req }) => {
      const ix = explorer(req.target);
      if (ix.find(req.what) === null) ctx.fail('not-found', `no record "${req.what}"`);
      return { ...cancelledExport };
    },
    dev_export_type: ({ req }) => {
      const ix = explorer(req.target);
      if (!ix.ofType(req.type_name, req.main_only).length) ctx.fail('not-found', `no records of type ${req.type_name}`);
      return { ...cancelledExport };
    },
    dev_sources: ({ req }) => (on(req), sources(req)),
    dev_add_source: ({ req }) => {
      on(req);
      folder = true;
      return sources(req);
    },
    dev_remove_source: ({ req }) => {
      on(req.target);
      if (req.source !== 'folder:0' || !folder) ctx.fail('not-found', `no source ${req.source}`);
      folder = false;
      diff = null;
      return sources(req.target);
    },
    dev_download_dump: ({ req }) => (on(req), dumpRefusal(ctx.dumpsOn())),
    dev_dataset: ({ req }) => structuredClone(datasetOf(req.target, req.source, req.dataset).info),
    dev_table: ({ req }) => tableInfo(tableOf(datasetOf(req.target, req.source, req.dataset), req.table)),
    dev_rows: ({ req }) => queryRows(tableOf(datasetOf(req.target, req.source, req.dataset), req.table), req, lookup),
    dev_groups: ({ req }) => groupRows(tableOf(datasetOf(req.target, req.source, req.dataset), req.table), req, lookup),
    dev_row: ({ req }) => {
      const t = tableOf(datasetOf(req.target, req.source, req.dataset), req.table);
      const row = t.rows[req.row] ?? ctx.fail('not-found', `no row ${req.row}`);
      const [json, truncated] = viewJson(row, '', null)!;
      const links =
        open !== null
          ? explorerLinks(row, (s) => {
              const i = index.byGuid(s) ?? index.findName(s);
              return i === null ? null : index.entries[i]!.guid;
            })
          : [];
      return { row: req.row, json, truncated, loc: rowLoc(row, (k) => ctx.stockText(k)), links, labels: labelsIn(lookup, row) };
    },
    dev_diff: async ({ req }) => {
      on(req.target);
      let summary: D.DevDiffSummary | null = null;
      await job('Reading datasets', 2, req.target.channel, async (p) => {
        const a = datasetsOf(req.target, req.a);
        const b = datasetsOf(req.target, req.b);
        await ctx.sleep(ctx.delay);
        ctx.emit({ ...p, step: 2, label: 'Comparing' });
        const tables = diffSources(a, b);
        summary = { a: req.a, b: req.b, tables: tables.map((t) => t.summary), ms: 12 };
        diff = { summary, tables, a, b };
        p.label = 'Compared';
      });
      return structuredClone(summary!);
    },
    dev_diff_summary: ({ req }) => (on(req), structuredClone((diff ?? noDiff()).summary)),
    dev_diff_records: ({ req }) => {
      on(req.target);
      const st = diff ?? noDiff();
      const find = (side: [string, Dataset][]) => side.find((x) => x[0] === req.dataset)?.[1].tables.find((t) => t.name === req.table);
      const ta = find(st.a);
      const tb = find(st.b);
      const pos = st.tables.findIndex((t) => t.summary.dataset === req.dataset && t.summary.table === req.table);
      if (pos < 0) ctx.fail('not-found', `no table ${req.table}`);
      if (req.identity !== null && st.tables[pos]!.summary.identity !== req.identity) st.tables[pos] = diffTable(req.dataset, req.table, ta, tb, req.identity);
      const id = st.tables[pos]!.summary.identity;
      return pageRecords(st.tables[pos]!, req, identityOptions(ta, tb), diffGroupFilter(req, st.a, st.b), (r) => recordTitle(r, id, ta, tb, lookup, lookup));
    },
    dev_diff_groups: ({ req }) => {
      on(req.target);
      const st = diff ?? noDiff();
      return diffTree(st.tables, st.a, st.b, req, lookup);
    },
    dev_diff_record: ({ req }) => {
      on(req.target);
      const st = diff ?? noDiff();
      const td = st.tables.find((t) => t.summary.dataset === req.dataset && t.summary.table === req.table) ?? ctx.fail('not-found', `no table ${req.table}`);
      const rec = td.records.find((r) => r.id === req.id) ?? ctx.fail('not-found', `no record ${req.id}`);
      const row = (side: [string, Dataset][], i: number | null): Json => (i === null ? {} : (side.find((x) => x[0] === req.dataset)?.[1].tables.find((t) => t.name === req.table)?.rows[i] ?? {}));
      const changes = fieldChanges(row(st.a, rec.a), row(st.b, rec.b));
      annotateChanges(changes, lookup, lookup);
      const find = (side: [string, Dataset][]) => side.find((x) => x[0] === req.dataset)?.[1].tables.find((t) => t.name === req.table);
      return { id: rec.id, label: recordTitle(rec, td.summary.identity, find(st.a), find(st.b), lookup, lookup), state: rec.state, changes };
    },
  };
  return { handlers, index, addFolder: () => (folder = true) };
}

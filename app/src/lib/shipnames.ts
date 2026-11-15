/**
 * Ship names in the front end (addendum §6.5, 6.6): the prefix and sort rules of the core's
 * `fleet::names` (so the preview follows a drag without waiting for the back end), and the list
 * operations of the Ships screen (move, group, rename, remove). Both rule sets are tested against
 * the shared vectors in `fixtures/ship-names-vectors.json` (tests/shipnames.test.ts).
 *
 * Pure functions on `ShipNamesSettings`; every edit returns a new object. No Svelte, no DOM.
 */
import type { FleetHull, HullOverride, ShipEntry, ShipGroup, ShipNamesSettings } from './contract/types';

export const SEPARATORS = [' ', '-', '. '] as const;
export const SYMBOLS = ['!', '#', '*', '+', '-', '.'] as const;
export const MAX_NAME = 48;
export const MAX_BLOCKS = 26;

/** Hull ids are compared like loc keys: case-insensitive, flag suffix dropped. */
export const fold = (k: string) => k.replace(/^@/, '').replace(/,[A-Z]+$/, '').toLowerCase();

/** The sort the preview assumes (unconfirmed, Q23): case-insensitive, character by character. */
export function compareNames(a: string, b: string): number {
  const cmp = (x: string, y: string) => {
    const xs = Array.from(x);
    const ys = Array.from(y);
    for (let i = 0; i < Math.min(xs.length, ys.length); i++) {
      const d = xs[i]!.codePointAt(0)! - ys[i]!.codePointAt(0)!;
      if (d) return d;
    }
    return xs.length - ys.length;
  };
  return cmp(a.toLowerCase(), b.toLowerCase()) || cmp(a, b);
}

/** Why a rename is refused, null when fine (checked trimmed). Same rules as the core. */
export function nameProblem(s: string): string | null {
  const t = s.trim();
  const n = Array.from(t).length;
  if (!n) return 'Empty';
  if (n > MAX_NAME) return `At most ${MAX_NAME} characters`;
  // eslint-disable-next-line no-control-regex
  if (/[\u0000-\u001f\u007f-\u009f]/.test(t)) return 'No control characters';
  if (/[%<>~\\]/.test(t)) return 'Not allowed: % < > ~ \\';
  if (t.startsWith('@')) return 'Cannot start with @';
  return null;
}

export interface Placed {
  entry: ShipEntry;
  group: ShipGroup | null;
}

/** Ships in order: the ungrouped ones first (list order), then each group in group order. A group
 *  id that does not exist counts as none; a hull listed twice keeps its first entry. */
export function ordered(s: ShipNamesSettings): Placed[] {
  const ids = new Map(s.groups.map((g) => [g.id, g]));
  const seen = new Set<string>();
  const ships = s.ships.filter((e) => {
    const f = fold(e.hull);
    if (seen.has(f)) return false;
    seen.add(f);
    return true;
  });
  const groupOf = (e: ShipEntry) => (e.group && ids.has(e.group) ? ids.get(e.group)! : null);
  const out: Placed[] = ships.filter((e) => !groupOf(e)).map((entry) => ({ entry, group: null }));
  for (const g of s.groups) out.push(...ships.filter((e) => groupOf(e)?.id === g.id).map((entry) => ({ entry, group: g })));
  return out;
}

export interface NameEdit {
  key: string;
  hull: string;
  short: boolean;
  prefix: string;
  replace: string | null;
}

export interface Plan {
  edits: NameEdit[];
  /** Per listed hull of this build (folded id): position and label. */
  labels: Map<string, { position: number; label: string; prefix: string }>;
  notInBuild: string[];
}

type Hull = Pick<FleetHull, 'hull' | 'name' | 'short'> & { short_key?: string | null };

/** The core's `names::plan`, for the preview. */
export function plan(s: ShipNamesSettings, catalog: Hull[]): Plan {
  const byId = new Map(catalog.map((h) => [fold(h.hull), h]));
  const sorted = [...catalog].sort((a, b) => compareNames(a.name, b.name) || (a.hull < b.hull ? -1 : a.hull > b.hull ? 1 : 0));
  const ships: { g: string | null; e: ShipEntry; h: Hull }[] = [];
  const notInBuild: string[] = [];
  for (const p of ordered(s)) {
    const h = byId.get(fold(p.entry.hull));
    if (h) ships.push({ g: p.group?.id ?? null, e: p.entry, h });
    else notInBuild.push(p.entry.hull);
  }
  const blockOf: number[] = [];
  const inBlock: number[] = [];
  const sizes: number[] = [];
  ships.forEach((x, i) => {
    if (i === 0 || ships[i - 1]!.g !== x.g) sizes.push(0);
    const b = sizes.length - 1;
    sizes[b]!++;
    blockOf.push(b);
    inBlock.push(sizes[b]!);
  });
  const listed = new Set(ships.map((x) => fold(x.h.hull)));
  const rest = s.style === 'numeric' && s.rest === 'numbered' ? sorted.filter((h) => !listed.has(fold(h.hull))) : [];
  const total = ships.length + rest.length;
  const digits = (n: number) => String(Math.max(n, 1)).length;
  const numW = Math.max(2, digits(total));
  const blkW = digits(Math.max(1, ...sizes));
  const pad = (n: number, w: number) => String(n).padStart(w, '0');
  const withSep = (label: string) => (label ? `${label}${s.separator}` : '');
  const edits: NameEdit[] = [];
  const labels = new Map<string, { position: number; label: string; prefix: string }>();
  ships.forEach(({ e, h }, i) => {
    const label =
      s.style === 'none'
        ? ''
        : s.style === 'numeric'
          ? pad(i + 1, numW)
          : s.style === 'group-letter'
            ? `${String.fromCharCode(65 + Math.min(blockOf[i]!, 25))}${pad(inBlock[i]!, blkW)}`
            : s.symbol;
    const prefix = withSep(label);
    if (prefix || e.name !== null) edits.push({ key: h.hull, hull: h.hull, short: false, prefix, replace: e.name });
    if (h.short !== null) {
      const renamed = e.short ?? e.name;
      const sp = s.prefix_short ? prefix : '';
      if (renamed !== null || sp) edits.push({ key: h.short_key ?? `${h.hull}_short`, hull: h.hull, short: true, prefix: sp, replace: renamed });
    }
    labels.set(fold(h.hull), { position: i + 1, label, prefix });
  });
  rest.forEach((h, j) => {
    const label = pad(ships.length + j + 1, numW);
    edits.push({ key: h.hull, hull: h.hull, short: false, prefix: withSep(label), replace: null });
    labels.set(fold(h.hull), { position: ships.length + j + 1, label, prefix: withSep(label) });
  });
  return { edits, labels, notInBuild };
}

export const editValue = (e: NameEdit, stock: string) => `${e.prefix}${e.replace ?? stock}`;

export interface PreviewRow {
  hull: string;
  value: string;
  stock: string;
  listed: boolean;
  overridden: boolean;
}

/** Every catalogue hull's final long name (plan, then overrides win), in the assumed game order. */
export function preview(s: ShipNamesSettings, catalog: Hull[], overrides: HullOverride[] = [], p: Plan = plan(s, catalog)): PreviewRow[] {
  const ov = new Map(overrides.filter((o) => o.long !== null).map((o) => [fold(o.hull), o.long!]));
  const listed = new Set(ordered(s).map((x) => fold(x.entry.hull)));
  const rows = catalog.map((h) => {
    const o = ov.get(fold(h.hull));
    const e = p.edits.find((x) => !x.short && x.hull === h.hull);
    return { hull: h.hull, stock: h.name, value: o ?? (e ? editValue(e, h.name) : h.name), listed: listed.has(fold(h.hull)), overridden: o !== undefined };
  });
  return rows.sort((a, b) => compareNames(a.value, b.value));
}

// ---------------------------------------------------------------------------------------------
// List edits (Ships screen). The flat order (ordered()) is the truth; writing ships back in that
// order with each entry's group reproduces it exactly.
// ---------------------------------------------------------------------------------------------

const withShips = (s: ShipNamesSettings, list: Placed[]): ShipNamesSettings => ({
  ...s,
  ships: list.map((p) => ({ ...p.entry, group: p.group?.id ?? null })),
});

const indexOf = (list: Placed[], hull: string) => list.findIndex((p) => fold(p.entry.hull) === fold(hull));

/** Move one place up (-1) or down (+1). At a block border the ship joins the neighbouring block. */
export function moveBy(s: ShipNamesSettings, hull: string, delta: -1 | 1): ShipNamesSettings {
  const list = ordered(s);
  const i = indexOf(list, hull);
  const j = i + delta;
  if (i < 0) return s;
  const me = list[i]!;
  const blocks = blockIds(s);
  const bi = blocks.indexOf(me.group?.id ?? null);
  if (j < 0 || j >= list.length) {
    // At the very top or bottom: step into an empty neighbouring block, if there is one.
    const nb = blocks[bi + delta];
    if (nb === undefined) return s;
    list[i] = { ...me, group: groupById(s, nb) };
    return withShips(s, list);
  }
  const other = list[j]!;
  if ((other.group?.id ?? null) === (me.group?.id ?? null)) {
    list[i] = other;
    list[j] = me;
  } else {
    // Crossing a border: take the neighbour block (an empty block in between counts).
    const nb = blocks[bi + delta]!;
    list[i] = { ...me, group: groupById(s, nb) };
  }
  return withShips(s, list);
}

const groupById = (s: ShipNamesSettings, id: string | null) => (id === null ? null : (s.groups.find((g) => g.id === id) ?? null));
/** Block ids in order: null (ungrouped) then every group. */
const blockIds = (s: ShipNamesSettings): (string | null)[] => [null, ...s.groups.map((g) => g.id)];

/** Move to the top or bottom of the whole list (joins the first or last block). */
export function moveToEnd(s: ShipNamesSettings, hull: string, end: 'top' | 'bottom'): ShipNamesSettings {
  const list = ordered(s);
  const i = indexOf(list, hull);
  if (i < 0) return s;
  const [me] = list.splice(i, 1);
  if (!list.length) return s;
  if (end === 'top') list.unshift({ ...me!, group: list[0]!.group });
  else list.push({ ...me!, group: list[list.length - 1]!.group });
  return withShips(s, list);
}

/** Move to the end of the previous block (-1) or the start of the next one (+1). */
export function moveToBlock(s: ShipNamesSettings, hull: string, dir: -1 | 1): ShipNamesSettings {
  const list = ordered(s);
  const i = indexOf(list, hull);
  if (i < 0) return s;
  const blocks = blockIds(s);
  const bi = blocks.indexOf(list[i]!.group?.id ?? null);
  const target = blocks[bi + dir];
  if (target === undefined) return s;
  return placeIn(s, hull, target, dir < 0 ? 'last' : 'first');
}

/** Put a ship first or last in a block (`null` = ungrouped), or before another ship. */
export function placeIn(s: ShipNamesSettings, hull: string, block: string | null, where: 'first' | 'last' | { before: string }): ShipNamesSettings {
  const list = ordered(s);
  const i = indexOf(list, hull);
  if (i < 0) return s;
  const [me] = list.splice(i, 1);
  const moved: Placed = { ...me!, group: groupById(s, block) };
  const inBlock = list.map((p, k) => [p, k] as const).filter(([p]) => (p.group?.id ?? null) === block);
  let at: number;
  if (typeof where === 'object') {
    const k = indexOf(list, where.before);
    at = k < 0 ? list.length : k;
  } else if (inBlock.length) at = where === 'first' ? inBlock[0]![1] : inBlock[inBlock.length - 1]![1] + 1;
  else {
    // Empty block: after every block before it.
    const bi = blockIds(s).indexOf(block);
    const before = new Set(blockIds(s).slice(0, bi));
    at = list.filter((p) => before.has(p.group?.id ?? null)).length;
  }
  list.splice(at, 0, moved);
  return withShips(s, list);
}

export function addShip(s: ShipNamesSettings, hull: string): ShipNamesSettings {
  if (s.ships.some((e) => fold(e.hull) === fold(hull))) return s;
  return withShips(s, [...ordered(s), { entry: { hull, group: null, name: null, short: null }, group: null }].sort((a, b) => (a.group === null ? 0 : 1) - (b.group === null ? 0 : 1)));
}

export function removeShip(s: ShipNamesSettings, hull: string): ShipNamesSettings {
  return { ...s, ships: s.ships.filter((e) => fold(e.hull) !== fold(hull)) };
}

/** Rename; an empty or null name resets to stock. */
export function rename(s: ShipNamesSettings, hull: string, name: string | null): ShipNamesSettings {
  const v = name?.trim() ? name.trim() : null;
  return { ...s, ships: s.ships.map((e) => (fold(e.hull) === fold(hull) ? { ...e, name: v, short: v === null ? null : e.short } : e)) };
}

export function addGroup(s: ShipNamesSettings, name: string): [ShipNamesSettings, string] {
  const base = name.trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'group';
  let id = base;
  for (let n = 2; s.groups.some((g) => g.id === id); n++) id = `${base}-${n}`;
  return [{ ...s, groups: [...s.groups, { id, name: name.trim() || 'Group' }] }, id];
}

export function renameGroup(s: ShipNamesSettings, id: string, name: string): ShipNamesSettings {
  return { ...s, groups: s.groups.map((g) => (g.id === id ? { ...g, name: name.trim() || g.name } : g)) };
}

/** Delete a group; its ships become ungrouped (they keep their order among themselves). */
export function deleteGroup(s: ShipNamesSettings, id: string): ShipNamesSettings {
  const list = ordered(s).map((p) => (p.group?.id === id ? { ...p, group: null } : p));
  return withShips({ ...s, groups: s.groups.filter((g) => g.id !== id) }, list);
}

/** Move a whole group one block up or down. */
export function moveGroup(s: ShipNamesSettings, id: string, dir: -1 | 1): ShipNamesSettings {
  const i = s.groups.findIndex((g) => g.id === id);
  const j = i + dir;
  if (i < 0 || j < 0 || j >= s.groups.length) return s;
  const groups = [...s.groups];
  [groups[i], groups[j]] = [groups[j]!, groups[i]!];
  return { ...s, groups };
}

/** "Gladius, position 3 of 12, group Combat" for the live region. */
export function announce(s: ShipNamesSettings, hull: string, label: string): string {
  const list = ordered(s);
  const i = indexOf(list, hull);
  if (i < 0) return `${label} removed from the list`;
  const g = list[i]!.group;
  return `${label}, position ${i + 1} of ${list.length}${g ? `, group ${g.name}` : ', no group'}`;
}

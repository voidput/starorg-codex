/**
 * TS mock of `blueprint_finder` (addendum §7.19): the same merge as the Rust mock (core
 * `blueprints::finder::build` over `mock_finder.rs`), over the synthetic catalogue in the
 * `blueprint_finder` sample and the TS mock's own owned sets, so imports and pulls show up.
 * tests/bpfinder.test.ts replays it against the Rust samples.
 */
import type * as T from './types';

export interface FinderChan {
  channel: string;
  owned: Map<string, T.OwnedBlueprint[]>;
  forgotten: Set<string>;
}

export interface FinderCharacter {
  id: string;
  name: string;
  last_seen: string | null;
}

/** `blueprints::strip_tags` + `normalize`: leading `[..]` groups off, quotes unified, spaces collapsed. */
export function normalize(name: string): string {
  let s = name.trimStart();
  while (s.startsWith('[')) {
    const end = s.indexOf(']');
    if (end < 0) break;
    s = s.slice(end + 1).trimStart();
  }
  return s
    .trim()
    .replace(/[‘’‚‛′`´ʼ]/g, "'")
    .replace(/[“”„‟″]/g, '"')
    .replace(/\s+/g, ' ')
    .toLowerCase();
}

const byte = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);

/** Owned sets per channel and character, from the sample's owners (initial mock state). */
/** Includes the `unattributed` set under its own key. */
export function seedOwned(sample: T.BlueprintFinder, channel: string): Map<string, T.OwnedBlueprint[]> {
  const out = new Map<string, T.OwnedBlueprint[]>();
  for (const c of sample.characters) out.set(c.id, []);
  for (const r of sample.rows)
    for (const o of r.owners)
      if (o.channel === channel) out.get(o.character)?.push({ name: r.name, ambiguous: o.ambiguous && r.candidates.length <= 1, favorite: o.favorite, sources: [...o.sources], acquired: o.acquired });
  return out;
}

/** Catalogue of one channel: the sample rows that channel's build has. `null` = not read. */
function catalog(sample: T.BlueprintFinder, channel: string): T.BlueprintRow[] | null {
  if (!sample.channels.some((c) => c.channel === channel && c.catalog !== null)) return null;
  return sample.rows.filter((r) => r.record !== null && r.channels.includes(channel)).map((r) => ({ ...r, owners: [], maybe: [] }));
}

/** Owner id of `unattributed.json` (core `finder::UNATTRIBUTED`). */
export const UNATTRIBUTED = 'unattributed';

/** An owned entry; `ids` = blueprint ids an import matched (the Rust store has them). */
type Entry = T.OwnedBlueprint & { ids?: string[] };

function addOwner(list: T.BlueprintOwner[], o: T.BlueprintOwner) {
  const e = list.find((x) => x.channel === o.channel && x.character === o.character);
  if (!e) return void list.push(o);
  for (const s of o.sources) if (!e.sources.includes(s)) e.sources.push(s);
  e.acquired = e.acquired && o.acquired ? (e.acquired < o.acquired ? e.acquired : o.acquired) : (e.acquired ?? o.acquired);
  e.favorite ||= o.favorite;
  e.ambiguous &&= o.ambiguous;
}

/** Same steps as core `blueprints::finder::build`. */
export function mockFinder(sample: T.BlueprintFinder, chans: FinderChan[], characters: FinderCharacter[]): T.BlueprintFinder {
  const rows: T.BlueprintRow[] = [];
  const byKey = new Map<string, number>();
  const namesIn = new Map<string, Map<string, string[]>>();
  const namesAll = new Map<string, string[]>();
  const addName = (m: Map<string, string[]>, n: string, k: string) => {
    const v = m.get(n) ?? [];
    if (!v.includes(k)) v.push(k);
    m.set(n, v);
  };
  for (const ch of chans) {
    const cat = catalog(sample, ch.channel);
    if (!cat) continue;
    const names = new Map<string, string[]>();
    namesIn.set(ch.channel, names);
    for (const c of cat) {
      let i = byKey.get(c.key);
      if (i === undefined) {
        rows.push({ ...structuredClone(c), channels: [], candidates: [], owners: [], maybe: [] });
        byKey.set(c.key, (i = rows.length - 1));
      }
      if (!rows[i]!.channels.includes(ch.channel)) rows[i]!.channels.push(ch.channel);
      addName(names, normalize(c.name), c.key);
      addName(namesAll, normalize(c.name), c.key);
    }
  }
  const catalogRows = rows.length;
  const inCatalog = (k: string) => (byKey.get(k) ?? Infinity) < catalogRows;
  const outChans: T.BlueprintFinderChannel[] = [];
  const chars = new Map<string, T.BlueprintFinderCharacter>();
  for (const ch of chans) {
    const live = characters.filter((c) => !ch.forgotten.has(c.id));
    for (const c of live) {
      const e = chars.get(c.id) ?? { id: c.id, name: c.name, last_seen: c.last_seen, channels: [] };
      e.channels.push(ch.channel);
      chars.set(c.id, e);
    }
    const sets: [string, Entry[]][] = [...live].sort((a, b) => byte(a.id, b.id)).map((c) => [c.id, ch.owned.get(c.id) ?? []]);
    const un = ch.owned.get(UNATTRIBUTED) ?? [];
    if (un.length) {
      sets.push([UNATTRIBUTED, un]);
      const e = chars.get(UNATTRIBUTED) ?? { id: UNATTRIBUTED, name: 'Unattributed', last_seen: null, channels: [] };
      e.channels.push(ch.channel);
      chars.set(UNATTRIBUTED, e);
    }
    let owned = 0;
    for (const [who, list] of sets) {
      // Entries keyed by normalised name, in key order (a BTreeMap on the Rust side).
      const entries = new Map<string, Entry>();
      for (const b of list) entries.set(normalize(b.name), b);
      for (const [norm, b] of [...entries].sort((x, y) => byte(x[0], y[0]))) {
        owned++;
        const look = (m: Map<string, string[]> | undefined, ok: (n: number) => boolean) => {
          const v = m?.get(norm);
          return v && ok(v.length) ? [...v] : null;
        };
        const names = namesIn.get(ch.channel);
        let key: string | null = null;
        let candidates: string[] = [];
        if (b.ambiguous) {
          const ids = (b.ids ?? []).map((x) => x.toLowerCase()).filter(inCatalog);
          candidates = ids.length ? ids : (look(names, (n) => n > 0) ?? look(namesAll, (n) => n > 0) ?? []);
        } else {
          key = (b.ids ?? []).map((x) => x.toLowerCase()).find(inCatalog) ?? look(names, (n) => n === 1)?.[0] ?? look(namesAll, (n) => n === 1)?.[0] ?? null;
          if (key === null) candidates = look(names, (n) => n > 1) ?? look(namesAll, (n) => n > 1) ?? [];
        }
        const ambiguous = key === null && (b.ambiguous || candidates.length > 0);
        key ??= `name:${norm}`;
        let i = byKey.get(key);
        if (i === undefined) {
          rows.push({ key, name: b.name, record: null, kind: 'other', type_label: null, class: null, manufacturer: null, manufacturer_name: null, size: null, grade: null, channels: [], candidates: [], owners: [], maybe: [] });
          byKey.set(key, (i = rows.length - 1));
        }
        for (const c of candidates) if (!rows[i]!.candidates.includes(c)) rows[i]!.candidates.push(c);
        const owner = (): T.BlueprintOwner => ({ channel: ch.channel, character: who, sources: [...b.sources], acquired: b.acquired, favorite: b.favorite, ambiguous });
        for (const c of candidates) {
          const ci = byKey.get(c);
          if (ci !== undefined) addOwner(rows[ci]!.maybe, owner());
        }
        addOwner(rows[i]!.owners, owner());
      }
    }
    const cat = catalog(sample, ch.channel);
    outChans.push({ channel: ch.channel, catalog: cat ? cat.length : null, characters: live.length, owned, skipped: 0 });
  }
  // A name row takes what all its candidates agree on.
  for (let i = catalogRows; i < rows.length; i++) {
    const r = rows[i]!;
    const cands = r.candidates.map((k) => rows[byKey.get(k)!]!).filter(Boolean);
    const first = cands[0];
    if (!first) continue;
    for (const f of ['kind', 'type_label', 'class', 'manufacturer', 'manufacturer_name', 'size', 'grade'] as const)
      if (cands.every((c) => c[f] === first[f])) (r as unknown as Record<string, unknown>)[f] = first[f];
    for (const c of cands) for (const x of c.channels) if (!r.channels.includes(x)) r.channels.push(x);
  }
  rows.sort((a, b) => byte(a.name.toLowerCase(), b.name.toLowerCase()) || byte(a.key, b.key));
  const outChars = [...chars.values()].sort(
    (a, b) => Number(a.id === UNATTRIBUTED) - Number(b.id === UNATTRIBUTED) || byte(a.name.toLowerCase(), b.name.toLowerCase()) || byte(a.id, b.id),
  );
  return { channels: outChans, characters: outChars, rows };
}

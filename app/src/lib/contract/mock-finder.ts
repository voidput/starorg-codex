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
export function seedOwned(sample: T.BlueprintFinder, channel: string): Map<string, T.OwnedBlueprint[]> {
  const out = new Map<string, T.OwnedBlueprint[]>();
  for (const c of sample.characters) out.set(c.id, []);
  for (const r of sample.rows)
    for (const o of r.owners)
      if (o.channel === channel) out.get(o.character)?.push({ name: r.name, ambiguous: o.ambiguous, favorite: o.favorite, sources: [...o.sources], acquired: o.acquired });
  return out;
}

/** Catalogue of one channel: the sample rows that channel's build has. `null` = not read. */
function catalog(sample: T.BlueprintFinder, channel: string): T.BlueprintRow[] | null {
  if (!sample.channels.some((c) => c.channel === channel && c.catalog !== null)) return null;
  return sample.rows.filter((r) => r.record !== null && r.channels.includes(channel));
}

export function mockFinder(sample: T.BlueprintFinder, chans: FinderChan[], characters: FinderCharacter[]): T.BlueprintFinder {
  const rows: T.BlueprintRow[] = [];
  const byKey = new Map<string, T.BlueprintRow>();
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
      let r = byKey.get(c.key);
      if (!r) {
        r = { ...structuredClone(c), channels: [], owners: [] };
        byKey.set(c.key, r);
        rows.push(r);
      }
      if (!r.channels.includes(ch.channel)) r.channels.push(ch.channel);
      addName(names, normalize(c.name), c.key);
      addName(namesAll, normalize(c.name), c.key);
    }
  }
  const outChans: T.BlueprintFinderChannel[] = [];
  const chars = new Map<string, T.BlueprintFinderCharacter>();
  for (const ch of chans) {
    const live = characters.filter((c) => !ch.forgotten.has(c.id));
    for (const c of live) {
      const e = chars.get(c.id) ?? { id: c.id, name: c.name, last_seen: c.last_seen, channels: [] };
      e.channels.push(ch.channel);
      chars.set(c.id, e);
    }
    let owned = 0;
    for (const c of live) {
      // Entries keyed by normalised name, in key order (a BTreeMap on the Rust side).
      const entries = new Map<string, T.OwnedBlueprint>();
      for (const b of ch.owned.get(c.id) ?? []) entries.set(normalize(b.name), b);
      for (const [norm, b] of [...entries].sort((x, y) => byte(x[0], y[0]))) {
        owned++;
        const unique = (m: Map<string, string[]> | undefined) => {
          const v = m?.get(norm);
          return v && v.length === 1 ? v[0]! : null;
        };
        const key = (b.ambiguous ? null : (unique(namesIn.get(ch.channel)) ?? unique(namesAll))) ?? `name:${norm}`;
        let r = byKey.get(key);
        if (!r) {
          r = { key, name: b.name, record: null, kind: 'other', type_label: null, class: null, manufacturer: null, manufacturer_name: null, size: null, grade: null, channels: [], owners: [] };
          byKey.set(key, r);
          rows.push(r);
        }
        r.owners.push({ channel: ch.channel, character: c.id, sources: [...b.sources], acquired: b.acquired, favorite: b.favorite, ambiguous: b.ambiguous });
      }
    }
    const cat = catalog(sample, ch.channel);
    outChans.push({ channel: ch.channel, catalog: cat ? cat.length : null, characters: live.length, owned });
  }
  rows.sort((a, b) => byte(a.name.toLowerCase(), b.name.toLowerCase()) || byte(a.key, b.key));
  const outChars = [...chars.values()].sort((a, b) => byte(a.name.toLowerCase(), b.name.toLowerCase()) || byte(a.id, b.id));
  return { channels: outChans, characters: outChars, rows };
}

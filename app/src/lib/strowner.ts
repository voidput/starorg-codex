/**
 * Strings owners in words: the grid's "String" column and the owners popover. The backend
 * sends `KeyOwner`s; this file only words them (and, for the mock, makes a key readable the
 * way `starorg_core::groups::owners::pretty_key` does).
 */
import type { KeyOwner, OwnerField, OwnerKind } from './contract/types';

export const FIELD_LABEL: Record<OwnerField, string> = {
  name: 'name',
  'short-name': 'short name',
  description: 'description',
  title: 'title',
  career: 'career',
  role: 'role',
};

export const KIND_LABEL: Record<OwnerKind, string> = {
  ship: 'Ship',
  'ground-vehicle': 'Ground vehicle',
  component: 'Ship component',
  'ship-weapon': 'Ship weapon',
  'fps-weapon': 'FPS weapon',
  missile: 'Missile',
  mission: 'Mission',
  reputation: 'Reputation',
  manufacturer: 'Manufacturer',
  named: 'Named by key',
  key: 'Key',
};

/** `Bracer · description`; a readable-key fallback is its name alone. */
export function ownerLabel(o: KeyOwner | null, key: string): string {
  if (!o) return prettyKey(key);
  return o.field ? `${o.name} · ${FIELD_LABEL[o.field]}` : o.name;
}

/** `Ship component · Aegis Dynamics` (empty for the fallback and for named keys without a maker). */
export function ownerContext(o: KeyOwner | null): string {
  if (!o || o.kind === 'key') return '';
  const kind = o.kind === 'named' ? '' : KIND_LABEL[o.kind];
  return [kind, o.detail].filter(Boolean).join(' · ');
}

/** What a shared key's cell says: `3 records · description`. */
export function sharedLabel(count: number, o: KeyOwner | null): string {
  const f = o?.field ? ` · ${FIELD_LABEL[o.field]}` : '';
  return `${count.toLocaleString('en-US')} records${f}`;
}

const FIELD_WORDS = new Set(['name', 'desc', 'description', 'shortname', 'nameshort', 'short', 'title']);
const isLower = (c: string | undefined) => !!c && c >= 'a' && c <= 'z';
const isUpper = (c: string | undefined) => !!c && c >= 'A' && c <= 'Z';
const isDigit = (c: string | undefined) => !!c && c >= '0' && c <= '9';

function stripFlag(k: string): string {
  const m = /,[A-Z]+$/.exec(k);
  return m ? k.slice(0, m.index) : k;
}

/** `law_TrespassAreaRevoked` -> `law trespass area revoked`; acronyms (`CI`) stay. */
function words(s: string): string {
  const out: string[] = [];
  for (const token of s.split('_').filter(Boolean)) {
    const chars = [...token];
    let cur = '';
    chars.forEach((c, i) => {
      const prev = chars[i - 1];
      const next = chars[i + 1];
      const boundary = isUpper(c) && prev !== undefined && (isLower(prev) || isDigit(prev) || (isUpper(prev) && isLower(next)));
      if (boundary && cur) {
        out.push(cur);
        cur = '';
      }
      cur += c;
    });
    if (cur) out.push(cur);
  }
  return out
    .map((w) => {
      const caps = [...w].filter(isUpper).length;
      return caps >= 2 && ![...w].some(isLower) ? w : w.toLowerCase();
    })
    .join(' ');
}

/** `item_DescCOOL_AEGS_S01_Bracer` -> `item desc · COOL_AEGS_S01_Bracer` (same rules as the core). */
export function prettyKey(raw: string): string {
  const key = stripFlag(raw.startsWith('@') ? raw.slice(1) : raw);
  const us = key.indexOf('_');
  if (us < 0) return words(key);
  let prefix = key.slice(0, us).toLowerCase();
  let rest = key.slice(us + 1);
  let lead = rest.length;
  for (let i = 1; i < rest.length; i++) {
    if (!isLower(rest[i])) {
      lead = i;
      break;
    }
  }
  const word = rest.slice(0, lead);
  if (word && FIELD_WORDS.has(word.toLowerCase())) {
    const after = rest[lead];
    if (after === undefined || after === '_' || isUpper(after) || isDigit(after)) {
      prefix += ` ${word.toLowerCase()}`;
      rest = rest.slice(lead).replace(/^_+/, '');
    }
  }
  if (!rest) return prefix;
  const coded = rest.split('_').some((t) => t.length >= 2 && [...t].some(isUpper) && ![...t].some(isLower));
  return `${prefix} · ${coded ? rest : words(rest)}`;
}

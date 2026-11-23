// Open or closed per viewer for the Explorer's foldable cards (Combat open by default,
// Sources closed). Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { loadOpen, saveOpen } from '../src/lib/cardopen.ts';
import { COMBAT_OPEN_KEY } from '../src/lib/combat.ts';
import { SOURCES_OPEN_KEY } from '../src/lib/sources.ts';

const mem = () => {
  const m = new Map<string, string>();
  return { m, s: { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v) } };
};
const blocked = {
  getItem: (): string | null => {
    throw new Error('blocked');
  },
  setItem: () => {
    throw new Error('blocked');
  },
};

test('a card keeps its default until the viewer toggles it', () => {
  const { m, s } = mem();
  assert.equal(loadOpen(s, COMBAT_OPEN_KEY, true), true);
  assert.equal(loadOpen(s, SOURCES_OPEN_KEY, false), false);
  saveOpen(s, COMBAT_OPEN_KEY, false);
  saveOpen(s, SOURCES_OPEN_KEY, true);
  assert.equal(m.get(COMBAT_OPEN_KEY), '0');
  assert.equal(m.get(SOURCES_OPEN_KEY), '1');
  assert.equal(loadOpen(s, COMBAT_OPEN_KEY, true), false);
  assert.equal(loadOpen(s, SOURCES_OPEN_KEY, false), true);
});

test('blocked, missing or odd storage falls back to the default', () => {
  assert.equal(loadOpen(blocked, 'k', true), true);
  assert.equal(loadOpen(blocked, 'k', false), false);
  assert.doesNotThrow(() => saveOpen(blocked, 'k', true));
  assert.equal(loadOpen(null, 'k', true), true);
  const { m, s } = mem();
  m.set('k', 'yes');
  assert.equal(loadOpen(s, 'k', false), false);
});

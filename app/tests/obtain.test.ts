// Where to get (src/lib/obtain.ts): mock preview samples against the Rust mock, mode and
// line switches, UEX file age. Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { fileAge, obtainPreview, OBTAIN_MODES } from '../src/lib/obtain.ts';
import { previewOrder } from '../src/lib/bpmarkers.ts';
import type { PreviewItem, Profile } from '../src/lib/contract/types.ts';
import samples from '../src/lib/contract/samples.gen.json' with { type: 'json' };

// samples.build_preview is the Rust mock with the Everything preset.
const everything = (): Profile => ({
  ...(samples.get_profile as unknown as Profile),
  obtain: true,
  obtain_mode: 'detailed',
  obtain_shops: true,
  fps_weapons: true,
  lines_off: [],
  header: '--- STATS ---',
  stats_above: false,
});
const stats = (prose: string, lines: string[]) => (lines.length ? `${prose}\\n\\n--- STATS ---\\n${lines.join('\\n')}` : prose);

test('the TS mock samples equal the Rust mock (Everything)', () => {
  const p = everything();
  p.mission = { ...p.mission, header_em: 3, label_em: 4 };
  const rust = (samples.build_preview as PreviewItem[]).filter((i) => i.category === 'obtain-hints');
  assert.equal(rust.length, 2);
  assert.deepEqual(obtainPreview(p, stats), rust);
});

test('modes, shops and line switches change the block', () => {
  const p = everything();
  p.mission = { ...p.mission, header_em: 0, label_em: 0 };
  p.obtain_mode = 'short';
  p.obtain_shops = false;
  const [gun, ore] = obtainPreview(p, stats);
  assert.ok(gun!.result.endsWith('\\n\\nGet: Loot (Rare) · Craft [BP] · Reward'), gun!.result);
  assert.ok(ore!.result.endsWith('Get: Mined (ship) at Lyria, Aaron Halo, Pyro I +1 · Quality most near 500'), ore!.result);
  p.obtain_mode = 'normal';
  const [g2] = obtainPreview(p, stats);
  assert.ok(g2!.result.includes('WHERE TO GET\\nLoot: Weapons rack (Rare) at Bunkers, Weapon crate (Uncommon) at Derelict outposts, Locker (Common) at Caves +3'), g2!.result);
  assert.ok(!g2!.result.includes('Buy:') && !g2!.result.includes('%'));
  p.lines_off = ['obtain.loot', 'obtain.craft', 'obtain.rewards'];
  assert.deepEqual(
    obtainPreview(p, stats).map((i) => i.key),
    ['items_commodities_SYN_ore_desc'],
  );
  p.obtain = false;
  assert.deepEqual(obtainPreview(p, stats), []);
  assert.deepEqual(OBTAIN_MODES.map((m) => m.id), ['short', 'normal', 'detailed']);
});

test('the Where to get tab puts its samples first', () => {
  const items = samples.build_preview as PreviewItem[];
  assert.equal(previewOrder(items, 'obtain-hints')[0]!.category, 'obtain-hints');
});

test('UEX file age', () => {
  const now = new Date('2026-11-21T12:00:00Z');
  assert.equal(fileAge(null, now), null);
  assert.equal(fileAge('bad', now), null);
  assert.equal(fileAge('2026-11-21T08:00:00Z', now), 'today');
  assert.equal(fileAge('2026-11-20T08:00:00Z', now), '1 day old');
  assert.equal(fileAge('2026-11-12T08:30:00Z', now), '9 days old');
});

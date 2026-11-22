// Where to get (src/lib/obtain.ts): mock preview samples against the Rust mock, mode and
// line switches, UEX file age. Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { fileAge, obtainPreview, OBTAIN_MODES, roughPrice, shortDay, uexFilesText, uexTokenText } from '../src/lib/obtain.ts';
import { sectionPreview } from '../src/lib/previewsection.ts';
import type { ObtainStatus, PreviewItem, Profile, UexTokenStatus } from '../src/lib/contract/types.ts';
import samples from '../src/lib/contract/samples.gen.json' with { type: 'json' };

// samples.build_preview is the Rust mock with the Everything preset.
const everything = (): Profile => ({
  ...(samples.get_profile as unknown as Profile),
  obtain: true,
  obtain_mode: 'detailed',
  obtain_shops: true,
  obtain_prices: true,
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
  p.obtain_prices = false;
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

test('the sell line: its switch, its line, rounded prices and the fetch day', () => {
  const p = everything();
  p.mission = { ...p.mission, header_em: 0, label_em: 0 };
  p.obtain_mode = 'normal';
  const ore = () => obtainPreview(p, stats)[1]!.result;
  assert.ok(ore().includes('WHERE TO GET\\nSell: ~9.6k aUEC/SCU at HUR-L2, ~9.2k at Rod\'s Fuel, ~9k at TDD Area 18 +1 (UEX, 21 Nov)\\nMined:'), ore());
  p.obtain_mode = 'short';
  assert.ok(ore().endsWith('Get: Sell ~9.6k aUEC/SCU at HUR-L2 (UEX, 21 Nov) · Mined (ship) at Lyria, Aaron Halo, Pyro I +1 · Quality most near 500'), ore());
  p.obtain_mode = 'detailed';
  assert.ok(ore().includes('~9k at TDD Area 18 (Stanton)  |  +1 (UEX, 21 Nov)'), ore());
  p.lines_off = ['obtain.sell'];
  assert.ok(!ore().includes('Sell'));
  p.lines_off = [];
  p.obtain_prices = false;
  assert.ok(!ore().includes('aUEC'));
  // The gun has no sell line.
  p.obtain_prices = true;
  assert.ok(!obtainPreview(p, stats)[0]!.result.includes('Sell'));
});

test('prices round to two digits; days read short', () => {
  const cases: [number, string][] = [
    [6240, '~6.2k'],
    [9000, '~9k'],
    [9612, '~9.6k'],
    [12_345, '~12k'],
    [123_456, '~120k'],
    [999_999, '~1M'],
    [1_540_000, '~1.5M'],
    [845, '~850'],
    [38, '~38'],
    [3.84, '~3.8'],
    [0.456, '~0.46'],
  ];
  for (const [v, want] of cases) assert.equal(roughPrice(v), want, String(v));
  assert.equal(shortDay('2026-11-21T09:00:00.000Z'), '21 Nov');
  assert.equal(shortDay('2026-01-05'), '5 Jan');
  assert.equal(shortDay('2026-13-05'), null);
  assert.equal(shortDay(null), null);
});

test('the Where to get tab shows only its samples', () => {
  const items = samples.build_preview as PreviewItem[];
  assert.deepEqual(sectionPreview(items, 'obtain').map((i) => i.label), ['Weapon (where to get)', 'Ore (where to get)']);
});

test('UEX file age', () => {
  const now = new Date('2026-11-21T12:00:00Z');
  assert.equal(fileAge(null, now), null);
  assert.equal(fileAge('bad', now), null);
  assert.equal(fileAge('2026-11-21T08:00:00Z', now), 'today');
  assert.equal(fileAge('2026-11-20T08:00:00Z', now), '1 day old');
  assert.equal(fileAge('2026-11-12T08:30:00Z', now), '9 days old');
});

test('UEX file ages and token status read without the token', () => {
  const now = new Date('2026-11-21T12:00:00Z');
  assert.equal(uexFilesText(samples.obtain_status as ObtainStatus, now), 'Item prices 9 days old · Commodity prices missing · Terminals missing');
  assert.equal(uexFilesText(samples.fetch_uex as ObtainStatus, now), 'Item prices today · Commodity prices today · Terminals today');
  assert.ok((samples.fetch_uex as ObtainStatus).prices_ready && !(samples.obtain_status as ObtainStatus).prices_ready);
  assert.match(uexTokenText(samples.uex_token_status as UexTokenStatus), /No token saved/);
  assert.equal(uexTokenText(samples.set_uex_token as UexTokenStatus), 'Token saved (keyring)');
  assert.equal(uexTokenText({ set: true, store: 'file' }), 'Token saved (file)');
  assert.ok(!JSON.stringify(samples.set_uex_token).includes('sample-token'));
});

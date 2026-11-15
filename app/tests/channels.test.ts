// Shared channel colours (addendum §7.19): fixed for the known channels, stable for others,
// readable on the card and muted backgrounds of both themes. Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { channelColor, channelStyle } from '../src/lib/channels.ts';

const lum = (hex: string) => {
  const c = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * c[0]! + 0.7152 * c[1]! + 0.0722 * c[2]!;
};
const contrast = (a: string, b: string) => {
  const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p);
  return (x! + 0.05) / (y! + 0.05);
};

test('known channels have fixed, distinct colours; case does not matter', () => {
  const live = channelColor('LIVE');
  assert.deepEqual(channelColor('live'), live);
  const set = new Set(['LIVE', 'PTU', 'EPTU', 'HOTFIX', 'TECH-PREVIEW'].map((c) => channelColor(c).dark));
  assert.equal(set.size, 5);
  assert.equal(channelStyle('PTU'), `--chan-d: ${channelColor('PTU').dark}; --chan-l: ${channelColor('PTU').light};`);
});

test('other channels get a stable colour', () => {
  assert.deepEqual(channelColor('EVOCATI'), channelColor('EVOCATI'));
});

test('every colour reads at 4.5:1 on card and muted, dark and light', () => {
  for (const ch of ['LIVE', 'PTU', 'EPTU', 'HOTFIX', 'TECH-PREVIEW', 'EVOCATI', 'TEST', 'CH0', 'CH1', 'X']) {
    const c = channelColor(ch);
    for (const bg of ['#0f172a', '#1e293b', '#0a0a0c']) assert.ok(contrast(c.dark, bg) >= 4.5, `${ch} dark on ${bg}: ${contrast(c.dark, bg).toFixed(2)}`);
    for (const bg of ['#ffffff', '#e3e9f1']) assert.ok(contrast(c.light, bg) >= 4.5, `${ch} light on ${bg}: ${contrast(c.light, bg).toFixed(2)}`);
  }
});

// Diff's first A and B: the channel's previous cached build against its current one when
// there is one, else the first two sources. Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { defaultPair } from '../src/lib/diffpick.ts';
import type { DevSource } from '../src/lib/contract/dev.ts';

const src = (id: string, kind: DevSource['kind'], state: DevSource['state'] = 'ready'): DevSource => ({
  id,
  kind,
  label: id,
  build: null,
  state,
  detail: null,
  datasets: [],
});

test('the previous cached build is A, the current build B', () => {
  const list = [src('build:PTU', 'build'), src('cached:PTU:5.0.201.1-0a1b2c3d', 'cached'), src('cached:PTU:5.0.200.1-0a1b2c3e', 'cached'), src('build:LIVE', 'build')];
  assert.deepEqual(defaultPair(list, 'PTU'), { a: 'cached:PTU:5.0.201.1-0a1b2c3d', b: 'build:PTU' });
});

test("another channel's cached build or an error one does not count", () => {
  const list = [src('build:LIVE', 'build'), src('cached:LIVE:4.9.0.1-00000000', 'cached', 'error'), src('build:PTU', 'build'), src('cached:PTU:5.0.0.1-00000000', 'cached')];
  assert.deepEqual(defaultPair(list, 'LIVE'), { a: 'build:LIVE', b: 'build:PTU' });
});

test('without a ready build: the first two ready sources, or none', () => {
  const list = [src('build:LIVE', 'build', 'missing'), src('cached:LIVE:4.9.0.1-00000000', 'cached'), src('folder:0', 'folder')];
  assert.deepEqual(defaultPair(list, 'LIVE'), { a: 'cached:LIVE:4.9.0.1-00000000', b: 'folder:0' });
  assert.deepEqual(defaultPair([src('folder:0', 'folder')], 'LIVE'), { a: 'folder:0', b: null });
  assert.deepEqual(defaultPair(null, 'LIVE'), { a: null, b: null });
});

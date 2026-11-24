// Diff's first A and B: the channel's previous cached build against its current one when
// there is one, else the first two sources. Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { defaultPair, keepPair } from '../src/lib/diffpick.ts';
import type { DevSource } from '../src/lib/contract/dev.ts';

const src = (id: string, kind: DevSource['kind'], state: DevSource['state'] = 'ready', build: string | null = null, files: string[] = ['game-weapons.json']): DevSource => ({
  id,
  kind,
  label: id,
  build,
  state,
  detail: null,
  datasets: files.map((file) => ({ file, bytes: 1, tables: 1, rows: 1 })),
});

test('a cached build with no dataset in common with the current one is not the default', () => {
  // Right after a game update: the current build has only GameData read so far.
  const list = [src('build:LIVE', 'build', 'ready', '4.10.200.1', ['gamedata.json']), src('cached:LIVE:4.10.197.2-0a1b2c3d', 'cached', 'ready', '4.10.197.2'), src('build:PTU', 'build')];
  assert.deepEqual(defaultPair(list, 'LIVE'), { a: 'build:LIVE', b: 'build:PTU' });
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

test('the same game version read again is skipped for one that differs', () => {
  const list = [
    src('build:LIVE', 'build', 'ready', '4.10.200.1'),
    src('cached:LIVE:4.10.200.1-deadbeef', 'cached', 'ready', '4.10.200.1'),
    src('cached:LIVE:4.10.197.2-0a1b2c3d', 'cached', 'ready', '4.10.197.2'),
  ];
  assert.deepEqual(defaultPair(list, 'LIVE'), { a: 'cached:LIVE:4.10.197.2-0a1b2c3d', b: 'build:LIVE' });
  // Only the same version kept: still that one, it is the build read before.
  assert.deepEqual(defaultPair(list.slice(0, 2), 'LIVE'), { a: 'cached:LIVE:4.10.200.1-deadbeef', b: 'build:LIVE' });
});

test('keepPair keeps listed picks and replaces a pruned one', () => {
  const list = [src('build:LIVE', 'build'), src('cached:LIVE:4.9.0.1-00000000', 'cached'), src('folder:0', 'folder')];
  assert.deepEqual(keepPair(list, 'LIVE', { a: 'folder:0', b: 'build:LIVE' }), { a: 'folder:0', b: 'build:LIVE' });
  assert.deepEqual(keepPair(list, 'LIVE', { a: null, b: null }), { a: 'cached:LIVE:4.9.0.1-00000000', b: 'build:LIVE' });
  // A pruned older build as A: the default pair again.
  assert.deepEqual(keepPair(list, 'LIVE', { a: 'cached:LIVE:4.8.0.1-00000000', b: 'build:LIVE' }), { a: 'cached:LIVE:4.9.0.1-00000000', b: 'build:LIVE' });
  // A pruned B: another source, never A itself.
  assert.deepEqual(keepPair(list, 'LIVE', { a: 'build:LIVE', b: 'cached:LIVE:4.8.0.1-00000000' }), { a: 'build:LIVE', b: 'cached:LIVE:4.9.0.1-00000000' });
  // B kept and the default A is B: A is another one.
  assert.deepEqual(keepPair(list, 'LIVE', { a: 'gone', b: 'cached:LIVE:4.9.0.1-00000000' }), { a: 'build:LIVE', b: 'cached:LIVE:4.9.0.1-00000000' });
  assert.deepEqual(keepPair(null, 'LIVE', { a: 'x', b: 'y' }), { a: null, b: null });
});

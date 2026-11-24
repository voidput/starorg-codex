// Settings > Storage wording (src/lib/storage.ts) and the TS mock's storage commands. Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { AREA_HINTS, areaText, backupRule, capPercent, capText, sizeText } from '../src/lib/storage.ts';
import samples from '../src/lib/contract/samples.gen.json' with { type: 'json' };
import type { StorageStatus } from '../src/lib/contract/types.ts';

const s = samples.storage_status as StorageStatus;
const cleared = samples.storage_clear_assets as StorageStatus;
const MIB = 1024 ** 2;

test('sizes and the cap read plainly', () => {
  assert.equal(sizeText(12), '12 B');
  assert.equal(sizeText(1024 ** 3), '1 GB');
  assert.equal(sizeText(1.5 * 1024 ** 3), '1.5 GB');
  assert.equal(sizeText(120 * MIB), '120 MB');
  assert.equal(capPercent({ cache_bytes: 512 * MIB, cap_bytes: 1024 * MIB }), 50);
  assert.equal(capPercent({ cache_bytes: 3000 * MIB, cap_bytes: 1024 * MIB }), 100);
  assert.equal(capPercent({ cache_bytes: 1, cap_bytes: 0 }), 0);
  assert.equal(capText({ cache_bytes: 312 * MIB, cap_bytes: 1024 ** 3 }), '312 MB of 1 GB');
  assert.match(capText({ cache_bytes: 2 * 1024 ** 3, cap_bytes: 1024 ** 3 }), /over the cap/);
  assert.equal(areaText({ bytes: 48 * MIB, files: 1 }), '48 MB · 1 file');
});

test('the samples list every area, with a hint, and backups outside the cap', () => {
  const ids = s.areas.map((a) => a.id);
  assert.deepEqual(ids, ['game-data', 'where-to-get', 'uex', 'asset-exports', 'temporary', 'telemetry', 'backups']);
  for (const a of s.areas) assert.ok(AREA_HINTS[a.id], a.id);
  const backups = s.areas.find((a) => a.id === 'backups')!;
  assert.equal(backups.in_cache, false);
  assert.equal(s.total_bytes, s.cache_bytes + backups.bytes);
  assert.equal(s.cap_bytes, 1024 ** 3);
  assert.equal(s.backup_keep_newest, 5);
  assert.match(backupRule(s), /5 newest.*last 3 game builds/);
});

test('clearing the asset cache frees exactly its bytes', () => {
  const a = (x: StorageStatus) => x.areas.find((y) => y.id === 'asset-exports')!.bytes;
  assert.ok(a(s) > 0);
  assert.equal(a(cleared), 0);
  assert.equal(s.cache_bytes - cleared.cache_bytes, a(s));
});

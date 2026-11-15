// Memory limit wording and the TS mock's slider (src/lib/memory.ts, mock.ts). Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { clampGb, enforcementText, fitsCount, gbText, limitText, machineText, memoryView, reminderText } from '../src/lib/memory.ts';
import samples from '../src/lib/contract/samples.gen.json' with { type: 'json' };
import type { MemoryStatus } from '../src/lib/contract/types.ts';

const base = samples.memory_status as MemoryStatus;

const GIB = 1024 ** 3;

test('sizes read like the Rust side', () => {
  assert.equal(gbText(4 * GIB), '4 GB');
  assert.equal(gbText(1.5 * GIB), '1.5 GB');
  assert.equal(gbText(300 * 1024 ** 2), '0.3 GB');
  assert.equal(gbText(64 * 1024 ** 2), '64 MB');
  assert.equal(limitText(6.5), '6.5 GB');
  assert.equal(machineText({ total_bytes: 32 * GIB, available_bytes: 18.4 * GIB }), '32 GB RAM, 18.4 GB free now');
  assert.equal(machineText({ total_bytes: null, available_bytes: null }), 'RAM size unknown');
  assert.match(reminderText({ limit_gb: 5 }), /capped at 5 GB/);
  assert.match(enforcementText('watchdog'), /Windows/);
});

test('the mock slider clamps to the step and the bounds, and fits follow the limit', () => {
  const b = { min_gb: 1, max_gb: 30, step_gb: 0.5, default_gb: 5 };
  assert.equal(clampGb(7.3, b), 7.5);
  assert.equal(clampGb(0.2, b), 1);
  assert.equal(clampGb(99, b), 30);
  assert.equal(clampGb(Number.NaN, b), 5);
  const def = memoryView(base, null);
  assert.equal(def.is_default, true);
  assert.equal(def.limit_gb, def.default_gb);
  const small = memoryView(base, 1);
  const big = memoryView(base, 12);
  assert.ok(fitsCount(small)[0] < fitsCount(big)[0]);
  assert.equal(fitsCount(big)[0], fitsCount(big)[1]);
  assert.equal(small.estimates.find((e) => e.id === 'mcp-ships')?.fits, false);
});

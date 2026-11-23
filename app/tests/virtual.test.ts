// The shared windowed-list helpers (VirtualList, Diff records, Files). Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { revealTop, windowRange } from '../src/lib/virtual.ts';

test('the window covers the visible rows plus overscan, clamped to the list', () => {
  assert.deepEqual(windowRange(0, 400, 40, 1000, 5), { first: 0, end: 15 });
  assert.deepEqual(windowRange(4000, 400, 40, 1000, 5), { first: 95, end: 115 });
  assert.deepEqual(windowRange(39_990, 400, 40, 1000, 5), { first: 994, end: 1000 });
  assert.deepEqual(windowRange(100, 400, 40, 0), { first: 0, end: 0 });
  assert.deepEqual(windowRange(-50, 80, 40, 10, 0), { first: 0, end: 2 });
});

test('a row already in view needs no scroll', () => {
  assert.equal(revealTop(0, 40, 0, 400), null);
  assert.equal(revealTop(9, 40, 0, 400), null);
  assert.equal(revealTop(12, 40, 400, 400), null);
});

test('a row above the view aligns to the top, one below to the bottom', () => {
  assert.equal(revealTop(3, 40, 400, 400), 120);
  assert.equal(revealTop(10, 40, 0, 400), 40);
  assert.equal(revealTop(999, 40, 0, 400), 39_600);
});

test('a bad index or row height leaves the scroll alone', () => {
  assert.equal(revealTop(-1, 40, 0, 400), null);
  assert.equal(revealTop(5, 0, 0, 400), null);
  assert.equal(revealTop(0, 40, 0, 10), 30);
});

// Update banner and Settings wording (docs/updates.md), over real Rust mock output.
// Run: npm test (node --test, type stripping).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { bannerRelease, bannerTitle, checkLine, requestHint } from '../src/lib/update.ts';

const samples = JSON.parse(readFileSync(new URL('../src/lib/contract/samples.gen.json', import.meta.url), 'utf8'));
const available = samples.check_for_updates;
const failed = samples.check_for_updates_failed;
const off = samples.update_status;

test('banner shows an available, not skipped version only while the switch is on', () => {
  const r = bannerRelease(available);
  assert.equal(r?.version, '0.2.0');
  assert.equal(bannerTitle(r!), 'StarOrg Strings 0.2.0 is available');
  assert.equal(bannerRelease({ ...available, enabled: false }), null);
  assert.equal(bannerRelease({ ...available, skipped_version: '0.2.0' }), null);
  assert.equal(bannerRelease(available, '0.2.0'), null, 'dismissed for this session');
  assert.equal(bannerRelease({ ...available, skipped_version: '0.1.9' })?.version, '0.2.0');
  assert.equal(bannerRelease(failed), null);
  assert.equal(bannerRelease(off), null);
  assert.equal(bannerRelease(null), null);
  const pre = { ...available.check.release, version: '0.3.0-beta.1', prerelease: true };
  assert.match(bannerTitle(pre), /pre-release/);
});

test('notes in the sample are plain text', () => {
  for (const n of available.check.release.notes) {
    assert.ok(['heading', 'item', 'text'].includes(n.kind));
    assert.doesNotMatch(n.text, /[<>]|onerror|\]\(/);
  }
});

test('settings line: off, never, up to date, available, not visible', () => {
  assert.equal(checkLine(off).tone, 'muted');
  assert.match(checkLine(off).text, /Nothing is requested/);
  assert.match(checkLine({ ...off, enabled: true }).text, /Not checked yet/);
  assert.match(checkLine({ ...available, check: { state: 'up-to-date' } }).text, /^Up to date \(0\.1\.0\)/);
  assert.match(checkLine(available).text, /^0\.2\.0 is available \(this is 0\.1\.0\)\.$/);
  assert.match(checkLine({ ...available, skipped_version: '0.2.0' }).text, /skip it/);
  const f = checkLine(failed);
  assert.equal(f.tone, 'warning');
  assert.match(f.text, /^Can't see releases for someone\/private-fork: the repo is private/);
});

test('the hint names the host and what is sent', () => {
  const h = requestHint('voidput/starorg-strings');
  assert.match(h, /api\.github\.com/);
  assert.match(h, /StarOrg-Strings\/<version>/);
  assert.match(h, /no account, token or install id/);
});

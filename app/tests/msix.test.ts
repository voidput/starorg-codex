// Microsoft Store package (scripts/msix.ts, docs/windows.md). Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { LOGOS, identityProblem, msixVersion, pngSize, renderManifest, stage } from '../scripts/msix.ts';

const ICONS = new URL('../src-tauri/icons/', import.meta.url);
const GOLDEN = new URL('./fixtures/AppxManifest.golden.xml', import.meta.url);
// Placeholder identity, the shape Partner Center gives (Product identity page).
const ID = { name: '12345Example.StarOrgStrings', publisher: 'CN=00000000-1111-2222-3333-444444444444', publisherDisplayName: 'Example & Co' };

test('manifest matches the golden file byte for byte', () => {
  const got = renderManifest({ ...ID, version: '0.3.1.0', executable: 'starorg-strings-app.exe' });
  if (process.env.UPDATE_GOLDEN === '1') writeFileSync(GOLDEN, got);
  assert.equal(got, readFileSync(GOLDEN, 'utf8'));
  assert.match(got, /TargetDeviceFamily Name="Windows\.Desktop"/);
  assert.match(got, /<rescap:Capability Name="runFullTrust" \/>/);
  assert.match(got, /EntryPoint="Windows\.FullTrustApplication"/);
  assert.match(got, /Example &amp; Co/, 'values are XML-escaped');
  assert.ok(!/[^\r]\n/.test(got), 'CRLF only');
});

test('every logo the manifest names exists in app/src-tauri/icons at its size', () => {
  const m = renderManifest({ ...ID, version: '1.0.0.0', executable: 'a.exe' });
  const named = [...m.matchAll(/Assets\\([A-Za-z0-9]+\.png)/g)].map((x) => x[1]);
  assert.deepEqual([...new Set(named)].sort(), LOGOS.map((l) => l.file).sort());
  for (const l of LOGOS) {
    assert.deepEqual(pngSize(readFileSync(new URL(l.file, ICONS))), { width: l.width, height: l.height }, l.file);
  }
});

test('package versions: four parts, last one 0, pre-release dropped', () => {
  assert.equal(msixVersion('0.3.1'), '0.3.1.0');
  assert.equal(msixVersion('v1.2.3-beta.1+abc'), '1.2.3.0');
  assert.throws(() => msixVersion('1.2'));
  assert.throws(() => msixVersion('1.70000.0'));
  assert.throws(() => renderManifest({ ...ID, version: '1.2.3.4', executable: 'a.exe' }));
});

test('identity from repo variables: all three needed, CN= publisher, schema-safe name', () => {
  assert.equal(identityProblem(ID), null);
  assert.match(identityProblem({ ...ID, name: '' })!, /must all be set/);
  assert.match(identityProblem({ ...ID, publisher: 'Example' })!, /CN=/);
  assert.match(identityProblem({ ...ID, name: 'has space' })!, /letters, digits/);
  assert.equal(identityProblem({ ...ID, publisher: 'CN=Example Org, O=Example Org, C=US' }), null);
  assert.throws(() => renderManifest({ ...ID, name: 'x', version: '1.0.0.0', executable: 'a.exe' }));
  assert.throws(() => renderManifest({ ...ID, version: '1.0.0.0', executable: '..\\evil.exe' }));
});

test('stage lays out the package root for makeappx', () => {
  const dir = mkdtempSync(join(tmpdir(), 'msix-'));
  const exe = join(dir, 'starorg-strings-app.exe');
  writeFileSync(exe, 'MZ fake');
  const out = join(dir, 'pkg');
  stage({ ...ID, version: '0.3.1.0', executable: 'starorg-strings-app.exe', exePath: exe, iconsDir: fileURLToPath(ICONS), out });
  assert.deepEqual(readdirSync(out).sort(), ['AppxManifest.xml', 'Assets', 'starorg-strings-app.exe']);
  assert.deepEqual(readdirSync(join(out, 'Assets')).sort(), LOGOS.map((l) => l.file).sort());
  assert.equal(readFileSync(join(out, 'AppxManifest.xml'), 'utf8'), readFileSync(GOLDEN, 'utf8'));
});

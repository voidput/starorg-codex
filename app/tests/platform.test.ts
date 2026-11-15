// Windows wording in the setup wizard (src/lib/platform.ts). Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { isWindowsAgent, notFoundHint, searchedWhere } from '../src/lib/platform.ts';

test('WebView2 on Windows is Windows, WebKitGTK is not', () => {
  assert.ok(isWindowsAgent('Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.0.0'));
  assert.ok(!isWindowsAgent('Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15'));
  assert.ok(!isWindowsAgent(''));
});

test('Windows wording names the launcher logs and drives, never Wine or /mnt', () => {
  for (const s of [searchedWhere(true), notFoundHint(true)]) assert.doesNotMatch(s, /Wine|LUG|Lutris|\/mnt|home folder/);
  assert.match(searchedWhere(true), /RSI Launcher logs/);
  assert.match(notFoundHint(true), /C:\\Program Files\\Roberts Space Industries\\StarCitizen/);
  assert.match(searchedWhere(false), /Wine prefixes/);
});

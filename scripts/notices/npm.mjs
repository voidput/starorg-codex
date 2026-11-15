#!/usr/bin/env node
// npm part of THIRD_PARTY_NOTICES.md (scripts/gen-notices.sh). Lists every package that
// app/package-lock.json does not mark "dev": the frontend's runtime dependencies (bundled into
// the app's JS/CSS, fonts included) and what they pull in. License texts come from the installed
// packages in app/node_modules (run `npm ci` in app/ first). Output: markdown on stdout,
// identical texts grouped like cargo-about does for the Rust crates.
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const app = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'app');
const lock = JSON.parse(readFileSync(join(app, 'package-lock.json'), 'utf8'));
if (!existsSync(join(app, 'node_modules'))) {
  console.error('app/node_modules missing: run `npm ci` in app/ first');
  process.exit(1);
}

// Packages whose metadata is incomplete, checked by hand against the package contents.
// license: SPDX id when package.json has none; noFile: MIT package that ships no license text,
// so the standard MIT text is rendered with the package author as copyright holder.
const OVERRIDES = {
  'svelte-toolbelt': { license: 'MIT' }, // LICENSE file is MIT; package.json has no field
  'is-reference': { noFile: true },
  'locate-character': { noFile: true },
};
const MIT = (holder) => `MIT License

Copyright (c) ${holder}

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.`;

// Dev packages whose code still ships: tailwindcss's preflight and theme CSS are compiled into
// the app's CSS (the bundle keeps its "tailwindcss vX | MIT License" banner). It used to be
// listed only as an optional peer of tailwind-variants 3, which marked it non-dev.
const BUNDLED_DEV = new Set(['node_modules/tailwindcss']);

const LICENSE_FILE = /^(licen[cs]e|copying|notice)(\.|-|$)/i;
const groups = new Map(); // license text -> { license, users[] }
let missing = 0;

for (const [key, meta] of Object.entries(lock.packages)) {
  if (!key || (meta.dev && !BUNDLED_DEV.has(key))) continue;
  const dir = join(app, key);
  const pkg = JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8'));
  const name = pkg.name ?? key.replace(/^.*node_modules\//, '');
  const ov = OVERRIDES[name] ?? {};
  let license = pkg.license ?? meta.license ?? ov.license;
  if (typeof license === 'object' && license) license = license.type;
  const files = readdirSync(dir)
    .filter((f) => LICENSE_FILE.test(f) && !f.endsWith('.spdx'))
    .sort();
  const author = typeof pkg.author === 'string' ? pkg.author : pkg.author?.name;
  if (ov.noFile && license === 'MIT' && !files.length && author) {
    const g = groups.get(MIT(author)) ?? { license, users: [] };
    g.users.push(`${name} ${pkg.version}`);
    groups.set(MIT(author), g);
    continue;
  }
  if (!license || !files.length) {
    console.error(`no license ${license ? 'file' : 'field'} for ${name}@${pkg.version}`);
    missing++;
    continue;
  }
  const text = files.map((f) => readFileSync(join(dir, f), 'utf8').replace(/\r\n/g, '\n').trim()).join('\n\n---\n\n');
  const g = groups.get(text) ?? { license, users: [] };
  g.users.push(`${name} ${pkg.version}`);
  groups.set(text, g);
}
if (missing) process.exit(1);

const sorted = [...groups.entries()].sort(([, a], [, b]) => a.license.localeCompare(b.license) || a.users[0].localeCompare(b.users[0]));
const out = ['## npm packages (app frontend)', ''];
const counts = new Map();
for (const [, g] of sorted) counts.set(g.license, (counts.get(g.license) ?? 0) + g.users.length);
for (const [l, n] of counts) out.push(`- ${l}: ${n} package(s)`);
out.push('');
for (const [text, g] of sorted) {
  out.push(`### ${g.license}`, '', 'Used by:');
  for (const u of g.users.sort()) out.push(`- ${u}`);
  out.push('', '```text', text, '```', '');
}
process.stdout.write(out.join('\n'));

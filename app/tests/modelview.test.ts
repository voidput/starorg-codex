// The Explorer's Model tab (addendum 7.20): the detail slider (left = LOD 4, least detail),
// what each step says and does (cache, export, ask, refused), the chunked GLB read with its
// cancel and errors, the readouts, and that three.js stays out of the main bundle. Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { fetchGlb, FetchCancelled, lodLabel, lodToSlider, mbText, sizeText, sliderToLod, stepNote, stepState, trianglesText, webglProblem } from '../src/lib/modelview.ts';
import { createAssetMock, type AssetMockData } from '../src/lib/contract/mock-assets.ts';
import type { AssetModelLod } from '../src/lib/contract/assets.ts';

const read = (f: string) => JSON.parse(readFileSync(new URL(`../src/lib/contract/${f}`, import.meta.url), 'utf8'));
const samples = read('samples.gen.json');
const data: AssetMockData = read('mock-assets.gen.json');
const GIB = 1024 ** 3;
const live = { install: 'inst-7f3a', channel: 'LIVE' };
const ship = (n: number) => `51000000-0000-4000-8000-${n.toString(16).padStart(12, '0')}`;

class Fail extends Error {
  code: string;
  constructor(code: string, message: string) {
    super(message);
    this.code = code;
  }
}
function mock(limitGb = 5, freeGb = 18) {
  const mem = { limit: limitGb * GIB, free: freeGb * GIB };
  const m = createAssetMock(data, {
    devOn: () => true,
    hasChannel: () => true,
    findRecord: (w) => (w.startsWith('51') ? { guid: w, name: 'SYN_Ship', type: 'SynShip' } : null),
    emit: () => {},
    sleep: async () => {},
    delay: 0,
    nextJob: () => 'job-1',
    fail: (code, message) => {
      throw new Fail(code, message);
    },
    memory: () => mem,
  });
  return { h: m.handlers, mem };
}

test('the slider runs from LOD 4 (least detail, left) to LOD 0 (most, right)', () => {
  assert.deepEqual([0, 1, 2, 3, 4].map(sliderToLod), [4, 3, 2, 1, 0]);
  assert.equal(lodToSlider(3), 1, 'LOD 3 starts one step from the left');
  for (let l = 0; l <= 4; l++) assert.equal(sliderToLod(lodToSlider(l)), l);
  assert.equal(sliderToLod(9), 0);
  assert.equal(sliderToLod(-1), 4);
  assert.equal(lodLabel(4), 'LOD 4 · least detail');
  assert.equal(lodLabel(0), 'LOD 0 · most detail');
  assert.equal(lodLabel(2), 'LOD 2');
});

test('each step says what choosing it does: cache, export, ask first or refused', () => {
  const info = { limit: 1.5 * GIB, max_bytes: 512 * 1024 * 1024 };
  const s = (over: Partial<AssetModelLod>): AssetModelLod => ({ lod: 3, uses: 3, need: 1.2 * GIB, fit: 'ok', cached: false, bytes: null, ...over });
  assert.equal(stepState(s({}), info.max_bytes), 'export');
  assert.match(stepNote(s({}), info), /^Exports in its own process, about 1\.2 GB of memory/);
  assert.equal(stepState(s({ cached: true, bytes: 5_200_548 }), info.max_bytes), 'cached');
  assert.equal(stepNote(s({ cached: true, bytes: 5_200_548 }), info), 'In the cache (5.0 MB): no export needed.');
  assert.equal(stepState(s({ fit: 'over-limit', need: 1.4 * GIB, lod: 0, uses: 0 }), info.max_bytes), 'refused');
  assert.equal(stepNote(s({ fit: 'over-limit', need: 1.4 * GIB, lod: 0, uses: 0 }), info), 'Needs about 1.4 GB, more than the 1.5 GB memory limit allows.');
  assert.equal(stepState(s({ fit: 'low-free' }), info.max_bytes), 'ask');
  assert.match(stepNote(s({ fit: 'low-free' }), info), /not much memory is free now/);
  assert.equal(stepState(s({ cached: true, bytes: 600 * 1024 * 1024 }), info.max_bytes), 'too-big');
  // A cached step is shown even when the guard would refuse exporting it (no worker needed).
  assert.equal(stepState(s({ cached: true, bytes: 10, fit: 'over-limit' }), info.max_bytes), 'cached');
  assert.equal(stepNote(s({ lod: 4, uses: 3, cached: true, bytes: 4264 }), info), 'The mesh has no LOD 4: shows LOD 3; in the cache (4 KB): no export needed.');
});

test('the steps of the Rust samples read as the slider shows them', () => {
  const i = samples.asset_model_info;
  assert.equal(i.default_lod, 3);
  assert.deepEqual(i.lods.map((l: AssetModelLod) => stepState(l, i.max_bytes)), ['export', 'export', 'export', 'export', 'export']);
  const k = samples.asset_model_info_known;
  assert.deepEqual(k.lods.map((l: AssetModelLod) => stepState(l, k.max_bytes)), ['export', 'export', 'export', 'cached', 'cached']);
  assert.match(stepNote(k.lods[4], k), /^The mesh has no LOD 4: shows LOD 3/);
});

test('the GLB is read in chunks, with progress, and can be stopped', async () => {
  const { h } = mock();
  const v = await h.asset_model_view({ req: { target: live, what: ship(3), name: '', lod: 0, force: false } });
  const seen: number[] = [];
  let calls = 0;
  const chunk = async (req: Parameters<typeof h.asset_model_chunk>[0]['req']) => {
    calls++;
    return h.asset_model_chunk({ req });
  };
  const buf = await fetchGlb({ ...v, chunk: 1024 }, live, chunk, (p) => seen.push(p.done));
  assert.equal(buf.byteLength, v.bytes);
  assert.equal(calls, Math.ceil(v.bytes / 1024));
  assert.equal(seen[0], 0);
  assert.equal(seen.at(-1), v.bytes);
  assert.equal(new TextDecoder().decode(new Uint8Array(buf, 0, 4)), 'glTF');
  let n = 0;
  await assert.rejects(
    fetchGlb({ ...v, chunk: 1024 }, live, chunk, () => {}, () => ++n > 2),
    (e) => e instanceof FetchCancelled,
  );
  // A file that changed under us (a short read) is an error, not a hang.
  await assert.rejects(fetchGlb({ ...v, bytes: v.bytes + 10 }, live, chunk), /changed while it was read/);
  // An LOD never loaded has nothing in the cache.
  await assert.rejects(fetchGlb({ ...v, lod: 2 }, live, chunk), (e: Fail) => e.code === 'not-found');
});

test('the guard: over the limit refused even when forced, short of free memory asks first', async () => {
  const { h, mem } = mock(1.5, 18);
  const info = await h.asset_model_info({ req: { target: live, what: ship(5), name: '' } });
  assert.deepEqual(info.lods.map((l) => stepState(l, info.max_bytes)), ['refused', 'export', 'export', 'export', 'export']);
  await assert.rejects(async () => h.asset_model_view({ req: { target: live, what: ship(5), name: '', lod: 0, force: true } }), (e: Fail) => e.code === 'invalid');
  mem.limit = 8 * GIB;
  mem.free = 1 * GIB;
  const asked = await h.asset_model_info({ req: { target: live, what: ship(5), name: '' } });
  assert.equal(stepState(asked.lods[3]!, asked.max_bytes), 'ask');
  await assert.rejects(async () => h.asset_model_view({ req: { target: live, what: ship(5), name: '', lod: 3, force: false } }), (e: Fail) => e.code === 'needs-decision');
  const v = await h.asset_model_view({ req: { target: live, what: ship(5), name: '', lod: 3, force: true } });
  assert.equal(v.cached, false);
  // Now cached: shown without the guard.
  mem.limit = 1.5 * GIB;
  const again = await h.asset_model_view({ req: { target: live, what: ship(5), name: '', lod: 3, force: false } });
  assert.equal(again.cached, true);
});

test('readouts', () => {
  assert.equal(sizeText([17.36, 5.78, 21.03]), '17.4 × 5.8 × 21.0 m');
  assert.equal(sizeText([218.04, 89.2, 492.77]), '218 × 89.2 × 493 m');
  assert.equal(trianglesText(85907), '85,907 triangles');
  assert.equal(trianglesText(3662991), '3.66 M triangles');
  assert.equal(trianglesText(null), 'triangles unknown');
  assert.equal(mbText(56_398_596), '54 MB');
  assert.equal(mbText(4_857_836), '4.6 MB');
  assert.match(webglProblem(new Error('Error creating WebGL context.')), /no WebGL here \(Error creating WebGL context\.\).*Export \.glb/);
});

test('three.js is only reached through the lazily imported viewer', () => {
  const src = new URL('../src/', import.meta.url).pathname;
  const files: string[] = [];
  const walk = (d: string) => {
    for (const f of readdirSync(d)) {
      const p = join(d, f);
      if (statSync(p).isDirectory()) walk(p);
      else if (/\.(ts|svelte)$/.test(f)) files.push(p);
    }
  };
  walk(src);
  const importsThree = files.filter((f) => /from ['"]three(\/|['"])/.test(readFileSync(f, 'utf8')));
  assert.deepEqual(importsThree.map((f) => f.slice(src.length)).sort(), ['lib/modelscene.ts']);
  const usesScene = files.filter((f) => /from ['"][^'"]*lib\/modelscene/.test(readFileSync(f, 'utf8')));
  assert.deepEqual(usesScene.map((f) => f.slice(src.length)), ['components/assets/ModelViewer.svelte']);
  const viewerImports = files.filter((f) => /(from |import\()['"][^'"]*ModelViewer\.svelte/.test(readFileSync(f, 'utf8')));
  assert.deepEqual(viewerImports.map((f) => f.slice(src.length)), ['components/screens/ExplorerScreen.svelte']);
  const explorer = readFileSync(join(src, 'components/screens/ExplorerScreen.svelte'), 'utf8');
  assert.ok(!/^\s*import .*ModelViewer/m.test(explorer), 'no static import');
  assert.match(explorer, /import\('@\/components\/assets\/ModelViewer\.svelte'\)/);
});

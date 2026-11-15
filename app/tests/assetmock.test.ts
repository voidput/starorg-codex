// Rust mock = TS mock for Developer mode assets (addendum 7.16): every asset sample in
// samples.gen.json is replayed against the TS mock over the same data (mock-assets.gen.json,
// mock-dev.gen.json) in the same order as asset_samples() in tests_assets.rs. Also the
// webview's audio LRU. Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createAssetMock, type AssetMockData, effectiveLod, glbTriangles, isDdsSibling, modelFileName } from '../src/lib/contract/mock-assets.ts';
import { AudioLru, playFailure } from '../src/lib/audiolru.ts';

const read = (f: string) => JSON.parse(readFileSync(new URL(`../src/lib/contract/${f}`, import.meta.url), 'utf8'));
const samples = read('samples.gen.json');
const data: AssetMockData = read('mock-assets.gen.json');
const devData = read('mock-dev.gen.json') as { entries: { guid: string; name: string; type: string; path: string }[] };

class Fail extends Error {
  code: string;
  constructor(code: string, message: string) {
    super(message);
    this.code = code;
  }
}

function mock(devOn = true) {
  let job = 0;
  const events: { phase: string; kind: string }[] = [];
  const settings = { dev: devOn };
  const m = createAssetMock(data, {
    devOn: () => settings.dev,
    hasChannel: (r) => r.install === 'inst-7f3a' && ['LIVE', 'PTU', 'HOTFIX'].includes(r.channel),
    findRecord: (w) => {
      const e = devData.entries.find((x) => x.guid === w || x.name === w || x.path === w);
      return e ? { guid: e.guid, name: e.name, type: e.type } : null;
    },
    emit: (p) => events.push(p),
    sleep: async () => {},
    delay: 0,
    nextJob: () => `job-${++job}`,
    fail: (code, message) => {
      throw new Fail(code, message);
    },
    memory: () => ({ limit: Math.round(samples.memory_status.limit_gb * 1024 ** 3), free: samples.memory_status.available_bytes }),
  });
  return { h: m.handlers, events, settings };
}

const live = { install: 'inst-7f3a', channel: 'LIVE' };
const ship = (n: number) => `51000000-0000-4000-8000-${n.toString(16).padStart(12, '0')}`;
const comp = (n: number) => `52000000-0000-4000-8000-${n.toString(16).padStart(12, '0')}`;
const tag = (n: number) => `53000000-0000-4000-8000-${n.toString(16).padStart(12, '0')}`;
const TEXTURE = 'Data\\Textures\\ships\\syn\\syn_ship_001_diff.dds';
const none = {} as Record<string, never>;

test('every asset sample of the Rust mock comes back equal from the TS mock', async () => {
  const { h, events } = mock();
  const got: Record<string, unknown> = {};
  const list = (dir: string, offset: number, limit: number) => ({ req: { target: live, dir, offset, limit } });
  const path = (p: string) => ({ req: { target: live, path: p } });
  got.asset_cache_status = await h.asset_cache_status(none);
  got.asset_status = await h.asset_status({ req: live });
  got.asset_list = await h.asset_list(list('', 0, 50));
  got.asset_list_data = await h.asset_list(list('Data', 0, 50));
  got.asset_list_page = await h.asset_list(list('Data/Textures/ships/syn', 2, 3));
  got.asset_search = await h.asset_search({ req: { target: live, query: 'syn_ship_001 dds', limit: 2 } });
  got.asset_preview = await h.asset_preview(path(TEXTURE));
  got.asset_preview_cached = await h.asset_preview(path(TEXTURE.replace(/\\/g, '/')));
  got.asset_preview_text = await h.asset_preview(path('Data\\Libs\\Config\\defaultProfile.xml'));
  got.asset_preview_cryxml = await h.asset_preview(path('Data\\Objects\\Spaceships\\Ships\\SYN\\syn_ship_002\\syn_ship_002.mtl'));
  got.asset_preview_audio = await h.asset_preview(path('Data\\Sounds\\wwise\\Media\\700001.wem'));
  got.asset_preview_none = await h.asset_preview(path(`${TEXTURE}.1`));
  got.asset_extract = await h.asset_extract({ req: { target: live, paths: ['Data\\Textures', 'Data/Libs/Config/defaultProfile.xml'], convert: true } });
  const sounds = await h.asset_sounds({ req: { target: live, what: ship(1) } });
  got.asset_sounds = sounds;
  got.asset_sounds_item = await h.asset_sounds({ req: { target: live, what: comp(4) } });
  got.asset_sounds_none = await h.asset_sounds({ req: { target: live, what: tag(2) } });
  got.asset_sound_search = await h.asset_sound_search({ req: { target: live, query: 'ship_001 engine', limit: 10 } });
  got.asset_sound_search_words = await h.asset_sound_search({ req: { target: live, query: 'door', limit: 5 } });
  got.asset_trigger = await h.asset_trigger({ req: { target: live, name: 'Play_SYN_Ship_001_Engine_Loop' } });
  const s0 = sounds.triggers[0]!.sounds[0]!;
  const first = { media_id: s0.media_id, source: s0.source, bank: s0.bank, media_path: s0.media_path };
  got.asset_sound = await h.asset_sound({ req: { target: live, sound: first } });
  got.asset_sound_cached = await h.asset_sound({ req: { target: live, sound: first } });
  got.asset_export_sounds = await h.asset_export_sounds({ req: { target: live, what: ship(1), sounds: [], wem: false } });
  const model = (preset: 'full' | 'print-shell', lod: number, stl: boolean) => ({ req: { target: live, what: ship(2), name: 'SYN_Ship_002', preset, lod, stl, force: false } });
  got.asset_export_model = await h.asset_export_model(model('print-shell', 0, true));
  got.asset_export_model_cached = await h.asset_export_model(model('print-shell', 0, false));
  got.asset_export_model_full = await h.asset_export_model(model('full', 1, false));
  const info = (what: string) => ({ req: { target: live, what, name: '' } });
  const view = (what: string, lod: number) => ({ req: { target: live, what, name: '', lod, force: false } });
  got.asset_model_info = await h.asset_model_info(info(ship(4)));
  got.asset_model_view = await h.asset_model_view(view(ship(4), 4));
  got.asset_model_view_cached = await h.asset_model_view(view(ship(4), 3));
  got.asset_model_info_known = await h.asset_model_info(info(ship(4)));
  got.asset_model_view_item = await h.asset_model_view(view(comp(8), 3));
  got.asset_status_open = await h.asset_status({ req: live });
  got.asset_cache_pick = await h.asset_cache_pick(none);
  got.asset_cache_move = await h.asset_cache_move(none);
  got.asset_cache_clear = await h.asset_cache_clear(none);
  got.asset_cache_reset = await h.asset_cache_reset(none);
  got.asset_close = await h.asset_close({ req: live });
  const keys = Object.keys(samples).filter((k) => k.startsWith('asset_'));
  assert.deepEqual(Object.keys(got).sort(), keys.sort(), 'every asset sample is replayed');
  for (const k of keys) assert.deepEqual(got[k], samples[k], k);
  assert.ok(events.length > 0 && events.every((e) => e.kind === 'dev'));
});

test('the asset calls refuse while Developer mode is off, the cache does not', async () => {
  const { h } = mock(false);
  await assert.rejects(async () => h.asset_list({ req: { target: live, dir: '', offset: 0, limit: 5 } }), (e: Fail) => e.code === 'dev-disabled');
  await assert.rejects(async () => h.asset_sounds({ req: { target: live, what: ship(1) } }), (e: Fail) => e.code === 'dev-disabled');
  assert.equal((await h.asset_cache_status(none)).is_default, true);
});

test('model chunks put together are the GLB the view describes', async () => {
  const { h } = mock();
  const v = await h.asset_model_view({ req: { target: live, what: ship(2), name: '', lod: 0, force: false } });
  const parts: Uint8Array[] = [];
  let n = 0;
  while (n < v.bytes) {
    const c = new Uint8Array(await h.asset_model_chunk({ req: { target: live, what: ship(2), lod: v.lod, offset: n, len: 1000 } }));
    assert.ok(c.byteLength > 0 && c.byteLength <= 1000);
    parts.push(c);
    n += c.byteLength;
  }
  const all = new Uint8Array(n);
  let at = 0;
  for (const p of parts) {
    all.set(p, at);
    at += p.byteLength;
  }
  assert.equal(new TextDecoder().decode(all.subarray(0, 4)), 'glTF');
  assert.equal(glbTriangles(all), v.triangles);
  await assert.rejects(async () => h.asset_model_chunk({ req: { target: live, what: ship(2), lod: 2, offset: 0, len: 10 } }), (e: Fail) => e.code === 'not-found');
  await assert.rejects(async () => h.asset_model_chunk({ req: { target: live, what: ship(2), lod: 0, offset: v.bytes + 1, len: 10 } }), (e: Fail) => e.code === 'invalid');
});

test('the model guard refuses over the limit and asks when memory is short', async () => {
  const GIB = 1024 ** 3;
  const mem = { limit: 1.5 * GIB, free: 32 * GIB };
  const m = createAssetMock(data, {
    devOn: () => true,
    hasChannel: () => true,
    findRecord: (w) => (w === ship(1) ? { guid: w, name: 'SYN_Ship_001', type: 'SynShip' } : null),
    emit: () => {},
    sleep: async () => {},
    delay: 0,
    nextJob: () => 'job-1',
    fail: (code, message) => {
      throw new Fail(code, message);
    },
    memory: () => mem,
  });
  const i = await m.handlers.asset_model_info({ req: { target: live, what: ship(1), name: '' } });
  assert.deepEqual(i.lods.map((l) => l.fit), ['over-limit', 'ok', 'ok', 'ok', 'ok']);
  await assert.rejects(async () => m.handlers.asset_model_view({ req: { target: live, what: ship(1), name: '', lod: 0, force: true } }), (e: Fail) => e.code === 'invalid' && e.message.includes('memory limit'));
  mem.limit = 8 * GIB;
  mem.free = GIB;
  await assert.rejects(async () => m.handlers.asset_model_view({ req: { target: live, what: ship(1), name: '', lod: 3, force: false } }), (e: Fail) => e.code === 'needs-decision');
  const v = await m.handlers.asset_model_view({ req: { target: live, what: ship(1), name: '', lod: 3, force: true } });
  assert.equal(v.lod, 3);
  assert.equal(effectiveLod(4, [0, 1, 2, 3]), 3);
  assert.equal(effectiveLod(2, []), 2);
});

test('helpers match the Rust ones', () => {
  assert.ok(isDdsSibling('x.dds.1') && isDdsSibling('x.DDS.3a') && isDdsSibling('x.dds.a'));
  assert.ok(!isDdsSibling('x.dds') && !isDdsSibling('x.dds.bak'));
  assert.equal(modelFileName('AEGS Javelin', { preset: 'print-shell', lod: 0 }), 'AEGS_Javelin-print-lod0.glb');
  assert.equal(modelFileName('', { preset: 'full', lod: 1 }), 'model-full-lod1.glb');
});

test('the audio LRU keeps the newest few and revokes the rest', () => {
  const revoked: string[] = [];
  const lru = new AudioLru(2, (u) => revoked.push(u));
  lru.put('a', 'blob:a');
  lru.put('b', 'blob:b');
  assert.equal(lru.get('a'), 'blob:a');
  lru.put('c', 'blob:c');
  assert.deepEqual(revoked, ['blob:b'], 'b was least recently used');
  assert.deepEqual(lru.keys(), ['a', 'c']);
  lru.put('a', 'blob:a2');
  assert.deepEqual(revoked, ['blob:b', 'blob:a']);
  assert.equal(lru.size, 2);
  lru.clear();
  assert.equal(lru.size, 0);
  assert.equal(revoked.length, 4);
});

test('a sound the webview refuses names the file and what is missing', () => {
  const ns = new DOMException('The operation is not supported.', 'NotSupportedError');
  const hint = 'Install gst-plugins-good.';
  const m = playFailure(ns, 'Vorbis (.ogg)', hint);
  assert.equal(m, "The app's webview could not play this Vorbis (.ogg) file. Install gst-plugins-good. Export it to play it in another player.");
  assert.ok(!m.includes('NotSupportedError'));
  assert.match(playFailure(ns, '', null), /^The app's webview could not play this sound\. Its media engine reported the format as not supported\./);
  assert.equal(playFailure(new DOMException('x', 'NetworkError'), 'PCM (.wav)', hint), 'NetworkError: x');
});

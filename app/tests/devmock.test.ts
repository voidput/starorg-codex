// Rust mock = TS mock for Developer mode (addendum 7.11): every dev sample in samples.gen.json
// (written by the Rust mock) is replayed against the TS mock over the same synthetic data
// (mock-dev.gen.json) and must come back equal. Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createDevMock, type DevMockCtx, type DevMockData } from '../src/lib/contract/mock-dev.ts';
import type { Filters, ListQuery } from '../src/lib/contract/dev.ts';

const NO_FILTERS: Filters = { sections: [], kinds: [], sub_types: [], sizes: [], manufacturers: [], grades: [], classes: [], tags: [], model: false, sounds: false, player_only: false, changes: [] };
const LIST: ListQuery = { node: '', query: '', filters: NO_FILTERS, sort: 'name', group: 'none', collapsed: [], offset: 0, limit: 5, main_only: true };

const read = (f: string) => JSON.parse(readFileSync(new URL(`../src/lib/contract/${f}`, import.meta.url), 'utf8'));
const samples = read('samples.gen.json');
const data: DevMockData = read('mock-dev.gen.json');

class Fail extends Error {
  code: string;
  constructor(code: string, message: string) {
    super(message);
    this.code = code;
  }
}

/** The synthetic stock text of the keys the samples touch (mock.rs `synthetic_strings`). */
function stockText(key: string): string | null {
  let m = /^vehicle_DescSYN_Ship_(\d{5})$/i.exec(key);
  if (m && Number(m[1]) % 8 === 0) return `Synthetic hull ${Number(m[1])}. A test ship description.\\nSecond line.`;
  m = /^item_NameSYN_Cooler_S(\d)_(\d{5})$/i.exec(key);
  if (m && Number(m[2]) % 8 === 1 && Number(m[1]) === (Number(m[2]) % 4) + 1) return `Sample Cooler ${Number(m[2])}`;
  m = /^ui_SYN_Label_(\d{5})$/i.exec(key);
  if (m && Number(m[1]) % 8 === 7) return `Label ${Number(m[1])}`;
  return null;
}

function mock(devOn = true) {
  let job = 0;
  const events: unknown[] = [];
  const settings = { dev: devOn, dumps: false };
  const ctx: DevMockCtx = {
    devOn: () => settings.dev,
    dumpsOn: () => settings.dumps,
    hasChannel: (r) => r.install === 'inst-7f3a' && ['LIVE', 'PTU', 'HOTFIX'].includes(r.channel),
    channelNames: () => ['LIVE', 'HOTFIX', 'PTU'],
    stockText,
    emit: (p) => events.push(p),
    sleep: async () => {},
    delay: 0,
    nextJob: () => `job-${++job}`,
    fail: (code, message) => {
      throw new Fail(code, message);
    },
  };
  return { m: createDevMock(data, ctx), events, settings };
}

const live = { install: 'inst-7f3a', channel: 'LIVE' };
const tree = (root: 'type' | 'path', node: string, limit: number) => ({ req: { target: live, root, node, offset: 0, limit, main_only: true } });
const rows = (source: string, dataset: string, table: string) => ({ target: live, source, dataset, table, columns: [] as string[], search: '', filters: [] as never[], sort: null, offset: 0, limit: 5, group: null as unknown });
const recs = (dataset: string, table: string) => ({ target: live, dataset, table, state: null, field: null, identity: null, query: null as string | null, offset: 0, limit: 5, group: null as unknown });
const f = (column: string, part: object) => ({ column, values: [], min: null, max: null, has: null, ...part });

test('every dev sample of the Rust mock comes back equal from the TS mock', async () => {
  const { m, settings } = mock();
  const h = m.handlers;
  // Same order as dev_samples() in tests_dev.rs: state carries over between calls.
  const got: Record<string, unknown> = {};
  got.dev_status = await h.dev_status({ req: live });
  got.dev_open = await h.dev_open({ req: live });
  got.dev_types = await h.dev_types({ req: live });
  got.dev_tree = await h.dev_tree(tree('type', '', 50));
  got.dev_tree_type = await h.dev_tree(tree('type', 't:SynComponent', 5));
  got.dev_tree_path = await h.dev_tree(tree('path', '', 50));
  got.dev_tree_folder = await h.dev_tree(tree('path', 'p:entities/scitem/ships/cooler/', 3));
  got.dev_search = await h.dev_search({ req: { target: live, query: 'cooler 01', limit: 5 } });
  got.dev_search_guid = await h.dev_search({ req: { target: live, query: '5100', limit: 3 } });
  got.dev_search_type = await h.dev_search({ req: { target: live, query: 'type:SynTag 5', limit: 3 } });
  got.dev_record = await h.dev_record({ req: { target: live, what: '51000000-0000-4000-8000-000000000001', pointer: '', depth: null } });
  got.dev_record_cut = await h.dev_record({ req: { target: live, what: data.huge.guid, pointer: '', depth: 2 } });
  got.dev_record_pointer = await h.dev_record({ req: { target: live, what: 'SYN_cooler_004', pointer: '/_RecordValue_/Ship', depth: null } });
  got.dev_export_record = await h.dev_export_record({ req: { target: live, what: 'SYN_Ship_001' } });
  got.dev_export_type = await h.dev_export_type({ req: { target: live, type_name: 'SynComponent', main_only: true } });
  got.dev_sources = await h.dev_sources({ req: live });
  got.dev_add_source = await h.dev_add_source({ req: live });
  got.dev_dataset = await h.dev_dataset({ req: { target: live, source: 'folder:0', dataset: 'game-weapons.json' } });
  got.dev_table = await h.dev_table({ req: { target: live, source: 'build:LIVE', dataset: 'gamedata.json', table: 'components' } });
  got.dev_table_kv = await h.dev_table({ req: { target: live, source: 'folder:0', dataset: 'game-strings-english.json', table: 'strings' } });
  got.dev_rows = await h.dev_rows({ req: { ...rows('build:LIVE', 'gamedata.json', 'components'), search: 'cooler', sort: { column: 'health', desc: true }, limit: 3 } });
  got.dev_rows_filtered = await h.dev_rows({
    req: { ...rows('build:LIVE', 'gamedata.json', 'components'), columns: ['name', 'size', 'manufacturer'], filters: [f('manufacturer', { values: ['SYNB'] }), f('size', { min: 2 })] as never[] },
  });
  got.dev_row = await h.dev_row({ req: { target: live, source: 'folder:0', dataset: 'game-weapons.json', table: 'shipWeapons', row: 1 } });
  got.dev_row_loc = await h.dev_row({ req: { target: live, source: 'build:LIVE', dataset: 'gamedata.json', table: 'components', row: 0 } });
  got.dev_diff = await h.dev_diff({ req: { target: live, a: 'build:LIVE', b: 'build:PTU' } });
  got.dev_diff_summary = await h.dev_diff_summary({ req: live });
  got.dev_diff_records = await h.dev_diff_records({ req: recs('gamedata.json', 'components') });
  got.dev_diff_records_added = await h.dev_diff_records({ req: { ...recs('gamedata.json', 'components'), state: 'added' } });
  const chain = ['type', 'subType', 'size'].map((column) => ({ column, folders: false }));
  const groups = (extra: object) => ({ req: { target: live, source: 'build:LIVE', dataset: 'gamedata.json', table: 'weapons', by: chain, open: [] as string[], columns: [] as string[], search: '', filters: [] as never[], ...extra } });
  got.dev_table_weapons = await h.dev_table({ req: { target: live, source: 'build:LIVE', dataset: 'gamedata.json', table: 'weapons' } });
  got.dev_groups = await h.dev_groups(groups({}));
  got.dev_groups_open = await h.dev_groups(groups({ open: ['["WeaponGun"]', '["WeaponGun","Gun"]'], search: 'repeater' }));
  got.dev_rows_group = await h.dev_rows({ req: { ...rows('build:LIVE', 'gamedata.json', 'weapons'), limit: 10, group: { by: chain, node: '["WeaponGun","Gun"]' } } });
  got.dev_diff_groups = await h.dev_diff_groups({ req: { target: live, by: 'auto', open: [] } });
  got.dev_diff_groups_open = await h.dev_diff_groups({ req: { target: live, by: 'auto', open: ['["gamedata.json","weapons"]'] } });
  got.dev_diff_records_group = await h.dev_diff_records({ req: { ...recs('gamedata.json', 'weapons'), group: { by: 'auto', node: '["gamedata.json","weapons","WeaponMining"]' } } });
  const first = (got.dev_diff_records as { records: { id: string }[] }).records[0]!.id;
  got.dev_diff_record = await h.dev_diff_record({ req: { target: live, dataset: 'gamedata.json', table: 'components', id: first, identity: null } });
  got.dev_rows_readable = await h.dev_rows({ req: { ...rows('build:LIVE', 'gamedata.json', 'components'), search: 'alpha synth', limit: 3 } });
  got.dev_diff_records_query = await h.dev_diff_records({ req: { ...recs('gamedata.json', 'components'), query: 'sample cooler 4' } });
  got.dev_diff_record_ship = await h.dev_diff_record({ req: { target: live, dataset: 'gamedata.json', table: 'ships', id: 'SYN_Ship_003', identity: null } });
  // Sections and the item finder (7.17), as in tests_dev.rs.
  const sec = (node: string) => ({ req: { target: live, root: 'section' as const, node, offset: 0, limit: 50, main_only: true, player_only: false } });
  const list = (q: Partial<ListQuery>) => h.dev_list({ req: { target: live, query: { ...LIST, ...q } } });
  got.dev_tree_sections = await h.dev_tree(sec(''));
  const shipsId = (got.dev_tree_sections as { nodes: { id: string; label: string }[] }).nodes.find((n) => n.label === 'Ships & vehicles')!.id;
  got.dev_tree_section = await h.dev_tree(sec(shipsId));
  got.dev_tree_models = await h.dev_tree(sec('m:models:[]'));
  got.dev_list = await list({ limit: 5 });
  got.dev_list_node = await list({ node: shipsId, sort: 'size', limit: 10 });
  got.dev_list_query = await list({ query: 'type:cooler maker:alpha frost', limit: 10 });
  got.dev_list_typo = await list({ query: 'kestral', limit: 10 });
  got.dev_list_abbrev = await list({ query: 'glcr', limit: 10 });
  got.dev_list_grouped = await list({ query: 'repeater', group: 'manufacturer', collapsed: ['SYNB'], limit: 10 });
  got.dev_list_filters = await list({ filters: { ...NO_FILTERS, sections: ['components'], sizes: [1], player_only: true, model: true }, sort: 'manufacturer', limit: 10 });
  got.dev_owner = await h.dev_owner({ req: { target: live, path: data.catalogue.geometry[1]![0] } });
  got.dev_compare = await h.dev_compare({ req: { target: live, other: 'PTU' } });
  got.dev_list_changed = await list({ filters: { ...NO_FILTERS, changes: ['changed'] }, limit: 10 });
  got.dev_status_compared = await h.dev_status({ req: live });
  got.dev_compare_clear = await h.dev_compare({ req: { target: live, other: null } });
  got.dev_close = await h.dev_close({ req: live });
  // Published LIVE builds (addendum §1, Diff only).
  got.dev_dump_releases_off = await h.dev_dump_releases({ req: live });
  settings.dumps = true;
  got.dev_dump_releases = await h.dev_dump_releases({ req: live });
  got.dev_download_dump = await h.dev_download_dump({ req: { target: live, tag: data.dumps[1]!.tag } });
  got.dev_dump_releases_cached = await h.dev_dump_releases({ req: live });
  for (const [k, v] of Object.entries(got)) assert.deepStrictEqual(JSON.parse(JSON.stringify(v)), samples[k], k);
});

test('the huge record is cut under 1 MiB and loads deeper nodes on expand', async () => {
  const { m } = mock();
  const r = await m.handlers.dev_record({ req: { target: live, what: data.huge.guid, pointer: '', depth: null } });
  assert.ok(r.header.bytes > 1_500_000);
  assert.ok(r.truncated);
  assert.ok(r.json.length <= 1 << 20);
  const port = await m.handlers.dev_record({ req: { target: live, what: data.huge.guid, pointer: '/_RecordValue_/Loadout/7', depth: null } });
  assert.match(port.json, /hardpoint_syn_00007/);
});

test('every dev command is refused while the setting is off', async () => {
  const { m } = mock(false);
  for (const [name, fn] of Object.entries(m.handlers)) {
    await assert.rejects(async () => (fn as (a: unknown) => unknown)({ req: { target: live, ...live } }), (e: Fail) => e.code === 'dev-disabled', name);
  }
});

test('published LIVE builds: switch, download as a job, diff old -> new, remove', async () => {
  const { m, settings, events } = mock();
  const h = m.handlers;
  const [nw, old] = [data.dumps[0]!.tag, data.dumps[1]!.tag];
  assert.deepEqual(await h.dev_dump_releases({ req: live }), { releases: [], error: null });
  await assert.rejects(async () => h.dev_download_dump({ req: { target: live, tag: old } }), (e: Fail) => e.code === 'network-off');
  settings.dumps = true;
  assert.deepEqual((await h.dev_dump_releases({ req: live })).releases.map((r) => r.tag), [nw, old]);
  // Same codes and text as the real backend (contract/dev.rs dump_not_published, bad_dump_tag).
  await assert.rejects(async () => h.dev_download_dump({ req: { target: live, tag: '9.9.9.9' } }), (e: Fail) => e.code === 'not-found' && e.message === 'Could not download LIVE 9.9.9.9: no published build 9.9.9.9');
  await assert.rejects(async () => h.dev_download_dump({ req: { target: live, tag: '../x' } }), (e: Fail) => e.code === 'invalid' && e.message === 'bad build tag "../x"');
  const before = events.length;
  await h.dev_download_dump({ req: { target: live, tag: old } });
  const seq = (events.slice(before) as { step: number; steps: number; label: string }[]).map((e) => [e.step, e.steps, e.label]);
  assert.deepEqual(seq, [
    [1, 1, `Asking GitHub for LIVE ${old}`],
    [1, 3, 'Downloading game-ships.json (1 of 2)'],
    [2, 3, 'Downloading game-weapons.json (2 of 2)'],
    [3, 3, `Checking LIVE ${old}`],
    [3, 3, `Downloaded LIVE ${old}`],
  ]);
  const src = await h.dev_download_dump({ req: { target: live, tag: nw } });
  assert.deepEqual(src.filter((s) => s.kind === 'dump').map((s) => s.id), [`dump:${nw}`, `dump:${old}`]);
  const jobs = (events as { kind: string; phase: string }[]).filter((e) => e.phase === 'done');
  assert.ok(jobs.length >= 2 && jobs.every((e) => e.kind === 'dev'));
  const sum = await h.dev_diff({ req: { target: live, a: `dump:${old}`, b: `dump:${nw}` } });
  const w = sum.tables.find((t) => t.dataset === 'game-weapons.json' && t.table === 'shipWeapons')!;
  assert.deepEqual([w.added, w.removed], [2, 2]);
  assert.ok(w.changed > 0);
  // Off again: still listed and readable, no download.
  settings.dumps = false;
  assert.equal((await h.dev_dump_releases({ req: live })).releases.length, 2);
  await h.dev_rows({ req: rows(`dump:${old}`, 'game-weapons.json', 'shipWeapons') });
  const after = await h.dev_remove_source({ req: { target: live, source: `dump:${old}` } });
  assert.equal(after.filter((s) => s.kind === 'dump').length, 1);
  await assert.rejects(async () => h.dev_rows({ req: rows(`dump:${old}`, 'game-weapons.json', 'shipWeapons') }), (e: Fail) => e.code === 'not-found');
});

test('open and diff report progress as dev jobs', async () => {
  const { m, events } = mock();
  await m.handlers.dev_open({ req: live });
  const kinds = (events as { kind: string; phase: string }[]).map((e) => `${e.kind}:${e.phase}`);
  assert.deepEqual(kinds, ['dev:running', 'dev:running', 'dev:done']);
});

test('group trees: picking a node filters rows to its count; folder chains are joined', async () => {
  const { m } = mock();
  const h = m.handlers;
  const t = await h.dev_table({ req: { target: live, source: 'build:LIVE', dataset: 'gamedata.json', table: 'weapons' } });
  const folders = t.group_presets.find((p) => p.id === 'folders:path')!;
  const base = { target: live, source: 'build:LIVE', dataset: 'gamedata.json', table: 'weapons', columns: [] as string[], search: '', filters: [] as never[] };
  const g = await h.dev_groups({ req: { ...base, by: folders.levels, open: ['["entities","scitem","ships","weapons"]'] } });
  assert.deepEqual(
    g.nodes.map((n) => [n.label, n.depth, n.count]),
    [
      ['entities/scitem/ships/weapons', 1, 10],
      ['mining', 2, 2],
    ],
  );
  for (const n of g.nodes) {
    const r = await h.dev_rows({ req: { ...rows('build:LIVE', 'gamedata.json', 'weapons'), limit: 50, group: { by: folders.levels, node: n.id } } });
    assert.equal(r.total, n.count, n.label);
  }
});

test('group tree sorts mixed number and text keys in a total order (numbers first)', async () => {
  const { infer, groupRows } = await import('../src/lib/contract/mock-dev.ts');
  const keys = [...Array.from({ length: 24 }, (_, i) => String(i + 1)), '10z', '12a', '19h', '1a', '1e3', 'abc', 'B'];
  for (let seed = 1; seed <= 32; seed++) {
    const rows: { k?: string; x: number }[] = keys.map((k) => ({ k, x: 1 }));
    rows.push({ x: 2 });
    let st = seed;
    for (let i = rows.length - 1; i > 0; i--) {
      st = (st * 1103515245 + 12345) % 2147483648;
      const j = st % (i + 1);
      [rows[i], rows[j]] = [rows[j]!, rows[i]!];
    }
    const t = infer('m.json', 0, { t: rows }).tables[0]!;
    const p = groupRows(t, { by: [{ column: 'k', folders: false }], open: [], columns: [], search: '', filters: [] });
    assert.deepEqual(p.nodes.slice(0, -1).map((n) => n.label), keys, `seed ${seed}`);
    assert.equal(p.nodes.at(-1)!.count, 1);
  }
});

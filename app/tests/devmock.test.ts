// Rust mock = TS mock for Developer mode (addendum 7.11): every dev sample in samples.gen.json
// (written by the Rust mock) is replayed against the TS mock over the same synthetic data
// (mock-dev.gen.json) and must come back equal. Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createDevMock, type DevMockCtx, type DevMockData } from '../src/lib/contract/mock-dev.ts';

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
const rows = (source: string, dataset: string, table: string) => ({ target: live, source, dataset, table, columns: [] as string[], search: '', filters: [] as never[], sort: null, offset: 0, limit: 5 });
const recs = (dataset: string, table: string) => ({ target: live, dataset, table, state: null, field: null, identity: null, offset: 0, limit: 5 });
const f = (column: string, part: object) => ({ column, values: [], min: null, max: null, has: null, ...part });

test('every dev sample of the Rust mock comes back equal from the TS mock', async () => {
  const { m } = mock();
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
  const first = (got.dev_diff_records as { records: { id: string }[] }).records[0]!.id;
  got.dev_diff_record = await h.dev_diff_record({ req: { target: live, dataset: 'gamedata.json', table: 'components', id: first, identity: null } });
  got.dev_close = await h.dev_close({ req: live });
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

test('dumps need the switch, then are a labelled follow-up', async () => {
  const { m, settings } = mock();
  await assert.rejects(async () => m.handlers.dev_download_dump({ req: live }), (e: Fail) => e.code === 'network-off');
  settings.dumps = true;
  await assert.rejects(async () => m.handlers.dev_download_dump({ req: live }), (e: Fail) => e.code === 'not-implemented');
});

test('open and diff report progress as dev jobs', async () => {
  const { m, events } = mock();
  await m.handlers.dev_open({ req: live });
  const kinds = (events as { kind: string; phase: string }[]).map((e) => `${e.kind}:${e.phase}`);
  assert.deepEqual(kinds, ['dev:running', 'dev:running', 'dev:done']);
});

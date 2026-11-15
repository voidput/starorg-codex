/**
 * Developer mode assets in the in-browser mock: the same behaviour as the Rust mock
 * (app/src-tauri/src/contract/mock_assets.rs) over the same synthetic data
 * (`mock-assets.gen.json`, written by the Rust contract tests). `tests/assetmock.test.ts`
 * checks the answers against the Rust samples. No JSON import here, so node can load it.
 */
import type * as T from './types';
import type * as A from './assets';

export interface AssetMockData {
  files: [string, number][];
  png: string;
  ogg: string;
  sounds: [string, A.RecordSounds][];
  exports: string;
  cacheDefault: string;
  cacheOther: string;
  cacheMax: number;
}

export interface AssetMockCtx {
  devOn(): boolean;
  hasChannel(r: T.ChannelRef): boolean;
  /** A mock-dev record by GUID, path or name. */
  findRecord(what: string): { guid: string; name: string; type: string } | null;
  emit(p: T.JobProgress): void;
  sleep(ms: number): Promise<void>;
  delay: number;
  nextJob(): string;
  fail(code: T.ErrorCode, message: string): never;
}

type H<A, R> = (args: A) => R | Promise<R>;
export interface AssetHandlers {
  asset_status: H<{ req: T.ChannelRef }, A.AssetStatus>;
  asset_close: H<{ req: T.ChannelRef }, A.AssetStatus>;
  asset_list: H<{ req: A.AssetListRequest }, A.DirPage>;
  asset_search: H<{ req: A.AssetSearchRequest }, A.FileSearch>;
  asset_preview: H<{ req: A.AssetPathRequest }, A.AssetPreview>;
  asset_extract: H<{ req: A.AssetExtractRequest }, A.AssetExportResult>;
  asset_sounds: H<{ req: A.AssetRecordRequest }, A.RecordSounds>;
  asset_sound_search: H<{ req: A.AssetSoundSearchRequest }, A.TriggerSearch>;
  asset_trigger: H<{ req: A.AssetTriggerRequest }, A.TriggerInfo>;
  asset_sound: H<{ req: A.AssetSoundRequest }, A.AssetAudio>;
  asset_export_sounds: H<{ req: A.AssetExportSoundsRequest }, A.AssetExportResult>;
  asset_export_model: H<{ req: A.AssetExportModelRequest }, A.AssetExportResult>;
  asset_cache_status: H<Record<string, never>, A.AssetCacheStatus>;
  asset_cache_pick: H<Record<string, never>, A.AssetCacheStatus>;
  asset_cache_reset: H<Record<string, never>, A.AssetCacheStatus>;
  asset_cache_clear: H<Record<string, never>, A.AssetCacheStatus>;
  asset_cache_move: H<Record<string, never>, A.AssetCacheStatus>;
}

/** The trigger search (7.17), as `search_triggers` in mock_assets.rs. */
export function searchTriggers(all: A.TriggerInfo[], query: string, limit: number): A.TriggerSearch {
  const words = query.split(/\s+/).filter(Boolean).map((w) => w.toLowerCase());
  if (words.length === 0) return { hits: [], total: 0 };
  const joined = words.join('_');
  const hits: [number, A.TriggerInfo][] = [];
  for (const t of all) {
    const n = t.name.toLowerCase();
    const other = `${t.event ?? ''} ${t.bank ?? ''}`.toLowerCase();
    if (!words.every((w) => n.includes(w) || other.includes(w))) continue;
    const rank = n.startsWith(joined) || n.includes(`_${joined}`) ? 0 : words.every((w) => n.includes(w)) ? 1 : 2;
    hits.push([rank, t]);
  }
  hits.sort((a, b) => a[0] - b[0] || a[1].name.length - b[1].name.length || (a[1].name < b[1].name ? -1 : a[1].name > b[1].name ? 1 : 0));
  return {
    hits: hits.slice(0, Math.min(limit, 500)).map(([, t]) => ({ name: t.name, event: t.event ?? '', bank: t.bank ?? '', duration_type: t.duration_type })),
    total: hits.length,
  };
}

export const norm = (p: string) => p.trim().replace(/\//g, '\\').replace(/^\\+|\\+$/g, '');
export const ext = (p: string) => {
  const name = p.split('\\').pop() ?? p;
  const i = name.lastIndexOf('.');
  return i < 0 ? '' : name.slice(i + 1).toLowerCase();
};
export const isDdsSibling = (p: string) => {
  const l = p.toLowerCase();
  const at = l.lastIndexOf('.dds.');
  if (at < 0) return false;
  const tail = l.slice(at + 5);
  return tail.length > 0 && /^[0-9]*$/.test(tail.replace(/a+$/, ''));
};
const cmpName = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);
const lodKey = (o: { preset: A.ModelPreset; lod: number }) => `${o.preset === 'full' ? 'full' : 'print'}-lod${o.lod}`;
export const modelFileName = (name: string, o: { preset: A.ModelPreset; lod: number }) => {
  const base = [...name].map((c) => (/[A-Za-z0-9_-]/.test(c) ? c : '_')).join('');
  return `${base || 'model'}-${lodKey(o)}.glb`;
};

export function createAssetMock(data: AssetMockData, ctx: AssetMockCtx): { handlers: AssetHandlers } {
  const files = data.files;
  const sounds = new Map(data.sounds);
  /** Every trigger the mock knows, first record first (as the Rust mock). */
  const allTriggers = (): A.TriggerInfo[] => {
    const out: A.TriggerInfo[] = [];
    for (const [, s] of data.sounds) for (const t of s.triggers) if (!out.some((x) => x.name === t.name)) out.push(t);
    return out;
  };
  let open: string | null = null;
  let datacore = false;
  let audio = false;
  let cached: [string, number][] = [];
  let custom = false;
  let previous: [string, number, number] | null = null;
  const key = (r: T.ChannelRef) => `${r.install}/${r.channel}`;

  const on = (r: T.ChannelRef) => {
    if (!ctx.devOn()) ctx.fail('dev-disabled', 'Developer mode is off (Settings > Advanced).');
    if (!ctx.hasChannel(r)) ctx.fail('not-found', `No channel ${r.channel} in install ${r.install}`);
  };
  const openW = (r: T.ChannelRef) => {
    on(r);
    open = key(r);
  };
  const has = (k: string) => cached.some((c) => c[0] === k);
  const add = (k: string, n: number) => {
    if (!has(k)) cached.push([k, n]);
  };
  const view = (): A.AssetCacheStatus => ({
    path: custom ? data.cacheOther : data.cacheDefault,
    default_path: data.cacheDefault,
    is_default: !custom,
    bytes: cached.reduce((a, c) => a + c[1], 0),
    files: cached.length,
    max_bytes: data.cacheMax,
    other_drive: custom,
    previous: previous ? { path: previous[0], bytes: previous[1], files: previous[2] } : null,
    problem: null,
  });
  const sw = (to: boolean) => {
    if (custom !== to) {
      const old = custom ? data.cacheOther : data.cacheDefault;
      const bytes = cached.reduce((a, c) => a + c[1], 0);
      previous = cached.length > 0 ? [old, bytes, cached.length] : null;
      cached = [];
      custom = to;
    }
    return view();
  };
  const job = async (channel: string, label: string) => {
    const id = ctx.nextJob();
    for (const [step, phase] of [
      [1, 'running'],
      [2, 'done'],
    ] as const) {
      if (step === 2 && ctx.delay) await ctx.sleep(ctx.delay);
      ctx.emit({ job: id, kind: 'dev', channel, phase, step, steps: 2, label, cancellable: true });
    }
  };
  const status = (r: T.ChannelRef): A.AssetStatus => {
    const o = open === key(r);
    return {
      state: o ? 'ready' : 'stopped',
      channel: r.channel,
      entries: o ? files.length : 0,
      datacore: o && datacore,
      audio: o && audio,
      memory: o ? 910 * 1024 * 1024 : null,
      error: null,
    };
  };
  const list = (dirRaw: string, offset: number, limit: number): A.DirPage | null => {
    const dir = norm(dirRaw);
    const prefix = dir ? `${dir}\\` : '';
    const rows: A.FileEntry[] = [];
    const seen = new Set<string>();
    for (const [p, size] of files) {
      if (!p.startsWith(prefix)) continue;
      const rest = p.slice(prefix.length);
      const i = rest.indexOf('\\');
      if (i >= 0) {
        const sub = rest.slice(0, i);
        if (!seen.has(sub)) {
          seen.add(sub);
          rows.push({ name: sub, path: `${prefix}${sub}`, dir: true, size: 0, packed: 0 });
        }
      } else rows.push({ name: rest, path: p, dir: false, size, packed: Math.floor((size * 3) / 4) });
    }
    if (rows.length === 0 && dir) return null;
    rows.sort((x, y) => (x.dir !== y.dir ? (x.dir ? -1 : 1) : cmpName(x.name.toLowerCase(), y.name.toLowerCase())));
    const parent = dir ? (dir.lastIndexOf('\\') >= 0 ? dir.slice(0, dir.lastIndexOf('\\')) : '') : null;
    return { dir, parent, entries: rows.slice(offset, offset + Math.min(limit, 500)), offset, total: rows.length };
  };

  const handlers: AssetHandlers = {
    asset_status: ({ req }) => {
      on(req);
      return status(req);
    },
    asset_close: ({ req }) => {
      on(req);
      open = null;
      datacore = false;
      audio = false;
      return status(req);
    },
    asset_list: ({ req }) => {
      openW(req.target);
      return list(req.dir, req.offset, req.limit) ?? ctx.fail('not-found', `no folder ${norm(req.dir)}`);
    },
    asset_search: ({ req }) => {
      openW(req.target);
      const words = req.query.replace(/\//g, '\\').split(/\s+/).filter(Boolean).map((w) => w.toLowerCase());
      const hits = words.length === 0 ? [] : files.filter(([p]) => words.every((w) => p.toLowerCase().includes(w)));
      return {
        query: req.query,
        total: hits.length,
        hits: hits.slice(0, Math.min(req.limit, 500)).map(([p, size]) => ({ name: p.split('\\').pop() ?? p, path: p, dir: false, size, packed: Math.floor((size * 3) / 4) })),
      };
    },
    asset_preview: ({ req }) => {
      on(req.target);
      const path = norm(req.path);
      const f = files.find(([p]) => p.toLowerCase() === path.toLowerCase()) ?? ctx.fail('not-found', `no file ${path}`);
      const x = ext(path);
      const k = `preview:${path.toLowerCase()}`;
      const p: A.AssetPreview = { path, size: f[1], kind: 'none', format: x, text: null, truncated: false, width: null, height: null, image: null, note: null, cached: false };
      if (x === 'dds') {
        p.kind = 'image';
        p.image = `data:image/png;base64,${data.png}`;
        if (has(k)) {
          p.cached = true;
          p.size = 0;
        } else {
          openW(req.target);
          p.width = 1024;
          p.height = 1024;
          p.note = '2048 x 2048, shown at 1024 x 1024';
          add(k, 151);
        }
        return p;
      }
      openW(req.target);
      if (x === 'xml' || x === 'mtl' || x === 'txt') {
        p.kind = 'text';
        p.format = x === 'mtl' ? 'cryxml' : x;
        p.text = p.format === 'txt' ? 'plain text' : `<Config path="${path.replace(/\\/g, '/')}">\n  <Value key="speed" v="200"/>\n  <Value key="mass" v="1200"/>\n</Config>\n`;
      } else if (x === 'wem') p.kind = 'audio';
      else if (isDdsSibling(path)) p.note = 'A mip level of a texture; open the .dds file';
      else p.note = 'No preview for this file type; extract it';
      return p;
    },
    asset_extract: async ({ req }) => {
      openW(req.target);
      if (req.paths.length === 0) ctx.fail('invalid', 'nothing selected');
      const picked = new Map<string, number>();
      for (const raw of req.paths) {
        const p = norm(raw);
        const pre = `${p.toLowerCase()}\\`;
        const hits = files.filter(([f]) => f.toLowerCase() === p.toLowerCase() || f.toLowerCase().startsWith(pre));
        if (hits.length === 0) ctx.fail('not-found', `no file or folder ${p}`);
        for (const [f, n] of hits) picked.set(f, n);
      }
      const all = [...picked.entries()];
      const merged = req.convert ? all.filter(([f]) => isDdsSibling(f)).length : 0;
      const n = all.length - merged;
      const bytes = all.filter(([f]) => !(req.convert && isDdsSibling(f))).reduce((a, [, s]) => a + s, 0);
      await job(req.target.channel, 'Extracting game files');
      return { cancelled: false, path: data.exports, files: n, bytes, cached: false, failed: [], note: merged > 0 ? `${n} converted, ${merged} texture mip files merged into their PNG` : null };
    },
    asset_sounds: ({ req }) => {
      openW(req.target);
      datacore = true;
      audio = true;
      const s = sounds.get(req.what) ?? [...sounds.values()].find((x) => x.name === req.what);
      if (s) return structuredClone(s);
      const r = ctx.findRecord(req.what) ?? ctx.fail('not-found', `no record ${JSON.stringify(req.what)}`);
      return { record: r.guid, name: r.name, triggers: [], sounds: 0, playable: 0, loadout: 0 };
    },
    asset_sound_search: ({ req }) => {
      openW(req.target);
      audio = true;
      return searchTriggers(allTriggers(), req.query, req.limit);
    },
    asset_trigger: ({ req }) => {
      openW(req.target);
      audio = true;
      const t = allTriggers().find((x) => x.name === req.name);
      return t ? { ...structuredClone(t), property: '', from: '' } : ctx.fail('not-found', `no sound event ${JSON.stringify(req.name)}`);
    },
    asset_sound: ({ req }) => {
      on(req.target);
      const k = String(req.sound.media_id);
      if (req.sound.source === 'unavailable') ctx.fail('not-found', `media ${req.sound.media_id} has no data in the game files`);
      const c = has(`audio:${k}`);
      if (!c) {
        openW(req.target);
        add(`audio:${k}`, 2824);
      }
      return { key: k, mime: 'audio/ogg', data: data.ogg, bytes: 2824, duration: c ? null : 0.25, cached: c, note: null };
    },
    asset_export_sounds: async ({ req }) => {
      on(req.target);
      let picks = req.sounds;
      if (picks.length === 0) {
        const s = sounds.get(req.what) ?? (ctx.findRecord(req.what) ? null : ctx.fail('not-found', `no record ${JSON.stringify(req.what)}`));
        const seen = new Set<string>();
        picks = [];
        for (const t of s?.triggers ?? [])
          for (const x of t.sounds)
            if (x.playable && !seen.has(String(x.media_id))) {
              seen.add(String(x.media_id));
              picks.push({ trigger: t.name, sound: { media_id: x.media_id, source: x.source, bank: x.bank, media_path: x.media_path } });
            }
      }
      if (picks.length === 0) ctx.fail('not-found', 'this record has no playable sounds');
      const c = !req.wem && picks.every((p) => has(`audio:${p.sound.media_id}`));
      if (!c) {
        openW(req.target);
        await job(req.target.channel, 'Exporting sounds');
        for (const p of picks) add(`audio:${p.sound.media_id}`, 2824);
      }
      const n = picks.length * (req.wem ? 2 : 1);
      return { cancelled: false, path: data.exports, files: n, bytes: n * 2824, cached: c, failed: [], note: null };
    },
    asset_export_model: async ({ req }) => {
      on(req.target);
      const r = ctx.findRecord(req.what) ?? ctx.fail('not-found', `no record ${JSON.stringify(req.what)}`);
      if (r.type !== 'SynShip' && r.type !== 'SynComponent') ctx.fail('invalid', `${r.name} is not an entity (EntityClassDefinition); only entities have models`);
      const o = { preset: req.preset, lod: Math.min(req.lod, 4) };
      const k = `model:${req.what}-${lodKey(o)}`;
      const c = has(k);
      const bytes = req.preset === 'full' ? 348136664 : 56398596;
      if (!c) {
        await job(req.target.channel, `Exporting model ${r.name}`);
        if (bytes <= data.cacheMax / 4) add(k, bytes);
      }
      const name = req.name || r.name;
      return { cancelled: false, path: `${data.exports}/${modelFileName(name, o)}`, files: 1 + (req.stl ? 1 : 0), bytes: bytes + (req.stl ? 183149634 : 0), cached: c, failed: [], note: null };
    },
    asset_cache_status: () => view(),
    asset_cache_pick: () => sw(true),
    asset_cache_reset: () => sw(false),
    asset_cache_clear: () => {
      cached = [];
      return view();
    },
    asset_cache_move: () => {
      if (!previous) ctx.fail('not-found', 'no previous cache folder to move from');
      const [, bytes, n] = previous;
      for (let k = 0; k < n; k++) cached.push([`moved:${k}`, Math.floor(bytes / Math.max(n, 1))]);
      previous = null;
      return view();
    },
  };
  return { handlers };
}

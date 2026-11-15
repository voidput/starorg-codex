/**
 * In-browser mock transport: the same behaviour as the Rust MockBackend
 * (app/src-tauri/src/contract/mock.rs), for `npm run dev` and headless renders without Tauri.
 * Seeded from samples.gen.json (real Rust mock output), so the shapes are the contract's.
 * All data is synthetic: no real character names, geids or CIG text.
 *
 * URL switches: `?delay=<ms>` per job step (default 250), `?state=ready` to start with LIVE
 * and HOTFIX set up (skips the wizard).
 */
import type { Args, Command, Result, Transport } from './client';
import { ContractError } from './client';
import samples from './samples.gen.json';
import type * as T from './types';

const clone = <X>(x: X): X => structuredClone(x);
const SAMPLES = samples as unknown as {
  overview_first_run: T.Overview;
  get_profile: T.Profile;
  list_patches: T.PatchView[];
  get_settings: T.Settings;
  list_characters: T.CharactersView;
  owned_blueprints: T.OwnedBlueprint[];
  hooks_status: T.HooksStatus;
  doctor: T.DoctorReport;
  starorg_status: T.StarOrgStatus;
  import_smart_citizen: T.ImportReport;
};

const NOW = '2026-11-14T21:30:05Z';
const STRING_COUNT = 91_000;
const CATEGORIES = ['ships', 'components', 'ship-weapons', 'missiles', 'fps-weapons', 'missions', 'items', 'ui'];

interface Chan {
  summary: T.ChannelSummary;
  backups: T.BackupView[];
  profile: T.Profile;
  overrides: Map<string, string>;
  patches: T.PatchView[];
  active: T.ActiveCharacter;
  owned: Map<string, T.OwnedBlueprint[]>;
}

const fail = (code: T.ErrorCode, message: string, detail: string | null = null): never => {
  throw new ContractError({ code, message, detail });
};

function presetProfile(p: T.Preset): T.Profile {
  const base = clone(SAMPLES.get_profile);
  base.preset = p;
  for (const c of base.categories) {
    c.enabled = p === 'everything' ? true : p === 'recommended' ? c.id !== 'blueprints' : c.id === 'name-tags';
    for (const l of c.lines) l.enabled = true;
  }
  base.missions.tag_bp = base.missions.tag_bp_maybe = base.missions.tag_rep = p !== 'minimal';
  base.missions.mark_owned = p === 'everything';
  base.fix_known_bugs = true;
  base.patches_disabled = [];
  return base;
}

function syntheticStrings(): T.StringRow[] {
  const rows: T.StringRow[] = new Array(STRING_COUNT);
  const pad = (i: number) => String(i).padStart(5, '0');
  for (let i = 0; i < STRING_COUNT; i++) {
    const cat = CATEGORIES[i % CATEGORIES.length]!;
    let key: string;
    let original: string;
    switch (cat) {
      case 'ships':
        key = `vehicle_DescSYN_Ship_${pad(i)}`;
        original = `Synthetic hull ${i}. A test ship description.\\nSecond line.`;
        break;
      case 'components':
        key = `item_NameSYN_Cooler_S${(i % 4) + 1}_${pad(i)}`;
        original = `Sample Cooler ${i}`;
        break;
      case 'ship-weapons':
        key = `item_NameSYN_Laser_S${(i % 6) + 1}_${pad(i)}`;
        original = `Test Repeater ${i}`;
        break;
      case 'missiles':
        key = `item_NameSYN_Missile_S${(i % 3) + 1}_${pad(i)}`;
        original = `Sample Missile ${i}`;
        break;
      case 'fps-weapons':
        key = `item_DescSYN_Rifle_${pad(i)}`;
        original = `A synthetic rifle, number ${i}.`;
        break;
      case 'missions':
        key = `mission_TitleSYN_Contract_${pad(i)}`;
        original = `Synthetic Contract ${i}`;
        break;
      case 'items':
        key = `item_NameSYN_Item_${pad(i)}`;
        original = `Sample item ${i}`;
        break;
      default:
        key = `ui_SYN_Label_${pad(i)}`;
        original = `Label ${i}`;
    }
    let result = original;
    let source: T.StringSource = 'unmodified';
    if (i % 97 === 0) {
      result = `${original} (fixed)`;
      source = 'patched';
    } else if (i % 9 === 0) {
      source = 'enhanced';
      if (cat === 'ships') result = `${original}\\n\\n--- STATS ---\\nRole: Test Fighter  |  Crew: 1\\nSCM: 226 m/s  |  Max: 1,193 m/s`;
      else if (cat === 'missions') result = `${original} <EM4>[BP]</EM4> <EM4>[150 Rep]</EM4>`;
      else if (cat === 'components' || cat === 'ship-weapons' || cat === 'missiles') result = `[S${(i % 4) + 1}-A] ${original}`;
      else result = `${original}\\n\\n--- STATS ---\\nDamage: 12.5  |  RPM: 600`;
    }
    rows[i] = { key, original, result, source, category: cat };
  }
  return rows;
}

function checkOverride(original: string, value: string): [string, string] | null {
  if (/[\n\r]/.test(value)) return ['newline', 'A line break must be written as the two characters \\n.'];
  if ((value.match(/<EM/g) ?? []).length !== (value.match(/<\/EM/g) ?? []).length) return ['em-tags', 'Every <EMn> tag needs its closing </EMn>.'];
  const spec = (s: string) => (s.match(/%[sd]/g) ?? []).length;
  if (spec(original) !== spec(value)) return ['format-specifiers', "The value must keep the original's %s / %d placeholders."];
  if (/% [A-Za-z]/.test(value)) return ['percent', 'A % must be followed by the end, \\n or two spaces.'];
  return null;
}

function emptyReport(s: T.ChannelSummary, result: T.ApplyResult): T.ApplyReport {
  return {
    channel: s.channel,
    result,
    reason: null,
    build: clone(s.build),
    counts: { changed: 0, ships: 0, components: 0, ship_weapons: 0, fps_weapons: 0, missiles: 0, missions: 0, overrides: 0, dropped: 0 },
    patches: [],
    warnings: [],
    user_cfg: { changed: false, path: null, link_target: null, shared_with: [], line: null },
    backup: null,
    backup_dir: null,
    target: null,
    target_is_link: false,
    duration_ms: 0,
  };
}

/** Preview samples from a profile: same rules as `mock::preview` in Rust. */
export function preview(p: T.Profile): T.PreviewItem[] {
  const on = (id: T.CategoryId) => p.categories.some((c) => c.id === id && c.enabled);
  const lineOn = (cat: T.CategoryId, l: string) => p.categories.some((c) => c.id === cat && c.enabled && c.lines.some((x) => x.id === l && x.enabled));
  const block = (prose: string, lines: string[]) => {
    if (!lines.length) return prose;
    const stats = `${p.format.header}\\n${lines.join('\\n')}`;
    return p.format.placement === 'below-prose' ? `${prose}\\n\\n${stats}` : `${stats}\\n\\n------\\n\\n${prose}`;
  };
  const shipProse = 'A synthetic light fighter used for testing.';
  const lines: string[] = [];
  if (on('ships')) {
    if (lineOn('ships', 'role')) lines.push('Role: Light Fighter  |  Career: Combat  |  Crew: 1');
    if (lineOn('ships', 'size')) lines.push('Size: 21 × 16.5 × 5.5 m');
    if (lineOn('ships', 'speed')) lines.push('SCM: 226 m/s  |  Max: 1,193 m/s');
    if (lineOn('ships', 'boost')) lines.push('Boost: 520 m/s fwd  |  268 m/s back');
  }
  const name = 'Sample Cooler';
  let tagged = name;
  const t = p.tags.find((x) => x.kind === 'component');
  if (on('name-tags') && t) {
    const word: Record<string, Record<T.TagStyle, string>> = {
      class: { short: 'IND', medium: 'Industrial', long: 'Industrial' },
      size: { short: 'S2', medium: 'S2', long: 'Size 2' },
      grade: { short: 'A', medium: 'A', long: 'Grade A' },
    };
    const parts = t.elements.filter((e) => e.enabled).map((e) => word[e.id]?.[e.style] ?? '').filter(Boolean);
    const sep = { dash: '-', none: '', space: ' ', underscore: '_', dot: '.', slash: '/', pipe: '|' }[t.separator];
    const [l, r] = { square: ['[', ']'], round: ['(', ')'], curly: ['{', '}'], angle: ['<', '>'], none: ['', ''] }[t.enclosing];
    if (parts.length) {
      const tag = `${l}${parts.join(sep)}${r}`;
      tagged = t.placement === 'prefix' ? `${tag} ${name}` : `${name} ${tag}`;
    }
  }
  const title = 'Synthetic Recovery Contract';
  let tt = title;
  if (on('missions')) {
    const wrap = (s: string) => (p.missions.em4_title_tags ? ` <EM4>${s}</EM4>` : ` ${s}`);
    if (p.missions.tag_bp) tt += wrap('[BP]');
    if (p.missions.tag_rep) tt += wrap('[150 Rep]');
  }
  return [
    { key: 'vehicle_DescSYN_Fighter', label: 'Ship description', category: 'ships', original: shipProse, result: block(shipProse, lines) },
    { key: 'item_NameSYN_Cooler_S2', label: 'Component name', category: 'components', original: name, result: tagged },
    { key: 'mission_TitleSYN_Recovery', label: 'Mission title', category: 'missions', original: title, result: tt },
  ];
}

type Handler<K extends Command> = (args: Args<K>) => Result<K> | Promise<Result<K>>;
type Handlers = { [K in Command]: Handler<K> };

export function createMockTransport(opts: { delay?: number; ready?: boolean } = {}): Transport {
  const params = typeof window !== 'undefined' ? new URLSearchParams(window.location.search) : new URLSearchParams();
  const delay = opts.delay ?? (params.has('delay') ? Number(params.get('delay')) : 250);
  const startReady = opts.ready ?? params.get('state') === 'ready';

  const installs: T.InstallSummary[] = clone(SAMPLES.overview_first_run.installs);
  const chans = new Map<string, Chan>();
  const id = (r: T.ChannelRef) => `${r.install}/${r.channel}`;
  for (const inst of installs) {
    for (const c of inst.channels) {
      const owned = new Map<string, T.OwnedBlueprint[]>();
      const chars = SAMPLES.list_characters.characters;
      owned.set(chars[0]!.id, clone(SAMPLES.owned_blueprints));
      owned.set(chars[1]!.id, clone(SAMPLES.owned_blueprints.filter((b, i) => i === 0)).concat([{ ...clone(SAMPLES.owned_blueprints[0]!), name: 'Sample Undersuit' }]));
      owned.set(chars[2]!.id, []);
      chans.set(`${inst.id}/${c.channel}`, {
        summary: c,
        backups: [],
        profile: presetProfile('recommended'),
        overrides: new Map(),
        patches: clone(SAMPLES.list_patches),
        active: { mode: 'auto', character: null, resolved: chars[0]!.id },
        owned,
      });
    }
  }
  let settings: T.Settings = { ...clone(SAMPLES.get_settings), network: { check_updates: false, published_dumps: false, starorg: false } };
  const starorg: T.StarOrgStatus = { ...clone(SAMPLES.starorg_status), connected: false, user: null, guild: null, expires: null, sync_characters: [] };
  let hooks: T.HooksStatus = clone(SAMPLES.hooks_status);
  const log: T.LogEntry[] = [];
  let nextLog = 1;
  let nextJob = 1;
  let strings: T.StringRow[] | null = null;
  const listeners = new Set<(p: T.JobProgress) => void>();

  const addLog = (level: T.LogLevel, job: T.JobKind | null, channel: string | null, message: string) => log.push({ id: nextLog++, time: NOW, level, job, channel, message });
  addLog('info', null, null, 'Found 2 installs: LUG Helper config, Lutris game entry');

  const chan = (r: T.ChannelRef): Chan => chans.get(id(r)) ?? fail('not-found', `No channel ${r.channel} in install ${r.install}`);
  const sync = () => {
    for (const inst of installs) inst.channels = inst.channels.map((c) => chans.get(`${inst.id}/${c.channel}`)!.summary);
  };
  const table = () => (strings ??= syntheticStrings());
  const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
  const detail = (r: T.ChannelRef, key: string): T.StringDetail => {
    const base = table().find((x) => x.key.toLowerCase() === key.toLowerCase()) ?? fail('not-found', `No key ${key} in stock`);
    const row = { ...base };
    const ov = chan(r).overrides.get(row.key) ?? null;
    const provenance: T.Provenance[] = [{ layer: 'stock', by: 'Data.p4k', text: row.original, winner: row.source === 'unmodified' && ov === null, note: null }];
    if (row.source === 'enhanced') provenance.push({ layer: 'generated', by: row.category, text: row.result, winner: ov === null, note: 'Owner by namesake rule; 2 paint variants shared the same text' });
    if (row.source === 'patched') provenance.push({ layer: 'patch', by: 'syn-typo-cargo', text: row.result, winner: ov === null, note: null });
    if (ov !== null) {
      provenance.push({ layer: 'override', by: 'overrides.ini', text: ov, winner: true, note: null });
      row.result = ov;
      row.source = 'edited';
    }
    return { row, provenance, override_value: ov };
  };
  const offers = (c: Chan): T.RestoreOffers => {
    const s = c.summary;
    const undo = [...c.backups].reverse().find((b) => b.reason === 'before-apply' && b.origin === 'ours' && b.build.change === s.build.change);
    const pinned = c.backups.find((b) => b.pinned && b.origin === 'foreign');
    return {
      remove: s.status.state !== 'not-applied',
      remove_foreign: s.status.state === 'changed-by-another-tool',
      undo_last_apply: undo?.id ?? null,
      restore_foreign: pinned
        ? { backup: pinned.id, tool: pinned.foreign_tool, made_for: clone(pinned.build), current: clone(s.build), build_mismatch: pinned.build.change !== s.build.change }
        : null,
    };
  };
  const mkBackup = (seq: number, reason: T.BackupReason, build: T.Build, origin: T.FileOrigin, tool: string | null): T.BackupView => ({
    id: `20261114-2130${String(seq).padStart(2, '0')}-${reason}`,
    created: NOW,
    reason,
    pinned: reason === 'pre-existing',
    build: clone(build),
    existed: origin !== 'none',
    size: origin !== 'none' ? 11_204_331 : null,
    origin,
    foreign_tool: tool,
  });
  const oldBuild: T.Build = { branch: 'sc-alpha-4.9.1', game_version: '4.9.1', change: 13_002_118, version: '4.9.141.51240' };
  const starorgView = (channel: string) => {
    const c = chans.get(`${installs[0]!.id}/${channel}`);
    const chars = SAMPLES.list_characters.characters;
    const ticked = new Set(starorg.sync_characters.length ? starorg.sync_characters.filter((x) => x.ticked).map((x) => x.id) : chars.map((x) => x.id));
    starorg.sync_characters = chars.map((x) => ({ id: x.id, name: x.name, ticked: ticked.has(x.id), owned: c?.owned.get(x.id)?.length ?? 0 }));
    const union = new Set<string>();
    for (const t of ticked) for (const b of c?.owned.get(t) ?? []) union.add(b.name);
    starorg.push_count = union.size;
    starorg.enabled = settings.network.starorg;
    return clone(starorg);
  };
  const charactersView = (c: Chan): T.CharactersView => ({
    ...clone(SAMPLES.list_characters),
    channel: c.summary.channel,
    characters: SAMPLES.list_characters.characters.map((x) => ({ ...x, owned: c.owned.get(x.id)?.length ?? 0 })),
    active: clone(c.active),
  });
  const firstRun = () => !installs.some((i) => i.channels.some((c) => c.set_up));

  const h: Handlers = {
    overview: () => {
      sync();
      const active = installs.find((i) => i.saved)?.id ?? null;
      return {
        contract: 1,
        app_version: SAMPLES.overview_first_run.app_version,
        first_run: firstRun(),
        installs: clone(installs),
        active_install: active,
        default_channel: settings.default_channel ?? installs.find((i) => i.id === active)?.channels.find((c) => c.set_up)?.channel ?? null,
        job: null,
      };
    },
    discover_installs: async () => {
      await sleep(Math.min(delay * 2, 600));
      sync();
      return clone(installs);
    },
    pick_install_folder: () => {
      const i = installs[1];
      return i ? { ...clone(i), found_by: 'manual', found_by_label: 'Chosen folder' } : null;
    },
    setup: ({ req }) => {
      const set_up: string[] = [];
      const skipped: string[] = [];
      for (const c of req.channels) {
        const ch = chan({ install: req.install, channel: c.channel });
        if (!c.consent) {
          skipped.push(c.channel);
          continue;
        }
        if (ch.summary.language === 'other' && !c.switch_language) {
          fail('needs-decision', `${c.channel}: the game language is not English. Choose whether to switch it.`, ch.summary.language_value);
        }
        ch.summary.set_up = true;
        ch.profile = { ...presetProfile(req.preset), fix_known_bugs: req.fix_known_bugs };
        set_up.push(c.channel);
      }
      for (const i of installs) i.saved = i.id === req.install;
      settings.network.published_dumps = req.data_source === 'published-dumps';
      settings.default_channel ??= set_up[0] ?? null;
      sync();
      addLog('info', null, null, `Set up ${set_up.join(', ')} (skipped: ${skipped.join(', ')})`);
      return { install: req.install, set_up, skipped };
    },
    channel_status: ({ req }) => clone(chan(req).summary),
    apply: async ({ req }) => {
      const c = chan(req.target);
      const name = req.target.channel;
      if (!c.summary.set_up) fail('needs-decision', `${name} is not set up. Run setup for this channel first.`, `starorg-strings setup --channel ${name} --preset recommended --yes`);
      const job = `job-${++nextJob}`;
      const steps = ['Checking the game is not running', 'Reading game data', 'Building text', 'Validating', 'Backing up and writing'];
      for (let i = 0; i < steps.length; i++) {
        const last = req.dry_run && i === steps.length - 1;
        const p: T.JobProgress = { job, kind: 'apply', channel: name, phase: 'running', step: i + 1, steps: steps.length, label: last ? 'Dry run: nothing written' : steps[i]!, cancellable: i < 3 };
        listeners.forEach((l) => l(p));
        if (delay > 0) await sleep(delay);
      }
      const r = emptyReport(c.summary, req.dry_run ? 'dry-run' : 'applied');
      const on = (cid: T.CategoryId) => c.profile.categories.some((x) => x.id === cid && x.enabled);
      r.counts = {
        ships: on('ships') ? 231 : 0,
        components: on('components') ? 1840 : 0,
        ship_weapons: on('ship-weapons') ? 402 : 0,
        fps_weapons: on('fps-weapons') ? 318 : 0,
        missiles: on('missiles') ? 96 : 0,
        missions: on('missions') ? 2140 : 0,
        overrides: c.overrides.size,
        dropped: 0,
        changed: 0,
      };
      const k = r.counts;
      k.changed = (k.ships + k.components + k.ship_weapons + k.fps_weapons + k.missiles + k.missions) * 2 + k.overrides + 1200;
      r.patches = c.patches.map((x) => ({ id: x.id, state: x.enabled ? x.state : 'disabled' }));
      if (c.patches.some((x) => x.enabled && x.state === 'target-changed')) r.warnings.push({ kind: 'patch-target-changed', detail: 'syn-moved-key: the target text changed in this build' });
      const inst = installs.find((i) => i.id === req.target.install)!;
      r.target = `${inst.root}/${name}/Data/Localization/english/global.ini`;
      const notEnglish = c.summary.language !== 'english';
      r.user_cfg = {
        changed: !req.dry_run && notEnglish,
        path: `${inst.root}/${name}/USER.cfg`,
        link_target: '/home/pilot/Games/star-citizen/shared/USER.cfg',
        shared_with: [],
        line: notEnglish ? 'g_language = english' : null,
      };
      r.backup_dir = `~/.local/share/starorg-strings/channels/${req.target.install}/${name}/backups`;
      r.duration_ms = 1830;
      if (!req.dry_run) {
        const seq = c.backups.length + 1;
        if (!c.backups.length) {
          c.backups.push(mkBackup(seq, 'pre-existing', c.summary.foreign_file ? oldBuild : c.summary.build, c.summary.foreign_file ? 'foreign' : 'none', c.summary.foreign_file ? c.summary.foreign_tool : null));
        } else if (c.summary.status.state === 'applied' || c.summary.status.state === 'stale') {
          c.backups.push(mkBackup(seq, 'before-apply', c.summary.build, 'ours', null));
        }
        r.backup = c.backups.at(-1)?.id ?? null;
        c.summary.status = { state: 'applied' };
        c.summary.foreign_file = false;
        c.summary.cache = 'fresh';
      }
      const done: T.JobProgress = { job, kind: 'apply', channel: name, phase: 'done', step: steps.length, steps: steps.length, label: req.dry_run ? 'Dry run finished' : `Applied to ${name}`, cancellable: false };
      listeners.forEach((l) => l(done));
      addLog(r.warnings.length ? 'warn' : 'info', 'apply', name, `${req.dry_run ? 'Dry run' : 'Applied'}: ${k.changed} strings changed, ${r.warnings.length} warning(s)`);
      sync();
      return r;
    },
    remove_changes: ({ req }) => {
      const c = chan(req);
      if (c.summary.status.state === 'not-applied') return emptyReport(c.summary, 'nothing-to-do');
      if (c.summary.foreign_file) c.backups.push(mkBackup(c.backups.length + 1, 'before-restore', c.summary.build, 'foreign', c.summary.foreign_tool));
      c.summary.status = { state: 'not-applied' };
      c.summary.foreign_file = false;
      addLog('info', 'remove', req.channel, 'Removed StarOrg changes (back to stock)');
      return emptyReport(c.summary, 'removed');
    },
    undo_last_apply: ({ req }) => {
      const c = chan(req);
      const undo = offers(c).undo_last_apply ?? fail('needs-decision', 'No backup of an earlier apply for this build. Re-apply instead.');
      c.backups = c.backups.filter((b) => b.id !== undo);
      c.summary.status = { state: 'stale', reasons: ['profile'] };
      return { ...emptyReport(c.summary, 'restored'), backup: undo };
    },
    restore_backup: ({ req }) => {
      const c = chan(req.target);
      const b = c.backups.find((x) => x.id === req.backup) ?? fail('not-found', `No backup ${req.backup}`);
      if (b.build.change !== c.summary.build.change && !req.allow_build_mismatch) {
        fail('needs-decision', 'This file was made for another game build.', `Made for ${b.build.version ?? 'unknown'}, the game is now ${c.summary.build.version ?? 'unknown'}. New text would show as raw @keys.`);
      }
      c.summary.status = { state: 'changed-by-another-tool', tool: b.foreign_tool };
      c.summary.foreign_file = b.origin === 'foreign';
      c.summary.foreign_tool = b.foreign_tool;
      return { ...emptyReport(c.summary, 'restored'), backup: b.id };
    },
    list_backups: ({ req }) => {
      const c = chan(req);
      return { channel: req.channel, folder: `~/.local/share/starorg-strings/channels/${req.install}/${req.channel}/backups`, backups: clone([...c.backups].reverse()), offers: offers(c), keep_newest: 10 };
    },
    backup_now: ({ req }) => {
      const c = chan(req);
      const st = c.summary.status.state;
      const b = mkBackup(c.backups.length + 1, 'manual', c.summary.build, st === 'not-applied' ? 'none' : st === 'changed-by-another-tool' ? 'foreign' : 'ours', c.summary.foreign_tool);
      c.backups.push(b);
      return clone(b);
    },
    delete_backup: ({ req }) => {
      const c = chan(req.target);
      if (c.backups.some((b) => b.id === req.backup && b.pinned)) fail('invalid', 'The pre-existing backup is pinned and cannot be deleted.');
      c.backups = c.backups.filter((b) => b.id !== req.backup);
      return h.list_backups({ req: req.target }) as T.BackupList;
    },
    cancel_job: () => null,
    build_preview: ({ req }) => preview(req.profile ?? (req.preset ? presetProfile(req.preset) : chan(req.target).profile)),
    get_profile: ({ req }) => clone(chan(req).profile),
    preset_profile: ({ preset }) => presetProfile(preset),
    save_profile: ({ req }) => {
      const c = chan(req.target);
      c.profile = clone(req.profile);
      if (c.summary.status.state === 'applied') c.summary.status = { state: 'stale', reasons: ['profile'] };
      return clone(c.profile);
    },
    copy_profile: ({ req }) => h.save_profile({ req: { target: req.to, profile: chan(req.from).profile } }),
    query_strings: ({ req }) => {
      const c = chan(req.target);
      const needle = req.search.toLowerCase();
      const srcCounts = new Map<string, number>();
      const catCounts = new Map<string, number>();
      let hits: T.StringRow[] = [];
      for (const base of table()) {
        const ov = c.overrides.get(base.key);
        const r = ov === undefined ? base : { ...base, result: ov, source: 'edited' as const };
        if (req.changed_only && r.source === 'unmodified') continue;
        if (needle && !r.key.toLowerCase().includes(needle) && !r.original.toLowerCase().includes(needle) && !r.result.toLowerCase().includes(needle)) continue;
        const srcOk = !req.sources.length || req.sources.includes(r.source);
        const catOk = !req.categories.length || req.categories.includes(r.category);
        if (catOk) srcCounts.set(r.source, (srcCounts.get(r.source) ?? 0) + 1);
        if (srcOk) catCounts.set(r.category, (catCounts.get(r.category) ?? 0) + 1);
        if (srcOk && catOk) hits.push(r);
      }
      const cmp = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);
      if (req.sort === 'key') hits.sort((a, b) => cmp(a.key, b.key));
      else if (req.sort === 'key-desc') hits.sort((a, b) => cmp(b.key, a.key));
      else if (req.sort === 'category') hits.sort((a, b) => cmp(a.category, b.category) || cmp(a.key, b.key));
      else hits.sort((a, b) => cmp(a.source, b.source) || cmp(a.key, b.key));
      const total = hits.length;
      const limit = Math.min(Math.max(req.limit, 1), 1000);
      hits = hits.slice(req.offset, req.offset + limit);
      const fc = (m: Map<string, number>) => [...m].sort((a, b) => cmp(a[0], b[0])).map(([id, count]) => ({ id, count }));
      return { total, all: STRING_COUNT, offset: req.offset, rows: clone(hits), sources: fc(srcCounts), categories: fc(catCounts) };
    },
    string_detail: ({ req }) => detail(req.target, req.key),
    set_override: ({ req }) => {
      const d = detail(req.target, req.key);
      const bad = checkOverride(d.row.original, req.value);
      if (bad) return { accepted: false, rule: bad[0], message: bad[1], detail: d };
      const c = chan(req.target);
      c.overrides.set(d.row.key, req.value);
      if (c.summary.status.state === 'applied') c.summary.status = { state: 'stale', reasons: ['overrides'] };
      return { accepted: true, rule: null, message: null, detail: detail(req.target, d.row.key) };
    },
    reset_override: ({ req }) => {
      const d = detail(req.target, req.key);
      chan(req.target).overrides.delete(d.row.key);
      return detail(req.target, d.row.key);
    },
    list_patches: ({ req }) => clone(chan(req).patches),
    set_patch_enabled: ({ req }) => {
      const c = chan(req.target);
      const p = c.patches.find((x) => x.id === req.id) ?? fail('not-found', `No patch ${req.id}`);
      p.enabled = req.enabled;
      c.profile.patches_disabled = c.patches.filter((x) => !x.enabled).map((x) => x.id);
      return clone(p);
    },
    activity: ({ limit }) => clone(log.slice(-limit).reverse()),
    get_settings: () => clone(settings),
    save_settings: ({ req }) => {
      settings = clone(req);
      if (!settings.network.starorg) Object.assign(starorg, { connected: false, user: null, guild: null });
      return clone(settings);
    },
    list_characters: ({ req }) => charactersView(chan(req)),
    set_active_character: ({ req }) => {
      const c = chan(req.target);
      if (req.mode === 'manual' && !SAMPLES.list_characters.characters.some((x) => x.id === req.character)) fail('invalid', 'Pick one of the characters found in the logs.');
      c.active = { ...c.active, mode: req.mode, character: req.mode === 'manual' ? req.character : null };
      return charactersView(c);
    },
    owned_blueprints: ({ req }) => clone(chan(req.target).owned.get(req.character) ?? []),
    import_smart_citizen: ({ req }) => {
      const c = chan(req.target);
      const set = c.owned.get(req.character) ?? [];
      const cands = ['Synthetic Rifle Mk1', 'Sample Armor Legs', 'Sample Armor Arms', 'Test Knife'];
      const known = cands.filter((n) => set.some((b) => b.name === n));
      const imported = cands.filter((n) => !known.includes(n));
      if (!req.dry_run) {
        for (const n of imported) set.push({ name: n, ambiguous: false, favorite: false, sources: ['import:smart-citizen'], acquired: '2026-11-01T10:00:00Z' });
        c.owned.set(req.character, set);
      }
      return { ...clone(SAMPLES.import_smart_citizen), dry_run: req.dry_run, imported, known };
    },
    starorg_status: ({ channel }) => starorgView(channel),
    starorg_login: ({ channel }) => {
      if (!settings.network.starorg) fail('network-off', 'Turn on "Connect StarOrg Tools" under Settings -> Network first.');
      Object.assign(starorg, { connected: true, user: 'pilot_a_discord', guild: 'Synthetic Org', expires: '2027-02-12T21:30:05Z' });
      return starorgView(channel);
    },
    starorg_logout: ({ channel }) => {
      Object.assign(starorg, { connected: false, user: null, guild: null, expires: null });
      return starorgView(channel);
    },
    set_sync_characters: ({ req }) => {
      starorgView(req.channel);
      for (const c of starorg.sync_characters) c.ticked = req.ticked.includes(c.id);
      if (!req.ticked.length) return { ...clone(starorg), push_count: 0 };
      return starorgView(req.channel);
    },
    hooks_status: () => clone(hooks),
    hooks_install: () => {
      if (!hooks.supported) fail('invalid', 'Launch hooks are for the LUG Helper on Linux.');
      hooks = { ...hooks, scripts: hooks.scripts.map((s) => ({ ...s, path: s.path ?? `/home/pilot/Games/star-citizen/sc-${s.kind}.sh`, status: { state: 'current' } })) };
      return clone(hooks);
    },
    hooks_uninstall: () => {
      hooks = { ...hooks, scripts: hooks.scripts.map((s) => ({ ...s, status: { state: 'absent' } })) };
      return clone(hooks);
    },
    doctor: () => clone(SAMPLES.doctor),
  };

  if (startReady) {
    const inst = installs[0]!;
    h.setup({
      req: {
        install: inst.id,
        channels: inst.channels.map((c) => ({ channel: c.channel, consent: c.language === 'english', switch_language: false })),
        data_source: 'local',
        preset: 'recommended',
        fix_known_bugs: true,
      },
    });
  }

  return {
    kind: 'mock',
    async call<K extends Command>(cmd: K, args: Args<K>): Promise<Result<K>> {
      // Answer asynchronously like IPC does, so the UI's loading states are exercised.
      await sleep(0);
      const fn = h[cmd] as Handler<K>;
      return fn(args);
    },
    async onJob(cb) {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
  };
}

/**
 * In-browser mock transport: the same behaviour as the Rust MockBackend
 * (app/src-tauri/src/contract/mock.rs), for `npm run dev` and headless renders without Tauri.
 * Seeded from samples.gen.json (real Rust mock output), so the shapes are the contract's.
 * All data is synthetic: no real character names, geids or CIG text.
 *
 * URL switches: `?delay=<ms>` per job step (default 250), `?state=ready` to start with LIVE
 * applied and HOTFIX set up but behind a game update (skips the wizard), with some backups,
 * overrides and hook history; add `&starorg=1` to start signed in to StarOrg Tools.
 * States for renders (channel names): `&running=LIVE` (game running), `&interrupted=LIVE`,
 * `&foreign=LIVE` (another tool's file in place), `&builderr=LIVE`, `&channels=5`,
 * `&installs=0` (nothing found), `&fail=overview`, `&log=empty`, `&nobackups=LIVE`,
 * `&accent=<accent>`, `&palette=black`. Updates: `&update=available|failed|skipped`
 * (mock-update.ts).
 */
import type { Args, Command, Result, Transport } from './client';
import { ContractError } from './client';
import samples from './samples.gen.json';
import type * as T from './types';
import { createUpdateMock } from './mock-update';

const clone = <X>(x: X): X => structuredClone(x);
const SAMPLES = samples as unknown as {
  overview_first_run: T.Overview;
  profile_schema: T.ProfileSchema;
  list_patches: T.PatchView[];
  get_settings: T.Settings;
  list_characters: T.CharactersView;
  owned_blueprints: T.OwnedBlueprint[];
  hooks_status: T.HooksStatus;
  doctor: T.DoctorReport;
  starorg_status: T.StarOrgStatus;
  import_smart_citizen: T.ImportReport;
  query_strings: T.StringsPage;
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
  forgotten: Set<string>;
  /** LIVE has run the loc-reference pass; other channels start approximate. */
  exactGroups?: boolean;
}

/** Synthetic manufacturers, as in the Rust mock (`MOCK_MANUFACTURERS`). */
const MOCK_MANUFACTURERS: [string, string][] = [
  ['SYNA', 'Alpha Synthetics'],
  ['SYNB', 'Beta Works'],
  ['SYNC', 'Gamma Test Yards'],
  ['SYND', 'Delta Sample Co'],
];

/**
 * Group, manufacturer and size of synthetic row `i`: the table of `mock_group` in the Rust
 * mock. A Rust test checks these pins:
 * // pin: components 16 -> components/shields SYNC 1
 * // pin: items 40 -> armor/helmets SYNB -
 * // pin: ui 24 -> dialogue/voice - -
 * // pin: ship-weapons 0 -> ship-weapons/countermeasures SYNA 1
 */
export function mockGroup(cat: string, i: number): [string, string | null, number | null] {
  const k = Math.floor(i / 8);
  const mfr = MOCK_MANUFACTURERS[k % MOCK_MANUFACTURERS.length]![0];
  const pick = (v: string[]) => v[k % v.length]!;
  switch (cat) {
    case 'ships':
      return [pick(['ships/ships', 'ships/ships', 'ships/ground', 'ships/paints']), mfr, null];
    case 'components':
      return [pick(['components/coolers', 'components/power-plants', 'components/shields', 'components/quantum-drives', 'components/radars', 'components/thrusters']), mfr, (i % 4) + 1];
    case 'ship-weapons':
      return [k % 5 === 0 ? 'ship-weapons/countermeasures' : 'ship-weapons/guns', mfr, (i % 6) + 1];
    case 'missiles':
      return ['missiles', mfr, (i % 3) + 1];
    case 'fps-weapons':
      return [pick(['fps-weapons/weapons', 'fps-weapons/weapons', 'fps-weapons/attachments', 'fps-weapons/gadgets']), mfr, null];
    case 'missions':
      return [pick(['missions/contracts', 'missions/text', 'missions/givers']), null, null];
    case 'items':
      return [pick(['armor/helmets', 'armor/torso', 'clothing/torso', 'consumables/food', 'items/misc']), k % 5 < 3 ? mfr : null, null];
    default:
      return [pick(['interface/hud', 'interface/menus', 'interface/mobiglas', 'dialogue/voice', 'other']), null, null];
  }
}

/** The type hierarchy (ids, parents, labels) as the Rust mock wrote it into the samples. */
const HIERARCHY = () => SAMPLES.query_strings.groups.map((g) => ({ id: g.id, parent: g.parent, label: g.label }));
const groupLabel = (() => {
  let m: Map<string, string> | null = null;
  return (id: string) => (m ??= new Map(HIERARCHY().map((g) => [g.id, g.label]))).get(id) ?? 'Other';
})();

const fail = (code: T.ErrorCode, message: string, detail: string | null = null): never => {
  throw new ContractError({ code, message, detail });
};

const tagKind = (elements: string[], styles: Record<string, T.TagStyle> = {}): T.TagKindSettings => ({
  enabled: true,
  elements,
  styles,
  separator: 'dash',
  enclosing: 'square',
  placement: 'prefix',
});

/** Same values as `mock::preset_profile` in Rust (= M2's presets). */
function presetProfile(p: T.Preset): T.Profile {
  const cats = p !== 'minimal';
  const all = p === 'everything';
  return {
    preset: p,
    ships: cats,
    components: cats,
    ship_weapons: cats,
    fps_weapons: cats,
    missiles: cats,
    missions: cats,
    lines_off: [],
    stats_above: false,
    header: '--- STATS ---',
    tags: {
      components: tagKind(['class', 'size', 'grade']),
      ship_weapons: tagKind(['damage', 'size'], { damage: 'short' }),
      missiles: tagKind(['tracking', 'size']),
      short_names: all,
      tool_size_tag: all,
    },
    mission: {
      title_blueprint: true,
      title_reputation: true,
      title_em: true,
      blueprints: true,
      item_rewards: true,
      details: true,
      bullet_tags: true,
      owned_marker: true,
      max_list: 25,
      header_em: 3,
      label_em: 4,
    },
    branding: all,
    patches_disabled: [],
  };
}

function counts(p: T.Profile, overrides: number): T.ApplyCounts {
  const n = (on: boolean, v: number) => (on ? v : 0);
  const c: T.ApplyCounts = {
    ships: n(p.ships, 231),
    components: n(p.components, 1840),
    ship_weapons: n(p.ship_weapons, 402),
    fps_weapons: n(p.fps_weapons, 318),
    missiles: n(p.missiles, 96),
    missions: n(p.missions, 2140),
    overrides,
    dropped: 0,
    changed: 0,
  };
  c.changed = (c.ships + c.components + c.ship_weapons + c.fps_weapons + c.missiles + c.missions) * 2 + overrides + 1200;
  return c;
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
    const [group, manufacturer, size] = mockGroup(cat, i);
    rows[i] = { key, original, result, source, category: cat, group, group_label: groupLabel(group), manufacturer, size };
  }
  return rows;
}

/** Same rules and M1 rule names as `mock::check_override` in Rust. */
function checkOverride(original: string, value: string): [T.OverrideRule, string] | null {
  // eslint-disable-next-line no-control-regex
  if (/[\u0000-\u001f\u007f]/.test(value)) return ['control-character', 'A line break must be written as the two characters \\n.'];
  if ((value.match(/<EM/g) ?? []).length !== (value.match(/<\/EM/g) ?? []).length) return ['added-em-unbalanced', 'Every <EMn> tag needs its closing </EMn>.'];
  const vars = (s: string) => (s.match(/~/g) ?? []).length;
  if (vars(original) !== vars(value)) return ['variable-tokens', "The value must keep the original's ~variable(...) tokens."];
  const spec = (s: string) => (s.match(/%[sd]/g) ?? []).length;
  if (spec(original) !== spec(value)) return ['format-specifiers', "The value must keep the original's %s / %d placeholders."];
  if (/% [A-Za-z]/.test(value)) return ['percent-rule', 'A % must be followed by the end, \\n or two spaces.'];
  if (value.length > 4000) return ['too-long', 'The value is longer than the game shows (4,000 characters).'];
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
    recovered: null,
    owned: null,
    duration_ms: 0,
  };
}

/** Preview samples from a profile: same rules as `mock::preview` in Rust. */
export function preview(p: T.Profile): T.PreviewItem[] {
  const lineOn = (key: string) => !p.lines_off.includes(key);
  const block = (prose: string, lines: string[]) => {
    if (!lines.length) return prose;
    const stats = `${p.header}\\n${lines.join('\\n')}`;
    return p.stats_above ? `${stats}\\n\\n------\\n\\n${prose}` : `${prose}\\n\\n${stats}`;
  };
  const shipProse = 'A synthetic light fighter used for testing.';
  const lines: string[] = [];
  if (p.ships) {
    const all: [string, string][] = [
      ['ships.role', 'Role: Light Fighter  |  Career: Combat  |  Crew: 1'],
      ['ships.size', 'Size: 21 × 16.5 × 5.5 m'],
      ['ships.speed', 'SCM: 226 m/s  |  Max: 1,193 m/s'],
      ['ships.boost', 'Boost: 520 m/s fwd  |  268 m/s back'],
      ['ships.cargo', 'Cargo: 0 SCU'],
    ];
    for (const [k, l] of all) if (lineOn(k)) lines.push(l);
  }
  const name = 'Sample Cooler';
  let tagged = name;
  const t = p.tags.components;
  if (t.enabled) {
    const word: Record<string, Record<T.TagStyle, string>> = {
      class: { short: 'IND', medium: 'Industrial', long: 'Industrial' },
      size: { short: 'S2', medium: 'S2', long: 'Size 2' },
      grade: { short: 'A', medium: 'A', long: 'Grade A' },
    };
    const parts = t.elements.map((e) => word[e]?.[t.styles[e] ?? 'medium'] ?? '').filter(Boolean);
    const sep = { dash: '-', none: '', space: ' ', underscore: '_', dot: '.', slash: '/', pipe: '|' }[t.separator];
    const [l, r] = { square: ['[', ']'], round: ['(', ')'], curly: ['{', '}'], angle: ['<', '>'], none: ['', ''] }[t.enclosing];
    if (parts.length) {
      const tag = `${l}${parts.join(sep)}${r}`;
      tagged = t.placement === 'prefix' ? `${tag} ${name}` : `${name} ${tag}`;
    }
  }
  const title = 'Synthetic Recovery Contract';
  let tt = title;
  if (p.missions) {
    const wrap = (s: string) => (p.mission.title_em ? ` <EM4>${s}</EM4>` : ` ${s}`);
    if (p.mission.title_blueprint) tt += wrap('[BP]');
    if (p.mission.title_reputation) tt += wrap('[150 Rep]');
  }
  return [
    { key: 'vehicle_DescSYN_Fighter', label: 'Ship description', category: 'ships', original: shipProse, result: block(shipProse, lines) },
    { key: 'item_NameSYN_Cooler_S2', label: 'Component name', category: 'components', original: name, result: tagged },
    { key: 'mission_TitleSYN_Recovery', label: 'Mission title', category: 'missions', original: title, result: tt },
  ];
}

const CHARACTER_IDS = ['1000000000001', '1000000000002', '1000000000003'];
const CHARACTER_NAMES = ['Pilot_A', 'Alt_B', 'Alt_C'];

/** Same replacements as `mock::redact` in Rust. */
export function redact(text: string): [string, number] {
  let out = text;
  let n = 0;
  const swap = (from: string, to: string) => {
    const c = out.split(from).length - 1;
    if (c) {
      out = out.split(from).join(to);
      n += c;
    }
  };
  swap('/home/pilot', '~');
  swap('users/pilot', 'users/<user>');
  CHARACTER_IDS.forEach((id, i) => {
    swap(id, '<geid>');
    swap(CHARACTER_NAMES[i]!, '<character>');
  });
  swap('pilot_a_discord', '<starorg-user>');
  return [out, n];
}

/** Same data as `mock::patch_detail` in Rust. */
function patchDetail(p: T.PatchView): T.PatchDetail {
  const ex = (key: string, before: string, after: string): T.PatchExample => ({ key, before, after });
  const by: Record<string, [string | null, string, T.PatchExample[]]> = {
    'syn-typo-cargo': [
      '4.9.0 - 4.10.x',
      'The stock text still has the typo in this build, so the patch applies.',
      [ex('vehicle_DescSYN_Ship_00194', 'Cargo grid: 12 SUC.', 'Cargo grid: 12 SCU.'), ex('vehicle_DescSYN_Ship_00388', 'Holds 8 SUC of cargo.', 'Holds 8 SCU of cargo.')],
    ],
    'syn-missing-rep': [null, 'The reputation line is still missing, so the patch adds it.', [ex('mission_DescSYN_Contract_00045', 'Reward: 12,500 aUEC', 'Reward: 12,500 aUEC\\nReputation: +150')]],
    'syn-old-name': ['up to 4.9.x', 'Not needed: the game text already reads the way this patch would make it.', [ex('item_NameSYN_Cooler_S1_00001', 'Sample Cooler 1', 'Sample Cooler 1')]],
  };
  const [range, reason, examples] = by[p.id] ?? [
    '4.10.0 - 4.10.x',
    'Target changed: the stock text is no longer what the patch expects, so it is skipped and reported.',
    [ex('item_DescSYN_Rifle_00004', 'A synthetic rifle, number 4 (new wording).', 'A synthetic rifle, number 4.')],
  ];
  return { patch: clone(p), kind: p.id === 'syn-missing-rep' ? 'data' : 'loc', range, reason, examples };
}

type Handler<K extends Command> = (args: Args<K>) => Result<K> | Promise<Result<K>>;
type Handlers = { [K in Command]: Handler<K> };

export function createMockTransport(opts: { delay?: number; ready?: boolean } = {}): Transport {
  const params = typeof window !== 'undefined' ? new URLSearchParams(window.location.search) : new URLSearchParams();
  const delay = opts.delay ?? (params.has('delay') ? Number(params.getAll('delay').at(-1)) : 250);
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
        patches: clone(SAMPLES.list_patches.filter((x) => x.origin === 'built-in')),
        active: { mode: 'auto', character: null, resolved: chars[0]!.id },
        owned,
        forgotten: new Set(),
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
  const cancelled = new Set<string>();
  const patchFiles: [T.PatchView, string][] = [];
  const builtinIds = SAMPLES.list_patches.filter((x) => x.origin === 'built-in').map((x) => x.id);
  const setupProfile = (req: T.SetupRequest): T.Profile => ({ ...presetProfile(req.preset), patches_disabled: req.fix_known_bugs ? [] : [...builtinIds] });

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
    const also_used_by = row.group.startsWith('components/') && row.size === 1 ? ['HUD'] : [];
    return { row, provenance, override_value: ov, also_used_by };
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
  const mkBackup = (seq: number, reason: T.BackupReason, build: T.Build, origin: T.FileOrigin, tool: string | null, created = NOW): T.BackupView => ({
    id: `${created.slice(0, 10).replace(/-/g, '')}-${created.slice(11, 19).replace(/:/g, '')}${String(seq).padStart(2, '0')}-${reason}`,
    created,
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
    characters: SAMPLES.list_characters.characters.filter((x) => !c.forgotten.has(x.id)).map((x) => ({ ...x, owned: c.owned.get(x.id)?.length ?? 0 })),
    active: clone(c.active),
  });
  const starorgGuild = (): string => {
    if (!settings.network.starorg) fail('network-off', 'Turn on "Connect StarOrg Tools" under Settings -> Network first.');
    if (!starorg.connected || !starorg.guild) fail('needs-decision', 'Sign in to StarOrg Tools first (Settings -> StarOrg Tools).');
    return starorg.guild!;
  };
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
    plan_setup: async ({ req }) => {
      await sleep(Math.min(delay, 300));
      const inst = installs.find((i) => i.id === req.install) ?? fail('not-found', `No install ${req.install}`);
      const warnings: T.Warning[] = [];
      const channels: T.PlannedChannel[] = [];
      const p = setupProfile(req);
      for (const c of req.channels.filter((x) => x.consent)) {
        const s = chan({ install: req.install, channel: c.channel }).summary;
        if (s.language === 'other' && !c.switch_language) {
          warnings.push({ kind: 'language', detail: `${c.channel}: the game language is not English; it will be skipped` });
          continue;
        }
        channels.push({
          channel: c.channel,
          target: `${inst.root}/${c.channel}/Data/Localization/english/global.ini`,
          target_is_link: false,
          link_target: null,
          backup_dir: `~/.local/share/starorg-strings/channels/${req.install}/${c.channel}/backups`,
          user_cfg_line: s.language === 'other' ? 'g_language = english' : null,
          user_cfg_path: `${inst.root}/${c.channel}/USER.cfg`,
          replaces_tool: s.foreign_tool,
          counts: { ...counts(p, 0), changed: counts(p, 0).changed - 37 * c.channel.length },
        });
      }
      if (req.data_source === 'published-dumps') {
        warnings.push({ kind: 'network', detail: 'Published game data is downloaded only when a dump exists for the exact build; otherwise it is read locally.' });
      }
      return { install: req.install, channels, warnings };
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
        // Setup again keeps a set-up channel's own settings.
        if (!ch.summary.set_up) ch.profile = setupProfile(req);
        ch.summary.set_up = true;
        set_up.push(c.channel);
      }
      for (const i of installs) i.saved = i.id === req.install;
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
      if (c.summary.game_running) {
        addLog('warn', 'apply', name, 'Apply skipped: the game is running');
        return { ...emptyReport(c.summary, 'skipped'), reason: `Star Citizen is running on ${name}. Close the game, then apply.` };
      }
      const job = `job-${++nextJob}`;
      const steps = ['Checking the game is not running', 'Reading game data', 'Building text', 'Validating', 'Backing up and writing'];
      for (let i = 0; i < steps.length; i++) {
        if (cancelled.delete(job)) {
          listeners.forEach((l) => l({ job, kind: 'apply', channel: name, phase: 'cancelled', step: i, steps: steps.length, label: 'Stopped', cancellable: false }));
          addLog('info', 'apply', name, 'Apply stopped before writing');
          fail('cancelled', 'Stopped. Nothing was written to the game.');
        }
        const last = req.dry_run && i === steps.length - 1;
        const p: T.JobProgress = { job, kind: 'apply', channel: name, phase: 'running', step: i + 1, steps: steps.length, label: last ? 'Dry run: nothing written' : steps[i]!, cancellable: i < 3 };
        listeners.forEach((l) => l(p));
        if (delay > 0) await sleep(delay);
      }
      const r = emptyReport(c.summary, req.dry_run ? 'dry-run' : 'applied');
      r.counts = counts(c.profile, c.overrides.size);
      const k = r.counts;
      if (c.profile.missions && c.profile.mission.owned_marker) {
        const who = c.active.resolved ?? '';
        r.owned = { mode: 'auto', characters: [who], count: c.owned.get(who)?.length ?? 0, ambiguous: 0 };
      }
      if (c.summary.interrupted_apply) {
        r.recovered = 'An earlier apply was interrupted; the file from before it was put back first.';
        if (!req.dry_run) c.summary.interrupted_apply = false;
      }
      r.patches = c.patches.map((x) => ({ id: x.id, state: x.enabled && !c.profile.patches_disabled.includes(x.id) ? x.state : 'disabled' }));
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
        c.summary.applied_build = clone(c.summary.build);
        c.summary.last_apply = { time: NOW, result: r.result, changed: k.changed, warnings: r.warnings.length, dropped: k.dropped, duration_ms: r.duration_ms };
      }
      const done: T.JobProgress = { job, kind: 'apply', channel: name, phase: 'done', step: steps.length, steps: steps.length, label: req.dry_run ? 'Dry run finished' : `Applied to ${name}`, cancellable: false };
      listeners.forEach((l) => l(done));
      addLog(r.warnings.length ? 'warn' : 'info', 'apply', name, `${req.dry_run ? 'Dry run' : 'Applied'}: ${k.changed} strings changed, ${r.warnings.length} warning(s)`);
      sync();
      return r;
    },
    remove_changes: ({ req: { target: req, allow_foreign } }) => {
      const c = chan(req);
      if (c.summary.status.state === 'not-applied') return emptyReport(c.summary, 'nothing-to-do');
      if (c.summary.game_running) return { ...emptyReport(c.summary, 'skipped'), reason: `Star Citizen is running on ${req.channel}. Close the game, then remove.` };
      if (c.summary.foreign_file && !allow_foreign) {
        return { ...emptyReport(c.summary, 'needs-decision'), reason: `The file in place belongs to ${c.summary.foreign_tool ?? 'another tool'}. Confirm to remove it (a backup is kept).` };
      }
      if (c.summary.foreign_file) c.backups.push(mkBackup(c.backups.length + 1, 'before-restore', c.summary.build, 'foreign', c.summary.foreign_tool));
      c.summary.status = { state: 'not-applied' };
      c.summary.foreign_file = false;
      c.summary.applied_build = null;
      addLog('info', 'remove', req.channel, 'Removed StarOrg changes (back to stock)');
      return emptyReport(c.summary, 'removed');
    },
    undo_last_apply: ({ req }) => {
      const c = chan(req);
      if (c.summary.game_running) return { ...emptyReport(c.summary, 'skipped'), reason: `Star Citizen is running on ${req.channel}. Close the game first.` };
      const undo = offers(c).undo_last_apply ?? fail('needs-decision', 'No backup of an earlier apply for this build. Re-apply instead.');
      c.backups = c.backups.filter((b) => b.id !== undo);
      c.summary.status = { state: 'stale', reasons: ['profile'] };
      return { ...emptyReport(c.summary, 'restored'), backup: undo };
    },
    restore_backup: ({ req }) => {
      const c = chan(req.target);
      const b = c.backups.find((x) => x.id === req.backup) ?? fail('not-found', `No backup ${req.backup}`);
      if (c.summary.game_running) return { ...emptyReport(c.summary, 'skipped'), reason: `Star Citizen is running on ${req.target.channel}. Close the game first.` };
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
      return { channel: req.channel, folder: `~/.local/share/starorg-strings/channels/${req.install}/${req.channel}/backups`, backups: clone([...c.backups].reverse()), offers: offers(c), keep_newest: 10, keep_builds: 3 };
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
    cancel_job: ({ job }) => {
      cancelled.add(job);
      return null;
    },
    build_preview: ({ req }) => preview(req.profile ?? (req.preset ? presetProfile(req.preset) : chan(req.target).profile)),
    profile_schema: () => clone(SAMPLES.profile_schema),
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
      // Same rules as the Rust side (`contract/strings_query.rs`).
      const c = chan(req.target);
      const needle = req.search.toLowerCase();
      const groups = req.groups ?? [];
      const mfrs = req.manufacturers ?? [];
      const sizes = req.sizes ?? [];
      const by: T.GroupBy = req.group_by ?? 'none';
      const srcCounts = new Map<string, number>();
      const catCounts = new Map<string, number>();
      const tree = new Map<string, { count: number; enhanced: number; edited: number }>();
      const sizesSeen = new Set<number>();
      let hits: T.StringRow[] = [];
      const inGroups = (g: string) => !groups.length || groups.some((s) => g === s || g.startsWith(`${s}/`));
      for (const base of table()) {
        if (base.size !== null) sizesSeen.add(base.size);
        const ov = c.overrides.get(base.key);
        const r = ov === undefined ? base : { ...base, result: ov, source: 'edited' as const };
        if (req.changed_only && r.source === 'unmodified') continue;
        if (needle && !r.key.toLowerCase().includes(needle) && !r.original.toLowerCase().includes(needle) && !r.result.toLowerCase().includes(needle)) continue;
        const srcOk = !req.sources.length || req.sources.includes(r.source);
        const catOk = !req.categories.length || req.categories.includes(r.category);
        const typeOk = inGroups(r.group);
        const mfrOk = !mfrs.length || mfrs.some((m) => m === r.manufacturer || (m === 'none' && r.manufacturer === null));
        const sizeOk = !sizes.length || sizes.includes(r.size ?? 0);
        const groupsOk = typeOk && mfrOk && sizeOk;
        if (catOk && groupsOk) srcCounts.set(r.source, (srcCounts.get(r.source) ?? 0) + 1);
        if (srcOk && groupsOk) catCounts.set(r.category, (catCounts.get(r.category) ?? 0) + 1);
        const ownOk = by === 'type' ? mfrOk && sizeOk : by === 'manufacturer' ? typeOk && sizeOk : by === 'size' ? typeOk && mfrOk : false;
        if (srcOk && catOk && ownOk) {
          const id = by === 'manufacturer' ? (r.manufacturer ?? 'none') : by === 'size' ? (r.size === null ? 'none' : String(r.size)) : r.group;
          const t = tree.get(id) ?? { count: 0, enhanced: 0, edited: 0 };
          t.count++;
          if (r.source === 'enhanced') t.enhanced++;
          if (r.source === 'edited') t.edited++;
          tree.set(id, t);
        }
        if (srcOk && catOk && groupsOk) hits.push(r);
      }
      const cmp = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);
      const hier = HIERARCHY();
      const pos = new Map(hier.map((g, i) => [g.id, i]));
      const mfrPos = new Map(MOCK_MANUFACTURERS.map(([code], i) => [code, i]));
      const rank = (r: T.StringRow) =>
        by === 'manufacturer' ? (r.manufacturer === null ? Infinity : (mfrPos.get(r.manufacturer) ?? Infinity)) : by === 'size' ? (r.size ?? Infinity) : (pos.get(r.group) ?? Infinity);
      if (req.sort === 'key') hits.sort((a, b) => cmp(a.key, b.key));
      else if (req.sort === 'key-desc') hits.sort((a, b) => cmp(b.key, a.key));
      else if (req.sort === 'category') hits.sort((a, b) => cmp(a.category, b.category) || cmp(a.key, b.key));
      else if (req.sort === 'group') hits.sort((a, b) => rank(a) - rank(b) || cmp(a.key, b.key));
      else hits.sort((a, b) => cmp(a.source, b.source) || cmp(a.key, b.key));
      const total = hits.length;
      const limit = Math.min(Math.max(req.limit, 1), 1000);
      hits = hits.slice(req.offset, req.offset + limit);
      const fc = (m: Map<string, number>) => [...m].sort((a, b) => cmp(a[0], b[0])).map(([id, count]) => ({ id, count }));
      const zero = { count: 0, enhanced: 0, edited: 0 };
      let nodes: T.GroupNode[] = [];
      if (by === 'type') {
        nodes = hier.map((g) => {
          const kids = hier.filter((x) => x.parent === g.id);
          const t = kids.length
            ? kids.reduce((a, k) => {
                const v = tree.get(k.id) ?? zero;
                return { count: a.count + v.count, enhanced: a.enhanced + v.enhanced, edited: a.edited + v.edited };
              }, zero)
            : (tree.get(g.id) ?? zero);
          return { ...g, ...t };
        });
      } else if (by === 'manufacturer') {
        nodes = [...MOCK_MANUFACTURERS.map(([id, label]) => ({ id, parent: null, label, ...(tree.get(id) ?? zero) })), { id: 'none', parent: null, label: 'No manufacturer', ...(tree.get('none') ?? zero) }];
      } else if (by === 'size') {
        nodes = [...[...sizesSeen].sort((a, b) => a - b).map((s) => ({ id: String(s), parent: null, label: `Size ${s}`, ...(tree.get(String(s)) ?? zero) })), { id: 'none', parent: null, label: 'No size', ...(tree.get('none') ?? zero) }];
      }
      const group_state: T.GroupState = (c.exactGroups ?? c.summary.channel === 'LIVE') ? 'exact' : 'approximate';
      return { total, all: STRING_COUNT, offset: req.offset, rows: clone(hits), sources: fc(srcCounts), categories: fc(catCounts), groups: nodes, group_state };
    },
    read_string_groups: async ({ req }): Promise<T.GroupState> => {
      const c = chan(req);
      await sleep(groupsRead === 'slow' ? 20_000 : 900);
      if (groupsRead === 'busy') fail('busy', 'another extraction is running');
      c.exactGroups = true;
      return 'exact';
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
    list_overrides: ({ req }) => [...chan(req).overrides].map(([key, value]) => ({ key, value, original: table().find((r) => r.key === key)?.original ?? '' })),
    list_patches: ({ req }) => clone(chan(req).patches),
    set_patch_enabled: ({ req }) => {
      const c = chan(req.target);
      const p = c.patches.find((x) => x.id === req.id) ?? fail('not-found', `No patch ${req.id}`);
      p.enabled = req.enabled;
      c.profile.patches_disabled = c.patches.filter((x) => !x.enabled).map((x) => x.id);
      return clone(p);
    },
    add_user_patch: ({ req }) => {
      if (!req.title.trim() || !req.key.trim()) fail('invalid', 'A patch needs a title and a key.');
      if (req.op === 'replace' && !req.find) fail('invalid', 'Replace needs the text to find.');
      const stock = table().find((r) => r.key.toLowerCase() === req.key.trim().toLowerCase());
      const state: T.PatchState = !stock
        ? 'missing'
        : req.op === 'replace' && stock.original.split(req.find ?? '').length - 1 !== 1
          ? 'target-changed'
          : req.op === 'set' && stock.original === req.value
            ? 'not-needed'
            : 'applies';
      const slug = req.title.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '');
      const view: T.PatchView = { id: `user-${slug}`, title: req.title.trim(), description: req.description.trim(), keys: 1, origin: 'user', state, enabled: true };
      const c = chan(req.target);
      if (c.patches.some((x) => x.id === view.id)) fail('invalid', `A patch with the id ${view.id} exists. Pick another title.`);
      const q = (x: string) => JSON.stringify(x);
      let text = `schema = 1\nid = ${q(view.id)}\ntitle = ${q(view.title)}\ndescription = ${q(view.description)}\n\n[[loc]]\nkey = ${q(req.key)}\nop = ${q(req.op)}\n`;
      if (req.find) text += `find = ${q(req.find)}\n`;
      text += `value = ${q(req.value)}\n`;
      c.patches.push(view);
      const at = patchFiles.findIndex(([x]) => x.id === view.id);
      if (at >= 0) patchFiles.splice(at, 1);
      patchFiles.push([clone(view), text]);
      addLog('info', null, req.target.channel, `Added patch ${view.title}`);
      return clone(view);
    },
    import_user_patch: ({ req }) => {
      const c = chan(req);
      const have = new Set(c.patches.map((x) => x.id));
      const found = [...patchFiles].reverse().find(([x]) => !have.has(x.id))?.[0];
      const pick: T.PatchView = found ?? { id: 'user-synthetic-sample', title: 'Synthetic sample patch', description: 'Imported from a .patch.toml file.', keys: 1, origin: 'user', state: 'applies', enabled: true };
      if (have.has(pick.id)) fail('invalid', `${pick.title} is already in the list.`);
      c.patches.push(clone(pick));
      return clone(pick);
    },
    export_user_patch: ({ req }) => {
      const c = chan(req.target);
      const p = c.patches.find((x) => x.id === req.id) ?? fail('not-found', `No patch ${req.id}`);
      let f = patchFiles.find(([x]) => x.id === p.id);
      if (!f) {
        f = [clone(p), `schema = 1\nid = ${JSON.stringify(p.id)}\ntitle = ${JSON.stringify(p.title)}\ndescription = ${JSON.stringify(p.description)}\n`];
        patchFiles.push(f);
      }
      return { file_name: `${p.id}.patch.toml`, text: f[1] };
    },
    delete_user_patch: ({ req }) => {
      const c = chan(req.target);
      const p = c.patches.find((x) => x.id === req.id) ?? fail('not-found', `No patch ${req.id}`);
      if (p.origin === 'built-in') fail('invalid', 'Built-in patches can be switched off, not deleted.');
      c.patches = c.patches.filter((x) => x.id !== req.id);
      c.profile.patches_disabled = c.profile.patches_disabled.filter((x) => x !== req.id);
      if (!patchFiles.some(([x]) => x.id === p.id)) patchFiles.push([clone(p), '']);
      addLog('info', null, req.target.channel, `Deleted patch ${p.title}`);
      return clone(c.patches);
    },
    patch_detail: ({ req }) => patchDetail(chan(req.target).patches.find((x) => x.id === req.id) ?? fail('not-found', `No patch ${req.id}`)),
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
    forget_character: ({ req }) => {
      const c = chan(req.target);
      if (!CHARACTER_IDS.includes(req.character)) fail('not-found', 'No such character in the logs.');
      c.owned.delete(req.character);
      c.forgotten.add(req.character);
      if (c.active.character === req.character || c.active.resolved === req.character) {
        c.active = { mode: 'auto', character: null, resolved: CHARACTER_IDS.find((x) => !c.forgotten.has(x)) ?? null };
      }
      return charactersView(c);
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
    starorg_pull: ({ req }) => {
      const guild = starorgGuild();
      if (!CHARACTER_IDS.includes(req.character)) fail('invalid', 'Pick one of the characters found in the logs.');
      const c = chan({ install: installs[0]!.id, channel: req.channel });
      const set = c.owned.get(req.character) ?? [];
      const added: string[] = [];
      let already = 0;
      for (const n of ['Synthetic Rifle Mk1', 'Sample Medical Gun', 'Test Multitool']) {
        if (set.some((b) => b.name === n)) already++;
        else {
          added.push(n);
          set.push({ name: n, ambiguous: false, favorite: false, sources: [`starorg:${guild}`], acquired: NOW });
        }
      }
      c.owned.set(req.character, set);
      starorg.last_sync = NOW;
      addLog('info', 'sync', req.channel, `Pulled from StarOrg (${guild}): ${added.length} added, ${already} already known`);
      return { direction: 'pull', channel: req.channel, guild, character: req.character, added, already, unmatched: ['Retired Sample Visor'], sent: 0, time: NOW };
    },
    starorg_push: ({ channel }) => {
      const guild = starorgGuild();
      const sent = starorgView(channel).push_count;
      starorg.last_sync = NOW;
      addLog('info', 'sync', channel, `Pushed ${sent} owned blueprints to StarOrg (${guild})`);
      return { direction: 'push', channel, guild, character: null, added: [], already: 0, unmatched: [], sent, time: NOW };
    },
    hooks_status: () => clone(hooks),
    hooks_install: () => {
      if (!hooks.supported) fail('invalid', 'Launch hooks are for the LUG Helper on Linux.');
      hooks = { ...hooks, scripts: hooks.scripts.map((s) => ({ ...s, path: s.path ?? `/home/pilot/Games/star-citizen/sc-${s.kind}.sh`, status: { state: 'current' } })) };
      addLog('info', 'hook', null, 'Installed the launch hook block in sc-prelaunch.sh and sc-postexit.sh');
      return clone(hooks);
    },
    hooks_uninstall: () => {
      hooks = { ...hooks, scripts: hooks.scripts.map((s) => ({ ...s, status: { state: 'absent' } })) };
      addLog('info', 'hook', null, 'Removed the launch hook block; the rest of each script is unchanged');
      return clone(hooks);
    },
    doctor: () => clone(SAMPLES.doctor),
    export_diagnostics: () => {
      let text = 'StarOrg Strings diagnostics\n\n';
      for (const sec of SAMPLES.doctor.sections) {
        text += `## ${sec.title}\n`;
        for (const c of sec.checks) text += `[${c.level}] ${c.label}${c.detail ? ` (${c.detail})` : ''}\n`;
        text += '\n';
      }
      text += '## Recent log\n';
      for (const e of log.slice(-50).reverse()) text += `${e.time} ${e.level} ${e.message}\n`;
      text += `\n## Characters\n${CHARACTER_NAMES[0]} (geid ${CHARACTER_IDS[0]}) active on LIVE\nStarOrg user: ${starorg.user ?? 'not signed in'}\n`;
      const [out, redacted] = redact(text);
      return { file_name: 'starorg-strings-diagnostics-20261114-213005.txt', text: out, redacted };
    },
    // The browser has no native save dialog: the UI downloads a blob instead and never calls this.
    save_diagnostics: () => {
      addLog('info', null, null, 'Saved diagnostics');
      return '~/starorg-strings-diagnostics-20261114-213005.txt';
    },
    ...createUpdateMock(() => settings, params),
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
    // A lived-in state for renders: LIVE applied over Smart Citizen's old file, HOTFIX behind a game update.
    const live = chans.get(`${inst.id}/LIVE`)!;
    live.backups.push(
      mkBackup(1, 'pre-existing', oldBuild, 'foreign', 'Smart Citizen', '2026-11-09T08:11:58Z'),
      mkBackup(2, 'before-apply', live.summary.build, 'ours', null, '2026-11-14T19:41:10Z'),
    );
    live.summary.status = { state: 'applied' };
    live.summary.foreign_file = false;
    live.summary.foreign_tool = null;
    live.summary.cache = 'fresh';
    live.summary.applied_build = clone(live.summary.build);
    live.summary.last_apply = { time: '2026-11-14T19:41:12Z', result: 'applied', changed: 11_254, warnings: 1, dropped: 0, duration_ms: 1830 };
    live.overrides.set('item_NameSYN_Cooler_S2_00001', 'My favourite cooler');
    live.overrides.set('mission_TitleSYN_Contract_00005', 'Synthetic Contract 5 <EM4>[Easy]</EM4>');
    const hot = chans.get(`${inst.id}/HOTFIX`)!;
    hot.backups.push(mkBackup(1, 'pre-existing', hot.summary.build, 'none', null, '2026-11-09T08:12:38Z'));
    hot.summary.applied_build = { branch: 'sc-alpha-4.10.0-hotfix', game_version: '4.10.0', change: 13_150_002, version: '1.0.188.55001' };
    hot.summary.status = { state: 'stale', reasons: ['build'] };
    hot.summary.last_apply = { time: '2026-11-09T08:12:40Z', result: 'applied', changed: 10_980, warnings: 0, dropped: 0, duration_ms: 1710 };
    hooks = { ...hooks, scripts: hooks.scripts.map((s) => ({ ...s, path: s.path ?? `/home/pilot/Games/star-citizen/sc-${s.kind}.sh`, status: { state: 'current' } })) };
    // Ready-state history, in time order: found, set up, hooks, applies, launches.
    for (const e of log) e.time = e.message.startsWith('Found') ? '2026-11-09T08:03:00Z' : '2026-11-09T08:04:10Z';
    const seed: [T.LogLevel, T.JobKind | null, string | null, string, string][] = [
      ['info', 'hook', null, 'Installed the launch hook block in sc-prelaunch.sh and sc-postexit.sh', '2026-11-09T08:05:00Z'],
      ['info', 'apply', 'LIVE', 'Applied: 11,102 strings changed, 0 warning(s)', '2026-11-09T08:12:00Z'],
      ['info', 'apply', 'HOTFIX', 'Applied: 10,980 strings changed, 0 warning(s)', '2026-11-09T08:12:40Z'],
      ['info', 'hook', 'LIVE', 'prelaunch: LIVE is up to date (0.2 s, no apply)', '2026-11-13T20:01:10Z'],
      ['info', 'hook', 'LIVE', 'postexit: 2 new blueprints for Pilot_A from Game.log; re-applied', '2026-11-13T23:48:02Z'],
      ['warn', 'apply', 'LIVE', 'Applied: 11,254 strings changed, 1 warning(s): syn-moved-key target changed', '2026-11-14T19:41:12Z'],
      ['warn', 'hook', 'HOTFIX', 'prelaunch: HOTFIX game build changed (13150002 -> 13172004); data cache missing, apply skipped (time budget 20 s)', '2026-11-14T20:15:31Z'],
      ['error', 'apply', 'PTU', 'PTU is not set up. Run setup for this channel first.', '2026-11-14T20:20:00Z'],
    ];
    for (const [level, job, channel, message, time] of seed) log.push({ id: 0, time, level, job, channel, message });
    log.sort((a, b) => a.time.localeCompare(b.time));
    log.forEach((e, i) => (e.id = i + 1));
    nextLog = log.length + 1;
    if (params.get('starorg') === '1') {
      settings.network.starorg = true;
      Object.assign(starorg, { connected: true, user: 'pilot_a_discord', guild: 'Synthetic Org', expires: '2027-02-12T21:30:05Z' });
    }
    if (params.get('log') === 'empty') log.length = 0;
    sync();
  }

  // Strings groups: `?groups=approximate` starts every channel approximate; `&groupsread=busy`
  // makes the read fail as while another extraction runs; `&groupsread=slow` holds it 20 s.
  const groupsMode = params.get('groups');
  const groupsRead = params.get('groupsread');
  if (groupsMode === 'approximate') for (const c of chans.values()) c.exactGroups = false;

  // Render states (any start state).
  const each = (key: string, f: (c: Chan) => void) => {
    for (const name of (params.get(key) ?? '').split(',').filter(Boolean)) {
      const c = chans.get(`${installs[0]!.id}/${name}`);
      if (c) f(c);
    }
  };
  each('running', (c) => (c.summary.game_running = true));
  each('nobackups', (c) => (c.backups = []));
  if (params.get('accent')) settings.appearance.accent = params.get('accent') as T.Accent;
  if (params.get('palette')) settings.appearance.palette = params.get('palette') as T.Palette;
  each('interrupted', (c) => (c.summary.interrupted_apply = true));
  each('builderr', (c) => (c.summary.build_error = 'Data.p4k: Branch record not found (file is being written?)'));
  each('foreign', (c) => {
    c.summary.status = { state: 'changed-by-another-tool', tool: 'Smart Citizen' };
    c.summary.foreign_file = true;
    c.summary.foreign_tool = 'Smart Citizen';
  });
  const extra = Number(params.get('channels') ?? 0) - installs[0]!.channels.length;
  for (let i = 0; i < extra; i++) {
    const name = ['TECH-PREVIEW', 'EPTU', 'EVOCATI', 'TEST'][i] ?? `CH${i}`;
    const base = clone(installs[0]!.channels[2]!);
    const summary: T.ChannelSummary = { ...base, channel: name, set_up: false, status: { state: 'not-applied' }, language: 'english', language_value: null };
    installs[0]!.channels.push(summary);
    chans.set(`${installs[0]!.id}/${name}`, { ...clone({ ...chans.get(`${installs[0]!.id}/PTU`)!, owned: new Map(), forgotten: new Set<string>() }), summary, owned: new Map(), forgotten: new Set() });
  }
  if (params.get('installs') === '0') installs.length = 0;
  const failing = new Set((params.get('fail') ?? '').split(',').filter(Boolean));

  return {
    kind: 'mock',
    async call<K extends Command>(cmd: K, args: Args<K>): Promise<Result<K>> {
      // Answer asynchronously like IPC does, so the UI's loading states are exercised.
      await sleep(0);
      if (failing.has(cmd)) fail('internal', `Could not read the game folder (${cmd}).`, 'EACCES: permission denied, open .../StarCitizen/LIVE/Data.p4k');
      const fn = h[cmd] as Handler<K>;
      return fn(args);
    },
    async onJob(cb) {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
  };
}

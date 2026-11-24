/**
 * In-browser mock transport: the same behaviour as the Rust MockBackend
 * (app/src-tauri/src/contract/mock.rs), for `npm run dev` and headless renders without Tauri.
 * Seeded from samples.gen.json (real Rust mock output), so the shapes are the contract's.
 * All data is synthetic: no real character names, geids or CIG text.
 *
 * URL switches: `?delay=<ms>` per job step (default 250), `?state=ready` to start with LIVE
 * applied and HOTFIX set up but behind a game update (skips the wizard), with some backups,
 * overrides and hook history; add `&starorg=1` to start signed in to StarOrg Tools
 * (`&starorgenv=dev` on dev.starorg.tools, `&sync=all` with every "What to sync" switch on).
 * States for renders (channel names): `&running=LIVE` (game running), `&interrupted=LIVE`,
 * `&foreign=LIVE` (another tool's file in place), `&builderr=LIVE`, `&channels=5`,
 * `&installs=0` (nothing found), `&fail=overview`, `&log=empty`, `&nobackups=LIVE`,
 * `&accent=<accent>`, `&palette=black`. Slow and failing calls: `&slow=<cmd>:<ms>,...`,
 * `&failonce=<cmd>,...` (fails the first call, then works: Retry; plan_setup answers as the real
 * backend does, Ok with a game-data warning and zero counts), `&busy=<n>` (n plan_setup calls
 * find another process reading), `&preview=nodata` (no game data
 * read until plan_setup, as the real backend before setup). Launch hooks: `&hooks=windows|nolug`. Updates: `&update=available|failed|skipped`
 * (mock-update.ts).
 * Developer mode: `&dev=1` turns it on (Settings > Advanced) with the sample dataset folder
 * added; `&dev=open` also opens LIVE's explorer (mock-dev.ts).
 * Ships screen: `&fleet=imported` (fleet imported, ship names off), `&fleet=grouped` (on, a group
 * and a rename), `&fleet=review` (imported, two rows still to match), `&fleet=nodata` (game data
 * not read yet). MCP: `&mcp=on` (the local MCP server running, with sample requests).
 * UEX market data: `&uex=fetched` (both files there; the default with `state=ready`), `&uex=missing`.
 * Blueprints: `&bphint=larger` (auto mode, another character owns far more: the warning),
 * `&bpnotice=1` (two characters share a name: the notice).
 * Memory limit: `&memory=1.5` (that limit in GB instead of the default).
 * Live telemetry: `&telemetry=on` (on for PTU, the mock's 5.0 channel), `&telemetry=triggers`
 * (game actions allowed too), `&telemetry=gamelog` (Game.log events and People on too); the
 * stream is the synthetic generator (mock-telemetry.ts).
 */
import { clampGb, memoryView } from '../memory';
import { fold, nameProblem, SEPARATORS, SYMBOLS } from '../shipnames';
import { prettyKey } from '../strowner';
import { obtainPreview } from '../obtain';
import type { Args, Command, Result, Transport } from './client';
import { ContractError } from './client';
import samples from './samples.gen.json';
import { serial } from '../loadstate';
import type * as T from './types';
import { createUpdateMock } from './mock-update';
import { mockFinder, seedOwned } from './mock-finder';
import { createDevMock, type DevMockData } from './mock-dev';
import devData from './mock-dev.gen.json';
import { createAssetMock, type AssetMockData } from './mock-assets';
import assetData from './mock-assets.gen.json';
import { mockLootDetail, mockLootList, type LootMockData } from './mock-loot';
import lootData from './mock-loot.gen.json';
import { createTelemetryMock } from './mock-telemetry';
import { mockItemSearch, type ShopItemCatalogue } from './mock-shops';

const clone = <X>(x: X): X => structuredClone(x);
const SAMPLES = samples as unknown as {
  overview_first_run: T.Overview;
  profile_schema: T.ProfileSchema;
  list_patches: T.PatchView[];
  get_settings: T.Settings;
  list_characters: T.CharactersView;
  owned_blueprints: T.OwnedBlueprint[];
  blueprint_finder: T.BlueprintFinder;
  hooks_status: T.HooksStatus;
  doctor: T.DoctorReport;
  starorg_status: T.StarOrgStatus;
  set_starorg_guild: T.StarOrgStatus;
  import_smart_citizen: T.ImportReport;
  rescan_logs: T.RescanReport;
  query_strings: T.StringsPage;
  fleet_view: T.FleetView;
  fleet_import: T.FleetImportReport;
  save_ship_names: T.FleetView;
  mcp_status: T.McpStatus;
  set_mcp_enabled: T.McpStatus;
  memory_status: T.MemoryStatus;
  set_memory_limit: T.MemoryStatus;
  storage_status: T.StorageStatus;
  obtain_status: T.ObtainStatus;
  uex_token_status: T.UexTokenStatus;
  set_uex_token: T.UexTokenStatus;
  fetch_uex: T.ObtainStatus;
  shop_inventory: T.ShopDirectory;
  shop_inventory_missing: T.ShopDirectory;
  shop_inventory_linked: T.ShopDirectory;
  shop_items: T.ShopItems;
  shop_items_all: T.ShopItems[];
  shop_items_all_linked: T.ShopItems[];
  shop_item_search: T.ShopItemSearch;
  shop_item_search_missing: T.ShopItemSearch;
  shop_item_catalogue: ShopItemCatalogue;
  shop_item_catalogue_linked: ShopItemCatalogue;
  commodity_list_not_read: T.CommodityDirectory;
  commodity_list: T.CommodityDirectory;
  commodity_list_priced: T.CommodityDirectory;
  commodity_detail: T.CommodityInfo;
  commodity_details_all: T.CommodityDetail[];
  commodity_details_all_priced: T.CommodityDetail[];
  telemetry_status: T.TelemetryStatus;
  telemetry_actions: T.TelemetryActions;
  telemetry_event_frame: T.TelemetryEvent;
  telemetry_gamelog: T.GameLogView | null;
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
  lastImport?: string | null;
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

/** Owners of synthetic row `i`, primary first (same rules as `mock::mock_owners` in Rust). */
export function mockOwners(cat: string, i: number, original: string, group: string, manufacturer: string | null): T.KeyOwner[] {
  const detail = manufacturer ? (MOCK_MANUFACTURERS.find(([c]) => c === manufacturer)?.[1] ?? null) : null;
  const own = (name: string, field: T.OwnerField, kind: T.OwnerKind, d: string | null = detail): T.KeyOwner => ({ name, field, kind, detail: d });
  switch (cat) {
    case 'ships':
      return [own(`Sample Hull ${i}`, 'description', group === 'ships/ground' ? 'ground-vehicle' : 'ship')];
    case 'components':
      return [own(original, 'name', 'component')];
    case 'ship-weapons':
      return i % 5 === 0 ? [own(original, 'name', 'ship-weapon'), own(`${original} Turret`, 'name', 'component')] : [own(original, 'name', 'ship-weapon')];
    case 'missiles':
      return [own(original, 'name', 'missile')];
    case 'fps-weapons':
      return [own(`Sample Rifle ${i}`, 'description', 'fps-weapon')];
    case 'missions':
      return [own(original, 'title', 'mission', null)];
    case 'items':
      return [own(original, 'name', 'named', null)];
    default:
      return [];
  }
}

/** Secondary owners' names per shared key, lower case (search only; `StringRow.other_owners` in Rust). */
const OTHER_OWNERS = new Map<string, string>();

/** A page row without an owner gets the readable key (as `strings_query::with_fallback`). */
const withFallback = (r: T.StringRow): T.StringRow => (r.owner ? r : { ...r, owner: { name: prettyKey(r.key), field: null, kind: 'key', detail: null } });

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
    quest_hints: cats,
    obtain: true,
    obtain_mode: p === 'minimal' ? 'short' : all ? 'detailed' : 'normal',
    obtain_shops: all,
    obtain_prices: all,
    missing_names: true,
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
      title_bp_em: 4,
      title_rep_em: 4,
      title_owned: false,
      owned_em: 4,
      owned_line: false,
      missing_marker: false,
      missing_em: 0,
      variant_em: 4,
      reward_em: 0,
    },
    branding: all,
    colour_test: false,
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
    obtain_hints: n(p.obtain, 3480),
    missing_names: n(p.missing_names, 421),
    overrides,
    dropped: 0,
    changed: 0,
  };
  c.changed = (c.ships + c.components + c.ship_weapons + c.fps_weapons + c.missiles + c.missions) * 2 + c.obtain_hints + c.missing_names + overrides + 1200;
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
    const owners = mockOwners(cat, i, original, group, manufacturer);
    if (owners.length > 1) OTHER_OWNERS.set(key, owners.slice(1).map((o) => o.name.toLowerCase()).join('\n'));
    rows[i] = { key, original, result, source, category: cat, group, group_label: groupLabel(group), manufacturer, size, owner: owners[0] ?? null, owners: owners.length };
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
    counts: { changed: 0, ships: 0, components: 0, ship_weapons: 0, fps_weapons: 0, missiles: 0, missions: 0, obtain_hints: 0, missing_names: 0, overrides: 0, dropped: 0 },
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
  const m = p.mission;
  const em = (n: number, t: string) => (n === 0 ? t : `<EM${n}>${t}</EM${n}>`);
  const title = 'Synthetic Recovery Contract';
  let tt = title;
  if (p.missions) {
    const wrap = (n: number, s: string) => ` ${em(m.title_em ? n : 0, s)}`;
    // With title_owned the title previews as a contract whose pool is all owned ([BP Owned]).
    if (m.title_blueprint) tt += wrap(m.title_bp_em, m.title_owned ? '[BP Owned]' : '[BP]');
    if (m.title_reputation) tt += wrap(m.title_rep_em, '[150 Rep]');
  }
  // The sample character owns the first blueprint of the list.
  const prose = 'Recover the synthetic cargo.';
  const body: string[] = [];
  if (p.missions && m.blueprints) {
    const list = [em(m.header_em, 'BLUEPRINTS')];
    for (const [bp, owned] of [['Sample Rifle', true], ['Sample Pistol', false]] as const) {
      if (owned && m.owned_marker) list.push(m.owned_line && m.owned_em !== 0 ? `- ${em(m.owned_em, `${bp} [Owned]`)}` : `- ${bp} ${em(m.owned_em, '[Owned]')}`);
      else if (!owned && m.missing_marker) list.push(`- ${bp} ${em(m.missing_em, '[Missing]')}`);
      else list.push(`- ${bp}`);
    }
    body.push(list.join('\\n'));
  }
  if (p.missions && m.item_rewards) body.push(`${em(m.header_em, 'ITEM REWARDS')}\\n- ${em(m.reward_em, 'Sample Scrip')} ×3`);
  if (p.missions && m.details) body.push(`${em(m.header_em, 'MISSION DETAILS')}\\n${em(m.label_em, 'Reputation:')} 150`);
  const out: T.PreviewItem[] = [
    { key: 'vehicle_DescSYN_Fighter', label: 'Ship description', category: 'ships', original: shipProse, result: block(shipProse, lines) },
    { key: 'item_NameSYN_Cooler_S2', label: 'Component name', category: 'components', original: name, result: tagged },
  ];
  // One stats-block description per kind (same in mock.rs), left out while it is off.
  const stats: [boolean, string, string, string, string, [string, string][]][] = [
    [p.components, 'item_DescSYN_Cooler_S2', 'Ship component', 'components', 'A synthetic cooler.', [['components.cooling', 'Cooling: 1,200/s'], ['components.health', 'Health: 900']]],
    [p.ship_weapons, 'item_DescSYN_Cannon_S3', 'Ship weapon', 'ship-weapons', 'A synthetic laser cannon.', [['ship_weapons.damage', 'Damage: 260 energy  |  DPS: 780'], ['ship_weapons.rate', 'Fire rate: 180 rpm']]],
    [p.fps_weapons, 'item_DescSYN_Pistol', 'Personal weapon', 'fps-weapons', 'A synthetic pistol.', [['fps_weapons.damage', 'Damage: 12 energy  |  DPS: 60'], ['fps_weapons.magazine', 'Magazine: 20']]],
    [p.missiles, 'item_DescSYN_Missile_S2', 'Missile', 'missiles', 'A synthetic heat-seeking missile.', [['missiles.damage', 'Damage: 2,400'], ['missiles.lock_range', 'Lock range: 4,000 m']]],
  ];
  for (const [on, key, label, category, prose, ls] of stats) {
    if (on) out.push({ key, label, category, original: prose, result: block(prose, ls.filter(([k]) => lineOn(k)).map(([, l]) => l)) });
  }
  out.push(
    { key: 'mission_TitleSYN_Recovery', label: 'Mission title', category: 'missions', original: title, result: tt },
    { key: 'mission_DescSYN_Recovery', label: 'Contract description', category: 'missions', original: prose, result: body.length ? `${prose}\\n\\n${body.join('\\n\\n')}` : prose },
  );
  if (p.quest_hints) {
    const chip = 'A synthetic access chip.';
    out.push({ key: 'item_DescSYN_Chip', label: 'Quest item', category: 'quest-hints', original: chip, result: `${chip}\\n\\n${em(m.label_em, 'Used in:')} Synthetic Recovery · Stanton` });
  }
  out.push(...obtainPreview(p, block));
  if (p.missing_names) {
    const key = 'item_Name_syn_flightsuit_01_01_02';
    out.push({ key, label: 'Item name (missing in stock)', category: 'missing-names', original: `@${key}`, result: 'Synthetic Flightsuit' });
  }
  if (p.colour_test) {
    const prompt = 'Please select a contract.';
    out.push({ key: 'mG_ContractsMgr_Select', label: 'Colour test', category: 'ui', original: prompt, result: prompt + COLOUR_TEST_LINE });
  }
  return out;
}

/** Same as `enhance::COLOUR_TEST_LINE` in Rust. */
export const COLOUR_TEST_LINE = ' Colour test: <EM1>EM1</EM1> <EM2>EM2</EM2> <EM3>EM3</EM3> <EM4>EM4</EM4> <EM5>EM5</EM5>';

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
  /** `&preview=nodata`: channels whose game data was read (plan_setup); build_preview is empty for the rest. */
  const previewNoData = params.get('preview') === 'nodata';
  const dataRead = new Set<string>();
  const chans = new Map<string, Chan>();
  const id = (r: T.ChannelRef) => `${r.install}/${r.channel}`;
  for (const inst of installs) {
    for (const c of inst.channels) {
      // Per channel, as the Rust mock seeds them (mock_finder.rs): PTU progress is not LIVE's.
      const owned = seedOwned(SAMPLES.blueprint_finder, c.channel);
      const chars = SAMPLES.list_characters.characters;
      chans.set(`${inst.id}/${c.channel}`, {
        summary: c,
        backups: [],
        profile: presetProfile('recommended'),
        overrides: new Map(),
        patches: clone(SAMPLES.list_patches.filter((x) => x.origin === 'built-in')),
        // `&bphint=larger`: another character owns far more (the Blueprints screen's warning).
        active: { mode: 'auto', character: null, resolved: chars[0]!.id, fallback_from: null, larger: params.get('bphint') === 'larger' ? (chars[1]?.id ?? null) : null },
        owned,
        forgotten: new Set(),
      });
    }
  }
  const defaultSync = (): T.StarOrgSettings => ({ environment: 'production', import_game_log: true, push_blueprints: false, pull_blueprints: false, pull_fleet: false, auto_sync: false });
  let settings: T.Settings = { ...clone(SAMPLES.get_settings), network: { check_updates: false, published_dumps: false, starorg: false, uex: false }, starorg: defaultSync() };
  const signedOut = { connected: false, user: null, guild: null, guild_id: null, guilds: [] as T.StarOrgGuild[], expires: null, token_store: null };
  const starorg: T.StarOrgStatus = { ...clone(SAMPLES.starorg_status), ...signedOut, expired: false, sync_characters: [] };
  const signIn = () =>
    Object.assign(starorg, { ...clone(SAMPLES.set_starorg_guild), sync_characters: starorg.sync_characters, push_count: starorg.push_count, last_sync: starorg.last_sync });
  const syncOff = (what: string) => fail('invalid', `${what} is off under Settings -> StarOrg Tools -> What to sync.`);
  let hooks: T.HooksStatus = clone(SAMPLES.hooks_status);
  // `&hooks=windows`: no launch hooks (Windows); `&hooks=nolug`: Linux without the LUG Helper.
  if (params.get('hooks') === 'windows') hooks = { supported: false, cli_on_path: false, scripts: [] };
  if (params.get('hooks') === 'nolug') hooks = { ...hooks, scripts: hooks.scripts.map((x) => ({ ...x, path: null, is_link: false, status: { state: 'missing' } })) };
  let mcp: T.McpStatus = clone(params.get('mcp') === 'on' ? SAMPLES.set_mcp_enabled : SAMPLES.mcp_status);
  // The mock's machine: 32 GB, 18.4 GB free (the samples). `?memory=1.5` starts at that limit.
  let memory: T.MemoryStatus = memoryView(SAMPLES.memory_status, Number(params.get('memory')) || null);
  // `?uex=fetched` (and `?state=ready` unless `&uex=missing`): every UEX file there (as after "Fetch now").
  let uex: T.ObtainStatus = clone(params.get('uex') === 'fetched' || (startReady && params.get('uex') !== 'missing') ? SAMPLES.fetch_uex : SAMPLES.obtain_status);
  let uexToken: T.UexTokenStatus = clone(SAMPLES.uex_token_status);
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
    const owners = mockOwners(row.category, Number(row.key.slice(-5)) || 0, row.original, row.group, row.manufacturer);
    return { row: withFallback(row), provenance, override_value: ov, also_used_by, owners };
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
    starorg.environment = settings.starorg.environment;
    starorg.host = settings.starorg.environment === 'dev' ? 'dev.starorg.tools' : 'starorg.tools';
    return clone(starorg);
  };
  const charactersView = (c: Chan): T.CharactersView => ({
    ...clone(SAMPLES.list_characters),
    channel: c.summary.channel,
    characters: SAMPLES.list_characters.characters.filter((x) => !c.forgotten.has(x.id)).map((x) => ({ ...x, owned: c.owned.get(x.id)?.length ?? 0 })),
    active: clone(c.active),
    last_import: c.lastImport ?? null,
    // `&bpnotice=1`: two characters share a name (the real backend's wording).
    notice: params.get('bpnotice')
      ? '2 characters are named Pilot_A on different accounts in the game logs, so their blueprints are kept apart. To mark blueprints from all of them, choose Any of my characters; or re-import from Smart Citizen into your current character (the newest).'
      : (SAMPLES.list_characters.notice ?? null),
  });
  const starorgGuild = (): string => {
    if (!settings.network.starorg) fail('network-off', 'Turn on "Connect StarOrg Tools" under Settings -> Network first.');
    if (!starorg.connected || !starorg.guild) fail('needs-decision', 'Sign in to StarOrg Tools first (Settings -> StarOrg Tools).');
    return starorg.guild!;
  };
  const firstRun = () => !installs.some((i) => i.channels.some((c) => c.set_up));

  // Fleet and ship names (addendum §6): one fleet and one shared list for every channel.
  let fleetMode = params.get('fleet');
  const catalog = clone(SAMPLES.fleet_view.catalog);
  const emptyNames = (): T.ShipNamesSettings => ({ enabled: false, style: 'numeric', separator: ' ', symbol: '*', rest: 'stock', prefix_short: false, groups: [], ships: [] });
  const fleet = {
    owned: [] as T.OwnedHull[],
    pending: [] as T.PendingShip[],
    matches: new Map<string, string | null>(),
    imported_at: null as string | null,
    format: null as T.FleetFormat | null,
  };
  let shared = emptyNames();
  const scopes = new Map<string, { scope: T.ShipNamesScope; own: T.ShipNamesSettings | null }>();
  let lastFile = false;
  const fv = SAMPLES.fleet_view;
  if (fleetMode && fleetMode !== 'nodata') {
    fleet.owned = clone(fv.owned);
    fleet.imported_at = fv.imported_at;
    fleet.format = fv.format;
    shared = fleetMode === 'grouped' ? clone(SAMPLES.save_ship_names.settings) : { ...emptyNames(), ships: fv.owned.map((o) => ({ hull: o.hull, group: null, name: null, short: null })) };
    if (fleetMode === 'review') fleet.pending = clone(SAMPLES.fleet_import.unmatched.concat(SAMPLES.fleet_import.ambiguous));
    else fleet.pending = clone(fv.pending);
  }
  const norm = (x: string) => x.toLowerCase().replace(/[\u2018\u2019]/g, "'").replace(/[^\p{L}\p{N}]+/gu, ' ').trim();
  /** A small version of the core matcher: exact name, name without its first word (maker), short name. */
  const matchName = (name: string): string[] => {
    const n = norm(name);
    const saved = fleet.matches.get(n);
    if (saved !== undefined) return saved === null ? ['-'] : [saved];
    const forms = (h: T.FleetHull) => [norm(h.name), norm(h.name).split(' ').slice(1).join(' '), norm(h.short ?? '')];
    let hits = catalog.filter((h) => forms(h).includes(n)).map((h) => h.hull);
    if (!hits.length) {
      const noMk = (x: string) => x.replace(/\bmk (i+|v|iv|\d+)\b/g, '').replace(/\s+/g, ' ').trim();
      hits = catalog.filter((h) => forms(h).map(noMk).includes(noMk(n))).map((h) => h.hull);
    }
    return hits;
  };
  const parseText = (text: string) => {
    const rows: { name: string; count: number; custom: string | null; maker: string | null }[] = [];
    const t = text.replace(/^\ufeff/, '').trim();
    if (t.startsWith('{')) fail('invalid', 'not a fleet export');
    if (t.startsWith('[')) {
      let arr: unknown;
      try {
        arr = JSON.parse(t);
      } catch {
        fail('invalid', 'not a fleet export');
      }
      for (const r of arr as Record<string, string>[]) {
        if (r.entity_type && r.entity_type !== 'ship') continue;
        const name = r.name || r.ship_name;
        if (!name) continue;
        const custom = r.ship_name && norm(r.ship_name) !== norm(name) ? r.ship_name : null;
        rows.push({ name, count: 1, custom, maker: r.manufacturer_code ?? null });
      }
      return { rows, format: 'xplor-json' as T.FleetFormat, total: (arr as unknown[]).length };
    }
    // Same rule as the core: a CSV-shaped first line is a CSV (refused here without the
    // HangarXPLOR columns); CSV-shaped or over-long lines are never read as ship names.
    const first = t.split(/\r?\n/)[0] ?? '';
    if (first.split(',').length >= 3 || first.includes('\t')) {
      const cols = first.split(',').map((c) => c.trim().replace(/^"|"$/g, '').toLowerCase());
      if (!(cols.includes('manufacturer') && cols.includes('ship'))) fail('invalid', 'not a fleet export: no Manufacturer and Ship columns');
    }
    for (const line of t.split(/\r?\n/)) {
      const l = line.trim();
      if (!l || l.startsWith('#')) continue;
      if (Array.from(l).length > 100 || /[,;\t"|]/.test(l)) continue;
      const m = /^(\d+)\s*[x×]\s+(.+)$/i.exec(l) ?? null;
      const m2 = /^(.+?)\s+[x×](\d+)$/i.exec(l) ?? null;
      if (m) rows.push({ name: m[2]!, count: Number(m[1]), custom: null, maker: null });
      else if (m2) rows.push({ name: m2[1]!, count: Number(m2[2]), custom: null, maker: null });
      else rows.push({ name: l, count: 1, custom: null, maker: null });
    }
    if (!rows.length) fail('invalid', 'the list names no ship');
    return { rows, format: 'text' as T.FleetFormat, total: rows.length };
  };
  const MOCK_FILE = JSON.stringify([
    { ship_code: 'KSTL_Kite', ship_name: 'Needle', manufacturer_code: 'KSTL', name: 'Kite', entity_type: 'ship', pledge_id: '90000001' },
    { ship_code: 'KSTL_Kite', ship_name: 'Kite', manufacturer_code: 'KSTL', name: 'Kite', entity_type: 'ship' },
    { ship_name: 'Talon', manufacturer_code: 'KSTL', name: 'Talon', entity_type: 'ship' },
    { ship_code: 'ORBW_Lantern', ship_name: 'Rusty', manufacturer_code: 'ORBW', name: 'Lantern', entity_type: 'ship' },
    { ship_code: 'RDGE_Hauler', ship_name: 'Hauler', manufacturer_code: 'RDGE', name: 'Hauler', entity_type: 'ship' },
    { ship_code: 'RDGE_Hauler', ship_name: 'Hauler', manufacturer_code: 'RDGE', name: 'Hauler', entity_type: 'ship' },
    { ship_code: 'VLMR_Pike', ship_name: 'Pike', manufacturer_code: 'VLMR', name: 'Pike', entity_type: 'ship' },
    { ship_code: 'TRNE_Starlance', ship_name: 'Starlance', manufacturer_code: 'TRNE', name: 'Starlance', entity_type: 'ship' },
    { name: 'Synthetic paint pack', entity_type: 'skin' },
  ]);
  const appendOrder = (hulls: string[]) => {
    let n = 0;
    for (const h of hulls) {
      if (shared.ships.some((e) => fold(e.hull) === fold(h))) continue;
      shared.ships.push({ hull: h, group: null, name: null, short: null });
      n++;
    }
    return n;
  };
  const scopeOf = (r: T.ChannelRef) => scopes.get(id(r)) ?? { scope: 'shared' as T.ShipNamesScope, own: null };
  const fleetView = (r: T.ChannelRef): T.FleetView => {
    const c = chan(r);
    const sc = scopeOf(r);
    const settings = sc.scope === 'channel' && sc.own ? sc.own : shared;
    const overrides = catalog
      .map((h) => ({ hull: h.hull, long: c.overrides.get(h.hull) ?? null, short: c.overrides.get(`${h.hull}_short`) ?? null }))
      .filter((o) => o.long !== null || o.short !== null);
    const ready = fleetMode !== 'nodata';
    return {
      ...clone(fv),
      channel: r.channel,
      catalog_ready: ready,
      catalog: ready ? clone(catalog) : [],
      owned: clone(fleet.owned),
      pending: clone(fleet.pending),
      not_in_game: [...fleet.matches].filter(([, v]) => v === null).map(([k]) => k),
      imported_at: fleet.imported_at,
      format: fleet.format,
      scope: sc.scope,
      settings: clone(settings),
      shared_settings: clone(shared),
      overrides,
      warnings: [],
      invalid: false,
      settings_file: sc.scope === 'channel' ? `~/.local/share/starorg-codex/channels/${r.install}/${r.channel}/profile.toml` : fv.settings_file,
    };
  };
  const checkNames = (st: T.ShipNamesSettings) => {
    if (!(SEPARATORS as readonly string[]).includes(st.separator)) fail('invalid', 'separator: one of " ", "-", ". "');
    if (!(SYMBOLS as readonly string[]).includes(st.symbol)) fail('invalid', 'symbol: one of ! # * + - .');
    for (const e of st.ships) for (const v of [e.name, e.short]) if (v !== null && v.trim() && nameProblem(v)) fail('invalid', `${JSON.stringify(v)}: ${nameProblem(v)}`);
    return { ...clone(st), ships: st.ships.map((e) => ({ ...e, name: e.name?.trim() || null, short: e.short?.trim() || null })) };
  };
  const markStale = () => {
    for (const c of chans.values()) if (c.summary.status.state === 'applied') c.summary.status = { state: 'stale', reasons: ['ship names'] };
    sync();
  };

  // Developer mode (mock-dev.ts): the same synthetic DataCore and datasets as the Rust mock.
  const dev = createDevMock(devData as unknown as DevMockData, {
    devOn: () => settings.advanced.developer_mode,
    dumpsOn: () => settings.network.published_dumps,
    hasChannel: (r) => chans.has(id(r)),
    channelNames: (install) => installs.find((i) => i.id === install)?.channels.map((c) => c.channel) ?? [],
    stockText: (k) => table().find((x) => x.key.toLowerCase() === k.toLowerCase())?.original ?? null,
    emit: (p) => listeners.forEach((l) => l(p)),
    sleep,
    delay,
    nextJob: () => `job-${nextJob++}`,
    fail: (c, m) => fail(c, m),
  });

  /** Settings > Storage: the samples' fixed areas, with the asset cache as it is now. */
  const storageView = (): T.StorageStatus => {
    const s = clone(SAMPLES.storage_status);
    const a = assets.handlers.asset_cache_status({}) as T.AssetCacheStatus;
    const area = s.areas.find((x) => x.id === 'asset-exports');
    if (area) {
      area.bytes = a.bytes;
      area.files = a.files;
      area.path = a.path;
      area.in_cache = a.is_default;
    }
    const inCache = s.areas.filter((x) => x.in_cache);
    s.cache_bytes = inCache.reduce((n, x) => n + x.bytes, 0);
    s.total_bytes = s.cache_bytes + s.areas.filter((x) => !x.in_cache).reduce((n, x) => n + x.bytes, 0);
    return s;
  };

  // Developer mode assets (mock-assets.ts): the Rust mock's synthetic archive and sounds.
  const devEntries = (devData as unknown as DevMockData).entries;
  const assets = createAssetMock(assetData as unknown as AssetMockData, {
    devOn: () => settings.advanced.developer_mode,
    hasChannel: (r) => chans.has(id(r)),
    findRecord: (w) => {
      const e = devEntries.find((x) => x.guid === w || x.name === w || x.path === w);
      return e ? { guid: e.guid, name: e.name, type: e.type } : null;
    },
    emit: (p) => listeners.forEach((l) => l(p)),
    sleep,
    delay,
    nextJob: () => `job-${nextJob++}`,
    fail: (c, m) => fail(c, m),
    memory: () => ({ limit: Math.round(memory.limit_gb * 1024 ** 3), free: memory.available_bytes }),
    takeCancel: (j) => cancelled.delete(j),
  });

  // Live telemetry (mock-telemetry.ts): the Rust mock's samples and a synthetic stream.
  const telemetry = createTelemetryMock(
    { status: SAMPLES.telemetry_status, actions: SAMPLES.telemetry_actions, frame: SAMPLES.telemetry_event_frame, gamelog: SAMPLES.telemetry_gamelog },
    { fail: (c, m) => fail(c, m), log: (m, ch) => addLog('info', null, ch, m) },
  );
  const teleParam = params.get('telemetry');
  if (teleParam === 'on' || teleParam === 'triggers' || teleParam === 'gamelog') {
    const ptu = SAMPLES.telemetry_status.channels.find((c) => c.offered);
    const gamelog = teleParam === 'gamelog';
    if (ptu) void telemetry.handlers.telemetry_configure({ req: { target: { install: ptu.install, channel: ptu.channel }, settings: { enabled: true, port: 50051, triggers: teleParam === 'triggers', gamelog, people: gamelog }, replace_other: false } });
  }

  const h: Handlers = {
    ...dev.handlers,
    ...assets.handlers,
    ...telemetry.handlers,
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
        dataRead.add(`${req.install}/${c.channel}`);
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
          backup_dir: `~/.local/share/starorg-codex/channels/${req.install}/${c.channel}/backups`,
          user_cfg_line: s.language === 'other' ? 'g_language = english' : null,
          user_cfg_path: `${inst.root}/${c.channel}/USER.cfg`,
          replaces_tool: s.foreign_tool,
          counts: { ...counts(p, 0), changed: counts(p, 0).changed - 37 * c.channel.length },
        });
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
      if (!c.summary.set_up) fail('needs-decision', `${name} is not set up. Run setup for this channel first.`, `starorg-codex setup --channel ${name} --preset recommended --yes`);
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
      if (c.summary.foreign_file) r.warnings.push({ kind: 'foreign-file', detail: `replacing a global.ini from ${c.summary.foreign_tool ?? 'another tool'} (kept as a backup); do not run two tools on one channel` });
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
      r.backup_dir = `~/.local/share/starorg-codex/channels/${req.target.install}/${name}/backups`;
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
      return { channel: req.channel, folder: `~/.local/share/starorg-codex/channels/${req.install}/${req.channel}/backups`, backups: clone([...c.backups].reverse()), offers: offers(c), keep_newest: 5, keep_builds: 3 };
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
    // Like the real backend: no game data read for the channel yet -> an empty list.
    build_preview: ({ req }) => (previewNoData && !dataRead.has(id(req.target)) ? [] : preview(req.profile ?? (req.preset ? presetProfile(req.preset) : chan(req.target).profile))),
    // `&preview=nodata`: until this runs (or plan_setup), the preview stays empty.
    read_game_data: async ({ req }) => {
      const c = chan(req.target);
      if (!c.summary.set_up) fail('needs-decision', `${req.target.channel} is not set up yet`);
      await sleep(Math.min(delay, 1500));
      dataRead.add(id(req.target));
      c.summary.cache = 'fresh';
      addLog('info', 'extract', req.target.channel, `Read the game data of ${req.target.channel}`);
      return clone(c.summary);
    },
    profile_schema: () => clone(SAMPLES.profile_schema),
    obtain_status: () => clone(uex),
    // The mock keeps no token value, only that one was saved (like the real backend's status).
    uex_token_status: () => clone(uexToken),
    set_uex_token: ({ req }) => {
      const t = (req.token ?? '').trim();
      if (/[^\x21-\x7e]/.test(t) || t.length > 512) fail('invalid', 'The token has spaces or characters a UEX token does not have.');
      uexToken = t ? clone(SAMPLES.set_uex_token) : clone(SAMPLES.uex_token_status);
      addLog('info', 'uex', null, t ? 'UEX token saved (keyring)' : 'UEX token removed');
      return clone(uexToken);
    },
    // No network here: the three files appear with the sample's time.
    fetch_uex: () => {
      if (!settings.network.uex) fail('network-off', 'Turn on "Download UEX market data" under Settings -> Network first.');
      uex = clone(SAMPLES.fetch_uex);
      addLog('info', 'uex', null, 'Downloaded UEX market data: 24,146 prices for 2,438 items, 2,598 for 124 commodities, at 826 terminals (7.9 MB)');
      return clone(uex);
    },
    // The Rust mock's synthetic loot once the channel's game data was read (setup or Read game data).
    // The detail waits longer than the list, so its skeleton shows while the list is usable.
    loot_list: async ({ req }) => {
      await sleep(delay);
      return mockLootList(lootData as unknown as LootMockData, !!req.target && dataRead.has(id(req.target)), req.view);
    },
    loot_detail: async ({ req }) => {
      await sleep(delay * 2);
      return mockLootDetail(lootData as unknown as LootMockData, !!req.target && dataRead.has(id(req.target)), req.view, req.id, req.filter ?? null, req.group_by ?? null, req.groups ?? null);
    },
    // The Rust mock's synthetic shops once both UEX files are there (`?uex=fetched` or Fetch now).
    // Ship items link once the channel's game data was read (Read game data), as in Rust.
    shop_inventory: ({ req }) =>
      clone(!uex.shops_ready ? SAMPLES.shop_inventory_missing : req.target && dataRead.has(id(req.target)) ? SAMPLES.shop_inventory_linked : SAMPLES.shop_inventory),
    shop_items: ({ req }) => {
      if (!uex.shops_ready) return { state: 'missing', folder: SAMPLES.shop_inventory_missing.folder, fetched: null, shop: null, items: [], linked: null };
      const all = req.target && dataRead.has(id(req.target)) ? SAMPLES.shop_items_all_linked : SAMPLES.shop_items_all;
      const hit = all.find((s) => s.shop?.id === req.terminal);
      return clone(hit ?? { ...SAMPLES.shop_items, shop: null, items: [] });
    },
    shop_item_search: ({ req }) => {
      const linked = !!req.target && dataRead.has(id(req.target));
      const dir = linked ? SAMPLES.shop_inventory_linked : SAMPLES.shop_inventory;
      const all = linked ? SAMPLES.shop_item_catalogue_linked : SAMPLES.shop_item_catalogue;
      return mockItemSearch(all, uex.shops_ready, SAMPLES.shop_inventory_missing.folder, dir.fetched, dir.linked, req.query, req.key ?? null);
    },
    // The Rust mock's synthetic commodities once the channel's game data was read; kiosk
    // prices once the commodity prices are there. A detail takes a moment (the skeleton shows).
    commodity_list: ({ req }) => {
      if (!req.target) return { ...clone(SAMPLES.commodity_list_not_read), state: 'no-channel' as const };
      if (!dataRead.has(id(req.target))) return clone(SAMPLES.commodity_list_not_read);
      return clone(uex.prices_ready ? SAMPLES.commodity_list_priced : SAMPLES.commodity_list);
    },
    commodity_detail: async ({ req }) => {
      await sleep(Math.min(delay, 300));
      if (!req.target) return { state: 'no-channel' as const, commodity: null };
      if (!dataRead.has(id(req.target))) return { state: 'not-read' as const, commodity: null };
      const all = uex.prices_ready ? SAMPLES.commodity_details_all_priced : SAMPLES.commodity_details_all;
      const hit = all.find((c) => c.id.toLowerCase() === req.id.toLowerCase());
      return { state: 'ready' as const, commodity: hit ? clone(hit) : null };
    },
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
        const has = (t: string | null | undefined) => !!t && t.toLowerCase().includes(needle);
        if (needle && !has(r.key) && !has(r.original) && !has(r.result) && !has(r.owner?.name) && !has(r.owner?.detail) && !has(OTHER_OWNERS.get(r.key))) continue;
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
      else if (req.sort === 'owner') {
        const FIELDS: T.OwnerField[] = ['name', 'short-name', 'description', 'title', 'career', 'role'];
        const k = (r: T.StringRow) => [r.owner ? 0 : 1, r.owner?.name.toLowerCase() ?? '', r.owner?.field ? FIELDS.indexOf(r.owner.field) : -1] as const;
        hits.sort((a, b) => {
          const [x, y] = [k(a), k(b)];
          return x[0] - y[0] || cmp(x[1], y[1]) || x[2] - y[2] || cmp(a.key, b.key);
        });
      }
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
      // Facets and the tree come with the first page only, as on the Rust side.
      const first = req.offset === 0;
      return { total, all: STRING_COUNT, offset: req.offset, rows: clone(hits).map(withFallback), sources: first ? fc(srcCounts) : [], categories: first ? fc(catCounts) : [], groups: first ? nodes : [], group_state };
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
    patch_states: ({ req }) => clone(chan(req).patches),
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
      // Tokens are per environment: switching signs out (the UI asks first).
      const envChanged = settings.starorg.environment !== req.starorg.environment;
      settings = clone(req);
      if (!settings.network.starorg || envChanged) Object.assign(starorg, signedOut);
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
    blueprint_finder: ({ req }) => {
      const inst = installs.find((i) => i.id === req.install);
      if (!inst) fail('not-found', `No install ${req.install}`);
      const all = inst!.channels.map((c) => c.channel);
      const names = req.channels.length
        ? req.channels.map((n) => all.find((a) => a.toLowerCase() === n.toLowerCase()) ?? fail('not-found', `no channel ${n} in this install`))
        : all;
      const picked = names.map((n) => ({ channel: n, ...chan({ install: req.install, channel: n }) }));
      return mockFinder(SAMPLES.blueprint_finder, picked, SAMPLES.list_characters.characters);
    },
    import_smart_citizen: ({ req }) => {
      const c = chan(req.target);
      const set = c.owned.get(req.character) ?? [];
      const cands = ['Synthetic Rifle Mk1', 'Sample Armor Legs', 'Sample Armor Arms', 'Test Knife'];
      const known = cands.filter((n) => set.some((b) => b.name === n));
      const imported = cands.filter((n) => !known.includes(n));
      // A re-import with replace drops what an earlier import added and this one lacks.
      const removed = req.replace ? set.filter((b) => b.sources.includes('import:smart-citizen') && !cands.includes(b.name)).map((b) => b.name) : [];
      if (!req.dry_run) {
        const kept = set.filter((b) => !removed.includes(b.name));
        for (const n of imported) kept.push({ name: n, ambiguous: false, favorite: false, sources: ['import:smart-citizen'], acquired: '2026-11-01T10:00:00Z' });
        c.owned.set(req.character, kept);
        c.lastImport = 'owned-blueprints.json';
      }
      return { ...clone(SAMPLES.import_smart_citizen), dry_run: req.dry_run, imported, known, removed };
    },
    rescan_logs: ({ req }) => {
      const c = chan(req.target);
      const characters = SAMPLES.list_characters.characters
        .filter((x) => !c.forgotten.has(x.id))
        .map((x) => {
          const set = c.owned.get(x.id) ?? [];
          const removed = req.replace ? set.filter((b) => !b.sources.includes('log')).map((b) => b.name) : [];
          return { id: x.id, name: x.name, before: set.length, after: set.length - removed.length, added: [], removed };
        });
      if (!req.dry_run && req.replace) for (const [k, v] of c.owned) c.owned.set(k, v.filter((b) => b.sources.includes('log')));
      return { ...clone(SAMPLES.rescan_logs), channel: c.summary.channel, dry_run: req.dry_run, replace: !!req.replace, characters };
    },
    fleet_view: ({ req }) => {
      if (req.read_game_data && fleetMode === 'nodata') fleetMode = null;
      return fleetView(req.target);
    },
    fleet_import: async ({ req }) => {
      chan(req.target);
      if (req.source.kind === 'star-org') {
        starorgGuild();
        if (!settings.starorg.pull_fleet) syncOff('Pull fleet/hangar');
      }
      if (req.source.kind === 'file') lastFile = true;
      if (req.source.kind === 'last' && !lastFile) fail('not-found', 'Choose the file again: it is no longer in memory.');
      await sleep(Math.min(delay, 400));
      const text = req.source.kind === 'text' ? req.source.text : MOCK_FILE;
      const { rows, format, total } = parseText(text);
      const hulls = new Map<string, T.ImportedHull>();
      const pending = new Map<string, T.PendingShip>();
      let notInGame = 0;
      for (const r of rows) {
        const hits = matchName(r.name);
        if (hits[0] === '-') {
          notInGame++;
          continue;
        }
        if (hits.length === 1) {
          const hc = catalog.find((x) => x.hull === hits[0])!;
          const e = hulls.get(hc.hull) ?? { hull: hc.hull, name: hc.name, count: 0, names: [], new: !fleet.owned.some((o) => o.hull === hc.hull) };
          e.count += r.count;
          if (r.custom && !e.names.includes(r.custom)) e.names.push(r.custom);
          hulls.set(hc.hull, e);
        } else {
          const e = pending.get(norm(r.name)) ?? { name: r.name, count: 0, maker: r.maker, names: [], candidates: hits.length > 1 ? hits : [] };
          e.count += r.count;
          pending.set(norm(r.name), e);
        }
      }
      const report: T.FleetImportReport = {
        file: req.source.kind === 'text' ? null : req.source.kind === 'star-org' ? 'StarOrg Tools hangar' : 'synthetic-fleet.json',
        format,
        dry_run: req.dry_run,
        merge: req.merge,
        total: format === 'xplor-json' ? total : rows.length,
        skipped: format === 'xplor-json' ? total - rows.length : 0,
        hulls: [...hulls.values()],
        unmatched: [...pending.values()].filter((p) => !p.candidates.length),
        ambiguous: [...pending.values()].filter((p) => p.candidates.length),
        not_in_game: notInGame,
        empty_catalog: fleetMode === 'nodata',
        added_to_order: 0,
        prune: !!req.prune,
        removed_from_order: [],
      };
      if (req.prune) {
        if (req.merge) fail('invalid', '"Remove all ships first" replaces the fleet; it cannot also add to it.');
        if (!req.dry_run && !hulls.size) fail('invalid', 'No ship matched, so My ships would be emptied: nothing removed or imported.');
        if (!report.empty_catalog) {
          const keep = new Set([...hulls.keys()].map(fold));
          report.removed_from_order = shared.ships
            .filter((e) => !keep.has(fold(e.hull)))
            .map((e) => ({ hull: e.hull, name: catalog.find((h) => fold(h.hull) === fold(e.hull))?.name.trim() ?? e.hull, rename: e.name }));
          if (!req.dry_run && report.removed_from_order.length) {
            shared.ships = shared.ships.filter((e) => keep.has(fold(e.hull)));
            if (shared.enabled) markStale();
          }
        }
      }
      if (!req.dry_run && !report.empty_catalog) {
        const next = req.merge ? clone(fleet.owned) : [];
        for (const h of hulls.values()) {
          const o = next.find((x) => x.hull === h.hull);
          if (o) o.count = Math.max(o.count, h.count);
          else next.push({ hull: h.hull, count: h.count, names: [...h.names] });
        }
        fleet.owned = next;
        fleet.pending = req.merge ? [...fleet.pending.filter((p) => !pending.has(norm(p.name))), ...pending.values()] : [...pending.values()];
        fleet.imported_at = NOW;
        fleet.format = format;
        report.added_to_order = appendOrder([...hulls.values()].filter((h) => h.new).map((h) => h.hull));
        if (shared.enabled && report.added_to_order) markStale();
        lastFile = false;
      }
      addLog('info', 'import', req.target.channel, `Fleet import${req.dry_run ? ' (dry run)' : ''}: ${report.hulls.length} hulls, ${report.unmatched.length} unmatched, ${report.ambiguous.length} ambiguous`);
      return report;
    },
    fleet_match: ({ req }) => {
      if (req.hull !== null && !catalog.some((h) => fold(h.hull) === fold(req.hull!))) fail('invalid', `${req.hull} is not a ship of this build`);
      fleet.matches.set(norm(req.name), req.hull);
      const p = fleet.pending.find((x) => norm(x.name) === norm(req.name));
      fleet.pending = fleet.pending.filter((x) => x !== p);
      if (p && req.hull) {
        const o = fleet.owned.find((x) => fold(x.hull) === fold(req.hull!));
        if (o) o.count += p.count;
        else {
          fleet.owned.push({ hull: req.hull, count: p.count, names: [...p.names] });
          appendOrder([req.hull]);
        }
      }
      return fleetView(req.target);
    },
    save_ship_names: ({ req }) => {
      const st = checkNames(req.settings);
      const before = fleetView(req.target).settings.enabled;
      if (req.scope === 'shared') shared = st;
      scopes.set(id(req.target), { scope: req.scope, own: req.scope === 'channel' ? st : null });
      if (before || st.enabled) markStale();
      return fleetView(req.target);
    },
    fleet_clear: ({ req }) => {
      fleet.owned = [];
      fleet.pending = [];
      fleet.imported_at = null;
      fleet.format = null;
      return fleetView(req);
    },
    forget_character: ({ req }) => {
      const c = chan(req.target);
      if (!CHARACTER_IDS.includes(req.character)) fail('not-found', 'No such character in the logs.');
      c.owned.delete(req.character);
      c.forgotten.add(req.character);
      if (c.active.character === req.character || c.active.resolved === req.character) {
        c.active = { mode: 'auto', character: null, resolved: CHARACTER_IDS.find((x) => !c.forgotten.has(x)) ?? null, fallback_from: null, larger: null };
      }
      return charactersView(c);
    },
    starorg_status: ({ channel }) => starorgView(channel),
    starorg_login: ({ channel }) => {
      if (!settings.network.starorg) fail('network-off', 'Turn on "Connect StarOrg Tools" under Settings -> Network first.');
      signIn();
      return starorgView(channel);
    },
    starorg_cancel_login: () => null,
    starorg_logout: ({ channel }) => {
      Object.assign(starorg, signedOut);
      return starorgView(channel);
    },
    set_starorg_guild: ({ req }) => {
      const g = starorg.guilds.find((x) => x.id === req.guild);
      if (!g) fail('invalid', 'Pick one of your orgs on StarOrg Tools.');
      Object.assign(starorg, { guild: g!.name, guild_id: g!.id });
      return starorgView(req.channel);
    },
    set_sync_characters: ({ req }) => {
      starorgView(req.channel);
      for (const c of starorg.sync_characters) c.ticked = req.ticked.includes(c.id);
      if (!req.ticked.length) return { ...clone(starorg), push_count: 0 };
      return starorgView(req.channel);
    },
    starorg_pull: ({ req }) => {
      const guild = starorgGuild();
      if (!settings.starorg.pull_blueprints) syncOff('Pull owned blueprints');
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
      if (!settings.starorg.push_blueprints) syncOff('Push owned blueprints');
      const sent = starorgView(channel).push_count;
      starorg.last_sync = NOW;
      addLog('info', 'sync', channel, `Pushed ${sent} owned blueprints to StarOrg (${guild})`);
      return { direction: 'push', channel, guild, character: null, added: [], already: 0, unmatched: [], sent, time: NOW };
    },
    starorg_sync: async ({ channel }) => {
      starorgGuild();
      const p = settings.starorg;
      if (!p.push_blueprints && !p.pull_blueprints && !p.pull_fleet) fail('invalid', 'Nothing to sync: turn on a StarOrg switch under Settings -> StarOrg Tools -> What to sync.');
      const reports: T.SyncReport[] = [];
      const skipped: string[] = [];
      const target = { install: installs[0]!.id, channel };
      const step = async (name: string, f: () => unknown) => {
        try {
          return await f();
        } catch (e) {
          skipped.push(`${name}: ${(e as T.AppError).message ?? String(e)}`);
          return null;
        }
      };
      if (p.pull_blueprints) {
        const who = chan(target).active.resolved;
        if (who) {
          const r = (await step('Pull', () => h.starorg_pull({ req: { channel, character: who } }))) as T.SyncReport | null;
          if (r) reports.push(r);
        } else skipped.push('Pull: no character found in the logs yet');
      }
      if (p.push_blueprints) {
        const r = (await step('Push', () => h.starorg_push({ channel }))) as T.SyncReport | null;
        if (r) reports.push(r);
      }
      const fleetR = p.pull_fleet ? ((await step('Fleet', () => h.fleet_import({ req: { target, source: { kind: 'star-org' }, dry_run: false, merge: true } }))) as T.FleetImportReport | null) : null;
      return { channel, reports, fleet: fleetR, skipped, time: NOW };
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
      let text = 'StarOrg Codex diagnostics\n\n';
      for (const sec of SAMPLES.doctor.sections) {
        text += `## ${sec.title}\n`;
        for (const c of sec.checks) text += `[${c.level}] ${c.label}${c.detail ? ` (${c.detail})` : ''}\n`;
        text += '\n';
      }
      text += '## Recent log\n';
      for (const e of log.slice(-50).reverse()) text += `${e.time} ${e.level} ${e.message}\n`;
      text += `\n## Characters\n${CHARACTER_NAMES[0]} (geid ${CHARACTER_IDS[0]}) active on LIVE\nStarOrg user: ${starorg.user ?? 'not signed in'}\n`;
      const [out, redacted] = redact(text);
      return { file_name: 'starorg-codex-diagnostics-20261114-213005.txt', text: out, redacted };
    },
    // The browser has no native save dialog: the UI downloads a blob instead and never calls this.
    save_diagnostics: () => {
      addLog('info', null, null, 'Saved diagnostics');
      return '~/starorg-codex-diagnostics-20261114-213005.txt';
    },
    // Experimental local MCP server. The browser cannot listen on a port: "on" shows the
    // sample status of a running server (fixed URL and token). `?mcp=on` starts it on.
    mcp_status: () => clone(mcp),
    set_mcp_enabled: ({ req }) => {
      if (req.enabled && !mcp.running) {
        mcp = { ...clone(SAMPLES.set_mcp_enabled), requests: [] };
        addLog('info', 'mcp', null, `Local MCP server started on ${mcp.url} (this computer only)`);
      } else if (!req.enabled && mcp.running) {
        mcp = clone(SAMPLES.mcp_status);
        addLog('info', 'mcp', null, 'Local MCP server stopped');
      }
      return clone(mcp);
    },
    memory_status: () => clone(memory),
    storage_status: () => storageView(),
    storage_measure: () => storageView(),
    storage_clear_assets: () => {
      assets.handlers.asset_cache_clear({});
      addLog('info', 'dev', null, 'Asset cache cleared');
      return storageView();
    },
    set_memory_limit: ({ req }) => {
      memory = memoryView(SAMPLES.memory_status, req.gb === null ? null : clampGb(req.gb, memory));
      addLog('info', 'dev', null, req.gb === null ? `Memory limit back to the default (${memory.default_gb} GB)` : `Memory limit for Developer mode and MCP workers: ${memory.limit_gb} GB`);
      return clone(memory);
    },
    // No system browser in the mock: the activity log records the click.
    open_link: ({ link }) => {
      const names: Record<T.ExternalLink, string> = { support: 'Support', donate: 'Buy me a coffee', patreon: 'Patreon', uex: 'UEX Corp' };
      addLog('info', null, null, `Opened the ${names[link]} link`);
      return null;
    },
    ...createUpdateMock(() => settings, params),
  };

  if (startReady) {
    const inst = installs[0]!;
    h.setup({
      req: {
        install: inst.id,
        channels: inst.channels.map((c) => ({ channel: c.channel, consent: c.language === 'english', switch_language: false })),
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
      signIn();
    }
    if (params.get('starorgenv') === 'dev') settings.starorg.environment = 'dev';
    if (params.get('sync') === 'all') Object.assign(settings.starorg, { push_blueprints: true, pull_blueprints: true, pull_fleet: true, auto_sync: true });
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
  // `&busy=<n>`: the first n plan_setup calls find another process reading (code busy).
  let busyLeft = Number(params.get('busy') ?? 0);
  const readSerial = serial();
  /** plan_setup with the URL faults, in the real backend's shapes. */
  async function planWithFaults({ req }: Args<'plan_setup'>): Promise<T.SetupPlan> {
    const unread = req.channels.some((c) => c.consent && !dataRead.has(`${req.install}/${c.channel}`));
    const hold = slow.get('plan_setup');
    if (hold && unread) await sleep(hold);
    if (busyLeft > 0) {
      busyLeft--;
      fail('busy', 'reading game data: another extraction is running');
    }
    if (failing.has('plan_setup')) fail('internal', 'Could not read the game folder (plan_setup).', 'EACCES: permission denied, open .../StarCitizen/LIVE/Data.p4k');
    const plan = await h.plan_setup({ req });
    if (failOnce.delete('plan_setup')) {
      // The real backend answers Ok with a game-data warning and zero counts per channel.
      for (const c of plan.channels) {
        dataRead.delete(`${req.install}/${c.channel}`);
        plan.warnings.push({ kind: 'game-data', detail: `${c.channel}: reading game data: Data.p4k: The process cannot access the file because it is being used by another process. (os error 32)` });
        c.counts = { ...c.counts, changed: 0, ships: 0, components: 0, ship_weapons: 0, fps_weapons: 0, missiles: 0, missions: 0 };
      }
    }
    return plan;
  }
  const failOnce = new Set((params.get('failonce') ?? '').split(',').filter(Boolean));
  // `&slow=plan_setup:20000,build_preview:3000`: hold these commands (a first Data.p4k read).
  const slow = new Map(
    (params.get('slow') ?? '')
      .split(',')
      .filter(Boolean)
      .map((x) => x.split(':') as [string, string])
      .map(([k, v]) => [k, Number(v ?? 20000)] as const),
  );
  if (params.get('dev')) {
    settings.advanced = { developer_mode: true };
    dev.addFolder();
    if (params.get('dev') === 'open' && installs[0]) void dev.handlers.dev_types({ req: { install: installs[0].id, channel: 'LIVE' } });
  }

  return {
    kind: 'mock',
    async call<K extends Command>(cmd: K, args: Args<K>): Promise<Result<K>> {
      // Answer asynchronously like IPC does, so the UI's loading states are exercised.
      await sleep(0);
      // Reads of game data run one at a time, as in the real backend: a second plan_setup waits
      // for the running one, then finds the data in memory (no second slow read).
      if (cmd === 'plan_setup') return (await readSerial(() => planWithFaults(args as Args<'plan_setup'>))) as Result<K>;
      if (cmd === 'apply') return (await readSerial(async () => (h.apply as Handler<'apply'>)(args as Args<'apply'>))) as Result<K>;
      const hold = slow.get(cmd);
      if (hold) await sleep(hold);
      if (failOnce.delete(cmd)) fail('internal', `Could not read the game data (${cmd}).`, 'Data.p4k: The process cannot access the file because it is being used by another process. (os error 32)');
      if (failing.has(cmd)) fail('internal', `Could not read the game folder (${cmd}).`, 'EACCES: permission denied, open .../StarCitizen/LIVE/Data.p4k');
      const fn = h[cmd] as Handler<K>;
      return fn(args);
    },
    async onJob(cb) {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    async onTelemetry(cb) {
      return telemetry.onTelemetry(cb);
    },
  };
}

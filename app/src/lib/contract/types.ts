/**
 * App <-> core command contract: TypeScript mirror of app/src-tauri/src/contract/types.rs.
 *
 * Kept in step by two Rust tests (contract::tests): every string union below must list exactly
 * the serde names of its Rust enum, and samples.gen.json (real mock output) must type-check
 * against these interfaces (samples.check.ts, run by `npm run check`).
 * Rules: snake_case fields, kebab-case values, `T | null` for Option (never omitted).
 */

import type { MinedReport, MinedSpot, OreQuality, QualityRange, QualityStep } from './dev';

export const CONTRACT_VERSION = 1;
export const JOB_EVENT = 'job-progress';

// Errors --------------------------------------------------------------------------------

export type ErrorCode = 'invalid' | 'not-found' | 'needs-decision' | 'busy' | 'network-off' | 'cancelled' | 'not-implemented' | 'internal' | 'dev-disabled';

export interface AppError {
  code: ErrorCode;
  message: string;
  detail: string | null;
}

// Builds, installs, channels ------------------------------------------------------------

export interface Build {
  branch: string | null;
  game_version: string | null;
  change: number | null;
  version: string | null;
}

export type ApplyStateName = 'applied' | 'stale' | 'changed-by-another-tool' | 'not-applied';

export type ApplyState =
  | { state: 'applied' }
  | { state: 'stale'; reasons: string[] }
  | { state: 'changed-by-another-tool'; tool: string | null }
  | { state: 'not-applied' };

export type Language = 'english' | 'other' | 'unknown';
export type CacheState = 'fresh' | 'missing' | 'stale';

export interface ChannelSummary {
  channel: string;
  build: Build;
  build_error: string | null;
  set_up: boolean;
  status: ApplyState;
  language: Language;
  language_value: string | null;
  foreign_tool: string | null;
  foreign_file: boolean;
  game_running: boolean;
  interrupted_apply: boolean;
  p4k_time: string | null;
  cache: CacheState;
  /** Build the file in place was made for (applied manifest); compare with `build`. */
  applied_build: Build | null;
  last_apply: LastApply | null;
}

export interface LastApply {
  time: string;
  result: ApplyResult;
  changed: number;
  warnings: number;
  dropped: number;
  duration_ms: number;
}

export type FoundBy = 'config' | 'game-dir' | 'prefix' | 'lug-config' | 'default-prefix' | 'lutris' | 'launcher-log' | 'common-path' | 'scan' | 'manual';

export interface InstallSummary {
  id: string;
  root: string;
  prefix: string | null;
  found_by: FoundBy;
  found_by_label: string;
  saved: boolean;
  channels: ChannelSummary[];
}

export interface Overview {
  contract: number;
  app_version: string;
  first_run: boolean;
  installs: InstallSummary[];
  active_install: string | null;
  default_channel: string | null;
  job: JobProgress | null;
}

export interface ChannelRef {
  install: string;
  channel: string;
}

// Jobs ----------------------------------------------------------------------------------

export type JobKind = 'discover' | 'extract' | 'apply' | 'remove' | 'restore' | 'import' | 'sync' | 'hook' | 'dev' | 'mcp' | 'uex';
export type JobPhase = 'running' | 'done' | 'failed' | 'cancelled';

export interface JobProgress {
  job: string;
  kind: JobKind;
  channel: string | null;
  phase: JobPhase;
  step: number;
  steps: number;
  label: string;
  cancellable: boolean;
}

// Setup ---------------------------------------------------------------------------------

export type Preset = 'recommended' | 'minimal' | 'everything';

export interface ChannelConsent {
  channel: string;
  consent: boolean;
  switch_language: boolean;
}

export interface SetupRequest {
  install: string;
  channels: ChannelConsent[];
  preset: Preset;
  fix_known_bugs: boolean;
}

export interface SetupResult {
  install: string;
  set_up: string[];
  skipped: string[];
}

export interface PlannedChannel {
  channel: string;
  target: string;
  target_is_link: boolean;
  link_target: string | null;
  backup_dir: string;
  user_cfg_line: string | null;
  user_cfg_path: string | null;
  replaces_tool: string | null;
  counts: ApplyCounts;
}

export interface SetupPlan {
  install: string;
  channels: PlannedChannel[];
  warnings: Warning[];
}

// Apply, backups ------------------------------------------------------------------------

export type ApplyResult = 'applied' | 'up-to-date' | 'skipped' | 'failed' | 'dry-run' | 'needs-setup' | 'needs-decision' | 'removed' | 'restored' | 'nothing-to-do';

export interface ApplyCounts {
  changed: number;
  ships: number;
  components: number;
  ship_weapons: number;
  fps_weapons: number;
  missiles: number;
  missions: number;
  /** Descriptions that got a WHERE TO GET block. */
  obtain_hints: number;
  /** Item name keys stock lacks that got a name, and description keys that got a sibling's description. */
  missing_names: number;
  overrides: number;
  dropped: number;
}

export interface Warning {
  kind: string;
  detail: string;
}

export interface UserCfgReport {
  changed: boolean;
  path: string | null;
  link_target: string | null;
  shared_with: string[];
  line: string | null;
}

export interface PatchOutcome {
  id: string;
  state: PatchState;
}

export type OwnedMode = 'auto' | 'any' | 'character';

/** What an apply used for `[Owned]` markers (M1 `blueprints::Selection`). */
export interface OwnedSelection {
  mode: OwnedMode;
  characters: string[];
  count: number;
  ambiguous: number;
}

/**
 * A returned report is not a success: M1 reports guards (game running, Data.p4k being written,
 * lock held, timeout) as `skipped` + `reason`, and failed / needs-setup / needs-decision /
 * up-to-date the same way. The UI switches on `result` (lib/outcome.ts).
 */
export interface ApplyReport {
  channel: string;
  result: ApplyResult;
  reason: string | null;
  build: Build;
  counts: ApplyCounts;
  patches: PatchOutcome[];
  warnings: Warning[];
  user_cfg: UserCfgReport;
  backup: string | null;
  backup_dir: string | null;
  target: string | null;
  target_is_link: boolean;
  /** Journal recovery: an interrupted earlier apply was rolled back first. */
  recovered: string | null;
  owned: OwnedSelection | null;
  duration_ms: number;
}

export interface ApplyRequest {
  target: ChannelRef;
  dry_run: boolean;
}

/** Without `allow_foreign`, another tool's loose file is kept and the report is needs-decision. */
export interface RemoveRequest {
  target: ChannelRef;
  allow_foreign: boolean;
}

export type BackupReason = 'pre-existing' | 'before-apply' | 'before-restore' | 'manual';
export type FileOrigin = 'ours' | 'foreign' | 'none';

export interface BackupView {
  id: string;
  created: string;
  reason: BackupReason;
  pinned: boolean;
  build: Build;
  existed: boolean;
  size: number | null;
  origin: FileOrigin;
  foreign_tool: string | null;
}

export interface ForeignRestore {
  backup: string;
  tool: string | null;
  made_for: Build;
  current: Build;
  build_mismatch: boolean;
}

export interface RestoreOffers {
  remove: boolean;
  remove_foreign: boolean;
  undo_last_apply: string | null;
  restore_foreign: ForeignRestore | null;
}

export interface BackupList {
  channel: string;
  folder: string;
  backups: BackupView[];
  offers: RestoreOffers;
  /** Retention: newest N, plus the newest of each of the last `keep_builds` builds, plus the pinned file. */
  keep_newest: number;
  keep_builds: number;
}

export interface BackupRef {
  target: ChannelRef;
  backup: string;
}

export interface RestoreRequest {
  target: ChannelRef;
  backup: string;
  allow_build_mismatch: boolean;
}

export interface PreviewItem {
  key: string;
  label: string;
  category: string;
  original: string;
  result: string;
}

export interface PreviewRequest {
  target: ChannelRef;
  profile: Profile | null;
  preset: Preset | null;
}

/** Read a set-up channel's game data into the cache ("Read game data"); nothing goes into the game folder. */
export interface ReadGameDataRequest {
  target: ChannelRef;
}

// Enhancements profile (M2 enhance::Profile, field for field) ---------------------------

export type CategoryId = 'ships' | 'components' | 'ship-weapons' | 'fps-weapons' | 'missiles' | 'missions' | 'obtain';
export type TagKind = 'components' | 'ship-weapons' | 'missiles';
export type TagStyle = 'short' | 'medium' | 'long';
export type TagSeparator = 'dash' | 'none' | 'space' | 'underscore' | 'dot' | 'slash' | 'pipe';
export type TagEnclosing = 'square' | 'round' | 'curly' | 'angle' | 'none';
export type TagPlacement = 'prefix' | 'suffix';

/** One tag kind. `elements`: parts that are on, in output order. `styles`: non-default (medium) styles. */
export interface TagKindSettings {
  enabled: boolean;
  elements: string[];
  styles: Record<string, TagStyle>;
  separator: TagSeparator;
  enclosing: TagEnclosing;
  placement: TagPlacement;
}

export interface TagSettings {
  components: TagKindSettings;
  ship_weapons: TagKindSettings;
  missiles: TagKindSettings;
  /** One switch for every kind. */
  short_names: boolean;
  tool_size_tag: boolean;
}

export interface MissionSettings {
  /** [BP], or [BP?] when only some variants can reward one. */
  title_blueprint: boolean;
  title_reputation: boolean;
  title_em: boolean;
  blueprints: boolean;
  item_rewards: boolean;
  details: boolean;
  bullet_tags: boolean;
  owned_marker: boolean;
  max_list: number;
  /** EM tags below: 0 = none, 1-5. Stock mobiGlas text uses 3 (underline) and 4 (blue). */
  header_em: number;
  label_em: number;
  /** [BP] / [BP?] / [BP Owned] while title_em is on. */
  title_bp_em: number;
  title_rep_em: number;
  /** [BP Owned] when every blueprint the contract can award is owned. */
  title_owned: boolean;
  owned_em: number;
  /** Colour the whole owned line, not only [Owned]. */
  owned_line: boolean;
  /** [Missing] on blueprints not owned (needs an owned set). */
  missing_marker: boolean;
  missing_em: number;
  variant_em: number;
  /** Item names under ITEM REWARDS. */
  reward_em: number;
}

/** How much the WHERE TO GET block says. */
export type ObtainMode = 'short' | 'normal' | 'detailed';

export interface Profile {
  preset: Preset | null;
  ships: boolean;
  components: boolean;
  ship_weapons: boolean;
  fps_weapons: boolean;
  missiles: boolean;
  missions: boolean;
  /** "Used in: <contract> · <system>" on items a mission spawns or a contract asks for. */
  quest_hints: boolean;
  /** WHERE TO GET in item and commodity descriptions. */
  obtain: boolean;
  obtain_mode: ObtainMode;
  /** Shop names from the user's UEX files in the block (never prices). */
  obtain_shops: boolean;
  /** The `sell` line: best commodity kiosks, rounded price per SCU, stamped with the UEX fetch day. Off by default, on in Everything. */
  obtain_prices: boolean;
  /** Names (and sibling descriptions) for items whose name (description) key stock lacks; only placeholder stock lines are replaced. */
  missing_names: boolean;
  /** `<category>.<line>` keys from ProfileSchema. */
  lines_off: string[];
  stats_above: boolean;
  /** Plain ASCII; no %, <, ~ or \n. */
  header: string;
  tags: TagSettings;
  mission: MissionSettings;
  branding: boolean;
  /** Appends "Colour test: EM1 … EM5" to the Contracts Manager's "Please select a contract." */
  colour_test: boolean;
  /** "Fix known text bugs" off = every built-in patch id here. */
  patches_disabled: string[];
}

/** One UEX file WHERE TO GET's shop and sell lines read. */
export interface UexFile {
  name: string;
  present: boolean;
  /** RFC 3339 UTC, when present. */
  modified: string | null;
}

/** Shop names and sell prices come from UEX files in `uex_folder`: downloaded by `fetch_uex` (opt-in) or put there by the user. Shops are names only; the sell line has rounded prices. */
export interface ObtainStatus {
  uex_folder: string;
  files: UexFile[];
  /** Item prices and terminals are there. */
  shops_ready: boolean;
  /** Commodity prices and terminals are there. */
  prices_ready: boolean;
}

/** Whether a UEX API token is saved, and where. Never the token itself. */
export interface UexTokenStatus {
  set: boolean;
  store: TokenStoreKind | null;
}

/** Save (`token`) or remove (`null` or empty) the UEX API token. */
export interface UexTokenRequest {
  token: string | null;
}

/** `missing`: the channel's loot data is not cached yet (Read game data). */
export type LootDataState = 'ready' | 'missing';

/** Which Loot list: loot areas, or items that drop in one. */
export type LootView = 'areas' | 'items';

/** How the item is there: rolled by a loot table, held by a rack, lying loose, or worn or wielded by an NPC (its loadout). */
export type LootDropKind = 'loot' | 'fixed' | 'loose' | 'worn';

/** How a detail's drops are grouped: by container ("Where in it"), by the top level of the other list's tree (an area's kind of place, an item's category), or one flat group. */
export type LootGroupBy = 'container' | 'place' | 'none';

/** `target`: the channel whose cached loot data is read (null: `missing`); nothing is extracted. */
export interface LootListRequest {
  target: ChannelRef | null;
  view: LootView;
}

export interface LootDetailRequest {
  target: ChannelRef | null;
  view: LootView;
  /** A `LootSummary.id` of that view. */
  id: string;
  /** Only the drops holding every word (name, path, container, rarity, id), over every drop. */
  filter?: string | null;
  /** null: the view's default (areas by container, items by place). */
  group_by?: LootGroupBy | null;
  /** Group keys whose rows to answer (opening a group); null: the default ones (the first and a few small ones, or a filter's matches). */
  groups?: string[] | null;
}

/** One row of a Loot list: an area or an item (summary only). */
export interface LootSummary {
  /** The area's slot preset path, or the item's class name. */
  id: string;
  name: string;
  /** Kind of place ("Caves") or item category ("FPS weapons"). */
  group: string;
  /** Its place in the list's tree, top first, without the row (["Caves", "Occupied"]); empty for a top-level row. */
  path: string[];
  /** Areas: items that can drop there. Items: areas it drops in. */
  count: number;
  /** Items: how many of `count` are NPC bodies ("NPCs (body loot)"); areas: 0. */
  npcs: number;
  /** Items: how many of `count` are NPC worn-gear rows ("NPCs (worn gear)"); areas: 0. */
  worn: number;
  /** Best rough odds per spawned box (or NPC body), 0 to 1. */
  best: number;
}

/** `loot_list`: areas by group (Other last), path then name, or items by name. */
export interface LootList {
  state: LootDataState;
  view: LootView;
  /** The build the data is of ("LIVE 4.10.201.48085"). */
  build: string | null;
  /** Whether the NPC worn-gear data is read: `missing` lists no "NPCs (worn gear)" rows until Read game data adds it. */
  gear: LootDataState;
  rows: LootSummary[];
}

/** One drop: an item of an area, or an area of an item. */
export interface LootDrop {
  id: string;
  name: string;
  /** Where the row sits in its list's tree ("Caves › Occupied", "FPS weapons › Rifles"). */
  group: string;
  /** Rough odds one spawned box holds the item, 0 to 1. */
  odds: number;
  /** What the spot is ("Large weapons crate", "Lying loose"). */
  container: string;
  rarity: string | null;
  kind: LootDropKind;
  /** What the odds depend on, when they do (worn gear of a gated loadout group: "if AI/NpcClass/Heavy"). */
  note: string | null;
}

/** A link to another row of the areas list. */
export interface LootLink {
  id: string;
  name: string;
}

/** `WearRange`: how worn an NPC's gear spawns, 0 (new) to 1. */
export interface Wear {
  min: number;
  max: number;
}

/** What an NPC row of the areas list says beyond its drops. */
export interface NpcInfo {
  /** Worn gear: the actor classes the row stands for (same name and gear). */
  classes: string[];
  /** Worn gear: the faction ("Criminals"). */
  faction: string | null;
  wear: Wear | null;
  /** Worn gear: the body-loot row of its loot table. */
  body_loot: LootLink | null;
  /** Body loot: the worn-gear rows of the NPCs whose bodies roll this table. */
  worn_gear: LootLink[];
}

/** One group of a detail: its header over every matching drop, its rows when asked for. */
export interface LootGroup {
  /** What `groups` takes: the label, or "all" ungrouped. */
  key: string;
  /** "Weapons rack", "Caves", "FPS weapons"; "All drops" ungrouped. */
  label: string;
  /** Drops in the group matching the filter. */
  count: number;
  /** Best rough odds per box in the group. */
  best: number;
  /** Likeliest first, at most 500 (2000 ungrouped); null when not asked for. */
  rows: LootDrop[] | null;
}

/** `loot_detail`: groups best odds first, then by name; `total` counts the drops matching the filter, `all` every drop. */
export interface LootDetail {
  state: LootDataState;
  view: LootView;
  /** null when nothing has that id. */
  entry: LootSummary | null;
  /** Items: the game record (Explorer link). */
  record: ItemRecord | null;
  /** NPC rows: classes, faction, condition and the links between body loot and worn gear. */
  npc: NpcInfo | null;
  group_by: LootGroupBy;
  groups: LootGroup[];
  total: number;
  all: number;
}

/** Readable bucket of what a shop sells (`starorg_mcp::shops::ShopCategory`), in display order. */
export type ShopCategory = 'fps-weapons' | 'fps-attachments' | 'armour' | 'ship-weapons' | 'missiles' | 'ship-components' | 'mining' | 'consumables' | 'other';

/** `missing`: one or both UEX files are not there (Settings -> Network -> Fetch now). */
export type ShopDataState = 'ready' | 'missing';

/** `target`: the channel whose cached item data names rows and links them to records; nothing is extracted. */
export interface ShopInventoryRequest {
  target: ChannelRef | null;
}

export interface ShopItemsRequest {
  target: ChannelRef | null;
  /** A `ShopSummary.id`. */
  terminal: number;
}

export interface CategoryCount {
  category: ShopCategory;
  count: number;
}

/** One shop (a UEX item terminal). */
export interface ShopSummary {
  /** UEX terminal id. */
  id: number;
  name: string;
  /** The place without the system ("Grim HEX, Yela, Crusader"). */
  location: string;
  system: string | null;
  /** Item rows with a price. */
  items: number;
  categories: CategoryCount[];
  not_on_live: boolean;
  /** Newest day UEX confirmed a row (YYYY-MM-DD). */
  updated: string | null;
}

export interface ShopPlaceGroup {
  name: string;
  shops: ShopSummary[];
}

export interface ShopSystem {
  name: string;
  locations: ShopPlaceGroup[];
}

/** Every shop by system, then place. */
export interface ShopDirectory {
  state: ShopDataState;
  folder: string | null;
  /** When the UEX files were fetched (ISO 8601). */
  fetched: string | null;
  systems: ShopSystem[];
  shop_count: number;
  item_count: number;
  /** The build whose item data linked rows ("LIVE 4.10.201.48085"); null = UEX names, no links. */
  linked: string | null;
  /** Ship guns, missiles and components (game-ship-components) link too; false until the channel's game data is read. */
  ship_items: boolean;
}

/** The game record a row is: the Explorer opens it by GUID. */
export interface ItemRecord {
  guid: string;
  class_name: string;
}

export interface ShopItemRow {
  /** The game's name when linked, else UEX's. */
  name: string;
  /** UEX's name when it differs. */
  uex_name: string | null;
  category: ShopCategory;
  price_buy: number | null;
  /** What the shop pays for it. */
  price_sell: number | null;
  updated: string | null;
  /** Not confirmed in 30 days. */
  stale: boolean;
  record: ItemRecord | null;
}

/** One shop's rows, by category then name. */
export interface ShopItems {
  state: ShopDataState;
  folder: string | null;
  fetched: string | null;
  shop: ShopSummary | null;
  items: ShopItemRow[];
  linked: string | null;
}

/** The item search: items whose game or UEX name matches `query`, and the shops of the item `key` names (else the best match). */
export interface ShopItemSearchRequest {
  target: ChannelRef | null;
  query: string;
  /** A `ShopItemMatch.key`; null = the best match. */
  key: string | null;
}

/** One item across every shop that lists it. */
export interface ShopItemMatch {
  /** The record's class name when linked, else `uex:` and the lower-case name. */
  key: string;
  name: string;
  uex_name: string | null;
  category: ShopCategory;
  record: ItemRecord | null;
  /** Shops with a buy price (a shop with two rows of it counts once). */
  sold_at: number;
  /** Shops that pay for it. */
  bought_at: number;
  min_buy: number | null;
  max_sell: number | null;
}

/** One shop's row for an item. */
export interface ShopItemOffer {
  /** `ShopSummary.id`. */
  terminal: number;
  shop: string;
  location: string;
  system: string | null;
  not_on_live: boolean;
  price_buy: number | null;
  price_sell: number | null;
  updated: string | null;
  stale: boolean;
}

/** `shop_item_search`: matches best first (at most 40 of `total`), the chosen item and its shops (one offer per shop, all of them) cheapest buy first. */
export interface ShopItemSearch {
  state: ShopDataState;
  folder: string | null;
  fetched: string | null;
  linked: string | null;
  query: string;
  items: ShopItemMatch[];
  total: number;
  item: ShopItemMatch | null;
  offers: ShopItemOffer[];
}

// The Commodities screen (`commodity_list`, `commodity_detail`), typed in
// `starorg_mcp::commodities`: list rows only, one resource in full from a second call.

/** `not-read`: the channel's game data was not read yet; `no-channel`: no install known. */
export type CommodityDataState = 'ready' | 'not-read' | 'no-channel';

export interface CommodityListRequest {
  target: ChannelRef | null;
}

export interface CommodityDetailRequest {
  target: ChannelRef | null;
  /** A `CommoditySummary.id` (`ResourceType.Gold`). */
  id: string;
}

export interface CommoditySummary {
  /** The resource record (`ResourceType.Raw_Quantainium`). */
  id: string;
  name: string;
  /** Top group, readable (`Metal`). */
  group: string;
  /** Groups below it (`Unrefined Ores`); null at the top level. */
  subgroup: string | null;
  volatile: boolean;
  /** A raw form: it refines into something. */
  raw: boolean;
}

/** Whether the UEX commodity files are there (prices are UEX data, player-reported). */
export interface MarketMeta {
  loaded: boolean;
  fetched: string | null;
  folder: string | null;
}

export interface CommodityDirectory {
  state: CommodityDataState;
  /** `LIVE 4.10.201.48085`. */
  build: string | null;
  commodities: CommoditySummary[];
  market: MarketMeta;
}

export interface CommodityRef {
  id: string;
  name: string;
}

export interface CommodityVolatility {
  volatility: number | null;
  decay_per_second: number | null;
}

/** Cargo damage settings; configurations with the same numbers are one row. */
export interface CargoDamage {
  configs: string[];
  /** m/s below which impacts do nothing. */
  minimum_speed: number | null;
  grace_period: number | null;
  explosion_damage: number | null;
  explosion_min_radius: number | null;
  explosion_max_radius: number | null;
}

export interface CargoItem {
  name: string;
  class_name: string;
  kind: string | null;
  capacity_scu: number | null;
}

/** One kiosk price, aUEC per SCU (UEX, player-reported). */
export interface KioskPrice {
  terminal: string;
  location: string;
  system: string | null;
  price: number;
  scu_stock: number | null;
  /** YYYY-MM-DD UEX last confirmed the row. */
  updated: string | null;
  stale: boolean;
}

export interface CommodityMarket {
  loaded: boolean;
  fetched: string | null;
  folder: string | null;
  /** Kiosks paying for it, highest first. */
  sell: KioskPrice[];
  sell_total: number;
  /** Kiosks selling it, lowest first. */
  buy: KioskPrice[];
  buy_total: number;
}

export interface CommodityDetail {
  id: string;
  name: string;
  group: string;
  subgroup: string | null;
  description: string | null;
  density: number | null;
  refined: CommodityRef | null;
  raw_sources: CommodityRef[];
  quality: OreQuality | null;
  volatility: CommodityVolatility | null;
  /** Only for volatile resources. */
  damage: CargoDamage[];
  cargo: CargoItem[];
  /** null: mining data not read, or a refined form (see raw_sources). */
  mined: MinedReport[] | null;
  market: CommodityMarket;
}

export interface CommodityInfo {
  state: CommodityDataState;
  commodity: CommodityDetail | null;
}

// The Commodities screen's Mining tab (`mining_list`, `mining_detail`), typed in
// `starorg_mcp::mining`: list rows only, one mineral in full from a second call.

export interface MiningListRequest {
  target: ChannelRef | null;
}

export interface MiningDetailRequest {
  target: ChannelRef | null;
  /** A `MineralSummary.id`, a commodity id or an ore key. */
  id: string;
}

/** Difficulty grade (an estimate). */
export type MiningGrade = 'easy' | 'medium' | 'hard';
/** The tool a rock takes, from its mining kind. */
export type MiningTool = 'hand' | 'roc' | 'ship' | 'unknown';
/** Where a place's curve comes from; `unknown`: a system override may apply, the place's system is not known. */
export type MiningCurveSource = 'base' | 'override' | 'unknown';

/** The S1 or S2 mining lasers' power range (first beam mode of each `WeaponMining`). */
export interface LaserClass {
  size: number;
  count: number;
  min: number;
  max: number;
  best: string;
}

export interface MineralSummary {
  /** The resource record, else `ore:<key>`. */
  id: string;
  name: string;
  refined: string | null;
  /** "hand", "vehicle", "ship". */
  kinds: string[];
  places: number;
  spots: number;
  /** Best place's estimated share of rolls at 700 or more (of the trusted places when there are any). */
  best_high: number | null;
  /** best_high comes from a place with a quality scale below 1 or a curve not known: it may be lower. */
  best_high_uncertain: boolean;
  difficulty: MiningGrade | null;
  has_override: boolean;
}

export interface MiningDirectory {
  state: CommodityDataState;
  build: string | null;
  minerals: MineralSummary[];
  /** `game-mining` (rock physics) was read. */
  physics: boolean;
  /** `game-weapons` (laser power) was read. */
  lasers_read: boolean;
  lasers: LaserClass[];
  /** `game-mining-spawns` was read (else no body or belt is known yet). */
  spawns_read: boolean;
  /** `game-item-sources` was read (else no spot is known yet). */
  spots_read: boolean;
}

export interface ElementPhysics {
  ore: string;
  resistance: number | null;
  instability: number | null;
  window_midpoint: number | null;
  window_randomness: number | null;
  window_thinness: number | null;
  explosion_multiplier: number | null;
  cluster_factor: number | null;
}

export interface Difficulty {
  grade: MiningGrade;
  points: number;
  max_points: number;
  resistance: number | null;
  instability: number | null;
  window_thinness: number | null;
  missing: string[];
}

export interface ShipOption {
  ship: string;
  laser_count: number;
  laser_size: number;
  best_laser: string | null;
  power: number | null;
  /** power × (1 − resistance). */
  effective: number | null;
}

export interface Recommendation {
  tool: MiningTool;
  label: string;
  reason: string;
  /** Only the ship comparison is an estimate; the tool follows from the rock's kind. */
  estimate: boolean;
  formula: string | null;
  options: ShipOption[];
}

export interface PlaceQuality {
  range: QualityRange;
  source: MiningCurveSource;
  /** The override that applies (`Pyro System`); null: base, or not known (see source). */
  override_location: string | null;
  /** Every quality scale of the row is 1: its shares compare with the others'. */
  full_scale: boolean;
  /** Estimated share of rolls at 700 or more. */
  high: number | null;
  /** Top two shown steps, highest last. */
  top: QualityStep[];
  top_share: number | null;
}

export interface MiningPlace {
  ore: string;
  location: string;
  /** The real star system; null when not known (Lagrange points, clusters). */
  system: string | null;
  body: string | null;
  /** Readable name (`Yela Belt`, `Lagrange A`). */
  place: string;
  rock: string;
  kind: string | null;
  tier: string | null;
  signature: number | null;
  spawn_share: number | null;
  min_percent: number | null;
  max_percent: number | null;
  quality_scales: number[];
  companions: string[];
  /** Always null: rolled at spawn, not in the game files. */
  rock_mass: number | null;
  quality: PlaceQuality;
  difficulty: Difficulty | null;
  recommendation: Recommendation;
}

export interface OverrideUse {
  location: string;
  range: QualityRange;
  /** Spawn rows it applies to (0: it names a place no row is). */
  places: number;
}

export interface MineralDetail {
  id: string;
  name: string;
  commodity: CommodityRef | null;
  refined: CommodityRef | null;
  ores: string[];
  kinds: string[];
  tier: string | null;
  base: QualityRange | null;
  steps: QualityStep[];
  base_high: number | null;
  overrides: OverrideUse[];
  elements: ElementPhysics[];
  places: MiningPlace[];
  spots: MinedSpot[];
}

export interface MiningInfo {
  state: CommodityDataState;
  mineral: MineralDetail | null;
}

export interface LineSchema {
  key: string;
  label: string;
}

export interface CategorySchema {
  id: CategoryId;
  label: string;
  blurb: string;
  lines: LineSchema[];
}

export interface ElementSchema {
  id: string;
  label: string;
}

export interface TagKindSchema {
  kind: TagKind;
  label: string;
  elements: ElementSchema[];
}

export interface ProfileSchema {
  categories: CategorySchema[];
  tag_kinds: TagKindSchema[];
}

export interface CopyProfileRequest {
  from: ChannelRef;
  to: ChannelRef;
}

export interface SaveProfileRequest {
  target: ChannelRef;
  profile: Profile;
}

// Strings -------------------------------------------------------------------------------

export type StringSource = 'unmodified' | 'enhanced' | 'patched' | 'edited';
export type StringSort = 'key' | 'key-desc' | 'category' | 'source' | 'group' | 'owner';
/** Which field of its owner a key is. */
export type OwnerField = 'name' | 'short-name' | 'description' | 'title' | 'career' | 'role';
/** What owns a key; `named` = the text of a sibling key, `key` = nothing (readable key). */
export type OwnerKind = 'ship' | 'ground-vehicle' | 'component' | 'ship-weapon' | 'fps-weapon' | 'missile' | 'mission' | 'reputation' | 'manufacturer' | 'named' | 'key';

/** The thing a key belongs to, in words (from the cached GameData; no Developer mode). */
export interface KeyOwner {
  name: string;
  /** `null` for `kind: 'key'`. */
  field: OwnerField | null;
  kind: OwnerKind;
  /** Manufacturer display name. */
  detail: string | null;
}
/** What the Strings group tree shows (addendum 8.4). */
export type GroupBy = 'none' | 'type' | 'manufacturer' | 'size';
/** `approximate`: the loc-reference pass has not run for this build yet. */
export type GroupState = 'exact' | 'approximate';

export interface StringsQuery {
  target: ChannelRef;
  search: string;
  sources: StringSource[];
  categories: string[];
  changed_only: boolean;
  sort: StringSort;
  offset: number;
  limit: number;
  /** Group ids; a parent selects its children. Empty = all. */
  groups?: string[];
  /** Manufacturer codes; `none` = rows without one. */
  manufacturers?: string[];
  /** Sizes 1-12; 0 = rows without one. */
  sizes?: number[];
  group_by?: GroupBy;
}

export interface StringRow {
  key: string;
  original: string;
  result: string;
  source: StringSource;
  category: string;
  /** Leaf id of the type hierarchy, e.g. `components/coolers`. */
  group: string;
  group_label: string;
  manufacturer: string | null;
  size: number | null;
  /** Primary owner; always set on page rows (a `kind: 'key'` fallback when nothing owns it). */
  owner: KeyOwner | null;
  /** Distinct owners of the key; above 1 the grid lists them. */
  owners: number;
}

/** A node of the group tree. Type: group id; manufacturer: code or `none`; size: `"1"`..`"12"` or `none`. */
export interface GroupNode {
  id: string;
  parent: string | null;
  label: string;
  count: number;
  enhanced: number;
  edited: number;
}

export interface FacetCount {
  id: string;
  count: number;
}

export interface StringsPage {
  total: number;
  all: number;
  offset: number;
  rows: StringRow[];
  /** `sources`, `categories` and `groups`: first page (`offset` 0) only, empty after. */
  sources: FacetCount[];
  categories: FacetCount[];
  groups: GroupNode[];
  group_state: GroupState;
}

export type Layer = 'stock' | 'generated' | 'patch' | 'override';

export interface Provenance {
  layer: Layer;
  by: string;
  text: string;
  winner: boolean;
  note: string | null;
}

export interface StringDetail {
  row: StringRow;
  provenance: Provenance[];
  override_value: string | null;
  /** Labels of other groups whose records also use the key. */
  also_used_by: string[];
  /** Every owner of the key, primary first. */
  owners: KeyOwner[];
}

export interface KeyRequest {
  target: ChannelRef;
  key: string;
}

export interface OverrideRequest {
  target: ChannelRef;
  key: string;
  value: string;
}

/** M1 `validate::Rule::name()`. */
export type OverrideRule = 'control-character' | 'added-em-unbalanced' | 'em-signature' | 'variable-tokens' | 'format-specifiers' | 'format-key' | 'percent-rule' | 'too-long' | 'raw-loc-key' | 'placeholder-text';

export interface OverrideResult {
  accepted: boolean;
  rule: OverrideRule | null;
  message: string | null;
  detail: StringDetail | null;
}

export interface OverrideEntry {
  key: string;
  value: string;
  original: string;
}

// Patches -------------------------------------------------------------------------------

export type PatchState = 'applies' | 'not-needed' | 'target-changed' | 'nothing-to-add' | 'missing' | 'out-of-range' | 'conflict' | 'disabled' | 'no-data' | 'retired' | 'checking';
export type PatchOrigin = 'built-in' | 'user';

export interface PatchView {
  id: string;
  title: string;
  description: string;
  keys: number;
  origin: PatchOrigin;
  state: PatchState;
  enabled: boolean;
}

export interface PatchRef {
  target: ChannelRef;
  id: string;
}

export interface PatchExample {
  key: string;
  before: string;
  after: string;
}

export interface PatchDetail {
  patch: PatchView;
  kind: string;
  range: string | null;
  reason: string;
  examples: PatchExample[];
}

export interface PatchToggle {
  target: ChannelRef;
  id: string;
  enabled: boolean;
}

export type LocOp = 'set' | 'append' | 'prepend' | 'replace';

export interface NewPatch {
  target: ChannelRef;
  title: string;
  description: string;
  key: string;
  op: LocOp;
  find: string | null;
  value: string;
}

export interface PatchExport {
  file_name: string;
  text: string;
}

// Activity ------------------------------------------------------------------------------

export type LogLevel = 'info' | 'warn' | 'error';

export interface LogEntry {
  id: number;
  time: string;
  level: LogLevel;
  job: JobKind | null;
  channel: string | null;
  message: string;
}

// Settings ------------------------------------------------------------------------------

export type Theme = 'system' | 'dark' | 'light';
export type Accent = 'cyan' | 'green' | 'amber' | 'orange' | 'rose' | 'violet';
export type Palette = 'void' | 'black';

export interface Appearance {
  theme: Theme;
  accent: Accent;
  palette: Palette;
  reduce_effects: boolean;
}

export interface NetworkSettings {
  check_updates: boolean;
  /** "Download published LIVE builds for Diff" (Developer mode only; never used by apply). */
  published_dumps: boolean;
  starorg: boolean;
  /** "Download UEX market data": `fetch_uex` may download the two UEX files from api.uexcorp.space. */
  uex: boolean;
}

export interface Settings {
  appearance: Appearance;
  network: NetworkSettings;
  /** Settings -> Game data. */
  data: DataSettings;
  default_channel: string | null;
  safe_gpu: boolean;
  advanced: AdvancedSettings;
  /** Settings -> StarOrg Tools; stored in config.toml [sync] (shared with the CLI and hooks). */
  starorg: StarOrgSettings;
}

/** Environment and "What to sync". Defaults: production, Game.log import on, the rest off. */
export interface StarOrgSettings {
  environment: StarOrgEnv;
  /** Read Game.log for new blueprints (app, CLI, post-exit hook). Off: no log is read. */
  import_game_log: boolean;
  push_blueprints: boolean;
  pull_blueprints: boolean;
  pull_fleet: boolean;
  /** Sync by itself after the game exits; off = only when asked. */
  auto_sync: boolean;
}

/** Settings > Game data. */
export interface DataSettings {
  /**
   * "Download prebuilt game data from voidput/sc-gamedata-dumps when it matches your game build" (on by default).
   * Only datasets of the exact same build, checked by checksum and shape; anything else, offline included, is read
   * from Data.p4k. Never the game text.
   */
  prebuilt_dumps: boolean;
}

/** Settings > Advanced (addendum 7.7). */
export interface AdvancedSettings {
  /** Explorer, Datasets and Diff screens. Off by default. */
  developer_mode: boolean;
}

// Characters and blueprints -------------------------------------------------------------

export type CharacterMode = 'auto' | 'manual' | 'union';

export interface CharacterView {
  id: string;
  name: string;
  last_seen: string | null;
  owned: number;
}

export interface ActiveCharacter {
  mode: CharacterMode;
  character: string | null;
  /** Who auto resolves to: the newest session's character, or the most recent one that owns
   * something when the newest owns nothing. */
  resolved: string | null;
  /** Auto passed over the newest character (it owns nothing): its id, for a hint. */
  fallback_from: string | null;
  /** Auto: another character (not merged with the one used) owns far more; its id. */
  larger: string | null;
}

export interface CharactersView {
  channel: string;
  characters: CharacterView[];
  active: ActiveCharacter;
  unattributed: number;
  last_scan: string | null;
  /** Characters of different accounts share a name: kept apart, the user picks. */
  notice: string | null;
  /** File name of the last Smart Citizen export imported on this channel. */
  last_import: string | null;
}

export interface SetActiveCharacter {
  target: ChannelRef;
  mode: CharacterMode;
  character: string | null;
}

export interface OwnedBlueprint {
  name: string;
  ambiguous: boolean;
  favorite: boolean;
  sources: string[];
  acquired: string | null;
}

export interface CharacterRequest {
  target: ChannelRef;
  character: string;
}

// Blueprint finder (addendum §7.19) --------------------------------------------------------

export type BlueprintKind = 'ship-component' | 'ship-weapon' | 'fps-weapon' | 'armor' | 'fps-ammo' | 'mission-item' | 'other';

export interface BlueprintFinderRequest {
  install: string;
  /** Empty = every channel of the install. */
  channels: string[];
}

export interface BlueprintFinderChannel {
  channel: string;
  /** Blueprints in this build; null = game data not read yet. */
  catalog: number | null;
  characters: number;
  owned: number;
  /** Set files that could not be read or placed: not shown. */
  skipped: number;
}

export interface BlueprintFinderCharacter {
  /** geid (personal data: shown, never uploaded), or `unattributed`. */
  id: string;
  name: string;
  last_seen: string | null;
  channels: string[];
}

export interface BlueprintOwner {
  channel: string;
  character: string;
  sources: string[];
  acquired: string | null;
  favorite: boolean;
  ambiguous: boolean;
}

export interface BlueprintRow {
  /** Blueprint id (lower case) or `name:<normalised>` for a name no single blueprint has. */
  key: string;
  name: string;
  record: string | null;
  kind: BlueprintKind;
  type_label: string | null;
  class: string | null;
  manufacturer: string | null;
  manufacturer_name: string | null;
  size: number | null;
  grade: string | null;
  channels: string[];
  /** Name rows: keys of the blueprints the name could be; empty = in no read build. */
  candidates: string[];
  owners: BlueprintOwner[];
  /** Owners of an ambiguous name that could be this blueprint ("maybe owned"). */
  maybe: BlueprintOwner[];
  /** Missions that can award it over every channel, sure ones first; each entry names the channels whose builds have it. */
  earn: BlueprintEarn[];
  /** Channels whose build's contracts were read for it: elsewhere its missions are not known (name rows: none). */
  earn_read: string[];
}

/** A mission (the contracts sharing one title) that can award blueprints. */
export interface BlueprintMission {
  /** In-game title (`~mission(Location)` reads `[Location]`); null = no readable title. */
  title: string | null;
  /** Contract debug name (`Name (+2 more)` for a shared title). */
  record: string;
  /** Contract generator record short name (an Explorer search finds it). */
  generator: string;
  /** Faction reputation records its success rewards pay, first seen first. */
  factions: string[];
  /** Those factions readable (one per `factions` entry): the game's name, else the record in words. */
  reputation: string[];
  /** Star systems of its locality prerequisites; empty = not known. */
  systems: string[];
  /** MISSION DETAILS `Type:` words (`Chain`, `Once only`), each when every contract is one. */
  kinds: string[];
  /** MISSION DETAILS `Requires:` standing. */
  requires: string | null;
  /** Contracts with this title. */
  contracts: number;
}

/** How sure one blueprint is from one mission: a sure pool of just it, a sure pool of several, or by chance (some variants only included). */
export type BlueprintAward = 'guaranteed' | 'guaranteed-pool' | 'chance';

/** How one mission awards one blueprint. */
export interface BlueprintEarn {
  /** Index in `BlueprintFinder.missions`. */
  mission: number;
  award: BlueprintAward;
  /** MISSION DETAILS `Blueprint reward:` wording (the pool's odds, as in game): `Guaranteed`, `Some variants only`, `25%`. */
  odds: string;
  /** Contracts of the title that can award it (of the mission's `contracts`). */
  variants: number;
  /** Blueprints in its pool: `6`, or `4–6`. */
  pool: string;
  /** Its share of the pool by weight (`17%`); null when the weights are not known. */
  pick: string | null;
  /** Channels whose builds have this entry. */
  channels: string[];
}

export interface BlueprintFinder {
  channels: BlueprintFinderChannel[];
  characters: BlueprintFinderCharacter[];
  rows: BlueprintRow[];
  /** Every mission the rows' `earn` names, each once (rows refer to them by index). */
  missions: BlueprintMission[];
}

export interface ImportRequest {
  target: ChannelRef;
  character: string;
  dry_run: boolean;
  /** Read the file the dry run just read, else the last export imported here (no picker). */
  reuse_last?: boolean;
  /** Re-import: drop what an earlier import added and this export no longer has. */
  replace?: boolean;
}

export interface AmbiguousName {
  name: string;
  candidates: string[];
}

export interface ImportReport {
  file: string;
  exported_at: string | null;
  dry_run: boolean;
  total: number;
  not_completed: number;
  duplicates: number;
  imported: string[];
  known: string[];
  unmatched: string[];
  ambiguous: AmbiguousName[];
  /** Game data was not read for this channel, so nothing could match. */
  empty_catalog: boolean;
  /** `replace`: entries of an earlier import this export no longer has. */
  removed: string[];
}

/** Re-read every log of a channel from the start. */
export interface RescanRequest {
  target: ChannelRef;
  dry_run: boolean;
  /** Start every set empty: only what the logs show is kept. */
  replace?: boolean;
}

export interface CharacterChange {
  /** geid; null for blueprints seen before any login. */
  id: string | null;
  name: string;
  before: number;
  after: number;
  added: string[];
  removed: string[];
}

export interface RescanReport {
  channel: string;
  dry_run: boolean;
  replace: boolean;
  files_read: number;
  /** Characters merged into another (same account and name, new geid). */
  merged: number;
  /** A log skipped (too large) or not readable, and similar. */
  warnings: string[];
  /** Replace with a log that could not be read: the confirm is refused with this reason. */
  replace_refused: string | null;
  characters: CharacterChange[];
}

// Fleet import and ship names (addendum §6) ---------------------------------------------

export type NameStyle = 'none' | 'numeric' | 'group-letter' | 'symbol';
export type RestMode = 'stock' | 'numbered';
/** profile.toml `[ship_names] use`: the shared list, this channel's own, or off. */
export type ShipNamesScope = 'shared' | 'channel' | 'off';
export type FleetFormat = 'xplor-json' | 'xplor-csv' | 'text';

export interface ShipGroup {
  id: string;
  name: string;
}

/** One `[[ship]]`: list order is the order. */
export interface ShipEntry {
  hull: string;
  group: string | null;
  /** Rename of the long name. */
  name: string | null;
  /** Rename of the short name; defaults to `name`. */
  short: string | null;
}

export interface ShipNamesSettings {
  enabled: boolean;
  style: NameStyle;
  /** ' ', '-' or '. '. */
  separator: string;
  /** One of ! # * + - . */
  symbol: string;
  rest: RestMode;
  prefix_short: boolean;
  groups: ShipGroup[];
  ships: ShipEntry[];
}

export interface FleetHull {
  /** Hull id = the long name key. */
  hull: string;
  name: string;
  short: string | null;
  maker: string | null;
  ground: boolean;
  /** Other records (AI, PU variants) that show the same name. */
  npc_shared: number;
  /** Cargo grid SCU of the stock loadout; null when not known. */
  cargo_scu: number | null;
}

export interface OwnedHull {
  hull: string;
  count: number;
  /** Custom names from the export: rename suggestions. */
  names: string[];
}

export interface PendingShip {
  name: string;
  count: number;
  maker: string | null;
  names: string[];
  /** Set for ambiguous rows. */
  candidates: string[];
}

export interface HullOverride {
  hull: string;
  long: string | null;
  short: string | null;
}

export interface FleetViewRequest {
  target: ChannelRef;
  /** Read the game data when it is not cached for this build (slow). */
  read_game_data: boolean;
}

export interface FleetView {
  channel: string;
  catalog_ready: boolean;
  catalog: FleetHull[];
  owned: OwnedHull[];
  pending: PendingShip[];
  not_in_game: string[];
  imported_at: string | null;
  format: FleetFormat | null;
  scope: ShipNamesScope;
  settings: ShipNamesSettings;
  /** The shared ship-names.toml (what "All channels" switches back to). */
  shared_settings: ShipNamesSettings;
  overrides: HullOverride[];
  warnings: string[];
  invalid: boolean;
  fleet_file: string;
  settings_file: string;
}

export type FleetSourceKind = 'file' | 'text' | 'last' | 'star-org';
/** `star-org`: the StarOrg Tools hangar (needs "Pull fleet/hangar" and a sign-in). */
export type FleetSource = { kind: 'file' } | { kind: 'text'; text: string } | { kind: 'last' } | { kind: 'star-org' };

export interface FleetImportRequest {
  target: ChannelRef;
  source: FleetSource;
  dry_run: boolean;
  merge: boolean;
  /** "Remove all ships first": replace the fleet and drop My ships entries the import lacks. Not with `merge`. */
  prune?: boolean;
}

export interface ImportedHull {
  hull: string;
  name: string;
  count: number;
  names: string[];
  new: boolean;
}

export interface FleetImportReport {
  file: string | null;
  format: FleetFormat;
  dry_run: boolean;
  merge: boolean;
  total: number;
  skipped: number;
  hulls: ImportedHull[];
  unmatched: PendingShip[];
  ambiguous: PendingShip[];
  not_in_game: number;
  empty_catalog: boolean;
  added_to_order: number;
  prune: boolean;
  /** With `prune`: ships dropped from the shared My ships list (would be, on a dry run). */
  removed_from_order: RemovedShip[];
}

export interface RemovedShip {
  hull: string;
  name: string;
  /** Its rename in My ships, lost with it. */
  rename: string | null;
}

export interface FleetMatchRequest {
  target: ChannelRef;
  name: string;
  /** null = "Not in the game". */
  hull: string | null;
}

export interface SaveShipNamesRequest {
  target: ChannelRef;
  scope: ShipNamesScope;
  settings: ShipNamesSettings;
}

// StarOrg Tools -------------------------------------------------------------------------

export type StarOrgEnv = 'production' | 'dev';

export interface SyncCharacter {
  id: string;
  name: string;
  ticked: boolean;
  owned: number;
}

/** One org from the account card. `blueprints`: the org has blueprint tracking. */
export interface StarOrgGuild {
  id: string;
  name: string;
  blueprints: boolean;
}

export type TokenStoreKind = 'keyring' | 'file';

export interface StarOrgStatus {
  enabled: boolean;
  connected: boolean;
  environment: StarOrgEnv;
  /** starorg.tools or dev.starorg.tools */
  host: string;
  user: string | null;
  guild: string | null;
  guild_id: string | null;
  guilds: StarOrgGuild[];
  expires: string | null;
  /** Signed in before, but the token expired or was revoked. */
  expired: boolean;
  token_store: TokenStoreKind | null;
  last_sync: string | null;
  sync_characters: SyncCharacter[];
  push_count: number;
}

export interface StarOrgGuildRequest {
  channel: string;
  guild: string;
}

/** "Sync now" / the run after game exit: pull, push, fleet as the switches allow. */
export interface SyncRun {
  channel: string;
  reports: SyncReport[];
  fleet: FleetImportReport | null;
  skipped: string[];
  time: string;
}

export interface SyncCharactersRequest {
  channel: string;
  ticked: string[];
}

export interface StarOrgPullRequest {
  channel: string;
  character: string;
}

export type SyncDirection = 'pull' | 'push';

export interface SyncReport {
  direction: SyncDirection;
  channel: string;
  guild: string | null;
  character: string | null;
  added: string[];
  already: number;
  unmatched: string[];
  sent: number;
  time: string;
}

// Hooks and doctor ----------------------------------------------------------------------

export type HookKind = 'prelaunch' | 'postexit';
export type HookStateName = 'absent' | 'current' | 'outdated' | 'broken' | 'missing';

export type HookState =
  | { state: 'absent' }
  | { state: 'current' }
  | { state: 'outdated' }
  | { state: 'broken'; why: string }
  | { state: 'missing' };

export interface HookScript {
  kind: HookKind;
  path: string | null;
  is_link: boolean;
  status: HookState;
}

export interface HooksStatus {
  supported: boolean;
  cli_on_path: boolean;
  scripts: HookScript[];
}

export type CheckLevel = 'ok' | 'info' | 'warn' | 'error';

export interface DoctorCheck {
  level: CheckLevel;
  label: string;
  detail: string | null;
}

export interface DoctorSection {
  title: string;
  checks: DoctorCheck[];
}

export interface DoctorReport {
  sections: DoctorSection[];
}

/** Doctor + recent log with paths, user names and character ids replaced. */
export interface Diagnostics {
  file_name: string;
  text: string;
  redacted: number;
}

// Updates (spec 11, 12; docs/updates.md). Only used when network.check_updates is on --------

export interface UpdatePrefs {
  /** owner/name on github.com. */
  repo: string;
  include_prereleases: boolean;
}

/** `store`: the Microsoft Store build; the Store updates it, the app never checks or installs. */
export type UpdateInstall = 'in-app' | 'download-only' | 'store';

export type UpdateProblem = 'not-visible' | 'rate-limited' | 'offline' | 'bad-response';

export type NoteKind = 'heading' | 'item' | 'text';

/** Plain text (sanitised in Rust); render as text, never as HTML. */
export interface NoteBlock {
  kind: NoteKind;
  text: string;
}

export interface ReleaseView {
  version: string;
  tag: string;
  title: string;
  url: string;
  published: string | null;
  prerelease: boolean;
  notes: NoteBlock[];
  notes_truncated: boolean;
}

export type UpdateCheck =
  | { state: 'not-checked' }
  | { state: 'up-to-date' }
  | { state: 'available'; release: ReleaseView }
  | { state: 'failed'; problem: UpdateProblem; message: string };

export type UpdateCheckState = 'not-checked' | 'up-to-date' | 'available' | 'failed';

export interface UpdateStatus {
  enabled: boolean;
  current_version: string;
  prefs: UpdatePrefs;
  last_checked: string | null;
  check: UpdateCheck;
  skipped_version: string | null;
  install: UpdateInstall;
  install_note: string;
}

export interface UpdateCheckRequest {
  force: boolean;
}

/** Footer / About links; the URL for each lives in Rust (`ExternalLink::url`). */
export type ExternalLink = 'support' | 'donate' | 'patreon' | 'uex';

// Developer mode (addendum 7): dev.ts ---------------------------------------------------
export type * from './dev';
export type * from './assets';
export type * from './telemetry';
// Experimental: local MCP server (docs/mcp.md) --------------------------------------------

/** One HTTP request the local MCP server answered. */
export interface McpRequest {
  time: string;
  method: string | null;
  tool: string | null;
  /** 401 = no or wrong token. */
  status: number;
  ok: boolean;
  millis: number;
}

/** Settings -> Experimental -> "Local MCP server". Off at every start of the app. */
export interface McpStatus {
  running: boolean;
  url: string | null;
  /** Bearer token of this run (new at every start). */
  token: string | null;
  started: string | null;
  error: string | null;
  channels: string[];
  claude_code: string | null;
  claude_desktop: string | null;
  stdio: string;
  /** Newest first, at most 100. */
  requests: McpRequest[];
}

export interface McpToggle {
  enabled: boolean;
}

/** How the memory cap is held on this machine (addendum 7.18). */
export type MemoryEnforcement = 'cgroup' | 'rlimit' | 'watchdog';

export interface MemoryEstimate {
  id: string;
  label: string;
  bytes: number;
  fits: boolean;
}

/** Settings > Advanced > Memory limit of the Developer mode and MCP workers. */
export interface MemoryStatus {
  total_bytes: number | null;
  available_bytes: number | null;
  limit_gb: number;
  is_default: boolean;
  min_gb: number;
  max_gb: number;
  step_gb: number;
  default_gb: number;
  enforcement: MemoryEnforcement;
  /** A worker is stopped at this share of the limit; `fits` is measured against it. */
  stop_percent: number;
  /** Smallest first. */
  estimates: MemoryEstimate[];
  /** Set on machines with too little RAM for a comfortable limit. */
  note: string | null;
}

export interface MemoryLimitRequest {
  /** null = back to the default. */
  gb: number | null;
}

export type StorageAreaId = 'game-data' | 'where-to-get' | 'uex' | 'asset-exports' | 'temporary' | 'telemetry' | 'telemetry-recordings' | 'telemetry-sessions' | 'backups';

export interface StorageArea {
  id: StorageAreaId;
  label: string;
  /** The folder shown for it. */
  path: string;
  bytes: number;
  files: number;
  /** Counted toward the cache folder's cap. */
  in_cache: boolean;
}

/** Settings > Storage: what the app keeps on this PC and the cache folder's cap. */
export interface StorageStatus {
  cache_path: string;
  cache_bytes: number;
  /** Least recently used goes first above it (`[storage] cache_max_mb`, default 1 GiB). */
  cap_bytes: number;
  /** Cache folder + backups (+ the asset cache when it is in a folder of its own). */
  total_bytes: number;
  /** Game data, WHERE TO GET, UEX, asset exports, temporary, backups. */
  areas: StorageArea[];
  asset_cap_bytes: number;
  backup_keep_newest: number;
  backup_keep_builds: number;
  /** Why "Clear asset cache" is refused right now, else null. */
  asset_busy: string | null;
}

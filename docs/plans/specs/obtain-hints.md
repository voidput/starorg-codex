---
title: "Obtain hints: where to get it, in item and commodity descriptions"
tags: [docs, plans, specs]
status: active
updated: 2026-11-22
---

# Obtain hints: where to get it, in item and commodity descriptions

A new description enhancement: a **WHERE TO GET** block appended to item and commodity
descriptions in `global.ini`, from the same data as the Explorer Sources card
([PR #47](https://github.com/voidput/starorg-codex/pull/47), [#48](https://github.com/voidput/starorg-codex/pull/48)).
Names are never touched.

## Decisions

- **Data: reuse the card's datasets** (user choice 2026-11-21). Apply runs the
  `starorg-gamedata` dataset workers the MCP server and the card use (item-tags, loot,
  item-sources, blueprints, wikelo-trades, mining-spawns, commodities), cached per build in
  the same folder. First apply per build pays their extraction (about 1-2 min, each worker
  up to ~1.4 GB, behind the memory-limit gate); later applies hit the cache. One logic for
  card, MCP and in-game text, no second parser.
- **Placement: after the stats block** (user choice 2026-11-21). The Engine keeps one edit per
  stock line today (`Engine::offer`, later categories are `OwnedByOther`). It gains an
  extend step: a later category may add to an existing edit's suffix, re-checked by
  `check_generated`. Order: stock text, stats block (or stats above with `stats_above`),
  then WHERE TO GET.
- **Shops only from UEX files**, names only, never prices (prices go stale between applies).
  Off by default. Originally nothing was downloaded; since `feat/uex-fetch` the app can fetch
  both files itself, opt-in (see Open).
- **Commodity sell prices in game after all** (user choice 2026-11-22, reverses "never
  prices in-game" for this one line): a `sell` line with the top 3 commodity kiosks that pay
  most. Mitigations for prices going stale: its own switch `obtain_prices`, off by default
  and on only in Everything; the price rounded to two significant digits (`~6.2k aUEC/SCU`);
  the line stamped with the day the prices were fetched (`(UEX, 21 Nov)`); kiosks that pay
  nothing or that UEX marks off LIVE left out. Shop lines stay names only. See *As built
  (phase 4)*.
- **Odds only in Detailed** (they are estimates).

## Data path

`starorg-core` names none of the gamedata types. Core gets a neutral model:

```rust
// crates/starorg-core/src/model/obtain.rs
pub struct ObtainHints { pub items: BTreeMap<String /* class, lower */, ItemObtain> }
pub struct ItemObtain {
    pub shops: Vec<Place>,            // terminal + short place, cheapest first
    pub loot: Vec<LootHint>,          // contents, size, rarity, place, odds, best first
    pub loot_total: usize,
    pub mined: Vec<MinedHint>,        // body or spot, kind, percent range
    pub quality: Option<QualityHint>, // min, max, mean, stddev, overrides
}
```

- Filled in `starorg-mcp` (it has the datasets and `Tools::sources`) by a new
  `Tools::obtain_hints(channel)` over every item and resource, and passed to
  `enhance::generate_with` as a new optional input. Craft and contract rewards keep coming
  from apply's own `GameData` (`blueprints`, `contracts`, `blueprint_pools`).
- **Loc keys.** The datasets carry class names, not loc keys. Apply's extraction sees every
  `EntityClassDefinition`, so a small pass in `starorg-data` adds
  `GameData.item_descs: class -> LocKeys` (bump `GAMEDATA_SCHEMA` to 4). Commodities: the
  `commodities` domain gains the raw `descriptionKey` of each `ResourceType` (and the cargo
  entity's) beside the localized text.
- A missing dataset (memory gate, extraction error) drops only the lines it feeds, with a
  warning in the apply report.

### As built (phase 2)

- **Desc keys live in `GameData`, not in a dataset.** Pass 6 of `starorg-data` reads
  `AttachDef.Localization.Description` of every `EntityClassDefinition` (about 10k of 30k on
  LIVE 4.10 have a real one) into `item_descs`, and walks the one `ResourceTypeDatabase`
  record (now listed by struct type) into `resource_descs` (resource name, lower case:
  `raw_quantainium`). The `commodities` dataset is unchanged, so no MCP cache goes stale.
  `ObtainHints::fill_descs` merges both at apply time; a resource owns its key, so the
  rock, hand-held mineral and cargo box entities that reuse `items_commodities_*_desc` do
  not contest it.
- **Filler:** `Tools::obtain_hints` (`starorg-mcp/src/hints.rs`), items from item-tags (loot
  areas and UEX shops) and resources with a quality spread (belts, spots, quality).
  `BulkObtain` in `query::obtain` answers every item at once (same rows as `obtain`; a
  shared pool-size memo, entry pools, rayon): 0.6 s for 4,047 sourced classes on LIVE 4.10
  (32 threads; about 5 s on one), first extraction of the five datasets about 7 s. Loot rows
  are one per container kind with two readable places (`Weapons rack (Common) at ASD
  facilities / Kaboos`).
- **Wiring:** `ApplyOpts::obtain` (`apply::ObtainSource`), asked only with `obtain` on; the
  CLI (`apply`, `build`, hooks with the remaining budget as the worker timeout) and the app
  (apply; the preview reads only cached datasets) pass `ToolsObtain` over the same cache and
  memory limit as the Sources card. `Report::obtain` carries time and item count.

### As built (phase 3)

- **Schema:** `CategoryId::Obtain` (`obtain`, label *Where to get*), lines = core
  `obtain::LINES` (a contract test compares them).
- **Tab:** Enhancements -> *Where to get*, last (after Quest items): master switch, Detail
  (Short / Normal / Detailed, one example each, `lib/obtain.ts` `OBTAIN_MODES`), line toggles,
  and a Shops card. The shops switch is `obtain_shops` plus the `obtain.shops` line (turning it
  on clears that line). New contract command `obtain_status` (file metadata only): the UEX
  folder `<cache>/mcp/uex` and the age of `uex-item-prices.json` / `uex-terminals.json`.
- **Preview:** real backend picks a personal weapon whose result gained a WHERE TO GET / `Get:`
  block, then an `items` row with `Mined` (else any item block), as category `obtain-hints`.
  The Rust mock renders core `obtain::block` over synthetic sources (rifle after its stats,
  ore); the TS mock (`obtainPreview`) is tested equal to it on the Everything sample.
- **Counts:** `obtain_hints` in apply `counts` (core JSON, app `ApplyCounts`, wizard review,
  CLI apply line), extensions of other categories' edits included.

### As built (phase 4): commodity sell prices

- **Data:** `starorg_net::uex::fetch_market` fetches a third endpoint,
  `commodities_prices_all` (public, about 1.1 MB, 2,598 rows for 124 commodities on
  2026-11-22), into `uex-commodity-prices.json` beside the other two: `{ _source, _fetched,
  prices, summary: { prices, commodities } }`, rows cut to `commodity_name`, `id_commodity`,
  `terminal_name`, `id_terminal`, `price_buy`, `price_sell`, `scu_sell_stock`, `status_sell`,
  `date_modified` (zero and blank dropped, as for item prices). Nothing is written unless all
  three answers are good. `obtain_status` lists the three files, with `shops_ready` (item
  prices + terminals) and the new `prices_ready` (commodity prices + terminals).
- **Matching** (`starorg_gamedata::query::sells`): UEX names no game id, so a row matches a
  `ResourceType` by name, display name first, then record name, both lower case with letters
  and digits only, and only when exactly one resource has that name. On LIVE 4.10 all 124
  UEX commodity names match (114 of them have a kiosk that buys). Ranking: highest
  `price_sell` first, no zero or missing price, no kiosk UEX marks off LIVE, one row per
  terminal and place, placed through `uex-terminals.json` like shop rows.
- **Card:** `RecordSources.sell` (`SellSources`: loaded, folder, resource, name, top 3
  offers, total, fetched) for a resource, a cargo entity holding one (`game-commodities`
  `items[].resource`) or a record whose quality names one. Missing files: a note, as Buy.
- **In game:** `ItemObtain.sells` (`SellHint`: terminal, place, system, price) per resource
  (lower-case resource name, like the mining lines), `ObtainHints.prices_fetched`. The prices
  are part of the cached hints. `OUTPUT_VERSION` 7 -> 8.
- **UEX-derived caches follow the files:** `Store::typed_uex` keys a cached value on
  `Store::uex_stamp()` (modified time and size of the three UEX files). The obtain corpus
  (shop rows of the Sources card and the shop tools) and the obtain hints use it, so a Fetch
  now while the app runs counts at the next card, tool call, preview or apply without a
  restart (before, shop names stayed as first read until the app restarted). Datasets are not
  extracted again. Any later UEX-derived cache (a shops index) uses the same call.

| Mode | Sell line |
|---|---|
| Short | `Get: Sell ~9.6k aUEC/SCU at HUR-L2 (UEX, 21 Nov) · Mined ...` |
| Normal | `Sell: ~9.6k aUEC/SCU at HUR-L2, ~9.2k at Rod's Fuel, ~9k at TDD Area 18 +4 (UEX, 21 Nov)` |
| Detailed | `Sell: ~9.6k aUEC/SCU at HUR-L2 (HUR-L2 Faithful Dream Station, Stanton)  \|  ...  \|  +4 (UEX, 21 Nov)` |

## Text

Header and labels use the mission EM styles (`mission.header_em`, `label_em`). Lines:
`shops`, `sell`, `loot`, `craft`, `rewards`, `mined`, `quality`, each switchable
(`lines_off = ["obtain.loot", ...]`). A line with nothing to say is left out; no sources at
all means no block.

| Mode | Shape |
|---|---|
| Short | one line: `Get: Buy · Loot (Rare) · Craft [BP]`; ores `Get: Mined (hand) at Lyria, Pyro I · Quality avg 500` |
| Normal | header + one line per source, top 3 places, `+N` for the rest, no odds |
| Detailed | places with system, loot odds per box, quality spread and place overrides, top 3 per line |

## Profile

- `obtain: bool`, `obtain_mode: short | normal | detailed`, `obtain_shops: bool`,
  `obtain_prices: bool`.
- Presets: Recommended `obtain = true, normal`; Minimal `obtain = true, short`; Everything
  `detailed, obtain_shops = true, obtain_prices = true`; Off false. `obtain_shops` and
  `obtain_prices` default false.
- `OUTPUT_VERSION` 5 -> 6 (Recommended output changes).

## App

- Enhancements: new **Where to get** tab (switch, mode select, line toggles, shops switch
  with the UEX folder and file age).
- Preview: one gun sample (stats block + WHERE TO GET) and one ore sample.
- Contract: `Profile` mirror, schema lines, convert, mock preview, samples.

## Tests

- Engine extend: stats block + obtain on one key; check failure drops only the extension.
- Generator: each mode and line toggle; empty sources means no block; length cap.
- Profile: presets, toml, hash.
- MCP `obtain_hints` over the test world; `starorg-data` item_descs pass.
- App: preview samples, contract samples, EnhancementsScreen tab.

## Phases

1. Engine extend + core model + generator + profile (fixtures, golden).
2. `starorg-data` item_descs pass, commodities `descriptionKey`, MCP `obtain_hints`, apply
   wiring (apply.rs, table.rs preview, CLI build).
3. App tab, preview, docs (user guide, addendum note), changelog.

## Open

- UEX download (done on `feat/uex-fetch`): Settings -> Network -> "Download UEX market data"
  (`network.uex`, off) with an optional token (keyring entry `uex` or `uex.token`, never sent
  to the webview). `fetch_uex` writes both files in sc-gamedata's exact shape
  (`starorg_net::uex`), `uex_token_status` / `set_uex_token` manage the token. Still open: no
  automatic refresh (only Fetch now).

- Commodity kiosks' best sell prices (top 3): done on `feat/kiosk-prices`, in the card and
  in game (phase 4). Still open: sc-gamedata has no `uex-commodity-prices` fetch yet (the
  file shape here is ours), and no MCP tool answers from it.

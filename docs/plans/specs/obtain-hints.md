---
title: "Obtain hints: where to get it, in item and commodity descriptions"
tags: [docs, plans, specs]
status: active
updated: 2026-11-21
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
- **Shops only from UEX files the user supplied**, names only, never prices (prices go stale
  between applies). Off by default. Nothing is downloaded.
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

## Text

Header and labels use the mission EM styles (`mission.header_em`, `label_em`). Lines:
`shops`, `loot`, `craft`, `rewards`, `mined`, `quality`, each switchable
(`lines_off = ["obtain.loot", ...]`). A line with nothing to say is left out; no sources at
all means no block.

| Mode | Shape |
|---|---|
| Short | one line: `Get: Buy · Loot (Rare) · Craft [BP]`; ores `Get: Mined (hand) at Lyria, Pyro I · Quality avg 500` |
| Normal | header + one line per source, top 3 places, `+N` for the rest, no odds |
| Detailed | places with system, loot odds per box, quality spread and place overrides, top 3 per line |

## Profile

- `obtain: bool`, `obtain_mode: short | normal | detailed`, `obtain_shops: bool`.
- Presets: Recommended `obtain = true, normal`; Minimal `obtain = true, short`; Everything
  `detailed, obtain_shops = true`; Off false. `obtain_shops` default false.
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

- Commodity kiosks' best sell prices (top 3) stay in the app card; they need a new
  `uex-commodity-prices` fetch in sc-gamedata first.

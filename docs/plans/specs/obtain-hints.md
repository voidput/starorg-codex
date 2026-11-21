---
title: "Obtain hints: where to get it, in item and commodity descriptions"
tags: [docs, plans, specs]
status: draft
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

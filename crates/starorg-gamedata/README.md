# starorg-gamedata

Rust port of [sc-gamedata](https://github.com/voidput/sc-gamedata-extractor): Star Citizen game data
read from `Data.p4k` and written as the same `game-<domain>.json` datasets the published
dumps carry, field for field and byte for byte. Dev-only for now: nothing in the shipped
CLI or app depends on it, and `starorg-data`'s `GameData` (apply/enhance) is unchanged. A
later step moves `GameData` onto this crate.

Status (LIVE 4.10.200.58746): `game-manufacturers` and `game-strings-english` are
byte-identical to the golden dumps (apart from the `_extracted` timestamp). Every other
domain has a slot in `src/domains/mod.rs`, waiting for its port.

Not ported, on purpose: `uex-item-prices.json` / `uex-terminals.json` (third-party UEX
API data, not game files), the community overlay downloads (`sources/overlays.ts`, network),
and the CLI-side query tools (`finder.ts`/fzf, `notes`, `diff`, `engagement`,
`deflection`, `search-index`, which derive from datasets rather than game files).

## Layout

| Module | Ports | What |
|---|---|---|
| `records` | `records.ts` | `GameRecord`, `RecordSet` (by type / name / record path / GUID, `follow` for both reference shapes), `at`, `num`, `str_`, `ref_*` |
| `entity` | `entity.ts` | components of `EntityClassDefinition` |
| `localization` | `localization.ts` | `global.ini` parsing, `localize`, overlays, `redaction` (publish-dump rules, off by default) |
| `cryxml` | `cryxml.ts` | CryXmlB documents |
| `glob`, `collate` | `glob.ts`, `Intl.Collator` | filter globs; `locale_compare` / `compare_names` orders |
| `output` | `JSON.stringify(_, null, 2)` | exact writer and the `_source`/`_extracted`/`_build` header |
| `domain`, `domains/` | `domains/types.ts`, `pipeline.ts` `DOMAINS` | `Domain` trait, `ParseContext`, the registry |
| `pipeline` | `pipeline.ts` (parse half) | extraction scoping, running a domain |
| `compare` | (new) | semantic diff against golden dumps |
| `source` | `starbreaker.ts` | the archive through `starorg-data`'s StarBreaker adapter (read only) |
| `strings` | `writeStrings` | `game-strings-english.json` |

Records come from StarBreaker's own JSON exporter (`FullDataCore` in
`starorg-data::starbreaker`, the exporter behind `starbreaker dcb extract --format json`),
so a domain sees exactly the values the TypeScript parsed.

## Compare with the golden dumps

```sh
scripts/compare-gamedata.sh                          # every ported domain, LIVE
scripts/compare-gamedata.sh --domain manufacturers   # one domain
GOLDEN_DIR=/path/to/dumps scripts/compare-gamedata.sh --channel PTU --p4k /path/PTU/Data.p4k
```

The script builds `gamedata-port` (into `.tmp/target-port`), extracts to
`.tmp/gamedata/<channel>/` and diffs against `$GOLDEN_DIR` (default
`~/git/sc-gamedata/data`, build 4.10.200.58746 = LIVE). It reads the game and the golden
directory only. Exit status 1 on any difference. The harness directly:

```sh
gamedata-port extract --channel LIVE --out .tmp/gamedata/LIVE [--domain X]...
gamedata-port compare .tmp/gamedata/LIVE ~/git/sc-gamedata/data [--domain X]...
gamedata-port list
```

`compare` ignores the top-level `_extracted`, compares numbers within 1e-6 relative,
matches array elements by the first of `id`, `recordName`, `guid`, `key`, `code`,
`className`, `ref`, `name` that identifies every element on both sides (else by position),
and reports per dataset `identical` or `N differing paths` with the first 20.

## Porting guide

Every port follows `src/domains/manufacturers.rs`.

1. **One file, one slot.** Write `src/domains/<slot>.rs` exporting `pub static DOMAIN`,
   then uncomment the slot in `src/domains/mod.rs`. Touch no other line there; shared code
   used by several TypeScript domains goes in its pre-listed helper slot (`item_common`,
   `tags`, `socpak`), ported once, before its dependants. Do not edit `app/`, the
   contract, `starorg-data`'s `GameData`, or another domain's file.
2. **Copy the TypeScript's inputs verbatim.** `name()` is the output base name
   (`game-ship-components` for `components.ts`), `filters()` the TS `filters` array
   (each must appear in `pipeline::GOLDEN_EXTRACTION_FILTERS`; a test checks),
   `files()` the TS `files`, `needs_localization()` the TS flag.
3. **Only the scoped records.** `ctx.records` holds exactly what the TS domain was handed,
   in the same order (sorted file paths). Resolve references through `RecordSet`
   (`by_name`, `by_record_path`, `by_id`, `follow`); a target outside the domain's filters
   does not resolve, as in the TS. Never read the DataCore around the set.
4. **Same output shape.** Serde structs with `#[serde(rename_all = "camelCase")]`, field
   order = the TS object literal's key order, `Option` + `skip_serializing_if =
   "Option::is_none"` for anything that can be `undefined`. Build the body as a
   `serde_json::Map` (`domain::field`) in the TS return order. `serde_json` has
   `preserve_order` on. Do not round or reformat numbers; `output` prints them as
   JavaScript does. Integer-like object keys (`"987"`) are moved first by `output` and
   iterated first with `output::js_key_order`, as JS objects do; use it when the TS
   iterates an object it built.
5. **Same order.** `Array.prototype.sort` is stable: use `sort_by`, not `sort_unstable_by`.
   `localeCompare` -> `collate::locale_compare`, `compareNames` -> `collate::compare_names`,
   default `sort()` on strings -> UTF-16 order (`a.encode_utf16().cmp(b.encode_utf16())`).
   `collate` covers ASCII and accented Latin; extend it (with Node-verified tests) if a
   domain needs more.
6. **Same text.** `localize` keeps the TS rules (`@LOC_EMPTY` and empty values are
   `None`, unresolved keys come back as the token). Redaction is for published string
   tables only; never apply it in a domain.
7. **Tests.** Port the domain's `*.test.ts` cases as unit tests with synthetic records
   (`records::normalize_record`, `fixtures::FixtureSource`). No game data in committed
   tests. Real data only through `scripts/compare-gamedata.sh --domain <slot>` or the
   `#[ignore]` test in `tests/live.rs`; outputs under `.tmp/`.
8. **Done means identical.** `scripts/compare-gamedata.sh --domain <slot>` prints
   `identical` on LIVE. If the golden dump itself looks wrong, match it anyway and note
   it in the module docs; fixing behaviour is a separate change on both sides.
9. **Gate.** `cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D
   warnings`, `cargo test --workspace --locked`. Code must build and pass on Windows (no
   Unix-only APIs, paths through `Path`, archive names as stored with `\`).

Porting order, from the TypeScript imports: independent (`ammo`, `commodities`,
`containers`, `fps_weapons`, `law`, `mining`, `missions`, `quantum`, `reputation`,
`salvage`, `starmap` + `socpak`, `wikelo`); then the helpers `item_common`, `tags`; then
`armor`, `blueprints`, `consumables`, `loot`, `item_tags`, `item_sources`,
`components` (needs `containers`), `weapons` (needs `ammo`), `cargo` (needs `wikelo`),
`refining` (needs `commodities`), `mining_spawns` (needs `starmap`); last `ships`
(needs `components`, `weapons`) and `shops` (needs `cargo`, `wikelo`).

## Provenance and licence

The code is a port of sc-gamedata's TypeScript sources (`src/records.ts`, `entity.ts`,
`localization.ts`, `cryxml.ts`, `glob.ts`, `pipeline.ts`, `domains/*.ts`,
`scripts/publish-dump.mjs` redaction), by the same author, who releases this port under
this repository's MIT licence. sc-gamedata itself is Apache-2.0; its NOTICE attribution
carries over:

> The extract-then-parse pipeline design, the per-domain dataset breakdown, and the
> identification of which DataCore record paths carry which game data follow prior work in
> dumpers_repo (https://github.com/Sinedrone-Sentinel/dumpers_repo, Copyright (c) Michael
> Linzenmeyer, Apache-2.0). This is an independent implementation. It shares no source with
> that project. The proprietary DFP pricing engine carved out of that repository's licence
> is not used, referenced, or reimplemented here.

StarBreaker (https://github.com/diogotr7/StarBreaker, through the pinned mirror) reads the
archive and the DataCore. This crate ships no game content; it reads an installation the
user already has. Not endorsed by or affiliated with the Cloud Imperium or Roberts Space
Industries group of companies. Star Citizen® is a registered trademark of Cloud Imperium
Rights LLC.

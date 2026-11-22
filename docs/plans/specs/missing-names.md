---
title: "Missing names: names for item keys stock lacks, and a raw-key guard"
tags: [docs, plans, specs]
status: active
updated: 2026-11-22
---

# Missing names: names for item keys stock lacks, and a raw-key guard

Some items show a raw key as their name in game, such as
`@item_Name_nvs_pilot_flightsuit_01_01_02`: the entity's `AttachDef.Localization.Name` points
at a key the stock `global.ini` does not have (a CIG data bug). A new enhancement adds those
keys with a name, and every generated text is checked so we never show a raw key or a
placeholder ourselves.

## Measurements (LIVE 4.10.201.48085)

- 26,823 entity classes have a real name reference (`ItemName` rows).
- First count: 672 references missing from stock, 421 of player-visible types. That count
  compared keys without dropping the `,P` flag suffix (`items_commodities_pressurized_ice,P`
  is in stock), so it was too high.
- Flag-aware, against the file in `Data.p4k`: **88 visible entities, 83 keys** missing
  (WeaponPersonal 35, Paints 12, Misc 10, WeaponAttachment 6, bomb and missile racks, cockpit
  flair, undersuits, a few clothes). The NVS flight suit `_01_01_02` .. `_04` is among them.
- Hidden types, left alone: `FlightController`, `SeatDashboard`, `ControlPanel`, `Relay`,
  `NOITEM_Vehicle`, `UNDEFINED`, `Door`, `Usable`.
- Stock keys whose value is a placeholder (about 6.6k, mostly hidden ship parts) are out of
  scope: they are not missing, and filling them would guess at CIG's intent.

## Decisions

- **Add keys, never edit a stock line.** `Edits::add` (`ini.rs`) holds keys stock lacks; the
  writer puts them after the last stock line, in folded-key order, with the stock's line
  ending (a last line without one gets one first). `validate::check_file_with` checks the
  rendered file: every stock line keeps its key and ending, each added line is a plain
  identifier stock lacks, once. `Engine::add` / `Generated::added` / `AddedKey` carry them
  through generation; `merge` keeps them (`GenEdit::added`, `merge::gen_edits`) and an
  `overrides.ini` line on an added key replaces its value. Loc patches cannot target them.
- **Value, in order** (`enhance/names.rs`):
  1. *variant*: entities of the same `AttachDef.Type` whose record name differs only by a
     trailing `_NN` / `_sfNN` / `_tintNN` / `_lrNN` (or is the bare base, even one that ends
     in a number itself, `behr_rifle_ballistic_01`) resolve to exactly one real name;
  2. *base*: the record name ends in such a segment and the base entity (segment dropped,
     same type) has a real name: `<base name> (<SEGMENT>)`, `P4-AR Rifle (LR01)`. A `_mag`
     whose base is a weapon (type `Weapon*`, not `WeaponAttachment`) is
     `<weapon name> Magazine`; any other `_mag` falls through;
  3. *description*: entities with the same description key resolve to exactly one real
     name;
  4. *readable*: from the key: prefixes `text_ui_charactercustomizer_item_`,
     `items_commodities_`, `crafting_machineName_`, `item_Name_`, `item_Name`, `item_Type`,
     `item_` dropped, `_` and camel case split, words title-cased, manufacturer codes (from
     stock `manufacturer_Name*`) and short vowel-less codes upper-cased, `S4` / `LR01` kept
     as codes, runs of numbers joined with `-` so variants stay distinct
     (`NVS Pilot Flightsuit 01-01-02`, `Ammo Crate S4`, `Piercing Ball`).
  "Real" = exists, not empty, not a placeholder (`model::is_placeholder_name`).
- **No loose sibling guessing.** Taking any sibling's name after stripping trailing segments
  was measured wrong (an iron box named "Agricium (Ore)", a flight suit and the Redeemer armor
  named `<= PLACEHOLDER =>`).
- **Guard.** `validate::check_value` refuses, for generated ops only, added text with a raw
  loc reference (`@` + identifier with `_`) or "placeholder" (any case) the stock value does
  not have: rules `raw-loc-key` and `placeholder-text`. The edit is dropped with a
  `dropped-edit` warning like any other rule. `is_placeholder_name`, which every name lookup
  of the generators uses (`enhance::stock_name`, quest titles, rewards, blueprints, owners,
  fleet), also treats a value with an embedded `@key` as missing.

## Profile and data

- `missing_names: bool`: on in Recommended, Minimal and Everything, off in `Profile::off`.
  `OUTPUT_VERSION` 6 -> 7. New `[enhance]` key: name it in the release notes.
- `GameData.item_names` (`ItemName { class, name, desc, kind }`), filled in pass 6 of
  `starorg-data` from the same record read as `item_descs`. `GAMEDATA_SCHEMA` 4 -> 5 (one fresh
  extraction per build). Drift warns when it is empty.
- Category `missing-names`; counts `missing_names` in apply, `build --json` adds
  `missing_names_by_rule`.

## App

- Enhancements -> General -> *Fixes and stamp*: **Fix missing item names**.
- Strings: added keys are rows after the stock ones, with an empty original.
- Preview (General): one added name, shown against the raw `@key` the game showed.
- Contract: `Profile.missing_names`, `ApplyCounts.missing_names`, the two new rule names.

## Validation (LIVE 4.10.201.48085, Recommended, `build` to a scratch file)

- 83 keys added: 13 variant, 11 base, 0 description, 59 readable; 0 dropped, 0 warnings
  (before the base rule: 12 variant, 71 readable).
- Counting the bare base as a member of its own variants' family changed four weapon
  `_lr01` names from one paint's name (`FS-9 "Blacklist" LMG`) to the base rule
  (`FS-9 LMG (LR01)`), and gave six others the name their whole family shares
  (`P8-SC SMG`, `Scalpel Sniper Rifle`).
- `item_Name_nvs_pilot_flightsuit_01_01_02=OMNI-AFS-Sapphire Armored Flight Suit` (variant).
- No output line holds a `@word_` reference or PLACEHOLDER its stock value lacks.

## Open

- Still readable: weapons whose base key is missing too (the whole `ksar_lmg_energy_01`,
  `ksar_pistol_energy_01` and `asad_rifle_energy_01` families, with their `_mag`s), paints,
  bomb and missile racks, Valakkar commodities.

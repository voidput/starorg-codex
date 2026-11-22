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
- Stock keys whose value is a placeholder: about 6.6k, mostly hidden ship parts (left
  alone). Of player-visible types, 5 keys (2026-11-22): see *Name tiers*.

## Decisions

- **Add keys; edit a stock line only when its name is a placeholder** (*Name tiers*). `Edits::add` (`ini.rs`) holds keys stock lacks; the
  writer puts them after the last stock line, in folded-key order, with the stock's line
  ending (a last line without one gets one first). `validate::check_file_with` checks the
  rendered file: every stock line keeps its key and ending, each added line is a plain
  identifier stock lacks, once. `Engine::add` / `Generated::added` / `AddedKey` carry them
  through generation; `merge` keeps them (`GenEdit::added`, `merge::gen_edits`) and an
  `overrides.ini` line on an added key replaces its value. A loc `set` with
  `fill_name = true` replaces it too (below the override): see *Curated names*.
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
     (`NVS Pilot Flightsuit 01-01-02`, `Ammo Crate S4`, `Piercing Ball`). Tidy-ups: a
     manufacturer code counts only as the first word or when the key writes it in capitals
     (`Perseus_Paint_black_grey_grey` is `... Black Grey Grey`, not `GREY`: Grey's Market); a
     word repeated across a camel-case split is written once (`Mining_MiningLaser` ->
     `Mining Laser`, `itemPort_port` -> `Item Port`), a trailing `_Name` is dropped, and
     description keys used as names lose `item_Desc` (`item_Descbehr_rifle_ballistic_1` ->
     `BEHR Rifle Ballistic 1`); also dropped: `item_commodities_`, `actor_species_creature_`.
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

## Name tiers

One chain, `enhance::names::Resolver`, built once per generator run and read by
missing-names and by the mission generator's blueprint and reward names (and blueprint
ownership matching), so generated text shows the name the file will show:

1. **Curated**: a `fill_name` patch entry (below), while the stock name is bad.
2. **Game**: the stock name when it is good (exists, not empty, not
   `model::is_placeholder_name`, which also catches an embedded `@key`). Never replaced.
3. **Generated**: variant, base, description, used when there is no curated name and the
   stock name is missing or bad. Sibling and base names are looked up through tiers 1-2, so a
   curated base names its variants and `_mag` like a stock base would; a key never borrows
   its own curated name (its patch keeps its outcome). A missing key is added
   (`Engine::add`); a placeholder stock name of a visible type is renamed in place
   (`Engine::rename`, a whole-value `GenEdit` with `rule`), never an added key.
4. **Readable** (the guard): from the key, else the record name, else the key without `@`,
   so no visible item ever shows a raw `@key`.

Where the curated map comes from: `patch::curated_names(set, game_data, build)`, the
`fill_name` values of the enabled, in-range patches (conflicting entries, entries whose
`requires` does not apply and values the added-key rules refuse are left out). `apply` and
the app's Strings table pass the same `PatchSet` they merge, so their generators and their
loc patches agree. `build` writes generators only (no data or loc patches, spec 4.5), so it
passes none: there the curated keys and their variants keep generated names. One rule:
patches reach a path entirely or not at all.

Not routed through the chain: stat-block placeholder checks (`item_placeholder` in
`generate_with`), quest-item eligibility and name tags. A name tag never goes on a
placeholder line; missing-names renames that line instead.

## Curated names

- The readable rule cannot know a real name. Built-in patch
  `crates/starorg-core/patches/missing-names.patch.toml` gives one, per key, for items whose
  name a public source confirms (each entry carries a comment with the URL). No source, no
  entry: the readable fallback stays. Paints get `<Ship> <Livery> Livery` only when the
  livery's name is confirmed for that exact paint.
- Engine (`patch.rs`): a loc entry with `fill_name = true` (`set` only). Stock lacks the
  key: it targets the added key (`LocTarget::Added`, `Snapshot::added_value`), `no-data` when
  nothing added it (the switch is off). Stock holds a placeholder: it sets that line
  (`LocTarget::Line`). Stock holds a real name: `not-needed` (the entry retires, stock wins).
  Also `not-needed` when the value is already ours. Added values go through `check_added`;
  `merge` layers it `generated < loc patch < override`, provenance `LocPatch` (Strings source
  *Patched*).
- Why a loc patch plus a read-only map for the generators: the patch engine gives per-entry
  outcomes, build ranges, per-channel switch-off, the patch-set hash in the applied state and
  retirement when CIG fixes the data; the generators only read the values
  (`curated_names`), so the patch still owns its key and reports its own outcome.

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
- Name tiers (2026-11-22, same build, Recommended): `build` (no patches) names 88 keys,
  83 added and 5 placeholder lines renamed: 16 variant, 11 base, 61 readable; 0 dropped, 0
  warnings. With the built-in patches (curated map and loc patches, as `apply`): 19
  variant, 12 base, 57 readable, 2 of them patched to curated names. Over the 7,935 distinct
  visible name keys: curated 2, game 7,846, generated 31 (19 variant, 12 base), readable 55,
  none 1 (left alone by design: a `LOC_*` sentinel or a key that is not a plain identifier).
  Verdict family: `ksar_lmg_energy_01=Verdict LMG` (curated), `_mag=Verdict LMG Magazine`
  (base), `_tint01..03=Verdict LMG` (variant). Placeholder lines renamed:
  `dmc_frontier_gloves_01_02_11=Adroit Gloves CFP`, `..._jacket_...=Silverton Jacket CFP`,
  `..._pants_...=Silverton Pants CFP` (variant), `cbd_hat_03_01_CFP_var2=CBD Hat 03-01 CFP
  Var2`, `spv_bioticorp_rep_jacket_01_01_01=SPV Bioticorp Rep Jacket 01-01-01` (readable).
- Curated names (2026-11-22, same build, Recommended, generators then the built-in patches
  through `merge`, before the name tiers): the rule counts stay 13 variant, 11 base, 59 readable; the
  `missing-names` patch applies 2 entries (`item_commodities_ventslug=Vent Slug`,
  `item_Nameksar_lmg_energy_01=Verdict LMG`), so 57 keys keep a readable name. 0 dropped,
  0 warnings. The tidy-ups changed 7 readable names, e.g. `Perseus Paint Black Grey Grey`,
  `Mining Laser GAMA Hyun S1`, `BEHR Rifle Ballistic 1`, `Item Port Salvage Arm R`.
- Researched and left out (no source ties a name to that exact item): the `asad_rifle_energy_01`
  family, the `ksar_pistol_energy_01` family (the roadmap's Kastak "Hellion" pistol matches
  only by elimination) and the Verdict's `_mag` / tints, all 18 paints (their classes are on no
  wiki paint page; `Reclaimer_Paint_Showdown` is borrowed by `paint_redeemer_template`), the
  Hyun mining laser and pod, Polaris and Idris radars, Perseus torpedo racks, Starlifter bomb
  racks, the Long Look Mk2 radar (wiki title only, not the in-game string), Valakkar apex,
  hide and pearls, LNA clothes, the holiday present, the GRIN cutter (the wiki says its name
  is a placeholder), the dashboard bobblehead and the collector/mag/mr01 weapon variants.

## Open

- A family named only through its bare base gets the base's name on every tint
  (`ksar_lmg_energy_01_tint01..03 = Verdict LMG`, like the stock-named
  `klwe_lmg_energy_01_lr01 = Demeco LMG`): the variant rule wins before the base rule, so the
  tints are not told apart. `<base> (TINT01)` would need the variant rule to skip a family of
  one; that changes stock-named families too, so it is not done here.

---
title: StarOrg Strings v1 — addendum (user requests 2026-11-14)
tags: [docs, plans, specs]
status: draft
updated: 2026-11-14
---

# StarOrg Strings v1 — addendum (user requests 2026-11-14)

Direction settled in [the v1 spec](starorg-strings-v1.md) revision 2 (sections 3.4, 8, 11);
the features below are **v1** (Q17 decided 2026-11-14): hooks in M1, published dumps in M2. Corrections from the
review are marked **Corrected**.

## 1. Data source: published dumps first, local extraction as fallback

- Primary source: [voidput/sc-gamedata-dumps](https://github.com/voidput/sc-gamedata-dumps) (public).
  One git tag + GitHub release per game build, tag = build version (e.g. `4.10.200.58746`,
  release title `4.10.0 LIVE (4.10.200.58746)`). Datasets under `data/`: `game-ships.json`,
  `game-ship-components.json`, `game-weapons.json`, `game-fps-weapons.json`, `game-ammo.json`,
  `game-missions.json`, `game-blueprints.json`, `game-reputation.json`, `game-mining*.json`,
  `game-item-tags.json`, `game-strings-english.json`, … Produced by `~/git/sc-gamedata`
  (StarBreaker JSON path). Localisation in dumps is **redacted to name keys only**, so the
  full English text still comes from the local `global.ini`.
- Local game version: `<channel>/build_manifest.id` → `Data.Version` (e.g. `4.10.200.58746`),
  also `Branch` (`sc-alpha-4.10.0`) and `RequestedP4ChangeNum`.
- Rule: if a dumps release exists for exactly the local build → download those datasets
  (cache per build). No exact match (e.g. PTU/EPTU/HOTFIX build not published) → extract
  locally from `Data.p4k` with the StarBreaker crates. Network stays opt-in: with the
  "Use published game data" switch off (the default), always extract locally, so local
  extraction must be complete on its own.
- **Corrected:** "the same build" is decided by `RequestedP4ChangeNum` together with `Branch`,
  not by ordering `Version` strings. `Version` is numbered per branch (LIVE
  `4.10.200.58746`, HOTFIX `1.0.191.55227`), so "local version higher than the newest tag"
  cannot be computed. Dump releases should carry the change number and branch in their
  metadata.
- Implication: the enhancement generators should consume one internal data model that both
  paths fill (the dumps' JSON schema is a good candidate for that model, or a superset).
- Verify integrity of downloaded datasets (pin by tag/commit, checksum), never execute content.

### Findings from M2 (2026-11-14, issue #12)

- **Implemented** as `source::dump::PublishedDump` + `DumpFirst` (consent off by default);
  HTTPS only in `starorg-net`, linked into the CLI only with the `published-dumps` feature.
  Integrity: tag resolved to a commit once, every dataset checked against that commit's git
  blob SHA-1 and its `_build` stamp, cached per `<tag>/<commit>` with SHA-256 re-checked on load.
- **Dump releases carry no change number** (release body has Channel and Branch only, no
  `data/_build.json`). The match is then tag = `Version` + same `Branch`, with a
  `dump-weak-match` warning. `sc-gamedata` should publish `data/_build.json` with
  `branch` and `change` (`RequestedP4ChangeNum`); the reader already uses it when present.
- **Dumps carry no loc keys** (resolved names only) and **no contract-generator contracts**
  (`game-missions.json` is legacy `MissionBrokerEntry`). Keys are recovered by the namesake
  convention against the local `global.ini` (1,281 of 5,423 records on 4.10.200.58746);
  contracts stay local-only, so `DumpFirst` falls back to local extraction when they are
  needed. `sc-gamedata` should add `locKeys` (name/short/desc) and the generator contracts.
- **Stale records:** the 4.10.200.58746 dump still lists 54 vehicles renamed since 4.10.197
  (`AEGS_Hammerhead` -> `AEGS_Hammerhead_GS`, `RSI_Constellation_Andromeda` -> `_GS`, ...):
  `sc-gamedata` parses leftover files in its extract directory. Every record both sources
  have agrees field for field (weapons, ships incl. flight/loadout totals, components,
  missiles, ammo).
- **No speed gain yet:** the stock text still needs the local `Data.p4k` (ini-only worker,
  ~1.5 s, ~0.9 GB peak for the p4k index), and full local extraction is 1.1-1.6 s wall /
  0.87-0.91 GB peak on LIVE, HOTFIX and PTU (32 threads; ~8 s CPU).

## 2. LUG launcher hooks (and Windows equivalent)

Provide CLI subcommands + installer for the LUG Helper `sc-launch.sh` pre-launch / post-exit
hooks (see `~/git/sc-setup/prefix/sc-prelaunch.sh`, `sc-postexit.sh`, `sc-smartcitizen.sh` for
the current hand-made versions):

- **Corrected (scope):** v1 covers the hook need with `starorg-strings apply --all --if-stale
  --timeout N` called from both hand-edited scripts. The subcommands below are v1.1.
- `starorg-strings hook prelaunch` — read `build_manifest.id` per channel; if the build changed
  since the last apply (or `Data.p4k` is newer than our applied `global.ini`), refresh data
  (dumps or local extraction per §1) and re-apply with the saved profile. Fast no-op otherwise.
  Must never block the game launch for long: time budget + non-fatal on error.
- `starorg-strings hook postexit` — when the launcher/game closes: scan `Game.log` and
  `logbackups/*.log` for `Received Blueprint: <name>` notifications
  (e.g. `<SHUDEvent_OnNotification> Added notification "Received Blueprint: Tailwind Flight Suit Raider Camo: "`),
  update the owned-blueprint list, and re-apply so `[Owned]` tags are current next launch.
  Idempotent (remember scanned log files / offsets).
- `starorg-strings hook install|uninstall|status` — add/remove a clearly marked block in the
  LUG prelaunch/postexit scripts (respect existing user content, back up before editing).
- Windows: no LUG; decide between a launcher-side option (scheduled task / watcher on
  `RSI Launcher.exe` exit) or a manual "Check now" — open question.

## 3. Blueprint ownership per character (alts)

Players often have several accounts/characters (the owner has four). Owned blueprints are
tracked **per character**, never merged.

- Who is playing: each login writes one line per session, e.g.
  `<AccountLoginCharacterStatus_Character> Character: createdAt … - updatedAt … - geid <GEID> - accountId <ACCOUNT> - name <HANDLE> - state STATE_CURRENT`.
  Key characters by `geid` (stable); keep `accountId` and the latest `name` for display.
  Ignore `nickname="…" playerGEID=…` lines: those are other players.
- Attribution: walk each log (`Game.log`, `logbackups/*.log`) in order; every
  `Received Blueprint: <name>` notification belongs to the most recent `STATE_CURRENT`
  character line **in the same file**. A blueprint seen before any login line in that file is
  "unattributed" (kept, shown, never applied to a character silently).
- Storage: one owned set per character (`blueprints/<geid>.json`), plus scan state per log
  file (path, size, mtime, last offset) so rescans are incremental and idempotent.
- Which set drives `[Owned]` tags on apply: setting "active character" — default **auto** =
  the character of the newest session in that channel's logs; manual pick in the app and
  `--character <name|geid>` on the CLI; or "any of my characters" (union) as an option.
  `hook postexit` re-applies with the character that just played.
- Channels: logs are per channel folder (LIVE, PTU, HOTFIX); same geid across channels is the
  same character (PTU copies) but PTU progress is not LIVE progress: keep sets per channel.
- Privacy: geid/accountId/handles are personal data. Never upload them, redact them in
  `doctor` diagnostics, and use synthetic names in test fixtures (no real handles in the repo).

## 4. Import Smart Citizen's blueprint tracker export

`starorg-strings blueprints import <file> --character <name|geid> [--channel LIVE] [--dry-run]`
(and an Import button in the app's blueprint screen).

- Input: Smart Citizen's export, e.g. the owner's
  `drive_c/users/<user>/SmartCitizen/owned-blueprints.json`:
  `{"version": 1, "exportedAt": "<ISO>", "missions": [], "blueprints": [{"name", "completed", "favorite"}]}`.
  Accept `version` 1 only; refuse anything else with a clear message.
- Names are display names **with Smart Citizen's Tag Builder prefixes** (180 of the owner's
  269): `[CIV-S3-A] 7CA 'Nargun'`, `[H] ADP Core`, `[E-S2] …`, `[IND-S1-C] …`, `[P-S3] …`, `[L] …`.
  Strip every leading `[...]` group (repeatable, then trim), then match the rest against the
  English display names of blueprint output items in `GameData` (case-insensitive, normalise
  quotes/apostrophes and whitespace). If one name matches several blueprints, keep all
  candidates and report it as ambiguous instead of guessing.
- `completed: true` → owned; `completed: false` → not imported (it was only tracked).
  `favorite` → our favourite flag.
- Target: the chosen character (`geid` resolved from the logs, see §3; a name must resolve to
  exactly one geid) and channel (default LIVE). Merge as a union with what the log scan found;
  never delete existing entries. Record source `import:smart-citizen` + `exportedAt` per entry.
- Report: imported / already known / unmatched (listed by original name) / ambiguous.
  `--dry-run` prints the report and writes nothing.
- Test data: synthetic export with fake names in the repo; the owner's real file is used
  only in an ignored local test (personal data, never committed).

## 2-4. Implementation notes (M1, branch `m1-apply`)

- **Commands.** `hook prelaunch [--timeout 60]`, `hook postexit [--timeout 120]`,
  `hook install|uninstall [--prelaunch-script F] [--postexit-script F] [--bin PATH]`,
  `hook status`; `blueprints scan|characters|list|use|import`; `apply --character
  <name|geid|auto|any>`.
- **Never fatal.** `hook prelaunch|postexit` exit 0 for every outcome (2 only for usage
  errors); the apply's own code is `exit_code` in `--json`. Pre-launch = `apply --all
  --if-stale` within the budget; post-exit = incremental log scan of every channel, then the
  same apply with what is left of the budget.
- **Hook block.** Lines between `# >>> starorg-strings hook >>>` and `# <<< starorg-strings
  hook <<<`, inserted before a final top-level `exit` (else at the end); no blank lines are
  added, so `uninstall` gives the script back byte for byte. Default scripts:
  `<prefix>/sc-prelaunch.sh` and `sc-postexit.sh`; links are followed and the real file is
  edited in place (mode kept), after a backup in `data/hooks/backups/`. Only existing files
  are edited; a script inside the game install or the prefix's `drive_c` is refused. An
  outdated block is rewritten in place; unbalanced markers are refused. `status` also flags a
  hand-made `starorg-strings` call outside the block (it would run twice) and the Smart Citizen
  / StarStrings hooks. Windows: not supported (exit 4), still open.
- **Log facts (owner's logs, 2026-11).** The notification text is the *localised* item name,
  so it carries the active localisation tool's tags (`Received Blueprint: [H] Palatino Arms
  Raider Camo: `): log names are tag-stripped like imported names. Only `Added notification
  "Received Blueprint: ..."` lines count; the queue dump lines and `UpdateNotificationItem`
  lines repeat the same text. Login lines in `STATE_UNSPECIFIED` exist and are ignored. 200
  LIVE logs (947 MB): first scan 0.2 s, rescan 2 ms; 23 unique blueprints, 4 characters.
- **Incremental scan.** Per file: size, mtime, offset after the last complete line, the
  character at that offset, and a hash of the first 4 KiB. The game renames `Game.log` into
  `logbackups/` on the next start; the head hash finds the old record, so the rotated file is
  not read again. A file whose head changed is read from the start.
- **Active character.** `channel.toml` `active_character` (`blueprints use`): unset = auto,
  `any`, or a geid. The owned-set hash is part of `applied.json`, so a changed set or choice
  shows as `Stale (owned blueprints)`. Post-exit applies with the channel setting: with the
  default auto that *is* the character that just played; a manual pick is kept (otherwise the
  next pre-launch would flip it back).
- **Import.** Ambiguous names are stored with every candidate id and `ambiguous: true`;
  unmatched names are reported, not stored. With no blueprint names in `GameData` (until M2)
  `import` refuses to write (exit 1) and `--dry-run` reports everything unmatched. Owner's
  export (269 entries, all `completed`), dry run against a stand-in catalog of all
  `item_Name*` values from LIVE's stock `global.ini` (9,926 names; not blueprint outputs, so
  it over-matches variants): 199 new, 23 already known from the logs, 22 ambiguous, 25
  unmatched (ship components such as mining lasers, whose names are not `item_Name*` keys).
- **Privacy.** Names, geids and account ids stay in `blueprints/*.json` in the data folder;
  `doctor` shows counts only.

## 3-4. M1 + M2 integration (merge into main)

- **Catalog.** `GameDataCatalog` names each `GameData` blueprint by its produced item's name
  key looked up in the same build's stock `global.ini` (placeholders left out); the id is the
  blueprint record GUID. `blueprints import` takes it from the extraction cache, or extracts
  from `Data.p4k` (read only) on a miss; a real import caches that extraction, a dry run
  writes nothing (only the worker's empty temp work folder under the cache dir remains).
- **Owner's export, LIVE 4.10.200.58746, real GameData (dry run, 2026-11-15):** 269 entries,
  244 new, 23 already known from the logs, 2 ambiguous (2 candidates each: two blueprints
  produce items with the same English name), 0 unmatched, 0 not completed, 0 duplicates;
  1.3 s, 0.89 GB peak (one extraction). The stand-in catalog above gave 199 / 22 / 25.
- **`[Owned]` on apply.** `apply` now runs the generators (profile `preset` + `[enhance]`
  from `profile.toml`) with the channel's selected owned set (`blueprints::Selection`
  implements `OwnedBlueprints`): owned by matched blueprint id (non-ambiguous imports) or by
  normalised display name (log entries carry names only). Generated parts are validated with
  the generators' rules (`enhance::check`, which allows CIG's open-tail values) so `apply`
  and `build` keep the same edits: on the fake LIVE install `apply` output was byte-identical
  to `build --owned-blueprints <the same set>` (3,294 edits, 0 dropped, 280 `[Owned]`
  bullets for the imported character).
- **One name key.** `enhance::owned::normalize_name` is `blueprints::normalize`.

## 2-4. Verification fixes (2026-11-14)

- **Ambiguous names mark nothing.** An imported name with several candidates is kept with
  every candidate id and reported, but adds neither its name nor its ids to the owned set
  (`Selection.ambiguous` counts them; apply warns `owned-ambiguous`, info only). A name-only
  match (log scan, import) marks a blueprint only when no other blueprint of the build
  produces an item with the same English name (`BlueprintRef.name_unique`); a match by
  blueprint id still marks. LIVE: `[IND-S2-B] BroadSpec` (2 candidates) gives no `[Owned]`.
- **One-off pick is kept.** `apply --character X` records the pick and the channel's
  `active_character` at that time in `applied.json`; while that setting is unchanged, status
  and the hooks use the pick, so the next pre-launch does not flip it back. `blueprints use`
  ends it.
- **Hook block edge cases.** The block's lines always end in LF (a CRLF block failed
  `bash -n`; a CRLF script keeps its own lines). A script without a final newline gets one
  `\n` before the block and none after `END`, and `uninstall` drops exactly that byte, so the
  round trip is byte for byte. A final `exit` that ends a `\`-continued command gets the block
  before the whole command; a script that ends inside a continued line is refused.
- **Shared `user.cfg`.** Apply re-reads and re-plans `user.cfg` under the cfg lock (it waits
  up to 10 s for it instead of skipping with exit 3), so two channels applying at once both
  end up on the ledger and nobody writes stale text.

## 5. StarOrg Tools account link (OAuth, like RSI+) — import and sync blueprints

Native "Connect StarOrg Tools" in the app (and `starorg-strings starorg login|logout|status`,
`starorg-strings blueprints pull|push --character … [--guild …]`). Opt-in, off by default
(network policy §11).

**Auth (needs a StarOrg server change):** RSI+ uses `/api/auth/extension/login` →
Discord → one-time `?code=` → `/api/auth/extension/exchange` with a PKCE verifier, and gets an
`aud=rsiplus` 90-day token limited to `RSIPLUS_ROUTES`. The redirect allowlist
(`EXTENSION_REDIRECT_URIS`) only accepts bare https extension URLs, so a desktop app cannot use
it as is. Proposed (RFC 8252, native apps):
- New audience `strings` (own route allowlist: `/api/auth/me`, `/api/auth/logout`, the
  `my-blueprints` routes, `blueprint-library` read).
- Loopback redirect `http://127.0.0.1:<ephemeral port>/callback` (any port, path fixed), PKCE
  S256 required, `state` checked; system browser opens the login URL (no embedded webview).
- Token stored in the OS keyring (Windows Credential Manager / Secret Service), never in
  config files or logs; logout revokes server-side.
- Environments: `https://starorg.tools`, `https://dev.starorg.tools` (as RSI+).

**Blueprint API already in StarOrg** (`apps/industrial/api/routes/owned_blueprints.py`):
`GET /guilds/{guild_id}/my-blueprints`, `POST …/my-blueprints {blueprint_ids, source, acquired_at}`,
`POST …/my-blueprints/remove`, `GET …/blueprint-library`. StarOrg blueprints carry `game_uuid`
and `game_key` (record GUID / name), so matching to our `GameData` blueprint records is exact,
not by display name. `source` already allows `log` and `smart_citizen`; add `starorg_strings`.

**Pull (import):** owned set from StarOrg → chosen character's set (union, never delete),
entries tagged `source: starorg:<guild>`. **Push (optional):** blueprints found in logs →
StarOrg with `source: log` and the log line time as `acquired_at`. Never push removals
automatically.

**Alts (open question Q22):** StarOrg ownership is per StarOrg user (Discord account) inside a
guild, not per in-game character. Options: (a) link one character per StarOrg account
(default: the main), alts stay local; (b) push the union of chosen characters; (c) StarOrg adds
an optional character/RSI-handle dimension (RSI+ already syncs hangars per RSI handle).
**Decided 2026-11-14: (b).** Settings → StarOrg → "Characters to sync": the user ticks any
of their characters (default: all known ones); push sends the union of their owned sets.
Pull still writes into one chosen character (never spread across alts). Each pushed entry keeps
its per-character origin locally so un-ticking a character never removes anything server-side.
(c) stays a possible StarOrg follow-up.

## 6. Fleet import and ship ordering

Request (2026-11-14): import the fleet from RSI+ / FleetView to get a short list of the user's
ships, and give a real menu to order and rename them. New **Ships** screen, new generator
category `ship-names`, new CLI group `fleet`. Opt-in: nothing is written to ship names until
the user turns it on.

**Premise, not a fact.** In-game ship lists (ASOP fleet manager, vehicle selection) are
*believed* to sort by the displayed name, so a sort prefix on the name orders them. That is an
open in-game check ([spec §13](starorg-strings-v1.md#in-game-checks-manual-recorded-in-the-spec-when-done)).
Until it is answered the feature carries an "Experimental" badge, and the prefix defaults are
provisional. Renaming works regardless of the answer.

### 6.1 Name keys in the real game data (read-only check, 2026-11-14)

Measured on the extraction caches of the three installed channels; nothing was written to the
game folders.

| | LIVE 4.10.200.58746 | PTU 5.0.201.16954 | HOTFIX 1.0.191.55227 |
|---|---|---|---|
| Hull name keys (`vehicle_Name*` referenced by a ship record) | 291 | 283 | 286 |
| … with at least one player-flyable record | 286 | 278 | 281 |
| … of those with a `_short` sibling key in stock | 279 | 271 | 274 |

- Stock LIVE has 681 `vehicle_Name*` keys: the long names (`vehicle_NameAEGS_Gladius` =
  `Aegis Gladius`, maker first, at most 53 characters) and 329 short names
  (`vehicle_NameAEGS_Gladius_short` = `Gladius`, at most 20 characters, often abbreviated:
  `Avenger Titan Rngd.`). 51 long keys are referenced by no ship record (concept ships, old
  editions); they are never written.
- A ship record references its name in three places, all the same key:
  `VehicleComponentParams.vehicleName`, `SAttachableComponentParams.AttachDef.Localization.Name`
  and `SCItemPurchasableParams.displayName`. The short name is
  `AttachDef.Localization.ShortName` and follows the `<name key>_short` convention (checked on
  the Gladius record). `GameData` carries only `ShipLoc.name` today.
- **Which key the game shows where is not known** (long in the ASOP and the vehicle manager,
  short on space-limited HUD labels is the working assumption). In-game check.
- The name key is shared: 189 of LIVE's name keys are referenced by more than one record (the
  Gladius key by 19: the player hull plus AI, PU and unmanned variants). A rename or prefix
  therefore also shows on NPC ships of the same hull.
- 7 hull keys carry the `,P` flag suffix; one stock name has a trailing space; one display
  name is used by two keys (`Origin 890 Jump`: the hull and its hijacked mission variant). No
  hull name is a format key and none holds `~`, `%` or `<`.
- 14 ship records name themselves with non-vehicle keys (`hud_wreckage`,
  `TheCollector_ShipMod_*`, `vehicles_NameOrbital_Sentry*`). Those are out of scope.
- Channels differ (LIVE has 8 hulls PTU lacks, HOTFIX has 2 LIVE lacks), so a shared list
  must tolerate hulls a build does not have.

### 6.2 Hull catalogue

`fleet::Catalog`, built from `GameData.ships` + stock, cached with the extraction (no schema
change).

- A **hull** is a stock key that starts with `vehicle_Name` (folded), does not end in
  `_short`, is referenced as `loc.name` by at least one ship record, and whose stock value is
  not empty or a placeholder.
- **Hull id** = that key as stock spells it, without the flag suffix
  (`vehicle_NameAEGS_Avenger_Titan`). Compared folded (spec 4.3). It is stable across builds
  and channels, readable in a hand-edited file, and it is exactly the key we write.
- Fields: `hull`, `name` (stock long name), `short` (stock value of `<hull>_short` when that
  key exists), `maker` (most common manufacturer code of its records, a `_2025`-style year
  suffix dropped), `flyable` (at least one record passes `is_player_hull`), `ground`
  (movement class), `npc_shared` (records that fail `is_player_hull`).
- Only `flyable` hulls are offered in the UI and matched by the importer.

### 6.3 Import

One importer, format auto-detected, same dry-run-first report as the Smart Citizen blueprint
import (§4).

| Format | Detected by | Read |
|---|---|---|
| **HangarXPLOR JSON** (RSI+ "Export fleet", FleetYards, myfleet.gg / FleetView, Starjump / hangar.link) | First non-space character is `[` | Per row: `name` (ship, maker stripped), `ship_name` (custom name, or the ship), `ship_code`, `manufacturer_code`, `entity_type`. Rows whose `entity_type` is present and not `ship` are skipped. Field names are read ignoring case and `_` (`shipCode` = `ship_code`) |
| **HangarXPLOR CSV** | First line has the columns `Manufacturer` and `Ship` | `Manufacturer`, `Ship`. Quoted fields with `""` escapes; unquoted also accepted |
| **Plain text** | Anything else | One ship per line; blank lines and `#` comments ignored; an optional count (`2x Gladius`, `Gladius x2`); a maker in front is fine (`Aegis Gladius`) |

- Source of truth for the JSON and CSV shapes: `rsi-plus/src/lib/export.ts` (`XplorRow`,
  `starOrgRows`, `xplorCsv`, `shipLines`). The RSI+ StarOrg preset (`acquired`, no
  `pledge_date`) is the same shape and needs no special case.
- **Never read into memory beyond the parse, never stored:** `pledge_id`, `pledge_name`,
  `pledge_date`, `pledge_cost`, `lti`, `warbond`, `insurance`, `acquired`.
- Limits: UTF-8 (BOM dropped), 5 MB, 5,000 rows. A JSON object at the top level, or an array
  with no usable row, is refused with "not a fleet export".
- **CSV caveat.** In the CSV, `Ship` holds the custom name when the ship has one, and the hull
  name is then nowhere in the row. Such rows come out unmatched; once matched by hand the row
  text becomes the rename suggestion. The JSON export has no such gap, so the UI recommends it.
- **FleetYards' own (non-HangarXPLOR) export:** not supported as a format in this version.
  Its shape could not be confirmed from public documentation. The case-and-underscore-blind
  field reading above accepts a camelCase file with the same fields; anything else is refused.
  Open question Q24.

**Matching a row to a hull** (first tier that gives a result wins; a tier that gives several
hulls stops with *ambiguous*):

1. **Saved hand match** for this row name (§6.4 `matches`).
2. **Alias table**: `crates/starorg-core/data/fleet-aliases.toml`, normalised row name →
   hull id. For store names that differ from the game name (`Ursa Rover Fortuna` →
   `vehicle_NameRSI_Ursa_Rover_Emerald`, `Cutlass Black Best In Show Edition 2949` →
   `…DRAK_Cutlass_Black_ShipShowdown`, paint editions that are not separate hulls such as
   `MOLE Carbon` → the MOLE). Small and hand-kept; an alias whose target is not in the build
   is ignored.
3. **Exact normalised name.** Normalise: lower case, typographic quotes to ASCII, every run of
   non-alphanumerics to one space, trim. The row name is tried as given and with a leading
   maker removed (maker names and codes from a table keyed by the game's manufacturer code:
   `aegs` → `Aegis Dynamics`, `Aegis`; `cnou` → `Consolidated Outland`, `C.O.`, …). It is
   compared with each hull's long name, long name without maker, and short name.
4. **Without "Mk".** The same with `Mk <roman or digits>` removed on both sides
   (`Constellation Andromeda` = `Constellation Mk IV Andromeda`). A row with no Mk that fits
   both a Mk I and a Mk II hull is ambiguous, not guessed.
5. **Same words, any order**, ignoring `edition` (`Hercules Starlifter C2` = `C2 Hercules
   Starlifter`, `Spirit C1` = `C1 Spirit`, `Valkyrie Liberator Edition`).
6. **`ship_code`**: the code with non-alphanumerics removed equals the hull id's suffix the
   same way (`AEGS_Avenger_Titan`).

Tie-breaks before calling a row ambiguous: `manufacturer_code` (mapped to game codes:
`CRSD`→`crus`, `XIAN`→`xnaa`, `RSI`→`rsi`, …) keeps the hulls of that maker; then `ship_code`
(tier 6) picks among the rest; then, when the candidates have the **same display name**, the
hull whose key is the namesake of a player-flyable record wins (`890 Jump` → the hull, not
the hijacked variant).

Trial against HangarXPLOR's public ship table (305 names, includes concept ships and retired
store names) and the LIVE catalogue, before any alias: 250 matched (229 exact, 5 Mk, 11 word
order, 5 by code), 8 ambiguous (6 Hornets without Mk, 2 × `890 Jump`, the latter settled by
the same-name tie-break), 47 unmatched: about 30 are not in the game build (Kraken, Orion,
Hull D, …), the rest are alias-table material.

**Result.**

- Duplicate rows collapse into one hull with a **count**.
- A **custom name** (`ship_name` differs from `name` after normalisation) is kept per hull as
  a **rename suggestion**. It is never applied by the import.
- Report: `total`, `skipped` (not a ship), `hulls` (matched, with count), `unmatched` (by row
  name, with count), `ambiguous` (row name + candidate hulls), and `empty_catalog` when the
  channel's game data has not been read. There is no "not in this build" category: the
  importer cannot tell a concept ship from a typo, so both are `unmatched`.
- `--dry-run` (and the dialog's first step) writes nothing.
- A real import **replaces** the fleet list (the export is the whole hangar); `--merge` adds
  to it instead (count = the larger of the two). Hand matches survive either way. Hulls new
  to the fleet are appended to the end of the shared order list (§6.4), ungrouped; nothing
  is removed from the order list.
- Unmatched and ambiguous rows are kept (name, maker code, ship code, count, custom names) so
  they can be matched by hand later: pick a hull, or "Not in the game" to stop asking.

### 6.4 Data model and persistence

Three places. Unknown keys are kept on write in all of them.

```
data/
  fleet.json                 what the user owns (all installs, all channels)
  ship-names.toml            order, groups, renames, prefix style (all channels)
  channels/<install-id>/<CHANNEL>/profile.toml   [ship_names] use = shared | channel | off
```

**`fleet.json`** (written atomically, `fsutil::rename_retry`):

```json
{
  "schema": 1,
  "imported_at": "2026-11-14T20:00:00Z",
  "format": "xplor-json",
  "hulls": [{ "hull": "vehicle_NameAEGS_Avenger_Titan", "count": 2, "names": ["Rusty"] }],
  "pending": [{ "name": "Kraken", "maker": "DRAK", "code": "DRAK_Kraken", "count": 1,
                "names": [], "candidates": [] }],
  "matches": { "kraken": null, "my old bucket": "vehicle_NameDRAK_Cutlass_Black" }
}
```

`matches`: normalised row name → hull id, or `null` for "Not in the game". `pending` rows with
`candidates` are the ambiguous ones. No source file path is stored.

**`ship-names.toml`** (the shared settings; the same table can live in a channel profile):

```toml
schema = 1
enabled = false          # opt-in
style = "numeric"        # none | numeric | group-letter | symbol
separator = " "          # " " | "-" | ". "
symbol = "*"             # for style = "symbol": one of ! # * + - .
rest = "stock"           # stock | numbered
prefix_short = false     # also prefix the _short key

[[group]]
id = "combat"
name = "Combat"

[[ship]]                 # file order is the order
hull = "vehicle_NameAEGS_Gladius"
group = "combat"         # optional
name = "Needle"          # optional rename
short = "Needle"         # optional; defaults to `name`
```

- **Order.** Ships without a group form the first block, in file order. Then each group in
  `[[group]]` order, its ships in file order. A `group` id that does not exist counts as no
  group. A hull listed twice keeps its first entry.
- **Listed** = has a `[[ship]]` entry. Only listed ships get a prefix (so they float above
  the rest), unless `rest = "numbered"`.
- **Channel scope.** `profile.toml` gets `[ship_names] use = "shared"` (default when the
  table is absent), `"channel"` (the table also holds `enabled`, `style`, …, `group`, `ship`
  with the meanings above, and the shared file is ignored for this channel) or `"off"`. The
  default is therefore: one list, every channel of every install.
- **Stale detection.** `applied.json` gains `ship_names_hash`: SHA-256 of the canonical JSON
  of the channel's *effective* settings (after `use` is resolved), or the empty string when
  the effective settings are off or `enabled = false`. An old manifest without the field
  reads as empty, so nobody goes stale on upgrade. A difference shows as
  `Stale (ship names)`. It needs no extraction: hulls missing from a build are part of the
  hash and simply produce no edit. `fleet.json` is **not** an input of the build (counts and
  suggestions only), so an import alone never makes a channel stale; the hulls it appends to
  the order list do, when ship names are on.
- **Validation on load** (a bad entry is dropped with a warning, the rest is used):
  `name`/`short` trimmed, 1–48 characters, no control characters, none of `% < > ~ \`, not
  starting with `@`. `symbol` and `separator` from the lists above. `style =
  "group-letter"` with more than 26 blocks, or an unknown `style`, makes the whole table
  invalid: warning `ship-names-invalid`, the category generates nothing, the apply goes on.

### 6.5 Generator: `ship-names`

New `Category::ShipNames` (`ship-names`), last in the fixed order. No preset turns it on. No
other generator writes `vehicle_Name*` keys (ships get no name tag, spec 5.2), so there is
no ownership contest; if one is added later it runs after this one and loses the key.

For each listed hull that exists in this build's catalogue, at position `i` (1-based, in the
order of §6.4):

| Key | Written when | Value |
|---|---|---|
| `<hull>` (long name) | always | prefix + (`name` or the stock value) |
| `<hull>_short` | the key exists in stock, and (`name`/`short` is set or `prefix_short`) | (prefix when `prefix_short`) + (`short`, else `name`, else the stock value) |

- **Prefix by style.**
  - `none`: empty (rename only; an entry with no `name` then produces no edit).
  - `numeric`: `i`, zero-padded to the width of the largest number, at least 2: `01 `, `02 `;
    `001 ` from 100 ships on.
  - `group-letter`: block letter (`A` for the first block, ungrouped included) + position
    inside the block, padded to the width of the largest block: `A1 `, `B3 `, or `A01 ` when
    any block has 10 or more.
  - `symbol`: the symbol for every listed ship (`* `): they float, in name order.
  - Then the separator.
- **`rest = "numbered"`** continues `numeric` numbering through every other flyable hull of
  the build in stock-name order (prefix on the long key only). Ignored for the other styles.
- **Skipped, counted, no warning:** a listed hull that this build does not have
  (`not-in-build`, shown as info in the report and the UI).
- **Parts.** `GenEdit` gains `replace: Option<String>`: the value is `prefix` +
  (`replace` or the stock part) + `suffix`. The prefix stays a prefix part and a rename is a
  whole-value part (spec 4.3), so validation and "the generated part" keep working. The
  stock value is used untrimmed.
- **Validation** (spec 7.3, `enhance::check`): the usual per-value rules. A rename is
  checked as a whole-value replacement: stock has no format specifiers or variables in these
  keys, so a rename cannot bring any. A key that is a format key in some future build is
  left as stock (`format-key`).
- **Layering.** Generated < loc patches < user overrides, unchanged. An override (or loc
  patch) on `<hull>` or `<hull>_short` wins with its whole value, so the prefix is gone for
  that key. The UI and `fleet list` say so per ship ("Overridden in Strings"), and the
  preview sorts by the final value, override included.
- **Coverage.** The category is reported like the others; it has no baseline entry (default
  settings give zero candidates) and the 90 % written threshold applies only when it has
  candidates.

**What the user must be told** (on the screen, next to the switch): the prefix and the new
name show everywhere the game shows the ship's name, not only in the list: HUD target labels,
contracts, hangar and insurance screens, and on NPC ships of the same model.

### 6.6 Ships screen

Sidebar: **Ships** in the Customize group, after Blueprints (Alt+6; Backups, Activity and
Settings move to Alt+7, 8, 9). Settings are saved with **Save** / **Save & apply** like
Enhancements, with the same unsaved-changes dialog.

```
Ships   [Experimental]            Order and rename ships in game lists   ( ) on/off
Applies to: [All channels ▾]      Style: [None | 01 | A1 | Symbol]  Separator [ ␠ ▾]
                                  [ ] Also number all other ships   [ ] Prefix short names
┌ My ships (12) ────────── Import fleet ┐ ┌ [In-game preview | All ships] ────────────┐
│ ⠿ 01  Needle        Aegis Gladius  ×2 │ │ 01 Needle                                 │
│ ── Combat ─────────────────────── ⋯ ──│ │ 02 Aegis Avenger Titan                    │
│ ⠿ 02  Aegis Avenger Titan   “Rusty”?  │ │ …                                         │
│ ⠿ 03  Anvil F7A Hornet Mk II          │ │ Aegis Eclipse            (not in my list) │
│ + New group                           │ │                                           │
└───────────────────────────────────────┘ └───────────────────────────────────────────┘
3 ships from your import are not matched  [Review]
```

- **My ships** (left): the order list. Row: drag handle, prefix chip, name (inline edit),
  the stock name in muted text when renamed, owned-count badge (`×2`, from `fleet.json`; no
  badge for a ship added by hand), state badges (`Overridden in Strings`, `Not in
  {CHANNEL}`), row menu (Move to top, Move to bottom, Move to group…, Reset name, Remove
  from list). Group headers are rows too: rename, move the block, delete the group (its
  ships become ungrouped). A rename suggestion from the import shows as a chip (`Use
  "Rusty"`); with two copies and two custom names both chips show, and a hint says one model
  has one name in the game.
- **In-game preview** (right, default tab): the list as the game would sort it, final values
  after overrides. Shows the user's ships and, below them, the neighbours from the full
  catalogue so the float effect is visible; "My ships only" toggle. A caption states the
  sort rule used (case-insensitive, character by character) and that it is unconfirmed.
- **All ships** (right, second tab): searchable catalogue of flyable hulls (name, maker,
  ship / ground), facet by maker, **Add** per row (Enter adds the focused row), already
  listed rows marked.
- **Drag** reorders within and across groups; dropping on a group header puts the ship
  first in that group.
- **Keyboard** (the list is one tab stop with a roving focus; Up/Down move the focus):

  | Keys | Action |
  |---|---|
  | Alt+Up / Alt+Down | Move the ship one place; crosses group borders |
  | Alt+Home / Alt+End | Move to top / bottom of the whole list |
  | Alt+Shift+Up / Down | Move to the previous / next group |
  | Space on the handle | Pick up; arrows move; Space drops; Esc puts it back |
  | F2 or Enter | Rename inline; Enter saves, Esc cancels; empty = reset to stock |
  | Delete | Remove from the list (toast with Undo) |

  Every move is announced in the polite live region ("Gladius, position 3 of 12, group
  Combat"). Reduced motion: no reorder animation.
- **Scope select:** "All channels" (the shared file), "Only {CHANNEL}" (copies the shared
  settings into this channel's profile, `use = "channel"`), "Off for {CHANNEL}". Going back
  to "All channels" asks before dropping the channel's own list.
- **Import dialog** (same shell as the blueprint import): choose a file or paste text → dry
  run → report (hulls with counts, unmatched, ambiguous with a pick per row, rename
  suggestions listed) → Import. Replace is the default; "Add to my current fleet" is a
  checkbox.
- **Unmatched review:** one row per pending name with a Combobox over the catalogue and
  "Not in the game".
- **Empty states.** No game data read: "Read game data first" with the action. No fleet and
  an empty list: full-panel empty state with three actions: Import fleet (primary; says
  where RSI+ puts the export), Paste a list, Pick ships by hand. Fleet imported but the
  feature off: the list is editable and a banner says nothing is written until it is on.
  Search with no hit: "No ship matches" with Clear.
- **Live preview is computed in the front end** (drag must not wait for the back end). The
  prefix and sort rules exist twice (Rust, TypeScript) and both are tested against one
  shared vector file (§6.9).

Contract (Rust contract + TS types/client + mock + samples + drift tests): `fleet_view`
(catalogue, fleet, effective settings, scope, per-ship state), `fleet_import` (file dialog or
pasted text; `dry_run`, `merge`), `fleet_match` (pending name → hull or none),
`save_ship_names` (scope + settings), `fleet_clear`.

### 6.7 CLI

```
starorg-strings fleet import <file|-> [--dry-run] [--merge] [--format auto|xplor-json|xplor-csv|text]
                                      [--channel LIVE] [--json]
starorg-strings fleet list [--channel LIVE] [--json]
starorg-strings fleet match <row name> <hull id | none>
starorg-strings fleet clear
```

- `import` needs a hull catalogue: it uses the channel's extraction cache, or extracts from
  `Data.p4k` read-only on a miss (as `blueprints import` does; a dry run caches nothing).
  `--channel` picks which build to match against (default channel otherwise). Exit 0 with
  the report, also when rows are unmatched; exit 1 for an unreadable or unrecognised file or
  an empty catalogue.
- `list` prints the paths of `fleet.json` and the settings file in use for that channel,
  then one line per listed ship in order: prefix, final name, stock name, count, flags
  (`overridden`, `not-in-build`), then the pending names.
- Order, groups, renames and style are edited in `ship-names.toml` (or the channel's
  `profile.toml`) by hand; `fleet list` is the check. No `fleet move` commands in this
  version.
- `apply`, `status` and the hooks need nothing new: the category runs with the build and
  `ship_names_hash` feeds stale detection.
- The CLI's default dependencies stay free of GTK and HTTP; the importer is `starorg-core`
  only (`serde_json` + a small CSV reader, no new crates).

### 6.8 Privacy

- The import is a local file read. No network, ever.
- Stored: hull id, count, custom ship names, and for unmatched rows the row's ship name,
  maker code and ship code. **Not stored:** pledge ids, pledge names, costs, dates, LTI,
  warbond, insurance, how a ship was acquired, the path of the imported file.
- Custom names and the size of the fleet are personal: `doctor` and exported diagnostics show
  counts only; the activity log names hulls, not custom names.
- Test fixtures are synthetic (made-up pledge ids and names). A real export is used only in
  an ignored local test and is never committed.

### 6.9 Tests

- **Importer (unit):** each format from a synthetic fixture; auto-detection; BOM, CRLF,
  quoted CSV with `""` and commas; counts in text lines; non-ship rows skipped; size and row
  limits; refusals. Fixtures compared byte for byte are listed in `.gitattributes` as `-text`.
- **Matching (unit, synthetic catalogue):** one case per tier and per tie-break; Mk
  ambiguity; same-name tie-break; saved match and "not in the game"; alias with a missing
  target; duplicates collapse to a count; custom name becomes a suggestion and is not
  applied.
- **Persistence:** unknown keys survive a round trip in all three files; `--merge` vs
  replace; hand matches survive a re-import; atomic write through `rename_retry` (Windows CI).
- **Generator (golden):** every style and separator; padding at 9/10 and 99/100 ships; group
  blocks; `rest = "numbered"`; `prefix_short`; rename with and without a short name; `,P`
  keys keep their spelling; hull missing from the build; invalid table generates nothing.
- **Layering and apply:** an override on the long key wins and is reported; whole-file rules
  still pass; `ship_names_hash` empty when off, changes on reorder, old manifest reads as
  not stale; `use = shared | channel | off`.
- **Shared vectors:** `fixtures/ship-names-vectors.json` (settings + catalogue → expected
  prefixes and preview order), written by a Rust test under an update flag, read by the Rust
  generator test and by the TypeScript preview test.
- **CLI:** `fleet import --dry-run` writes nothing; `--json` shapes; exit codes.
- **UI:** component tests for the keyboard model (each row of the table in §6.6), rename
  inline, empty states; contract drift tests and samples; headless renders of the screen
  (empty, imported, grouped, review) at the supported sizes.
- **Local integration (ignored):** the public HangarXPLOR ship table against each installed
  channel's catalogue must not match fewer rows than the trial in §6.3; every alias target
  exists in LIVE.

### 6.10 Open questions

| # | Question | Recommended default |
|---|---|---|
| Q23 | Do the in-game ship lists sort by the displayed name, and with which comparison (case, `2` vs `10`)? | Assume yes, case-insensitive, character by character; zero-padded numbers are safe under any comparison. "Experimental" badge until checked |
| Q24 | FleetYards' own export format | Not supported; its HangarXPLOR import/export covers it. Add when a sample file is at hand |
| Q25 | Do lists use the long or the `_short` name? | Prefix the long key only (`prefix_short = false`); flip the default if the check says short |
| Q26 | Import replaces or merges the fleet list? | Replace; `--merge` / checkbox for the other |
| Q27 | Take the short-name key from DataCore (`AttachDef.Localization.ShortName`, needs a `GameData` schema bump) or from the `_short` convention? | Convention now; add `ShipLoc.short_name` with the next schema bump |
| Q28 | Fleet from the StarOrg Tools account link (§5) as a further import source | After the account link ships; same report and storage |
| Q29 | A "drop the maker" option (`01 Gladius` instead of `01 Aegis Gladius`) | Not in this version: the stock short names are abbreviated, so it needs its own name source. A rename covers single ships |

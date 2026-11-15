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

## 9. Rust port of sc-gamedata (branch `gamedata-port`, Refs #12)

The published dumps come from sc-gamedata (TypeScript, StarBreaker CLI). Its extraction is
being ported into the workspace as `crates/starorg-gamedata`, so the app can produce the
same datasets itself instead of only downloading them.

- **Scaffold:** record layer (record sets, references, components, localization with the
  publish-dump redaction off by default, CryXmlB), a `Domain` trait with a registry of one
  pre-listed slot per sc-gamedata domain (parallel ports touch separate lines), and a
  JSON writer reproducing `JSON.stringify(_, null, 2)` (numbers, key order, header).
  Records come from StarBreaker's own exporter via a new additive `FullDataCore` in the
  `starorg-data` adapter; `GameData` is untouched.
- **Ported:** `game-manufacturers` (template) and `game-strings-english`, byte-identical to
  the golden dumps of LIVE 4.10.200.58746 apart from `_extracted`.
- **Gate per domain:** `scripts/compare-gamedata.sh --domain <name>` (dev harness
  `gamedata-port`, not in the shipped CLI) prints `identical` against
  `~/git/sc-gamedata/data`. Porting rules: `crates/starorg-gamedata/README.md`.
- **Out of scope:** `uex-item-prices.json` and `uex-terminals.json` (third-party UEX API
  data, not game files) and sc-gamedata's query/CLI tools.
- **Next:** port the remaining domains (order in the crate README), then move
  `starorg-data`'s `GameData` onto this crate and drop the duplicated accessors.

---
title: StarOrg Codex v1 — addendum (user requests 2026-11-14)
tags: [docs, plans, specs]
status: draft
updated: 2026-11-15
---

# StarOrg Codex v1 — addendum (user requests 2026-11-14)

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

Native "Connect StarOrg Tools" in the app (CLI: `starorg-strings sync status|set` only; see
5.1). Opt-in, off by default
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

### 5.1 Implementation (branch `starorg-client`, 2026-11-15, Refs #15)

Server side: SC-ORG-BOT#1659 (commit `d8ed2d3a`), behind `STRINGS_AUTH_ENABLED` (on in dev, off
in prod until the owner enables it). The client follows its real routes; nothing is invented.

- **Environment** (owner, 2026-11-15: "the option to set dev/prod"): Settings -> StarOrg Tools ->
  Production (`https://starorg.tools`) / Dev (`https://dev.starorg.tools`). Stored in
  `config.toml` `[sync] environment`; the CLI sets it with `starorg-strings sync set
  --environment production|dev`; a bare `sync set` saves `$STARORG_ENV` (it never rides along
  on other changes). Tokens and account cards are per environment; switching (after a confirm)
  forgets the old one, revoking it when the network switch is on and the host answers, before
  the new environment is saved. A change made outside the app (CLI, hand edit) is caught the
  next time the app looks at StarOrg Tools, which signs the other environment out the same way.
- **Sign-in** (`starorg_net::starorg`): probe `GET /api/auth/strings/login` (404 = disabled on
  that server, refused before a browser opens) -> PKCE S256 (86-char verifier, 43-char state) ->
  listener on `127.0.0.1:<ephemeral>` -> system browser to `/api/auth/strings/login?redirect_uri=
  http://127.0.0.1:<port>/callback&code_challenge=…&code_challenge_method=S256&state=…` ->
  `/callback?code=&state=` (other paths 404; a foreign `state` ends the attempt; `error=` =
  cancelled) -> `POST /api/auth/strings/exchange {code, code_verifier, redirect_uri}` ->
  `{access_token, expires_in, token_type, aud: "strings", user}`. Then `GET /api/auth/me` for
  the orgs (`guilds[]`: Discord id, name, features); the first org with `industrial` is picked,
  changeable. Cancel while waiting (`starorg_cancel_login`); 5-minute timeout.
- **Token**: OS keyring (`keyring` 3, service `starorg-codex`, user `starorg-tools-<env>`), else
  `<config>/starorg-tokens/<env>.token` (0600 in a 0700 folder on Unix). No refresh token exists
  on the server: the 90-day token is dropped at expiry or on a 401, and the status asks to sign
  in again (`expired`). Sign out = `POST /api/auth/logout` (when the network switch is on) +
  delete. The keyring is only touched once an account card says a token exists.
- **What to sync** (owner: "option to import blueprints from logs or not with a toggle and other
  things"): `[sync]` `import_game_log` (on; gates every log scan in app, CLI `blueprints scan`
  and `hook postexit` via `logscan::scan_if`), `push_blueprints`, `pull_blueprints`,
  `pull_fleet`, `auto_sync` (off). Each command refuses while its switch is off; nothing is sent.
- **Push**: union of ticked characters -> public `GET /api/industrial/blueprint-lookup` (no
  token) -> each local entry to one server id by record key (`canonical_internal_key`), else by a
  single normalised name or alias; ambiguous/unknown reported as `unmatched`, not sent -> `POST
  /api/industrial/crafting/guilds/{guild}/my-blueprints {blueprint_ids, source:
  "starorg_strings", acquired_at}` in chunks of 2000. Never removals (`…/remove` is not called).
  `acquired_at` carries receipt times only (log line, or the server's own from a pull; never a
  Smart Citizen export time) and only well-formed ISO datetimes, since one bad value makes the
  server refuse the whole chunk. A grant without `aud: "strings"` is refused.
- **Pull**: `GET …/my-blueprints` -> union into one character with source `starorg:<org>`; names
  not in this build's game data (when read) are `unmatched` and skipped.
- **Hangar**: `GET /api/fleet/hangar` -> HangarXPLOR-shaped rows -> the fleet importer
  (`FleetSource::StarOrg`; dry run in the dialog, merge in Sync now).
- **Sync now / after exit**: `starorg_sync` runs pull (active character), push and hangar as
  switched on. With `auto_sync`, a 20 s watcher in the app syncs a set-up channel when its game
  stops, and the post-exit hook (no HTTP in the CLI) leaves `starorg-sync-pending.txt` for it.
- **Contract (additive)**: `Settings.starorg: StarOrgSettings`; `StarOrgStatus` + `host`,
  `guild_id`, `guilds`, `expired`, `token_store`; `StarOrgGuild`, `TokenStoreKind`, `SyncRun`,
  `StarOrgGuildRequest`; `FleetSource::StarOrg`; commands `starorg_cancel_login`,
  `set_starorg_guild`, `starorg_sync`. Rust, TS, both mocks, samples.
- **Tests**: PKCE (RFC 7636 vector, server rules), loopback-only redirect, state mismatch,
  cancel, timeout; client routes and the disabled/401/404 cases against
  `starorg_net::starorg::mock::MockServer` (127.0.0.1, random tokens); token stores (0600);
  real backend: network switch off sends nothing, each switch off sends nothing, sign-in, push,
  pull, hangar, org change, sign out revokes (and says when it could not), environment switch
  in the app and from the CLI, a token refused on the first call, revoked token, Game.log off
  reads no log, auto sync after exit and on the hook note; CLI `sync` and the gates; renders
  `app/scripts/render-starorg.cjs`.
- **Not done**: no refresh (server has none); the CLI has no `starorg login|pull|push` (it links
  no HTTP client by design); RSI-handle/character dimension on StarOrg (Q22 option c) remains a
  server follow-up; production sign-in waits for `STRINGS_AUTH_ENABLED` in prod.

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
  `TheCollector_ShipMod_*`, `vehicles_NameOrbital_Sentry*`). Six of them are player-ownable
  Wikelo hulls (`ANVL_Hornet_F7_Mk2_Collector_Mod` → `TheCollector_ShipMod_F7_Hornet_VehicleName`
  = `Anvil F7 Hornet Mk Wikelo`; the Fortune, Guardian QI, Pulse, Ursa Medivac and Nox Wikelo
  specials), so they are hulls too (§6.2); their short keys end `VehicleNameShort` without the
  `_`. The wreckage and sentry keys stay out.
- Two flyable LIVE hulls spell the short key differently from the long key:
  `vehicle_NameAEGS_Firebird_Collector_Milt` / `vehicle_NameAGES_Firebird_Collector_Milt_short`
  and `vehicle_NameKRIG_L22_Alpha_Wolf` / `vehicle_NameKRIG_L-22_Alpha_Wolf_short`.
- Channels differ (LIVE has 8 hulls PTU lacks, HOTFIX has 2 LIVE lacks), so a shared list
  must tolerate hulls a build does not have.

### 6.2 Hull catalogue

`fleet::Catalog`, built from `GameData.ships` + stock, cached with the extraction (no schema
change).

- A **hull** is a stock key that is referenced as `loc.name` by at least one ship record,
  does not end in `short` (folded), whose stock value is not empty or a placeholder, and that
  either starts with `vehicle_Name` (folded) or is named by a player-hull record whatever its
  prefix as long as it still contains `VehicleName` (ignoring case and `_`). The second rule
  takes in the Wikelo `TheCollector_ShipMod_*_VehicleName` keys and leaves out
  `vehicles_NameOrbital_Sentry`. LIVE then has 292 flyable hulls (PTU 284, HOTFIX 287).
- **Short key**: `<hull>_short`, else `<hull>Short`; else, only when exactly one stock key fits,
  a `…short` key equal to the hull id with punctuation ignored (`KRIG_L-22` for `KRIG_L22`) or
  with the maker code's letters transposed (`AGES` for `AEGS`).
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
| *Other CSV* | Any other first line with three or more comma-separated fields, or a tab | Refused: "not a fleet export: no Manufacturer and Ship columns". Never read as text |
| **Plain text** | Anything else | One ship per line; blank lines and `#` comments ignored; an optional count (`2x Gladius`, `Gladius x2`); a maker in front is fine (`Aegis Gladius`). Lines with `,` `;` tab `"` `|` or over 100 characters are skipped (counted in `skipped`) |

- Source of truth for the JSON and CSV shapes: `rsi-plus/src/lib/export.ts` (`XplorRow`,
  `starOrgRows`, `xplorCsv`, `shipLines`). The RSI+ StarOrg preset (`acquired`, no
  `pledge_date`) is the same shape and needs no special case.
- **Never read into memory beyond the parse, never stored:** `pledge_id`, `pledge_name`,
  `pledge_date`, `pledge_cost`, `lti`, `warbond`, `insurance`, `acquired`.
- **A record of another file is never a ship name.** A spreadsheet re-export or a headerless
  paste would otherwise put whole rows (pledge name, cost, date) into `pending[]` as unmatched
  names; the *Other CSV* and text rules above stop that (synthetic test with a
  `Name,Manufacturer,LTI,Pledge ID,…` file).
- Limits: UTF-8 (BOM dropped), 5 MB, 5,000 rows. A JSON object at the top level, or an array
  with no usable row, is refused with "not a fleet export".
- **CSV caveat.** In the CSV, `Ship` holds the custom name when the ship has one, and the hull
  name is then nowhere in the row. Such rows come out unmatched; once matched by hand the row
  text becomes the rename suggestion. The JSON export has no such gap, so the UI recommends it,
  and the report says so (CLI note, a line in the dialog's "Not matched" box) when a CSV import
  has unmatched rows.
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
   is ignored. Tried with the row name as given and with a leading maker name removed
   (`Drake Interplanetary Dragonfly Black`, the RSI+ `shipLines` shape).
3. **Exact normalised name.** Normalise: lower case, typographic quotes to ASCII, every run of
   non-alphanumerics to one space, trim. The row name is tried as given and with a leading
   maker removed (maker names and codes from a table keyed by the game's manufacturer code:
   `aegs` → `Aegis Dynamics`, `Aegis`; `cnou` → `Consolidated Outland`, `C.O.`, …). It is
   compared with each hull's long name, long name without maker, and short name.
4. **Without "Mk".** The same with `Mk <roman or digits>` removed on both sides
   (`Constellation Andromeda` = `Constellation Mk IV Andromeda`). A row with no Mk that fits
   both a Mk I and a Mk II hull is ambiguous, not guessed, unless its `ship_code` equals one
   of those hull ids exactly (`F7C Hornet` + `ANVL_Hornet_F7C` = the Mk I).
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

### 6.11 Implementation notes (branch `fleet-order`, 2026-11-14)

Built as specified above; local only, not pushed. Differences and findings:

- **Core.** `starorg_core::fleet` (`catalog`, `import`, `store`, `names`). `enhance::generate`
  is unchanged; `generate_with(.., Some(&ShipNames))` adds the category, and `apply`, the app's
  Strings table / previews and `build --ship-names FILE` call it with the channel's active
  settings. In `merge`, a rename travels as a whole value (`GenEdit.whole` = prefix + rename),
  so loc patches' `from_part = "generated"` see the full generated text.
- **Contract.** `fleet_view` takes `{ target, read_game_data }` (the "Read game data" action of
  the empty state runs the extraction as a job). `FleetView` also carries `shared_settings`:
  going back from "Only {CHANNEL}" to "All channels" loads them into the draft (the UI asks
  first). `fleet_import` sources are `file` (native picker), `text` (pasted) and `last` (the
  file picked for the dry run, kept in memory, so Import does not ask again).
- **Alias table** (`crates/starorg-core/data/fleet-aliases.toml`, 14 entries): Ursa Rover /
  Fortuna, Cutlass Black BIS 2949, 600i Explorer, Dragonfly Black, F7C-M Heartseeker Mk I,
  C8R Pisces, Carrack w/C8X and Expedition w/C8X, the MOLE Carbon / Talus paint editions,
  Mustang Alpha Vindicator. Every target exists and is flyable in LIVE (ignored test).
- **Matching, HangarXPLOR's public table (305 rows, synthetic JSON export)** with aliases:
  LIVE 265 matched (232 exact, 14 alias, 5 Mk, 11 word order, 3 code), 6 ambiguous (the six
  Hornets without Mk), 34 unmatched; PTU 258; HOTFIX 265. After the exact-ship-code rule for
  Mk rows and the Wikelo hulls (verifier fixes): LIVE 271 matched (11 Mk), 0 ambiguous, 34
  unmatched; PTU 264; HOTFIX 271. The unmatched ones were checked:
  concept ships and keys no ship record names (Hull D/E, Kraken, Orion, Odyssey, Ranger, G12,
  ATLS, Nautilus, Retaliator Bomber, Zeus Mk II MR, …).
- **Finding: letter prefixes interleave.** With `group-letter`, `A2 …` sorts before
  `Aegis …` but `B1 …` after `Argo …` (case-insensitive, letter by letter), so groups after
  the first do not float above the rest. The preview shows it and the screen says so next to
  the style; numbers and symbols float. Same rule in Rust and TypeScript (shared vectors).
- **UI.** Ships screen (Alt+6; Backups/Activity/Settings Alt+7/8/9), the import and review
  dialogs; the row menu of §6.6 is a set of row buttons (rename, reset, top, bottom, remove)
  plus the keyboard model, and "Move to group…" is Alt+Shift+Up/Down or a drag. Headless
  renders: `app/scripts/render-fleet.cjs`; keyboard check: `app/scripts/keyboard-fleet.cjs`.
- **CLI.** `fleet import|list|match|clear` as in §6.7; `fleet clear` keeps hand matches and the
  order list. Test hook `--test-gamedata FILE` (with `--test-stock-ini`, feature `test-hooks`).
- **Still open:** the in-game checks of spec §13 (Q23, Q25, where else the name shows, symbol
  rendering), Q24 (FleetYards native format), a real export only in the ignored local test.

## 7. Developer mode (explorer, dataset viewer, diff)

Off by default. Decided scope: a **raw DataCore explorer plus a generic dataset viewer and
diff**; we do not port `sc-gamedata`'s 28 TypeScript parsers. Read only: nothing in this
section writes to the game folder or changes what `apply` produces.

### 7.1 Feasibility, measured (2026-11-14, read only, pinned StarBreaker `041e5592`)

Release build, 32 threads, warm page cache, `Data.p4k` opened read only.

| | LIVE 4.10.200.58746 | PTU | HOTFIX |
|---|---|---|---|
| `Game2.dcb`, decompressed | 341.0 MB | 353.5 MB | 331.4 MB |
| Records (all / main) | 118,499 / 63,440 | 121,814 / 65,897 | 116,921 / 61,963 |
| Struct types defined / used by a record / with a main record | 6,700 / 604 / 567 | 6,854 / 618 / 578 | 6,694 / 604 / 567 |
| Enum definitions, property definitions | 775, 23,810 | 781, 24,057 | 774, 23,788 |
| p4k open (index of 1.40 M entries) | 0.50-0.72 s | 1.03 s | 0.58 s |
| Read + decompress DCB and `global.ini` | 0.13-0.20 s | 0.17 s | 0.13 s |
| `Database::from_bytes` | 8-12 ms | 9 ms | 9 ms |
| **Full record index** (name, path, GUID, type, main flag, tag for every record) | **69-85 ms** | 70 ms | 70 ms |
| Whole run, wall | 0.84 s | 1.44 s | 0.95 s |
| Peak RSS | 866-868 MB | 885 MB | 847 MB |

More from LIVE:

- Index strings are 20 MB (names + paths + GUID text). Main records sit in 249 top-level
  folders below `libs/foundry/records/`; one file per main record (63,440 files).
- Largest types: `EntityClassDefinition` 30,083, `DialogueContext` 23,831 (173 main), `Tag`
  18,961 (none main), `BuildingBlocks_Canvas` 3,905, `MissionBrokerEntry` 2,584.
- Record tags (DCB v8 `tag_offset`): 25 values, but 86,038 records are `Unknown`, so the tag is
  a detail field, not a tree level.
- Search: a plain lower-case substring scan of name + path over all 118,499 records takes 6 ms.
- One record as compact JSON: p50 11 KB, p90 88 KB, p99 276 KB, p99.9 767 KB, largest 4.3 MB.
  2,000 records one after another: 0.11-0.14 s.
- **Every main record** as JSON: 2,042 MB in 0.41 s wall (about 8 s CPU), no failures.
  Per type: `EntityClassDefinition` 1,312 MB, `BuildingBlocks_Canvas` 471 MB,
  `MissionBrokerEntry` 56 MB, `ContractGenerator` 49 MB (113 records).
- After the p4k index is dropped the process holds about 616 MB (the DCB bytes plus allocator
  leftovers); the steady need is the DCB (341 MB) plus the index.

API check (`starbreaker-datacore`, pinned commit): `Database::{records, record_by_id,
is_main_record, records_of_type, records_by_type_name, struct_name, struct_defs,
resolve_string, resolve_string2}` cover the index and lookups. `walker::walk_record` is public
and generic over `sink::ExportSink`, whose `write_record_ref(name, record_id, record_name,
path)` hook lets our own sink write references as objects instead of relative file paths.
`export::write_json` streams to any `Write`. `dcb_builder::DcbBuilder` (public) builds a small
valid DCB for tests. Nothing needs a StarBreaker change.

Not measured: a cold disk cache, and machines with 4-8 cores (CPU time above divided by cores
is the estimate).

### 7.2 Data flow and process model

```
Dev screen opens ─► app starts `starorg-strings-app dev-worker --p4k <P>`   (child process)
                      open p4k ► count check ► read Game2.dcb (+CRC) ► drop p4k ► parse ► index
                      then answers requests: JSON lines on stdin/stdout
app backend (async command) ─► worker request ─► reply ─► webview (paged)
```

- **Child process, as in spec 3.3.** DataCore parsing can panic on bad data; a crash is a
  "worker stopped" banner with Retry, never an app crash. The UI thread never touches
  StarBreaker. The same steps and checks as extraction are reused (`extract.rs`: count check,
  size limits, CRC-32C).
- **One worker, for the selected channel.** Started on the first Dev request, restarted when
  the channel or its build fingerprint changes, stopped after 5 minutes without a request and
  when the app exits (the parent closes stdin; the worker exits on EOF). Starting takes the
  machine-wide `locks/extract.lock` until the p4k index is dropped, so it never overlaps an
  extraction's peak.
- **Lazy everywhere.** The worker keeps the DCB bytes and the index only. Record JSON is made
  per request and not kept (an LRU of the last 32 records, at most 16 MB, serves
  back/forward). The webview never receives the whole index: tree levels, search hits and table
  rows are paged.
- The game may be running: `Data.p4k` is opened read only and shared; no lock is taken on
  game files.

### 7.3 Memory and time budget

| | Budget | Measured basis |
|---|---|---|
| Worker start to first tree | under 2 s warm, progress shown | 0.84-1.44 s |
| Worker peak while starting | about 0.9 GB for under 1 s | 847-885 MB |
| Worker steady | under 450 MB (DCB + index + LRU) | 341 MB DCB + 20 MB index strings |
| Tree level / search reply | under 50 ms | 6 ms full scan |
| Record open | under 100 ms for p99, 4.3 MB worst case under 500 ms | 0.06 ms average export |
| Webview | never more than 500 tree rows, 500 hits, or 1 MiB of record JSON per reply | |
| Export whole type | streamed, under 64 MB extra memory | 1.3 GB written for the largest type |

`unsafe` is forbidden in our crates, so the worker holds the DCB in memory instead of
memory-mapping a cached copy. Nothing new is written to the cache by default (see Q-D1).
The steady 341 MB is the cost of having Dev mode open; it is returned when the worker stops.

### 7.4 Explorer

- **Tree.** Two roots switchable with a toggle: *By type* (struct type ► records; 604 types,
  sorted by name, count badges, "main only" on by default) and *By path* (the 249 folders ►
  sub-folders ► records). Children are fetched per node, 500 at a time ("Show 500 more").
- **Search.** One box, server side, over record name, path and GUID. A full or partial GUID
  matches by prefix. Otherwise fuzzy: every query token must appear (case-insensitive);
  rank = exact name > name prefix > name substring > path substring > in-order subsequence.
  Optional `type:` prefix filter (`type:EntityClassDefinition behr`). 200 ms debounce, 500
  hits at most, with the total count.
- **Record detail.** Header: name, type, path, GUID, tag, main flag, JSON size; copy buttons
  for path and GUID. Body: collapsible JSON tree, virtualised (only visible lines in the DOM),
  first two levels open. Above 1 MiB the reply is cut at depth 3 and deeper nodes load on
  expand (`pointer` + `depth`). Search inside the record (Ctrl+F) highlights and steps through
  matches.
- **References.** Our sink writes each reference as `{"_ref": "<guid>", "name", "type",
  "path"}`. The tree shows it as a link; Enter or click navigates. Same-file sub-records stay
  inline (StarBreaker's behaviour). Strings starting with `@` that are keys of the channel's
  stock `global.ini` show the English text beside them and link to the Strings screen.
- **History.** Back/forward stack of visited records (Alt+Left / Alt+Right, mouse buttons),
  50 entries, per session.
- **Export.** "Export record…" and "Export all `<type>` records…" open a native save dialog
  from Rust (the webview never sends a path, as today). One record: pretty JSON. A type: one
  JSON array written record by record (`[`, record, `,` …, `]`) to `<name>.tmp` then renamed;
  a cancellable job with progress; the dialog states the record count first and warns above
  5,000 records ("`EntityClassDefinition` is about 1.3 GB"). Exports use StarBreaker's own
  JSON form (references as relative paths), so files match `starbreaker` CLI output.

### 7.5 Dataset viewer

**Sources** (a list at the top of the Datasets screen):

1. **This build** – our own `GameData` from the extraction cache, one dataset per table
   (components, weapons, missiles, ammo, ships, contracts, blueprints, blueprint pools,
   reputation standings). Always available, no extra work.
2. **Local folder or file** – chosen with a native dialog; up to 5 remembered in
   `config.toml` (`[dev] folders`). Only `*.json` directly inside, at most 64 MiB each,
   parsed as data only. Example: `~/git/sc-gamedata/data` (30 files, 51 MB; largest
   `game-strings-english.json` 11.3 MB, `uex-item-prices.json` 24,141 rows).
3. **Published dump for this exact build** – `voidput/sc-gamedata-dumps`, through the existing
   `source::dump::fetch_verified` + `starorg-net::GitHubTransport` (on main since the update
   check landed; the app already links `starorg-net`). Enabled only when Settings ► Network ►
   "Use published game data" is on; otherwise the row reads "Off (Settings ► Network)". Same
   build match, blob-SHA and SHA-256 checks and per-`<tag>/<commit>` cache as addendum §1.
   Dumps already cached are listed as sources even with the switch off (no request needed).
   This is the last slice; if it slips it ships as a labelled follow-up and 1-2 stand alone.

**Shape inference** (generic; checked against all 30 `sc-gamedata` files):

- A file is an envelope object. Keys starting with `_` (`_source`, `_extracted`, `_fetched`,
  `_build`) are metadata shown in the header; `summary` is shown as header facts.
- Every top-level **array of objects** is a table (`game-weapons.json`: `shipWeapons` 241,
  `fpsWeapons` 492, `missiles` 67). A top-level array file is one table.
- A top-level **object of objects or arrays** (`byManufacturer`, `byCode`, `oreLocations`) is
  an index table: key column + value; listed under "Indexes", collapsed.
- An **object of scalars** (`strings`: 91,358 key ► text) is a two-column table.
- **Arrays of arrays** with a sibling `columns` list of the same width
  (`game-search-index.json`) use those names; otherwise `0…n`.
- Anything else is shown as a JSON tree only.

**Table.** Virtualised like Strings (pages of 200, about 40 rows in the DOM). Columns = union
of keys over the table in first-seen order, each with a type (text, number, bool, object,
list, mixed). Default visible: `name`, `recordName`, `id` first, then the first scalar
columns up to 8; the column picker (searchable checklist) is remembered per dataset+table in
the webview's local storage. Nested cells show `{…}` or `[n]`; text cells are cut at 200
characters in the page (full value in the detail). Sort by any scalar column.

**Search and filters.** Substring over all scalar cells (or the picked columns), server side.
Per column: value chips when a text column has at most 50 distinct values, min/max for
numbers, "has a value". Counts shown as `shown / total`.

**Record detail.** Side sheet with the same JSON tree component as the explorer.
- **Loc keys:** a string that is `@key` or equals a key of the channel's stock `global.ini`
  (case-insensitive) gets the English text beside it and a link to the Strings screen. In our
  own `GameData` these are the `loc` fields; the published dumps carry resolved names only
  (§1), so there this mostly applies to `game-strings-english.json`.
- **Explorer links:** a GUID-shaped value found in the DataCore index, or a
  `recordName`/`className` value equal to a record name, gets "Open in Explorer".

Parsing runs in the backend on a blocking thread (plain `serde_json`, no worker needed). Two
parsed datasets are kept (LRU); a file is re-read when its size or mtime changes.

### 7.6 Diff

- Pick **A** and **B** from the sources above: two builds (LIVE vs PTU, or two cached dumps),
  or local vs published for the same build.
- **Summary:** one row per dataset table present in either side: added / removed / changed /
  same, sorted by changed + added + removed, largest first; tables only in one side are
  marked.
- **Record identity:** the first of `id`, `recordName`, `className`, `name`, `key`, `code`
  that is present and unique on both sides; otherwise position, flagged "by position".
  The user can override the identity column per table.
- **Record list:** changed records first, ordered by number of changed fields; filter by
  added/removed/changed and by changed field.
- **Field diff:** values flattened to JSON-pointer paths; old ► new, numbers with delta and
  percent; lists of objects matched by the same identity rule, else by index. Envelope
  metadata (`_extracted`, `_fetched`, `_source`) is ignored.
- Budget: both sides of all 30 files (about 100 MB of JSON) compared in under 5 s on a
  blocking thread, with progress; the result is kept in memory until a source changes.

### 7.7 Contract (self-contained)

`Settings` gains `advanced: { developer_mode: bool }` (default false; missing = false, so
older config files load). Everything else lives in new files so the fleet-order and
update-check work does not conflict: `app/src-tauri/src/contract/dev.rs` (+ `real/dev.rs`,
mock in `mock_dev.rs`), `app/src/lib/contract/dev.ts` (types + client, re-exported), samples
added to `samples.gen.json` by the generator, covered by the two drift tests. Every dev
command returns error code `dev-disabled` while the setting is off.

| Command | Request ► response |
|---|---|
| `dev_status` | target ► worker state (`stopped`/`starting`/`ready`/`failed`), build, record/type counts, start time, memory |
| `dev_open` | target ► job id (progress events as today) |
| `dev_tree` | target, root (`type`/`path`), node id, offset, limit, main_only ► children (id, label, kind, count) + total |
| `dev_search` | target, query, limit ► hits (guid, name, type, path, main) + total |
| `dev_record` | target, guid or path or name, optional pointer + depth ► header + JSON text (at most 1 MiB) + `truncated` |
| `dev_export_record` / `dev_export_type` | target, guid / type ► save dialog ► job id, or `cancelled` |
| `dev_sources` | target ► sources (kind, label, build, state) with datasets (file, tables, row counts, bytes) |
| `dev_add_source` / `dev_remove_source` | folder/file dialog ► updated sources |
| `dev_download_dump` | target ► job id (refused with `network-off` when the switch is off) |
| `dev_table` | source, dataset, table ► columns (name, type, distinct values when at most 50), row count, identity column |
| `dev_rows` | source, dataset, table, columns, search, filters, sort, offset, limit ► rows + total |
| `dev_row` | source, dataset, table, row id ► JSON text + loc resolutions + explorer links |
| `dev_diff` | A, B ► job id; `dev_diff_summary` ► tables; `dev_diff_records` (paged); `dev_diff_record` ► field changes |

The mock backend serves a synthetic 300-record DataCore (three types, cross references, one
1.5 MiB record) and three synthetic datasets, OS-independent, so samples match on Windows.

### 7.8 UI layout and keyboard

- **Settings ► Advanced ► "Developer mode"** switch, hint: "Adds tools to browse the game's
  data records and compare datasets. Read only. Uses about 400 MB of memory while open."
- When on: a **Dev** sidebar group after Game with **Explorer** (Alt+9), **Datasets**,
  **Diff**, and Ctrl+K entries "Go to Explorer/Datasets/Diff", "Find record…" (focuses the
  explorer search with the typed text), "Open record by GUID". When off none of this is
  rendered and the routes fall back to Home.
- **Explorer:** three resizable panes — tree (280 px) | results or node children (virtualised
  list) | record detail. Below 1,100 px the tree becomes a drawer. Each pane scrolls inside
  itself; nothing grows the window (as fixed for ImportDialog and the Enhancements preview).
- **Datasets:** source + dataset list (left, 260 px) | table with filter bar and column
  picker | detail sheet. **Diff:** A/B pickers on top, summary table, then record list |
  field diff.
- **Keyboard.** F6 / Shift+F6 cycles panes. Tree: `role="tree"`, Up/Down, Right opens or goes
  to the first child, Left closes or goes to the parent, Home/End, Enter opens, typeahead.
  JSON tree: same keys, plus `*` to open all siblings, Enter on a reference to follow it,
  Ctrl+C copies the value under the cursor, Shift+Ctrl+C its JSON pointer. `/` or Ctrl+F
  focuses search; Esc returns to the list; Alt+Left/Right is history. Tables keep the Strings
  grid model (arrows, Page Up/Down, Home/End, Enter). One tab stop per pane.

### 7.9 CLI (cheap parity)

```
starorg-strings dev record <path|guid|name> [--channel <NAME>] [--compact]   # JSON to stdout
starorg-strings dev export-type <type> --out <file> [--channel <NAME>] [--ndjson]
starorg-strings dev types [--channel <NAME>]                                  # type, records, main
```

Runs in the calling process under `catch_unwind` (about 1 s, about 0.9 GB peak, no cache
written), takes `extract.lock`, exit codes as section 8. Needs no setting (the CLI is already
a developer surface). No GTK, no HTTP: the published-dump source stays behind the existing
`published-dumps` feature and is not part of `dev`.

### 7.10 Privacy and safety

- Nothing leaves the machine. The only network use is the opt-in dump download (two GitHub
  hosts, no identifiers, §1); with the switch off, Dev mode makes no request.
- Game files are opened read only. Exports go only to a path the user picked in a native
  dialog; local dataset folders are only read.
- All opened JSON is untrusted data: size caps (64 MiB per file, 4 GiB per folder), nesting
  depth cap 128, rendered as text (no HTML), never executed. Paths shown, never taken from the
  webview.
- Exported records are CIG's game data: the export dialog says "for your own use".
- Tests and fixtures stay free of CIG prose: synthetic DCB via `DcbBuilder`, synthetic
  datasets with the real files' *shapes* only.

### 7.11 Tests

- `starorg-data`: index over a `DcbBuilder` database (counts, main flag, by-type, by-path);
  reference sink output; truncation by pointer/depth; streamed type export is valid JSON and
  byte-equal to per-record export; worker protocol (start, request, EOF exit, panic ► error
  reply, test hooks as in 3.3). Real-archive test (ignored by default, like
  `real_archives.rs`): LIVE opens, at least 100,000 records, every main record exports.
- `starorg-core`: shape inference against shape-only copies of the 30 files; column typing;
  search/filter/sort paging; identity choice; diff counts and field diff on hand-made pairs.
- Contract: samples for every dev command, Rust mock = TS mock, `dev-disabled` when off.
- App: vitest for the JSON tree (virtualisation, keys, reference follow, history) and column
  picker; headless renders (Explorer, Datasets, Diff at 1280x800 and 1000x700, dark + light)
  into `.tmp/renders/dev/`; `keyboard-check.cjs` extended with the Dev screens.
- CI: the CLI dependency check still finds no GTK/HTTP; Windows runs the same mock tests.

### 7.12 Open questions (recommended default first)

| # | Question | Recommended default |
|---|---|---|
| Q-D1 | Cache the decompressed DCB on disk (330-355 MB per build) so the worker starts in about 0.3 s without the p4k index peak? | **No** in the first version: start is 0.8-1.4 s and the cache would triple in size. Revisit if cold starts are slow on HDDs |
| Q-D2 | "Referenced by" (backlinks) in the record detail? | **Follow-up.** Needs one full reference scan (same pass as §8.2); add once that pass exists |
| Q-D3 | One switch for dump download and for using dumps in `apply`? | **Yes, the existing "Use published game data" switch**, with the hint updated to say what it enables in this build |
| Q-D4 | Export format for a whole type | **One JSON array file**; `--ndjson` in the CLI only |
| Q-D5 | Show non-main (sub) records in the tree? | **Hidden by default** ("main only" toggle); of the 55,059 sub-records on LIVE about 53,500 are dialogue and tag entries |
| Q-D6 | Alt+9 for Explorer only, or digits for all three? | **Alt+9 Explorer only**; the rest through Ctrl+K |
| Q-D7 | Keep UEX files (`uex-*.json`) in the viewer although they are not game data? | **Yes**, they are just JSON in the chosen folder; never downloaded by us |

### 7.13 Implementation notes (branch `devmode-build`, 2026-11-15, local only)

Sections 7.1-7.12 are implemented with the answered defaults (Q-D1 no DCB cache, Q-D2 backlinks a
follow-up, Q-D3 the existing switch, Q-D4 one JSON array and `--ndjson` in the CLI, Q-D5 main
records only by default, Q-D6 Alt+9 for Explorer only, Q-D7 UEX files are just JSON).

- **Core** `starorg-core::dev`: `explorer` (index, type/path trees, ranked search, GUID / path /
  name lookup), `record` (replies cut by pointer and depth under 1 MiB, `@key` text), `dataset`
  (shape inference, typed columns, rows), `diff`. Core's `serde_json` now keeps file order
  (`preserve_order`, as the rest of the build already did).
- **Worker** `starorg-data::devworker`: `starorg-strings-app dev-worker --p4k <P>` (or `--dcb
  <F> [--ini <F>]`), JSON lines; ops `status`, `types`, `tree`, `search`, `record`,
  `export-record`, `export-type` (streamed via `<out>.tmp`, progress lines, other requests
  answered between steps), `cancel`, `resolve` (dataset links). A panic in a request is an
  `internal` error reply; EOF ends the worker; exports never go into the game folder or prefix.
- **Measured on LIVE 4.10.200.58746** (release, read only, `tests/dev_real.rs`): load 0.64 s,
  parse + index 92 ms, 118,499 records (63,440 main), 604 types; RSS 458 MB after start, peak
  911 MB; path root + type root + a search 12 ms; one record 0.5 ms; all 63,440 main records
  exported (2,141 MB) in 2.9 s with no failure. The app's worker (`dev-worker --p4k`, release)
  is ready in 0.66 s; its steady RSS is 490-520 MB (above the 450 MB of 7.3: allocator
  leftovers of the archive read), peak 920 MB, so the Settings hint says "about 500 MB".
- **Contract** as 7.7, plus `dev_close`, `dev_types` and `dev_dataset`. Deviations: the calls with
  progress (`dev_open`, `dev_export_type`, `dev_diff`, `dev_download_dump`) return their result
  when the job ends and stream `job-progress` meanwhile (as `apply`), instead of returning a job
  id; dataset folders live in `app.json` (`dev.folders`, app-only like the other app settings)
  instead of `config.toml`; a build is one dataset (`gamedata.json`) whose tables are the
  GameData lists, not one dataset per list. New error code `dev-disabled`, job kind `dev`.
- **Mocks**: the Rust mock serves the synthetic 300-record DataCore and datasets through the
  core functions; `mock-dev.gen.json` carries the data to the TS mock, which ports the same
  functions; `tests/devmock.test.ts` replays every dev sample against it.
- **UI**: Settings > Advanced; Dev group; with Developer mode on, Settings moves to Alt+0 so
  Alt+9 can open Explorer. Tests are `node:test` (the repo has no vitest).
- **Not done (follow-ups):** the published-dump source (the "Use published game data" switch is
  not built in this app version, so the row reads Off and `dev_download_dump` answers
  `network-off`, or `not-implemented` with the switch on); backlinks (Q-D2); the
  `starorg-gamedata` domain outputs as a source (that crate is not on main yet).

### 7.14 Readable ids (branch `readable-dev`, 2026-11-15, Refs #15)

Owner request: "the diff and original data parts should be human readable, not just ids".
The Explorer, Datasets and Diff screens show what an id reads as first and the raw id beside
it. Raw data and every export stay unchanged.

- **Resolution lives in the backend**, one function for every screen:
  `starorg-core::dev::readable::label(lookup, raw, key)` returns a `Readable { kind, text,
  detail, enhanced }`, or `None` (the raw value stays). Kinds:
  - `record`: a GUID, or a full or short record name. The text is the record's display name
    (stock text of its name key) with the short name as `detail`. Without a display name the
    text is the short name.
  - `loc`: an `@key` (in datasets also a bare stock key) reads as the stock text of the same
    channel's `global.ini`. `enhanced` holds our Strings text when it differs. Empty stock
    text (`@LOC_EMPTY`) and placeholders stay raw.
  - `tag`: a tag GUID reads as the `tagName` of its `Tag` record.
  - `manufacturer`: a 2 to 6 character code under a `manufacturer` key reads as
    `manufacturer_Name<CODE>`.
- **Lookups** (trait `Lookup`):
  - **dev-worker**: every record of the index by GUID. A record's display key is read lazily
    from the record itself (`display_of`, own name fields only: the body's
    `Localization.Name` or top-level `displayName`/`DisplayName`/`locDisplayName`/`Name`/
    `name`; for an entity its components' `AttachDef.Localization.Name`, then
    `VehicleComponentParams.vehicleName`, then `SCItemPurchasableParams.displayName`; an
    `@key`, placeholders skipped; `tagName` for tags). Nested parts never name the record (an
    interaction's "Enter", a port, a filter): without an own name the short name serves.
    LIVE check, 120 random entities: 58 named as global.ini, 0 wrong, 62 short names.
    At most 256 new reads per
    request, cached per record up to 100,000 entries (the cache starts over when full).
    `record` replies carry `labels`; the new op `labels {values}` answers single values.
  - **app**: per channel build, a `MapLookup` from its GameData (items, ships, ammo,
    blueprints, standings, contracts), its stock text and the cached Strings table for
    enhanced text. It is built in about 4 ms and the last 3 are kept. Folder sources use the
    target channel's lookup. Each side of a diff uses its own build's lookup.
  - **mocks**: the synthetic records (`known`, `makers`, `enhanced` in `mock-dev.gen.json`).
    The TS mock ports the same functions, and the drift test replays the new samples.
- **Contract (additive, `#[serde(default)]`)**:
  - `Row.labels` (one per cell) and `Row.title`.
  - `RecordView.labels` and `DevRowDetail.labels` (raw value -> `Readable`).
  - `DiffRecord.label` and `DevFieldDiff.label` (the item's display name).
  - `FieldChange` gains `path_label` (`[column=value]` segments read), `group`, `unit` (by
    field name: kg, m/s, dmg/s, m, s, HP, rpm, SCU) and `old_label` / `new_label` (each read
    on its own side).
  - `DevDiffRecordsRequest.query`: matches the id or the display name.
  - Dataset search matches cell text, what cells read as, and row titles.
- **UI**:
  - The label is primary. The raw id is secondary (muted, mono, in the tooltip, with a copy
    button where it fits). An `enh` mark shows when our text differs from stock.
  - A **Show IDs** switch (Explorer header, Datasets bar, Diff bar) brings raw ids back on all
    three screens, the Explorer tree's inline stock text included. It is kept per viewer
    (`localStorage` `starorg.dev.showIds`).
  - Unresolved GUIDs and `@key`s show raw, marked "unresolved", never blank.
  - Diff records are titled "Bracer (COOL_AEGS_S01_Bracer_SCItem)" and filter by name or id.
    Field changes are grouped by item or top-level field and read as names, text and numbers
    with units.
  - The record JSON tree shows labels inline. Refs and record GUIDs follow, by click or by
    Enter.
- **Measured on real data (read only)**:
  - **LIVE against PTU GameData** with their stock `global.ini`
    (`real_builds_read_as_names`, ignored test): 2,292 of 2,293 changed, added or removed
    records titled, e.g. "Aegis Javelin", "'WEAK' Repeater", "Applicant". The diff takes 53
    ms. A search for "bracer" finds `Bracer [EntityClassDefinition.COOL_AEGS_S01_Bracer_SCItem]
    | Aegis Dynamics [AEGS]` in 7 ms.
  - **dev-worker against the LIVE `Data.p4k`**: the Bracer record reads its GUID as "Bracer",
    tags as "Cooler", "Size1", "ActionArea", and the manufacturer ref as "Aegis Dynamics"; the
    Avenger Stalker record carries 97 labels.
  - **Worker memory**: RSS 521 to 526 MB after two records against 522 to 524 MB without
    labels, with the same 920 MB peak at load. The labels add about 2 MB and stay inside the
    ~500 MB dev budget.
- **Renders**: `app/scripts/render-readable.cjs` (readable and Show IDs, dark and light,
  1280x800, into `.tmp/renders/readable/`).
- **Not done**:
  - A "readable" export variant. It was optional: exports stay raw.
  - Value chips in the dataset filter list raw values.
  - The worker answers dataset GUIDs only while it runs (as for links).

## 8. Strings grouping

Request: "add grouping for different types of items/things in strings". The Strings screen
gets a group tree beside the table, derived from which game record uses each key, with a
key-prefix fallback for the rest.

### 8.1 Feasibility, measured on LIVE 4.10.200.58746 (read only)

- 91,358 keys in the stock `global.ini`.
- Scanning every main record's export for `@key` strings: **976,376 references; 58,235 keys
  (63.7 %) are used by at least one record**; 2,542 keys are used by more than one struct
  type. The scan is the 0.41 s wall / about 8 s CPU pass of §7.1.
- Keys by the struct type that references them most:

  | Struct type | Keys | | Struct type | Keys |
  |---|---|---|---|---|
  | `DialogueContextBank` | 22,912 | | `MissionOrganization` | 723 |
  | `EntityClassDefinition` | 16,304 | | `HintUIData` | 488 |
  | `BuildingBlocks_Canvas` | 4,308 | | `JournalEntry` | 475 |
  | `DialogueContentBank` | 2,290 | | `ControlHints_Preset` | 258 |
  | `ContractGenerator` | 2,062 | | `ChatEmoteRecord` | 243 |
  | `StarMapObject` | 2,051 | | `SCItemManufacturer` | 234 |
  | `MissionBrokerEntry` | 1,671 | | `ContractTemplate` | 232 |
  | `MissionLocationTemplate` | 1,298 | | `FactionReputation` + `SReputationStandingParams` | 277 |

- Entities: 26,901 of 30,083 `EntityClassDefinition` records have
  `SAttachableComponentParams.AttachDef`; 153 distinct `Type` values. Size is set on nearly
  all; a manufacturer on most player-facing items (armour, clothing, weapons, paints,
  vehicles) and rarely on cargo, doors, displays. Largest by key references: `NOITEM_Player`
  4,603, `NOITEM_Vehicle` 4,423, `Cargo` 3,772, `Misc` 3,522, `Paints` 3,365, `Usable` 3,098,
  armour parts (`Char_Armor_*`) about 9,200 together, clothing (`Char_Clothing_*`) about
  5,800, `WeaponPersonal` 1,896, `WeaponAttachment` 1,176, `WeaponGun` 776.
- **33,123 keys have no record reference.** 705 distinct first tokens; the 20 largest cover
  24,715: `human` 9,138, `ui` 3,311, `item` 3,300, `dlg` 1,679, `mg` 1,014, `mission` 895,
  `dxsh` 845, `pu` 504, `pause` 499, `dfm` 458, `input` 396, `planetcollect` 373, `port` 358,
  `hud` 329, `mobiglas` 321, … The tail is long (event and mission names), so "Other" will
  hold a few thousand keys.

### 8.2 Data flow and caching

```
extract worker (existing)                     core                         app
  DataCore ► GameData                           group rules (pure)           Strings table rows
  NEW: loc-reference pass ► keyrefs      ──►    key ► {group, mfr, size} ──► tree counts, filters
  written to the cache entry                     memoised per cache key
```

- **Reference pass (worker, `starorg-data`).** For every main record, collect the `@key`
  strings it contains, with the record's struct type and, for entities, `AttachDef` `Type`,
  `SubType`, `Size` and manufacturer code. Implemented as an `ExportSink` that only looks at
  `write_str` values (no JSON is built), run with rayon; target is under half the 0.41 s JSON
  pass. Output: `keyrefs.json.zst` in the cache entry: per key, the candidate
  `(struct type, item type, sub type, size, manufacturer, count)` tuples, at most 4 per key
  (about 58,000 keys, expected under 1 MB compressed). `meta.json` hashes it like the other
  files and records its own `keyrefs` schema number; the cache key is unchanged, so entries
  written by a hook stay valid.
- **Not on the launch-hook path.** `apply --if-stale` from a hook must stay as fast as
  today. The pass runs only when the extraction is started by the app; a cache entry without `keyrefs` is valid. The
  app, on opening Strings for such an entry, starts a background job that adds it; until it
  finishes the tree is built from the fallback rules alone and says "Approximate groups –
  reading game data…".
- **Assignment (core, pure function, `starorg-core::groups`).** `assign(keys, keyrefs,
  GameData) -> GroupIndex`, computed once per cache key when the Strings table is built and
  kept with it (the table is already rebuilt per `app.rev`; the group index depends only on
  the cache entry, so it is memoised separately and survives profile changes). Under 50 ms for
  91,358 keys. No second cache file: it is cheap to recompute and its rules change with app
  versions.
- **One primary group per key**, so counts add up to the total. Order of rules:
  1. A `GameData` link (the model already knows components, weapons, missiles, ships,
     contracts, blueprints, reputation with their `loc` keys) – most precise, includes
     sub-type.
  2. `keyrefs`: the most specific candidate wins (entity item type > mission/contract types >
     blueprint > reputation > star map > UI canvas > dialogue); ties by reference count, then
     by name.
  3. Key-prefix and name-pattern rules for unreferenced keys (table below).
  4. **Other**.
  - An entity that only *displays* a key loses to the key's prefix rule: an entity without
    `AttachDef` always, an entity with one when the prefix names Interface, Game modes or
    Dialogue (`hud_Label_On` on a usable, `ui_Close` on a crate, `Frontend_Context_*`).
    Mission, location and crafting prefixes do not override (`Hockrow_*_Sample04_Name` is a
    crate). `port_Name*` are hardpoint names and go to Ship components > Other with
    `itemPort_*` (LIVE 4.10: 358 keys were in Locations > Areas).
- **Secondary facets** on the same key: `manufacturer` (code + display name) and `size`
  (1-12), present only for entity-linked keys. From `keyrefs` a facet counts only when every
  entity using the key agrees (a generic `panel_open` used by many makers has none), and
  `port_*` / `itemPort_*` keys have none. **Size** is kept only for Ships, Ground vehicles,
  Ship components, Ship weapons, Missiles, FPS weapons and FPS attachments; elsewhere
  `AttachDef.Size` is the default 1 (LIVE 4.10: 12,129 of 15,934 sized keys were "Size 1",
  mostly paints and clothing; after the rule 4,670 sized keys, 1,648 of size 1).

### 8.3 Hierarchy (two levels; ids are stable strings)

| Group | Children | Source |
|---|---|---|
| Ships & vehicles | Ships · Ground vehicles · Paints | entity `NOITEM_Vehicle`, `Paints`; `GameData.ships` |
| Ship components | Coolers · Power plants · Shields · Quantum drives · Jump drives · Radars · Thrusters · Fuel (tanks, intakes) · Armor · Turrets & mounts · Missile racks · Mining & salvage · Flight controllers & computers · Other | entity `Cooler`, `PowerPlant`, `Shield`, `QuantumDrive`, `Radar`, `MainThruster`, `ManneuverThruster`, `FuelTank`, `FuelIntake`, `QuantumFuelTank`, `Armor`, `Turret`, `MissileLauncher`, … |
| Ship weapons | Guns · Countermeasures | `WeaponGun`, `WeaponDefensive` |
| Missiles & bombs | – | `Missile`, `Bomb` |
| FPS weapons | Weapons · Attachments · Gadgets & grenades | `WeaponPersonal`, `WeaponAttachment` |
| Armor | Helmets · Torso · Arms · Legs · Undersuits · Backpacks | `Char_Armor_*` |
| Clothing | by slot | `Char_Clothing_*` |
| Consumables | Food · Drink · Medical · Other | `Food`, `Drink`, medical sub-types |
| Cargo & commodities | Cargo · Commodities & resources | `Cargo`, `ResourceTypeDatabase` |
| Other items | Usables · Displays & props · Doors & seats · Misc | `Usable`, `Misc`, `Door`, `Seat*`, `Display`, `UNDEFINED`, `NOITEM_Player` |
| Missions & contracts | Contracts · Mission text · Locations · Organizations & givers | `ContractGenerator`, `ContractTemplate`, `MissionBrokerEntry`, `MissionLocationTemplate`, `MissionOrganization`, prefixes `mission`, `mg`, `missionmanager` |
| Blueprints & crafting | – | `CraftingBlueprintRecord`, `crafting` |
| Reputation | – | `FactionReputation`, `SReputationStandingParams`, `repstanding` |
| Locations | Star map · Areas | `StarMapObject`, prefixes `area`, `port`, `crus`, `hockrow`, … |
| Manufacturers | – | `SCItemManufacturer` |
| Dialogue | Voice lines · Conversations · Comms | `DialogueContextBank`, `DialogueContentBank`, prefixes `human`, `dlg`, `dxsh`, `dxsm` |
| Interface | HUD · mobiGlas · Menus & settings · Input & hints · Chat & emotes · Journal · Other | `BuildingBlocks_Canvas`, `HintUIData`, `ControlHints_Preset`, `MobiGlasAppData`, `ChatEmoteRecord`, `JournalEntry`, prefixes `ui`, `hud`, `mobiglas`, `pause`, `input`, `chat`, `frontend`, `text` |
| Game modes | Arena Commander · Star Marine | `GameMode`, prefixes `dfm`, `ea`, `soo2` |
| Other | – | everything else |

The type ► group table is data in core (`groups/rules.rs`), not scattered `match` arms; an
unknown `AttachDef.Type` lands in "Other items ► Misc" and is counted in a drift test so new
types are noticed. Unreferenced `item_*` keys (3,300) go to the group of their namesake
record when the existing namesake convention finds one, else "Other items ► Misc".

### 8.4 Contract (additive)

- `StringsQuery`: `groups: string[]` (group ids; a parent selects its children; empty = all),
  `manufacturers: string[]`, `sizes: number[]`, `group_by: 'none' | 'type' | 'manufacturer' |
  'size'`. All optional with defaults, so existing callers and samples keep working.
- `StringRow`: `group: string` (e.g. `components/coolers`), `manufacturer: string | null`,
  `size: number | null`. The existing `category` (which generator touched the row) stays.
- `StringsPage`: `groups: GroupNode[]` – `{ id, parent, label, count, enhanced, edited }` for
  the active `group_by` – and `group_state: 'exact' | 'approximate'`.
- Counts follow the usual facet rule: they reflect the search, source and "changed only"
  filters but not the group selection itself, so the tree keeps showing where matches are.
- `StringSort` gains `group` (group order, then key). With `group_by` other than `none` the
  default sort is `group`.
- Mock: the synthetic keys get groups from the same core rules; samples regenerated; drift
  tests cover the new unions.

### 8.5 UI and keyboard

- **Layout:** `[ group tree 240 px ] [ filter bar + table ]`. The tree pane scrolls on its
  own, can be collapsed to a 36 px rail (state remembered), and below 1,000 px window width
  becomes a "Groups" popover button in the filter bar. The table keeps its virtualisation and
  column rules; a "Group" column appears when there is room.
- **Tree rows:** label, total count, and small badges for enhanced and edited counts when not
  zero (`Coolers 412 · 96 enhanced · 2 edited`). Zero-count groups under the current search
  are dimmed, not removed, so the tree does not jump while typing.
- **Group by:** segmented control above the tree: None (tree hidden, today's screen) / Type /
  Manufacturer / Size. Manufacturer and Size are flat lists with "No manufacturer" /
  "No size" last. Default: Type.
- **Chips:** every selected group, manufacturer and size shows as a removable chip in the
  filter bar next to the source facets; "Clear" removes all. The Category select is replaced
  by the tree (its values map onto top-level groups).
- **Keyboard:** the tree is one tab stop before the filter bar: Up/Down, Right/Left
  (open/close, child/parent), Home/End, typeahead, Enter or Space selects only this group,
  Ctrl+Enter / Ctrl+click adds or removes it from the selection, Esc clears the selection.
  F6 / Shift+F6 moves between tree, filter bar and grid. Screen readers get `role="tree"`,
  `aria-level`, `aria-expanded`, `aria-selected`, and the counts in the accessible name.
- **Unchanged:** search, source facets, "changed only", sorting, the detail sheet and the
  override flow. Selecting a group only adds a filter; the active row is kept when it is
  still in the result. Selection, open nodes and group-by are remembered per channel in the
  webview's local storage.

### 8.6 Budget

| | Budget |
|---|---|
| Reference pass in the worker | under +0.5 s wall on 8 cores, no change to the 0.9 GB peak |
| `keyrefs.json.zst` | under 1 MB |
| `assign` for 91,358 keys | under 50 ms, once per cache entry |
| Per-row cost in the table | 4 bytes (group, manufacturer, size indexes) |
| `query_strings` with group filter + tree counts | same order as today (one pass over the filtered rows) |

### 8.7 Privacy

Computed locally from the user's own game files; nothing is sent anywhere and no setting is
needed. With published dumps in use and no local DataCore pass, only rule 1 and the fallback
rules apply (`group_state: approximate`) until `sc-gamedata` publishes loc keys (§1).

### 8.8 Tests

- Core: rule table unit tests (each `AttachDef.Type` we name maps to its group; precedence
  when a key is used by an item and by a UI canvas; prefix rules; namesake fallback);
  every key gets exactly one group; counts sum to the total; stable group ids.
- `starorg-data`: reference sink on a `DcbBuilder` database; real-archive test (ignored by
  default) asserting on LIVE at least 55,000 referenced keys and "Other" under 10 % of keys.
- Contract: samples with groups, facet-count rule, `group_by` variants, Rust mock = TS mock.
- App: vitest for the tree (keys, multi-select, chips, dimmed zero groups), virtualised table
  unchanged with a group filter; headless renders of Strings with the tree open, collapsed
  and in the narrow popover form; `keyboard-check.cjs` covers F6 and tree keys.

### 8.9 Open questions (recommended default first)

| # | Question | Recommended default |
|---|---|---|
| Q-G1 | Run the reference pass in hook-triggered extractions too? | **No**; app-started extractions and a background job only, so launch hooks stay as fast as today |
| Q-G2 | Sticky group header rows inside the table when grouped? | **No** in the first version (tree + "Group" column); headers complicate the virtual grid and keyboard model |
| Q-G3 | A key used by several things (2,542 keys): one group or several? | **One primary group** (counts add up); the detail sheet lists the other users under "Also used by" |
| Q-G4 | Split Dialogue (about 37,000 keys with the `human`/`dlg` prefixes) by speaker or mission? | **No**; three children only. Revisit if anyone edits dialogue |
| Q-G5 | Manufacturer and size as group-by modes or only as filters? | **Both**: flat group-by lists plus chips |
| Q-G6 | Default when the screen opens | **Group by Type, nothing selected** (same rows as today, tree visible) |
| Q-G7 | Group labels: English only? | **Yes**, as the rest of the app |

### 8.10 Implementation notes (branch `devmode`, 2026-11-14, local only)

Section 8 is implemented; section 7 (Developer mode) followed on branch `devmode-build` (7.13).

- **Core** `starorg-core::groups`: `rules.rs` holds the hierarchy (19 top-level groups, 84
  nodes) and the item-type, struct-type, canvas-path and key-prefix tables; `assign` gives one
  leaf per stock key, a manufacturer (only codes with a stock `manufacturer_Name<code>` key:
  manufacturer records also hold paint and decal logos) and a size (1-12). Row cost 4 bytes
  (`u8` group, `u16` manufacturer, `u8` size).
- **Reference pass** `starorg-data::keyrefs` + a no-output StarBreaker `ExportSink`
  (`starbreaker.rs`) over every main record, in parallel. The worker runs it only with
  `--keyrefs`, which only the app passes (Q-G1). Output `keyrefs.json` (**plain JSON, not
  zstd**: core has no zstd dependency; 3.9 MB on LIVE, 0.46 MB if compressed), hashed in
  `meta.json`, optional; `cache::add_keyrefs` exists, the app uses `apply::reload_channel`.
- **Measured on LIVE 4.10.200.58746** (release, 32 threads, warm cache, read only):
  worker without the pass 1.02 s wall / 7.7 s CPU / 892 MB peak; with it 1.22-1.24 s wall /
  11.1-11.3 s CPU / 891 MB peak. 58,233 keys referenced; `assign` 27-31 ms; Other 6,277 keys
  (6.9 %) exact, 11,790 approximate; 108 named manufacturers. 13 entity item types (67 keys)
  still fall to "Other items › Misc" (`AIModule`, `*Controller`, `Hangar`, `Room`, …).
- **Contract** as 8.4, plus `read_string_groups(ChannelRef) -> GroupState` and
  `StringRow.group_label`. Manufacturer filter `none` and size `0` mean "without one". The
  query lives in one module (`contract/strings_query.rs`) shared by the real and mock
  backends; `diag::query` was removed.
- **UI deviations from 8.5:** group-by is a select in the filter bar (not a segmented
  control above the tree), so "None" can hide the pane entirely; the sort select labels the
  generator sort "Generator"; an approximate build reads its groups automatically once per
  channel and session (Retry on failure, e.g. `busy`). The Category select (renamed
  "Generator") came back on `devmode-build` as its own filter beside the tree: which generator
  touched a row is a different question from what the key names.
- **Not done:** sticky group headers (Q-G2, as recommended); CLI output of groups; dump
  sources still give approximate groups until `sc-gamedata` publishes loc keys (8.7).

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
- **Ported:** all 26 record domains, `game-strings-english` and the derived
  `game-search-index` (28 datasets), plus the overlay registry/loading of
  `sources/overlays.ts` (download supplied by the caller, no HTTP client). Against LIVE
  4.10.200.58746, 24 are byte-identical to the golden dumps apart from `_extracted`; the
  other 4 (`ships`, `fps-weapons`, `item-tags`, and `search-index` derived from them)
  differ only by 56 records that the golden took from stale files in sc-gamedata's
  extraction folder and that are not in that build. Details: the crate README, "Status".
- **Gate:** `scripts/compare-gamedata.sh [--domain <name>]` (dev harness `gamedata-port`,
  not in the shipped CLI): byte compare against `~/git/sc-gamedata/data`, golden-only
  datasets reported. Porting rules: `crates/starorg-gamedata/README.md`.
- **Out of scope:** `uex-item-prices.json` and `uex-terminals.json` (third-party UEX API
  data, not game files) and sc-gamedata's dataset query/report tools (`find`, `obtain`,
  `where`, `shops`, `notes`, `diff`, `changes`, `engagement`, `deflection`). The query layer
  of `find`/`obtain`/`where`/`shops` was ported later for the local MCP server (§10).
- **Next:** cut the full-extract peak memory (about 10 GB at the port; 7.3 GB after the
  record pruning of §10: every scoped record is held as a `serde_json::Value`), required before `GameData`, the CLI or the app use the
  crate; then move `starorg-data`'s `GameData` onto it and drop the duplicated accessors
  (also the two `item_common` and `tags` copies left by the parallel ports).

## 10. Experimental local MCP server (branch `local-mcp`, Refs #15)

Read-only game-data tools for an AI assistant on the same computer, over the Model Context
Protocol. User-facing docs, measurements and client setup: [docs/mcp.md](../../mcp.md).

### 10.1 Design (approved)

- **Query layer:** sc-gamedata's `src/query/{fuzzy,search-index,obtain,where,shops}.ts` ported as
  `starorg_gamedata::query` with their tests. `scripts/compare-query.sh` answers the same
  query file with sc-gamedata's compiled `dist/query` (node, read only) and with
  `gamedata-port query`, then compares the JSON: 514 of 514 identical on LIVE 4.10.200.58746.
- **Server:** new crate `starorg-mcp`, a minimal JSON-RPC 2.0 / MCP implementation (no SDK:
  the official Rust SDK pulls an async runtime and, for HTTP, a web framework; one local client
  answered in order does not need them). Ten read-only tools: `search`, `item`,
  `where_to_buy`, `where_found`, `how_to_obtain` (`owned_by` a character, `active` or `any`,
  from the owned-blueprint store), `mission`, `compare`, `string` (stock and the text applied
  to the channel), `diff` (two channels, or a cached older build), `status`. `channel`
  parameter (default: the app's default channel, else LIVE); results compact and paginated.
- **Transports:** `starorg-strings mcp [--channel]` over stdio in the CLI (the CLI links the
  crate without its `http` feature; ci.yml's cli-deps check still passes). In the app,
  Settings -> Experimental -> "Local MCP server": streamable HTTP on `127.0.0.1` (free port),
  bearer token random per start, off at every start (the switch is not saved), stopped when
  switched off and on `RunEvent::Exit`; copy-config buttons for Claude Code and Claude Desktop
  (via `mcp-remote`); status and the last 100 requests in Activity, starts, stops and
  extractions in the log as job kind `mcp`.
- **Data and memory:** a tool loads only the datasets it needs, lazily. Each is extracted once
  per build by a short-lived worker process (`mcp-extract-worker`, one dataset per process,
  which records its own peak RSS) into `<cache>/mcp/<CHANNEL>/<build>-<fingerprint>/`, then
  served from that JSON; three builds per channel kept. Server peak 325 MB answering all ten
  tools with LIVE and PTU loaded (target < 1.5 GB). Workers: ships 3.9 GB, ship-components
  2.2 GB, every other dataset 0.85 to 1.4 GB.
- **Cheap memory cuts in the extraction:** `Domain::prune` drops the parts of a scoped record a
  domain never reads as it loads. item-sources 9.0 -> 1.4 GB, item-tags 8.7 -> 1.2 GB,
  blueprints 6.5 -> 1.3 GB, armor 3.0 -> 0.9 GB; a full extract 10 -> 7.3 GB.
  `scripts/compare-gamedata.sh` unchanged (24 of 28 identical, the same four stale-golden
  datasets) and the pruned outputs byte-identical to the unpruned ones. ships (loadouts and
  ports read deep) and ship-components are not pruned.
- **Name resolution (review fixes):** a name matched only by fuzzy ranking (a typo, or no
  such item) answers about the closest name with an `inexact` object (`asked`, `used`,
  `others`, `note`) in `item`, `compare`, `where_to_buy`, `where_found` and `how_to_obtain`;
  sc-gamedata's CLI warns on stderr, which an MCP client never sees. An exact name or one
  holding every word asked is a match. `game-item-tags` holds only 7 of 241 LIVE ship guns, so
  names it lacks are looked up in `game-ship-components` (named, non-template records), which
  `search` also indexes: the M5A Cannon resolves to `BEHR_LaserCannon_S3`, its stats from
  game-weapons, its UEX prices by item id.
- **Contract (additive):** `mcp_status`, `set_mcp_enabled`; `McpStatus`, `McpRequest`,
  `McpToggle`; `JobKind::Mcp`. Rust, TS, both mocks, samples, drift tests.

### 10.2 Privacy and safety

- No network: nothing in the server opens a connection; the HTTP server binds loopback only,
  requires the token (constant-time compare), refuses non-localhost `Origin`s, caps bodies at
  1 MiB. Shop prices only from UEX files the user put in `<cache>/mcp/uex/`.
- Read-only: no tool writes game files, settings or the owned-blueprint store; the worker
  refuses an output folder inside the channel folder or Wine prefix.
- Owned blueprints, character names and fleet are answered only to the connected local client;
  geids are never shown. docs/privacy.md says that a cloud-hosted assistant sends tool answers
  to its provider.

### 10.3 Tests

Tool schemas (object, no extra properties, required fields defined, channel enum, read-only
annotations); every tool on synthetic datasets with a fake extractor (lazy extraction once per
build, cache reuse, failures as tool errors); the stdio round trip in-process and through the
CLI binary; HTTP: token required (401), loopback bind, origin, session, protocol-version and
404/405 cases, stop closes the port; app: off by default, start/stop on the mock and the real
backend, Activity entries; the worker's refusal to write into the game.

### 10.4 Not done / open

- Windows: the worker's peak RSS is not measured (`None`; the call needs `unsafe`).
- `ships` and `ship-components` extraction are not pruned (3.9 / 2.2 GB peak, once per build).
- `item` details cover the common item types; other types answer with item-tags only.
- Ship weapons and components found via ship-components have no tags: no loot eligibility,
  and `where_to_buy {place}` inventories still name items from item-tags (UEX names otherwise).
- The server's in-memory dataset cache never evicts (grows with channels touched; ~460 MB
  with LIVE, PTU and HOTFIX). `HttpServer::stop` joins the accept thread only; a connection
  thread mid-request ends on its own (30 s socket timeouts).
- `mission` titles of broker missions are the game's unresolved `~mission(...)` tokens.

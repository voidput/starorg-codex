---
title: StarOrg Codex v1 — addendum (user requests 2026-11-14)
tags: [docs, plans, specs]
status: draft
updated: 2026-11-15
---

# StarOrg Codex v1 — addendum (user requests 2026-11-14)

Direction settled in [the v1 spec](starorg-codex-v1.md) revision 2 (sections 3.4, 8, 11);
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

- **Corrected (scope):** v1 covers the hook need with `starorg-codex apply --all --if-stale
  --timeout N` called from both hand-edited scripts. The subcommands below are v1.1.
- `starorg-codex hook prelaunch` — read `build_manifest.id` per channel; if the build changed
  since the last apply (or `Data.p4k` is newer than our applied `global.ini`), refresh data
  (dumps or local extraction per §1) and re-apply with the saved profile. Fast no-op otherwise.
  Must never block the game launch for long: time budget + non-fatal on error.
- `starorg-codex hook postexit` — when the launcher/game closes: scan `Game.log` and
  `logbackups/*.log` for `Received Blueprint: <name>` notifications
  (e.g. `<SHUDEvent_OnNotification> Added notification "Received Blueprint: Tailwind Flight Suit Raider Camo: "`),
  update the owned-blueprint list, and re-apply so `[Owned]` tags are current next launch.
  Idempotent (remember scanned log files / offsets).
- `starorg-codex hook install|uninstall|status` — add/remove a clearly marked block in the
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

`starorg-codex blueprints import <file> --character <name|geid> [--channel LIVE] [--dry-run]`
(and an Import button in the app's blueprint screen).

- Input: an owned-blueprints export (JSON) in Smart Citizen's format, wherever the user saved
  it (the picker opens in Downloads, or the folder of the last import; we never look in
  another tool's folder):
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
- Re-import (v0.2.1): `blueprints reimport [file] --character <name|geid> [--replace]
  [--dry-run]` reads the last export imported on the channel (`blueprints/import.json`) unless
  a file is given; `--replace` (also on `import`) drops what an earlier Smart Citizen import
  added and this export no longer has. Log and StarOrg entries are never removed. The app's
  Fix owned blueprints card does the same; a moved file opens the picker. The export may be
  UTF-8 with or without BOM, or UTF-16 with BOM.
- Re-scan (v0.2.1): `blueprints rescan [--channel] [--dry-run] [--replace]` re-reads every log
  from the start: the `log` source is rebuilt, other sources stay; `--replace` starts every set
  empty and is refused when a log cannot be read. A log that cannot be read keeps what it gave.
- Characters (v0.2.1): a reset or wipe gives a character a new geid. Same `accountId` and same
  name merge into the newest (old geid kept as an alias for logs and settings; store backed up
  to `blueprints/backups/<time>-<reason>/` first; a one-time repair does the same for stores of
  older versions). Same name without that proof stays apart with a notice; `auto` then uses the
  most recent character that owns something when the newest owns nothing, and says so.

## 2-4. Implementation notes (M1, branch `m1-apply`)

- **Commands.** `hook prelaunch [--timeout 60]`, `hook postexit [--timeout 120]`,
  `hook install|uninstall [--prelaunch-script F] [--postexit-script F] [--bin PATH]`,
  `hook status`; `blueprints scan|characters|list|use|import`; `apply --character
  <name|geid|auto|any>`.
- **Never fatal.** `hook prelaunch|postexit` exit 0 for every outcome (2 only for usage
  errors); the apply's own code is `exit_code` in `--json`. Pre-launch = `apply --all
  --if-stale` within the budget; post-exit = incremental log scan of every channel, then the
  same apply with what is left of the budget.
- **Hook block.** Lines between `# >>> starorg-codex hook >>>` and `# <<< starorg-codex
  hook <<<`, inserted before a final top-level `exit` (else at the end); no blank lines are
  added, so `uninstall` gives the script back byte for byte. Default scripts:
  `<prefix>/sc-prelaunch.sh` and `sc-postexit.sh`; links are followed and the real file is
  edited in place (mode kept), after a backup in `data/hooks/backups/`. Only existing files
  are edited; a script inside the game install or the prefix's `drive_c` is refused. An
  outdated block is rewritten in place; unbalanced markers are refused. `status` also flags a
  hand-made `starorg-codex` call outside the block (it would run twice) and the Smart Citizen
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

Native "Connect StarOrg Tools" in the app (CLI: `starorg-codex sync status|set` only; see
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
  `config.toml` `[sync] environment`; the CLI sets it with `starorg-codex sync set
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
open in-game check ([spec §13](starorg-codex-v1.md#in-game-checks-manual-recorded-in-the-spec-when-done)).
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
  is removed from the order list, unless the import asks for a prune.
- **Prune** (`--prune`, "Remove all ships first", `prune` in the contract): a replacing
  import that also drops the ships of the shared order list that the new fleet does not
  have, sold ships and hand picks alike. Ships that stay keep their group, rename and place;
  groups stay even when empty; hand matches and the prefix style are untouched. The dry run
  lists the ships it would drop (`removed_from_order`, with their renames). Refused with
  `--merge`, and refused for a real import that matched no ship (it would empty the list).
  Like the append, it acts on the shared list only; a channel's own list is left alone.
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
  suggestions listed) → Import. Replace is the default; "Add to my current fleet" and
  "Remove all ships first" (prune, §6.3; the report lists what goes) are checkboxes, at most
  one ticked.
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
pasted text; `dry_run`, `merge`, `prune`), `fleet_match` (pending name → hull or none),
`save_ship_names` (scope + settings), `fleet_clear`.

### 6.7 CLI

```
starorg-codex fleet import <file|-> [--dry-run] [--merge | --prune] [--format auto|xplor-json|xplor-csv|text]
                                      [--channel LIVE] [--json]
starorg-codex fleet list [--channel LIVE] [--json]
starorg-codex fleet match <row name> <hull id | none>
starorg-codex fleet clear
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
Dev screen opens ─► app starts `starorg-codex-app dev-worker --p4k <P>`   (child process)
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
- Files, sounds and models use a second child process, `asset-worker`, started only when they
  are asked for (7.16).

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
starorg-codex dev record <path|guid|name> [--channel <NAME>] [--compact]   # JSON to stdout
starorg-codex dev export-type <type> --out <file> [--channel <NAME>] [--ndjson]
starorg-codex dev types [--channel <NAME>]                                  # type, records, main
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
- **Worker** `starorg-data::devworker`: `starorg-codex-app dev-worker --p4k <P>` (or `--dcb
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
    LIVE check, 120 random entities: 58 named as global.ini, 0 wrong, 62 short names; the
    verifier's 2,101-record cross-type sample: 0 borrowed names (36 before).
    Records are also found by full or short name (`find_name_or_short`, binary search over
    two sorted index vectors, about 1 MB), so `factionreputation_lawful_ftlcourier` reads
    "FTL Courier"; a nameless record that would only repeat its own name stays raw.
    At most 256 new reads per
    request, cached per record up to 100,000 entries (the cache starts over when full).
    `record` replies carry `labels`; the new op `labels {values}` answers single values.
    While the explorer runs, the app asks it about dataset-row and diff values (GUIDs and
    record-name shapes) that the build's GameData does not name.
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
    Titles several records of a result share get the differing end of their record names
    ("Aegis Tiburon · PU AI CRIM" next to plain "Aegis Tiburon"), else the short id when there
    is no readable record name. The record list pane is resizable (drag the divider, arrow
    keys; double-click resets; width remembered). A search box above the tables searches
    every table's changed records by name or id (`dev_diff_search`; Enter opens the first hit);
    a filter box on the field changes keeps the fields whose path, read path or either value
    contains every word. Datasets reads number columns with the unit their name says
    ("25,740 HP", grid and row tree). Rows that name other rows of the same dataset (a ship's
    loadout naming its weapons by record name; `row_links`, core `rowrefs`) list them: in the
    Datasets row sheet to open them, in the Diff field pane to open their changes (when they
    changed too) or their row in Datasets. "Open in Datasets" and "Show changes in Diff"
    jump between the two screens. Field changes are grouped by item or top-level field and read
    as names, text and numbers with units. Positional list segments are named from the
    element (`loadout › weapons › M9A Cannon › dps`; LIVE vs PTU: 2,018 of 2,064 positional
    paths named). A key swapped for one with the same text shows both keys, the shared text
    as the secondary line.
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
  - The worker answers dataset values only while it runs (as for links). Both diff sides use
    the target channel's explorer for names GameData lacks.

### 7.15 Group trees (branch `dataset-tree`, 2026-11-15, Refs #15)

Datasets and Diff get a group tree like the Strings one (§8.5): a flat table of 733 weapons
reads better as `type > subType > size`.

- **Presets** (`starorg-core::dev::group::presets`, in `TableInfo.group_presets`, default
  first): the chain of the first groupable column of each tier (`type|category|kind|
  classification|itemType`, `subType|subCategory|subKind`, `size`); `manufacturer`; the
  folders of a path-like text column (`path`, `*Path`, `file`); then each other groupable
  column. Groupable = scalar, filled in at least half the rows, 2-200 distinct values that
  repeat. Tables without one (JSON trees, unique key/value tables) show no tree.
- **Nodes**: the id is the JSON array of keys from the root (`["WeaponGun","Gun","3"]`, `null`
  = no value, labelled "No size"; numbers read "Size 3"; readable labels per §7.14 when the
  backend has them). Only visible nodes are sent (roots and the children of open nodes, at
  most 500 per node); counts are exact and follow the search and filters (facet rule: zero
  counts dim, the structure stays). A folder that only holds one folder is joined (`a/b`).
- **Contract**: `dev_groups` (Datasets) and `dev_diff_groups` (Diff: dataset, left out when
  there is one, then table, then the table's default preset, `folders`, or tables only; only
  records that differ, with added / removed / changed per node); `DevRowsRequest.group` and
  `DevDiffRecordsRequest.group` filter by a node.
- **UI**: a collapsible Groups pane between Sources and the grid (Datasets) and left of the
  tables (Diff), 192-240 px so 1024x700 has no horizontal overflow; the group-by is
  remembered per table (`dataset|table`, per viewer) and the Diff one globally. Keys as §8.5
  (single selection); F6 order sources > tree > bar > grid. A removable chip names the picked
  group.
- **Tests**: core unit tests (`dev/group_tests.rs`), contract samples + TS-mock replay,
  `tests/datatree.test.ts`, `scripts/render-tree.cjs` (renders into `.tmp/renders/tree/` and
  checks that a picked node filters the grid and records to its count), and the ignored
  real-LIVE test `real_live_weapons_group_by_type_subtype_size` (733 weapons, 50 nodes, every
  count equal to a plain count and to the rows the node filters).

### 7.16 Assets: files, sounds and models (branch `dev-assets`, 2026-11-15, local only)

"Grab the models for things and sounds and stuff": Developer mode can now browse `Data.p4k`,
play and export a record's sounds and export an entity as a 3D model. Everything reuses
StarBreaker (MIT, Copyright (c) 2026 Diogo Trindade) at the pinned rev `041e5592`: the crates
`starbreaker-3d` (entity -> GLB), `-dds`, `-cryxml`, `-wem`, `-wwise` (incl. `datacore_audio`,
`game_audio`) are workspace dependencies at the same rev as `-p4k`/`-datacore`; the sounds logic
is a port of StarBreaker's app `audio_commands.rs` (source classification, bank path index,
event resolution, the Vorbis WEM -> Ogg; the other codecs are ours, see *Sound formats*); the presets are `sc-ship-models/scripts/export_ships.sh`'s.

**Process model.** A new child process, `starorg-codex-app asset-worker --p4k <P>`
(`starorg-data::assetworker`), same JSON-lines protocol and replies as `dev-worker`; every
request runs on its own thread under `catch_unwind`; `extract`, `export-sounds` and
`export-model` stream progress and are cancellable (`cancel_job`, job kind `dev`), one at a
time. Read only on the game folder; every output path goes through `devworker::check_out` (never
inside the game folder or its Wine prefix).

- **Lazy, on demand.** Nothing starts when the Dev screens open. The shared worker starts on the
  first Files listing, preview or Sounds request (opening Dev > Files lists the archive root, so
  it counts as asking: about 0.5 s and 0.45 GB, returned after 5 idle minutes or with the power
  button on the screen; the Explorer and the other Dev screens never start it); it opens only the archive index. The DataCore
  is read on the first sounds/model request, the Wwise/ATL index (GameAudio documents + every
  `.bnk`/`.wem` name) on the first sounds request; both stay while the worker lives (64 parsed
  banks at most). The app stops it after 5 idle minutes (the worker also exits by itself after
  10), when Developer mode is turned off and on app exit.
- **Model exports run in a fresh worker** that is stopped right after the export, so the memory
  (GBs for big ships with textures) goes back to the OS at once.
- **`locks/extract.lock`** is held while a worker opens the archive index, for a whole model
  export, and for the first sounds request (DataCore + audio index); the app waits up to 30 s for
  it (an explorer start), then answers `busy`.

**Asset cache** (`starorg_core::assetcache`). Decoded sounds (`audio/<media id>.ogg|.opus|.wav`,
or `.wem` for a codec that is not converted), exported models (`models/<record>-<full|print>-lod<n>.glb`) and texture
previews (`previews/<path hash>.png`) under `<root>/<CHANNEL>/<build key>/`. The root is
`<app cache>/assets` by default; for a folder the user picks it is always a subfolder of its
own, `<chosen>/StarOrg Codex asset cache/`, so nothing of the user's is ever inside the layout. The build key
is the extraction cache key (p4k size, mtime, change number, StarBreaker rev), so a game patch
misses; the first write for a new build removes the channel's older builds. LRU by modification
time (a hit touches the file), 2 GiB cap (`[dev] asset_cache_max_mb`), files over a quarter of
the cap are not kept, stale `.part` files go after an hour. A hit is answered without starting a
worker (preview, play, sounds export without `.wem`, model export); a sound that is not converted
is cached as its WEM (a cached WEM of a codec this build converts is not a hit: it is converted
again), and an export caches only the files written by that run.

- **Safety.** Eviction, pruning, clear and move act only under a root that holds our marker
  (`.starorg-asset-cache`, a regular file) and only on files of the layout: channel folders named
  like `LIVE`/`TECH-PREVIEW`, build folders that are hex cache keys, the three kind folders, and
  our file names and extensions. No symlink is followed, for folders or files (`symlink_metadata`
  throughout), every kind folder must canonicalize inside the canonical root, and writes refuse a
  layout folder that is a link. Temporary files (`.part`) are removed and created anew
  (`create_new`), so a write never follows a link either.
- **Checked at every use**, not only when picked: the CLI (`--cache-dir` or the setting) and the
  app refuse a root that is relative, a link, or inside any registered or discovered game folder
  or its Wine prefix (a hand-edited `config.toml`), with a clear error; the Settings row shows the
  problem (and creates or writes nothing just to show its status) until the folder is changed or
  reset.

- **Folder setting**: `[dev] asset_cache_dir` in `config.toml` (default `<app cache>/assets`;
  the CLI's `--cache-dir` overrides; `--no-cache` skips it). Settings > Advanced > Asset cache
  shows path, size of cap, Browse… (native dialog; must be absolute, a folder, writable — a probe
  file is written and removed — and not inside any registered or discovered game folder or its
  Wine prefix; another drive is fine and shown as a note), Reset to default, Clear (confirm).
  Switching never moves or deletes the old folder: while it still holds cache files the row
  offers "Move existing cache" (rename, or copy + remove across drives) or "Leave it".
- **Exports** (extract, sounds, models) go only to a folder or file from a native dialog, and
  the app refuses any inside a registered or discovered game folder (the whole install, so a
  sibling channel too) or its Wine prefix; the worker checks again (channel folder and prefix).
- **Webview**: only an 8-entry LRU of object URLs of played sounds (`app/src/lib/audiolru.ts`).

**Contract** (`contract/assets.rs`, `assets.ts`, the `AssetBackend` half of `Backend`):
`asset_status`, `asset_close`, `asset_list`, `asset_search`, `asset_preview` (image as a
`data:` URL, text inline at most 1 MiB), `asset_extract`, `asset_sounds`, `asset_sound` (the
converted file as base64, its MIME type, `format` such as `Vorbis (.ogg)`, `note` naming the
codec when it is kept as `.wem`, `hint` for a webview that cannot play it), `asset_export_sounds`, `asset_export_model`, `asset_cache_status|pick|reset|clear|move`.
All but the cache calls answer `dev-disabled` while Developer mode is off. Mocks in Rust
(`mock_assets.rs`) and TS (`mock-assets.ts`, data `mock-assets.gen.json`); `assetmock.test.ts`
replays every asset sample against the TS mock.

**UI.** Dev > Files: folder listing (folders first, 500 per page, "Show more"), path search
(every word), preview (texture PNG at most 1024 px a side, CryXML/XML text, WEM play button),
Extract (selected file or current folder; "Convert": textures to PNG with their mip files merged,
CryXmlB to XML). Explorer record header: Sounds (opens a list of triggers with their sounds, play,
Export sounds… / + .wem) and Export model… (Print shell or Full, LOD 0-4, optional STL). Next to
every export: "For personal use: these are Cloud Imperium's assets; don't redistribute."

**Presets** (`assets::model`): *Full (textures)* = materials and textures (mip 2), interiors,
lights, attachments, LOD 1 by default. *Print shell (geometry only)* = `--materials none
--no-interior --no-lights`, LOD 0 by default. STL: pure Rust from the GLB (all triangle
primitives of the default scene, node transforms baked, Y-up -> Z-up), what
`glb_to_stl_trimesh.py` did; no Blender needed.

**CLI.** `starorg-codex dev extract <p4k path|folder|glob>... --out DIR [--convert]`,
`dev export-model <guid|name> --out FILE [--preset full|print] [--lod N] [--stl]
[--cache-dir DIR]`, `dev export-sounds <guid|name> --out DIR [--wem] [--list]
[--cache-dir DIR]` (`--list` names each sound's codec and the file it becomes; the summary counts
files by extension and lists the sounds kept as `.wem`, with why); in process, `extract.lock` while the archive is read, never into the install
or a Wine prefix.

**Measured on LIVE 4.10.200.58746** (release, 32 threads, warm page cache, `Data.p4k` read only,
outputs under `.tmp/`):

| Run | Wall | Peak RSS | Output |
|---|---|---|---|
| Open the archive index only (any extract) | 0.6 s | 0.89 GB | |
| `dev extract "**/javelin*_diff.dds*" --convert` | 2.4 s | 0.89 GB | 2 PNG 2048x2048, 14 mip files merged |
| AEGS_Javelin, Print shell, LOD 0, + STL | 2.3 s | 1.37 GB | GLB 56.4 MB (`export_ships.sh` made 56.39 MB at the previous patch), STL 3.66 M triangles |
| AEGS_Idris_M, Print shell, LOD 0 | 1.4 s | 1.75 GB | GLB 96.4 MB |
| AEGS_Javelin, Full, LOD 1 | 24.0 s | 6.37 GB | GLB 348 MB |
| AEGS_Javelin, Full, LOD 0 | 24.0 s | 6.91 GB | GLB 513 MB |
| AEGS_Idris_M, Full, LOD 1 | 32.9 s | 10.87 GB | GLB 502 MB (`RAYON_NUM_THREADS=4`: 48.5 s, 9.56 GB) |
| AEGS_Gladius sounds: list (DataCore + audio index) | 0.9 s | 0.89 GB | 73 triggers (53 from the loadout), 2,328 sounds |
| AEGS_Gladius sounds: export all | 13.9 s | 0.89 GB | 2,305 Ogg + 23 WEM (before *Sound formats*), 197 MB |
| Same, after *Sound formats* | | | 2,328 Ogg, 0 WEM, 197 MB; all decode with `ffmpeg -xerror` |
| Same again from the asset cache | 0.9 s | 0.89 GB | no decoding |

The archive index (about 0.9 GB) is the floor of every asset run; the Full preset's textures
dominate above it, and fewer threads barely help. That is why model exports run in their own
process, why the Full preset warns in the export dialog, and why the cache keeps only models
under a quarter of its cap (512 MB by default: Print shell GLBs of every ship and Full GLBs of
small ships fit; a Full Javelin at 348-513 MB fits only just, a Full Idris at 502 MB does not).
Before a model export the app compares `MemAvailable` (Linux; not read on Windows) with a rough
need (Full 8 GB, Print shell 2 GB) and, when short, asks first ("Export anyway"); the CLI prints
the same note for `--preset full`.
The app's worker (`asset-worker`, release) on the same archive: ready in 0.49 s (1,399,137
entries) at 449 MB RSS, 899 MB peak while the index is built; listing the root 0.11 s, a folder
and a search (71 hits) under 10 ms, a 2048x2048 texture previewed at 1024 px in 30 ms, a 215 KB
CryXML as text in under 10 ms, all at +4 MB. The first sounds request (AEGS_Gladius) reads the
DataCore and the audio index in 0.83 s (868 MB held); one sound decodes in 10 ms. A Javelin print
shell exported in that same worker took 0.38 s but left 1.26 GB held (peak 1.42 GB): the reason
model exports get a fresh worker that is stopped afterwards.

**Tests.** Core: cache hit/miss, hit touches, build invalidation, LRU eviction and the
quarter-cap rule, stale parts, clear/move only ours, the verifier's repro (user files with
look-alike paths, symlinked kind and build folders and a symlinked file pointing outside: nothing
counted, evicted, pruned, cleared or moved; a marker-less folder never ours), use-time checks, setting round trip and resolve order,
validation (game folder, Wine prefix, file, relative, read-only). Data: a synthetic stored-ZIP
p4k (`assets::synthetic`: DataCore, split DDS, XML, PCM WEM) for listing, search, previews,
extraction (raw, converted, cancelled, refused), unsafe paths, globs, records, lazy audio index,
model errors and preset mapping, GLB -> STL; the worker protocol over a pipe (lazy status,
streaming, panics, errors, idle exit). CLI: extract/glob/refusal, cached model without an archive
read, `--no-cache`, sounds list. App: contract samples for every asset command, unions,
`dev-disabled`, IPC; `tests/real_assets.rs` runs the real `asset-worker` on the synthetic archive:
nothing starts before a request, cache hits start no worker, extract/export dialogs and the
game-folder refusal (channel, sibling channel, install root; no Wine prefix), fresh worker per model, cache folder validate/switch/move/clear, a config pointing into the install refused at use (status shows it, nothing created). Renders:
`app/scripts/render-assets.cjs` (`.tmp/renders/assets/`).

**Sound formats** (branch `sounds-codecs`, 2026-11-15; `assets::wemcodec`). Two user reports:
"Play_PHOB_Med_Cargo_Box_Metal_b_Impacts 1044913056 cannot be played here — Not Vorbis audio" and
"Could not play Play_SSDM_Shared_Explosion_Base_Light 1020390043 — NotSupportedError: The
operation is not supported."

- *1044913056 was Vorbis.* Every sound in LIVE is Wwise Vorbis (fmt `0xFFFF`): 118,478 of the
  118,484 loose `.wem` and 2,314 of the 2,428 bank-embedded media; the rest (6 + 114) is `PLUG`
  plugin data (convolution-reverb impulse responses), not audio. No PCM, IMA ADPCM or Opus. The
  failures came from StarBreaker's `vorbis_to_ogg` quality check (ww2ogg `validate`: more than
  10% of the samples of any of the first 10 packets at full scale is "likely wrong codebook"),
  which rejects loud, clipped impacts, gunshots and explosions; the app then said "Not Vorbis".
  Now, on that error, the WEM is converted again with the aoTuV codebooks and kept when lewton
  decodes the whole stream without error, with samples, and under 25% of them at full scale.
- *1020390043 converted fine* (a valid stereo 48 kHz Ogg Vorbis that ffmpeg and GStreamer's
  `oggdemux ! vorbisdec` decode); the webview could not play anything. On Linux the webview is
  WebKitGTK, whose `<audio>` runs a GStreamer `playbin` with `autoaudiosink`, and that element
  (plugin `autodetect`) is in gst-plugins-good, which was not installed: `gst-launch-1.0 playbin3
  uri=…` fails with "The autoaudiosink element is missing", which WebKit reports as
  `NotSupportedError`. Nothing in the file can fix that. `asset_sound` now carries `hint`
  (Linux: no `libgstautodetect.so` in the GStreamer plugin folders) and the player turns a refused
  play into "The app's webview could not play this Vorbis (.ogg) file. <what is missing>. Export
  it to play it in another player." with an **Export…** action, instead of the raw error. `.wav`
  also needs gst-plugins-good (`wavparse`); `.ogg`/`.opus` need only gst-plugins-base.

| fmt tag | Codec | Becomes |
|---|---|---|
| `0xFFFF` | Wwise Vorbis | `.ogg` (StarBreaker, then the aoTuV retry) |
| `0x0001`, `0xFFFE` | PCM | `.wav` (Wwise chunks dropped; WAVE_FORMAT_EXTENSIBLE with the Wwise channel mask above 2 channels) |
| `0x0002`, `0x0069` | Wwise IMA ADPCM | `.wav`, 16-bit PCM (0x24-byte blocks per channel, header sample + 63 nibbles, vgmstream's layout) |
| `0x3040` | Opus (Ogg) | `.opus`: the Ogg stream in the data chunk (after `meta`'s skip) |
| `0x3041` | Wwise Opus | `.opus`: the packets (sizes from the `seek` table) in an Ogg Opus, pre-skip and length from `fmt`, mapping family 0, 1 or 255 |
| `PLUG`, others | named, kept | `.wem`, with "Wwise plugin data … not audio" or "<codec> audio cannot be converted" |

MIME: `audio/ogg` for `.ogg` and `.opus`, `audio/wav` for `.wav`. The asset cache layout accepts
`.opus` and `.wav` (they were not files of the layout before, so they would not have been
counted or evicted). New direct dependencies of `starorg-data`, already in the tree through
`starbreaker-wem`: `ww2ogg` 0.1.0 (BSD-3-Clause), `lewton` 0.10.2 (MIT OR Apache-2.0), `ogg`
0.8.0 (BSD-3-Clause).

Real data (`crates/starorg-data/tests/real_sounds.rs`, ignored, LIVE read only):

| Set | Before | After |
|---|---|---|
| The two reported triggers | PHOB: 11 of 19 sounds kept as WEM (1044913056 among them); SSDM: 1 of 16 (576434116) | all 36 sound entries Vorbis -> `.ogg` (1044913056: 1.62 s; 1020390043: 7.21 s) |
| 6 records (Gladius, Arrow, Cutlass Black, Freelancer, two FPS weapons; 7,709 unique) | 7,608 Ogg, 101 WEM | 7,709 Ogg, 0 WEM; each Ogg decodes to its end |
| 2,000 random loose media | 1,987 Ogg, 12 WEM (11 Vorbis, 1 PLUG) | 1,999 Ogg, 1 kept (PLUG, named) |

Tests: synthetic WEMs built in the tests for every codec (PCM and extensible PCM to WAV, IMA
round trip through an encoder, Wwise Opus to Ogg Opus with granules and pre-skip, Ogg Opus
unwrapped, multichannel family 1, Opus TOC durations, the Vorbis full-decode check, broken
Vorbis / PLUG / unknown codecs named), the worker and app on a synthetic archive with a PCM and
a PTADPCM WEM, the cache extensions, the GStreamer hint, and the webview failure message.

**Not done / follow-ups:**
- A texture size choice for the Full preset (mip 2 is fixed; mip 3-4 would cut the memory).
- Decomposed / Blender exports (`--kind decomposed`), animations, interiors as separate files.
- Sounds of a type or a bank without a record; music and dialogue browsers.
- A 3D preview in the app (exports only).
- Progress labels inside a model export (the protocol carries done/total only).

### 7.17 Explorer item finder: sections, filters, models and sounds (branch `explorer-sections`, 2026-11-15, local only)

Owner feedback: "explorer needs better sections and quick filters", then "better filtering,
searching and grouping", "easy ways to find models or sounds". The Explorer is used for four
jobs: finding items and ships with their stats, grabbing assets, raw DataCore digging, and
seeing what changed against another channel. It now has three modes (top bar, kept per
viewer): **Items** (the default item finder), **Assets** (models and sounds) and **Raw** (the
type / path tree of 7.4, unchanged). The record pane (references, JSON tree, exports, sounds,
model export, history) is shared by all three.

**Sections** (`starorg-core::dev::sections::rules`, one data table with tests). Every record
lands in exactly one section; rules are checked in order, first hit wins: entity
`AttachDef.Type` (exact or prefix), then entity folders (entities without a type), then struct
types (exact or prefix, non-entities only), then folders below the records root; an entity no
rule names is an item ("Props & other items"), anything else "Other". Chosen from the LIVE
4.10.200.58746 `Data.p4k` (118,499 records, 63,440 main, 30,083 entities with 170 item types).
Main records per section on LIVE, all / player-usable (see the heuristic below):

| Section | Sub-levels | Main | Player-usable |
|---|---|---|---|
| Models (view) | section > ... | 25,913 | 21,923 |
| Sounds (view) | section > ... | 12,419 | 9,673 |
| Ships & vehicles | type > sub-type > manufacturer | 2,440 | 1,576 |
| Ship weapons | type > size > manufacturer | 1,322 | 1,275 |
| Ship components | type > size > manufacturer | 3,709 | 3,580 |
| Missiles & torpedoes | type > size > manufacturer | 266 | 260 |
| FPS weapons | type > sub-type > manufacturer | 624 | 609 |
| Armor & clothing | type > manufacturer | 4,585 | 4,560 |
| Attachments | sub-type > size | 261 | 254 |
| Consumables & medical | type > sub-type | 329 | 316 |
| Mining & salvage | type > size | 1,388 | 1,371 |
| Commodities & resources | type > folder | 1,974 | 1,931 |
| Missions & contracts | struct type > folder | 5,777 | 5,777 |
| Locations & star map | struct type > folder | 2,951 | 2,951 |
| Factions & reputation | struct type > folder | 2,217 | 2,214 |
| Blueprints & crafting | struct type > folder | 1,966 | 1,966 |
| Loadouts & loot | struct type > folder | 972 | 972 |
| Shops | type > folder | 767 | 338 |
| Characters & NPCs | type > folder | 4,069 | 2,390 |
| Props & other items | type > sub-type | 11,156 | 10,024 |
| Audio, UI & dialogue | struct type > folder | 11,143 | 11,142 |
| Other | struct type > folder | 5,524 | 5,524 |

Review fixes (same day): human-made props without a type (`entities/scitem/human/`: food,
furniture, storage, lights, screens; 811) moved from Characters to Props; weapon animation and
recoil records (`WeaponProcedural*`, 440) from FPS weapons to Other; the ship weapon parts that
share the FPS attachment type (`WeaponAttachment` sub-types `FiringMechanism`, `PowerArray`,
`Ventilation`; 240) from Attachments to Ship weapons; `GPI_Destructible*` (destructible props
on the gun type) to Props. The shop displays' "Display" copies count as variants now, so most
of Shops is not player-usable.

All records (sub-records included) sum to 118,499 (Audio, UI & dialogue 46,190 with the
dialogue sub-records; Other 24,743 with the 18,961 tags). A level where no record has a value
is skipped; "Type" reads through a small label table (`Cooler` > Coolers, `NOITEM_Vehicle` >
Vehicles, `Vehicle_Spaceship` > Spaceships). Paints and decals sit with Ships & vehicles,
missile racks with missiles, mining/salvage heads and modules with Mining & salvage.

**AI / NPC / test / template variants** ("Player-usable only" hides them). Entities only: a
name token (split at `_ - . space`, case-insensitive) of `ai npc template test debug dummy
unmanned placeholder temp tmp shipboarded boarded derelict wreck tutorial ea pir`, or a
folder named `npc_archetypes featuretests templates template test tests ai npc debug
placeholder`. Tokens also `hijacked indestructible nointerior gamemaster`, and a trailing
`Display` after a maker code or `ARMR` (showroom copies such as `RSI_Aurora_Display`; shop racks,
`Radar_Display_*` props and `Flair_viVid_Display` stay real items). `Teach` is not a token: the
`*_Teach` ships are "Teach's Special" editions sold at Teach's Ship Shop;
name prefixes `SalvageableDebris Orbital_Sentry probe_comms EAObjectiveDestructable`. `PIR` is
not a token: `AEGS_Gladius_PIR` ("Aegis Gladius Pirate") is a player ship (a regression test
and the real-data test check it). Not for other records: `Template` and `Derelict` are words
of mission and location names, `Legacy` armor is player armor. A heuristic, so the badge says
"AI/test" and nothing is hidden unless the filter is on.

**Worker index** (`dev-worker`, at start, before `ready`):
- A facet sink walks every main entity once (no JSON built, rayon): `AttachDef` type,
  sub-type, size, manufacturer code, `Localization.Name` and `.Description` keys (else the
  vehicle name), geometry file names under `SGeometryResourceParams`, the vehicle definition,
  and the entity's own `audioTrigger` values. The display key reads through the stock
  `global.ini`; grade and class come from the description's `Grade:` / `Class:` lines.
- **Has model / has sounds are checked, not guessed**: while the archive is still open the
  worker hashes every geometry file name of the archive (`.cga .cgf .skin .chr .cdf`, 401,308
  on LIVE) and every ATL trigger name of the GameAudio documents (62,657; parsed in parallel,
  StarBreaker's sequential reader took 0.8 s). A model means a named geometry file exists (or
  a vehicle definition is set); sounds means one of the entity's own triggers exists (its
  loadout's are not counted; the Sounds panel still shows them). The sets are FNV-1a hashes,
  about 5 MB; the archive is dropped as before.
- A geometry file -> entities map serves "Open owning record" in Files.
- The section index is a compact row per record (32 bytes: section, size, flags, interned
  kind, sub-type, maker, folder, grade, class, tag) plus display names and a name order.
- A bare `--dcb` input (tests) trusts the fields (`verified: false` in the status).

**Search** (`list`, every word, case-insensitive). Three passes, each only when the one
before found nothing (so looser hits never mix with real ones or into the counts; the list
says which pass answered, `ListPage.matching`):

1. *As typed*: every word in the readable name, short name, type, sub-type, manufacturer
   code or name, class or tag. Tiers for records with a display name: the name is the query;
   every word is a whole word of it; a word start; anywhere in it; then the name with the
   item type read out ("Atlas" + "Quantum drives" for `quantum drive`); then the same tiers on
   the short name (records without a display name start here); then words spread over both
   names; then the other fields.
2. *Close spellings*: a word of 5-8 letters may be one edit from a word of a name or of the
   item type, 9+ letters two (insert, delete, change, or swap two neighbours): `galdius` finds
   Gladius, `qauntum drive` the quantum drives. Shorter words never.
3. *Abbreviation*: the words run together in order inside a name, from a word start, at most
   three times their length (`glds`); readable names before short names, tighter spans first.

Ties: player items before variants, named records before raw ones, the sections in table order
(ships before props), fewer name words left over (the base "Drake Cutlass Black" before
"Cutlass Black PYAM Exec"), then name order. Prefixes are hard filters, shown as chips:
`type:` `maker:` `size:2` `size:1-3` (`size:5-1` reads as 1-5) `section:` `grade:` `class:`
`tag:` `has:model` `has:sounds`. Words are highlighted in the rows. Recent searches (10) are
kept when a search is confirmed with Enter or a result is opened from it, not while typing.

Search on LIVE (main records; rows, which pass, first hits):

| Query | Before review | After |
|---|---|---|
| `bracer` | 1,609 (subsequence noise) | 3: Bracer, its loot and blueprint records |
| `mole` | 8,613 | 121: Argo MOLE, MOLE Alliance, liveries |
| `ATLS` | 12,396 | 132: Argo ATLS, ATLS GEO, ATLS IKTI |
| `arrow` | 1,399 | 92: Anvil Arrow, its liveries |
| `panther` | 1,032 | 4: CF-337 Panther Repeater, ... |
| `medpen` | 668 | 30: MedPen (Hemozal) first |
| `glds` | 1,993 | 281 (abbreviation): Fury Goldsmith Livery, Aegis Gladius, ... |
| `galdius` | 46 (Human records first) | 202 (close spellings): Aegis Gladius first |
| `cutlass black` | 84: Cutlass Black Plushie first | 80: Drake Cutlass Black first |
| `quantum drive` | 100: audio records 2nd-5th | 100: Mauler Quantum Drive, then the drives (Agni, Aither, Allegro, Atlas) |
| `qauntum drive` | 0 | 98 (close spellings): the quantum drives |

As-typed searches take 35-55 ms in the worker; close spellings and abbreviations (two or three
passes) 70-110 ms.

**Filters** (facet sidebar and quick toggles, OR within a facet, AND across): section, type,
sub-type, size, manufacturer, grade, class, tag, player-usable, has model, has sounds, and
"Added / Changed vs <channel>". Counts follow the facet rule (each facet counts what passes
every other filter and the search), at most 60 values per facet plus any picked value outside
them (with its count, 0 when nothing has it now). Filters are remembered per
section, presets (search + filters + sort + grouping + section, 20 at most) are saved by name;
both in local storage, every access guarded. The record history carries the finder view, so
Back/Forward brings back the list, filters and grouping a record was opened from.

**List**: sort by name, size or manufacturer; group by section, type, manufacturer or size
with collapsible headers and counts (a folded group keeps its count, its rows are left out).
Rows: section icon, readable name, short id (muted, mono), type / sub-type / size / maker /
grade / class, model and sound badges, "AI/test", "New" / "Changed", and headline stats from
the build's GameData (ships size, crew, cargo, SCM, DPS; weapons DPS, alpha, range, speed;
components output, shield HP, QT speed, power, health; missiles damage, lock range and time).
Stat columns show while the name keeps about 300 px; the rest read inline. The tree and the
list stay in sync (a node is the list's scope, shown as a removable chip).

**Compare** ("Compare with…", every mode): the Diff screen's machinery on the two builds'
GameData (A = the other channel, B = this one), then the added and changed rows' GUIDs are
sent to the worker as marks (`marks` op). Computed when picked; needs the other channel's
game data to have been read (as the Diff screen). Records only in the other build are counted,
not listed. LIVE against PTU GameData: 1,833 marks (63 added), 459 only in PTU, 1,790 found
in the LIVE index; GameData read 100 ms, diff + marks 57 ms, marking 0.4 ms, the list of all
changes 8 ms (Ships & vehicles 1,088, Ship weapons 329, Ship components 337, ...).

**Assets mode**: the Models and Sounds views (by section, then the section's levels), the same
filters and list, the record pane's Sounds and Export model. "Sound events" searches ATL
triggers (name, Wwise event, bank; every word) in the asset worker, which starts on the first
such search and reads the audio index only (not on Explorer open); a hit shows its event, bank
and sounds with play buttons and "Export sounds…" (the §7.16 cache and dialogs). At most 500
hits are listed with the total ("add words to narrow them"); an unknown trigger is
`not-found`.

**Files**: a geometry file offers "Open owning record" (one owner opens it, several are
listed).

**Contract** (additive): `dev_list` (`ListQuery` > `ListPage`: items, total, matched, facet
counts, group heads, title, understood prefixes, words), `dev_owner`, `dev_compare` (`other:
null` clears); `dev_tree` root `section` with `player_only`; `TreeRoot::Section`,
`NodeKind::Section`; `DevStatus.sections` (build stats) and `.compared`; `asset_sound_search`,
`asset_trigger`. Both mocks carry a synthetic item catalogue (57 records over the sections);
the TS mock ports the section index and gets each record's section from the Rust table
(`mock-dev.gen.json`), so the rules stay in one place; samples replay equal.

**Measured on LIVE 4.10.200.58746** (release, 32 cores, warm cache, read only). The app's
worker (`dev-worker --p4k`) against the same binary built from main:

| | main | this branch |
|---|---|---|
| Ready (load, parse, index; here also the name sets and the section index) | 0.69-0.76 s | 1.23-1.31 s |
| Steady RSS after `ready` | 512 MB | 554 MB |
| Peak RSS (archive index) | 921 MB | 922-923 MB |

- Inside that: archive name sets 77-104 ms (about 5 MB held), entity scan 0.32-0.35 s on the
  4-thread pool, index build about 85 ms; the index counts about 11.9 MB (rows, names,
  strings, media sets, owner map).
- Thread count is a memory trade-off: the scans first ran on rayon's global pool (one thread
  per core): ready 0.97 s but 702 MB steady (the allocator kept the threads' memory); 6 or
  more threads 690+ MB; 4 threads (a pool that ends after each scan) 555 MB. Building the
  name sets after the archive is dropped and handing each entity's facts over compactly (no
  dense per-record vectors) took another 30 MB off. The Settings hint now says about 550 MB.
- Requests in the worker: Sections root 1 ms; `list` of every main record grouped by section
  20-36 ms; `gladius` (fuzzy, ranked) 75-105 ms; `type:cooler size:1 maker:aegis` 14-17 ms;
  `has:model` player only (22,357 rows) 17-21 ms; `owner` 0.1 ms. Steady RSS +9 MB after
  these (record LRU and display names, as before).
- Search finds "Aegis Gladius" first for `gladius` (246 hits over 13 sections); `owner` of
  `AEGS_Gladius.cga` gives AEGS_Gladius, its Dunlevy and PIR variants.

**Tests**: core (`dev/sections/tests.rs`: one section per record, rule table examples, variant
heuristic, trees and media views, list filters / facet rule / sorts / paging, query prefixes,
description facts, fuzzy ranking, grouping and folding, change marks, compare marks); worker
(facet sink events, protocol ops `list` / `owner` / `marks` / section tree, lazy build); asset
worker (trigger search on a synthetic GameAudio document); app (samples for every new command,
Rust mock = TS mock replay, `dev-disabled`, the real worker on the synthetic DCB); webview
(`tests/finder.test.ts`); renders `app/scripts/render-finder.cjs` (`.tmp/renders/finder/`,
1024x700 and 1280x800, dark and light; checks a picked facet value gives its count, no
overflow) and `keyboard-dev.cjs` finder checks (tree typeahead, F6, list keys, folding with
Enter, Alt+Left restoring filters). The older dev scripts pin the Raw mode. Real-archive
measurement: `tests/dev_real.rs::live_sections_index_counts_and_cost` (ignored; set
`STARORG_GD_A`/`STARORG_GD_B` to two GameData files for the compare numbers).

**Not done / follow-ups**:
- Raw mode lists keep raw names and show no change badges (the compare marks live in the
  finder's list).
- Stats come from GameData only (ships, weapons, components, missiles); other sections show
  none. Sorting by a stat is not offered.
- "Has sounds" counts the entity's own triggers, not its loadout's.
- The sound-event search starts the asset worker (0.5 s, 0.45 GB; its archive index) as §7.16
  does for Files.
- `render-dev.cjs` `datasets-columns` at 1024x700 reports `main+149` horizontal overflow on the
  Datasets screen (not touched here; not run at that size before).

### 7.18 Memory limit (Developer mode and MCP workers; branch `ram-limit`, 2026-11-15)

Owner request: "in devmode/mcp mode give a warning it may eat lots of ram and give it a ram
limiter slider". Code: `starorg_core::memlimit` (shared), the app's `contract/real/memory.rs`,
`Worker::spawn` in `contract/real/dev.rs`, `starorg_mcp::worker::ProcessExtractor`, the CLI's
`mem_opts.rs`.

- **Warning.** Turning on Developer mode (Settings > Advanced) or the local MCP server
  (Settings > Experimental) first shows a dialog (`MemoryWarningDialog.svelte`): the measured
  peaks (7.16 and docs/mcp.md: Explorer/Files 0.5-1 GB, Print shell 1.4-1.8 GB, Full 6-7 GB and
  an Idris about 11 GB; MCP datasets up to 1.5 GB, ship components 2.2 GB, ships 4.1 GB; every
  worker at least 0.5-0.9 GB for the archive index), this computer's total and free RAM, and the
  limit; each worker has its own cap. Rows that do not fit under the limit are marked, and the
  MCP dialog says so when Ships (4.1 GB, needed by `search`) does not fit (an 8 GB machine's
  default is 4 GB). Short reminders stay: the Dev mode and MCP hints, the MCP card and its Activity panel,
  the Files screen line, and the model export dialog (the preset's estimate against the limit).
- **Setting.** `[dev] memory_limit_gb` in `config.toml` (shared with the CLI). Slider in Settings
  > Advanced > Memory limit: **1.5 GB** .. total RAM - 2 GB, 0.5 GB steps (1.5 GB, not 1 GB: the
  workers' baseline is 0.92-0.94 GB while the archive index is built, above 90% of 1 GB; under
  3.5 GB of RAM the row and the dialog explain that workers compete with the system for it,
  `MemoryStatus.note`); default **5 GB** or half the
  RAM when that is less (deviation from the 4 GB first agreed: the MCP `ships` extraction peaks
  at 3.9-4.1 GB, above the watchdog's 90% of 4 GB, so 4 GB would break `search` out of the box).
  It lists which measured jobs fit, judged like the guard (below). Contract: `memory_status`, `set_memory_limit`
  (`MemoryStatus`, `MemoryEstimate`, `MemoryEnforcement`, `MemoryLimitRequest`). A value is read at
  every worker start; a running worker keeps its cap. A hand-edited value below the slider is honoured.
- **CLI.** `dev ... --memory-limit <GB>` and `mcp --memory-limit <GB>`, or
  `STARORG_MEMORY_LIMIT_GB`; default the setting, else as above; `0` = none; an empty
  `STARORG_MEMORY_LIMIT_GB=` counts as unset. A refusal names the limit that would pass
  (`--memory-limit 7.5` for a Full export at LOD 1). `dev` runs in the
  CLI's own process, which arms itself; `mcp` caps each extraction worker (not the server).
- **Scope.** Only the `dev-worker`, the asset workers (shared and per model export) and the
  `mcp-extract-worker`. Setup and apply (`extract-worker`) are never limited.

**Enforcement** (in order; revised after the first version's real cgroup OOM kills popped KDE
"out of memory" notifications on the owner's desktop):

1. *Soft guard before a start* (`memlimit::check`): a model export (estimate per preset, LOD and
   ship: capital ships by name, Full 11 GB) or an MCP dataset whose estimate is over the
   watchdog's stop point (`memlimit::stop_at`, 90% of the limit) is refused with "... needs
   about X GB, more than the Y GB memory limit allows ... Set the limit to at least Z GB ...".
   The guard, `MemoryEstimate.fits` and the TS view (`fits`/`stopAt` in `app/src/lib/memory.ts`,
   using `MemoryStatus.stop_percent`) all use that one threshold, so a job that passes is never
   then stopped for being in the 90-100% band (review finding: `game-item-sources`, 1.5 GB, at a
   1.5 GB limit). A test checks `check(need) == Ok` implies `need <= stop_at`. Over
   the limit's free memory but under the limit: the app asks ("Export anyway", replacing the old
   fixed Full 8 GB / Print 2 GB check); the MCP extractor refuses (no one to ask); the CLI warns.
   Explorer and Files starts (about 1 GB, under the slider's minimum) are not pre-checked.
2. *Watchdog while running* (primary stop): the parent passes the limit in
   `STARORG_WORKER_MEMORY_LIMIT`; the worker's first act (`memlimit::arm_from_env`) starts a thread
   that reads its anonymous memory every 25 ms (`RssAnon` + `VmSwap`; page cache from reading the
   archive does not count; Windows: committed private bytes, one `sysinfo` instance refreshing
   only its own process, every 100 ms) and at **90%** prints `memory-limit: Stopped: needed more
   than X GB. ...` and exits with code 7. The parent calls only exit 7 a memory stop (the MCP
   extractor, which keeps the worker's stderr, also Rust's "memory allocation of N bytes failed"
   after a hard-cap abort): the explorer/asset worker status and the call's error, the MCP tool
   error. Any other abort or kill reads "the explorer stopped unexpectedly (signal ...)".
3. *Hard cap, last resort, with headroom* (`limit + max(10%, 512 MB)`): Linux `RLIMIT_DATA` in the
   worker (rustix, safe API): an allocation past it fails and the worker aborts, no kernel OOM kill
   and no desktop notification. Workers are made non-dumpable (`PR_SET_DUMPABLE` 0, rustix
   `set_dumpable_behavior`), so that abort is never handed to systemd-coredump and no crash
   reporter pops up (`RLIMIT_CORE=0` alone does not stop a core-pattern pipe). `RLIMIT_AS` is not used: thread stacks and malloc
   arenas reserve far more address space than they touch. `Data.p4k` is **not** memory-mapped
   (StarBreaker's `MappedP4k` reads with `pread` from a pool of handles), so neither limit counts
   the archive. A cgroup v2 `memory.max` (`systemd-run --user --scope -p MemoryMax=... -p
   MemorySwapMax=0`) is opt-in with `STARORG_MEMORY_CGROUP=1`: it also covers page cache, but its
   hit is a kernel OOM kill, which KDE/GNOME announce with a system notification. Windows has no
   hard cap: a Job Object with `JOB_OBJECT_LIMIT_PROCESS_MEMORY` needs `unsafe` FFI, which the
   workspace forbids, and no safe crate exposes it (`win32job` sets only working-set limits).

**Measured on LIVE 4.10.200.58746** (release, read only, game not running, limit 1.5 GB):
`ProcessExtractor` refused `game-ships` up front in under 1 ms ("needs about 4.1 GB, more than
the 1.5 GB memory limit allows") and extracted `game-armor` (1.26 s, peak 0.95 GB) with the worker's
`Max data size` at 2147483648 (1.5 GB + 512 MB, `/proc/<pid>/limits`). `game-ships` started
directly under the same limit (guard bypassed) was stopped by its watchdog with exit 7 and the
message, its anonymous memory peaking at 1.38 GB (25 ms polling; 1.74 GB at the first version's
100 ms), no file written.

**Tests.** Core: slider bounds and clamping, the low-RAM note, config round trip, CLI
resolution (empty env unset), estimates, the guard against the stop point over a grid of needs
and limits and `limit_for`, usage parsing (`RssAnon` + `VmSwap`), exit mapping (`ExitStatus::from_raw`), command wrapping; `tests/memlimit_child.rs`
starts its own test binary as a child that allocates past 256 MB and is stopped by the watchdog
(exit 7, `RLIMIT_DATA` 768 MB shown from `/proc/self/limits`), and one under the limit finishes;
the cgroup case runs only with `STARORG_TEST_REAL_CGROUP=1`, which nothing sets (not CI
either: runners may lack a user systemd; on a desktop a failing scope pops a notification). MCP: the guard. CLI: flag and
env refusal of a Full export with the limit that would pass, bad and empty values, and the real-archive check above
(`STARORG_REAL_P4K`, `STARORG_REAL_OUT`). App: contract samples, mock round trip,
`tests/real_memory.rs` (setting saved and clamped; the real `dev-worker` under a 1 MB limit
reports "Stopped: needed more than 1 MB" and opens again after the limit is raised). TS:
`tests/memory.test.ts` (the same `fits` as Rust, checked against the samples). Renders: `app/scripts/render-memory.cjs` (`.tmp/renders/memory/`).

**Not done.** A Windows hard cap (Job Object; needs an `unsafe` exception or a safe crate); the
CLI `dev` runs in-process, so its cap is `RLIMIT_DATA` + watchdog, never a cgroup; estimates are
static (the MCP `measurements.json` peaks could refine them per build).


### 7.19 Blueprint finder: sort, filter and channel labels (branch `blueprints-finder`, 2026-11-15, local only)

Owner request: "also need sort and filter on blueprints also labels per channel". Not a
Developer mode feature (the Blueprints screen is always on); numbered here because 7.18 was the
last free slot. Code: `starorg_core::blueprints::finder` (catalogue + merge), the app's
`contract/finder.rs` (shared by both backends), `lib/bpfinder.ts` (query), `lib/channels.ts` +
`ChannelBadge.svelte` (channel colours), `components/blueprints/BlueprintFinder.svelte`.

**Ownership scope (checked before building, unchanged).** Owned sets were already per channel
*and* per character, never global: `channels/<install-id>/<CHANNEL>/blueprints/<geid>.json`
(§3). The log scan, the Smart Citizen import and StarOrg pull write into one channel's folder;
`[Owned]` on apply reads only that channel's selection. The owner's data (read only, counts
only): LIVE has 2 non-empty sets (271 and 260 entries; 4 characters known), PTU and HOTFIX
none. Same geid on two channels = the same character with two separate sets. The finder only
reads; **no storage semantics changed**, nothing is merged or moved between channels.

**Catalogue.** Every named blueprint of the channel's build (`GameData.blueprints` named
through the stock `global.ini`, as `GameDataCatalog`), classified without new extraction:
- kind and type from the record path (`crafting/blueprints/crafting/<fpsgear|vehiclegear|
  missionitems>/<kind>/<type>/...`), category as fallback (`FPSArmours`, `VehicleComponentS2`...);
- size from the category suffix (`VehicleComponentS2` = 2), else the produced item's size;
  armor gets its weight (Light / Medium / Heavy) instead;
- manufacturer, size, grade (A-D, ship components only) from the produced item when `GameData`
  lists it (components, weapons, missiles); armor and undersuits get maker and slot from the
  strings grouping (§8, loc-reference pass) when that build has it; a maker needs a stock
  `manufacturer_Name<code>` text to show its name (else the code).

LIVE 4.10.200.58746 (owner's cache, read only): 1,634 blueprints, 1,630 named (54 ms) =
937 FPS armor (Helmet 224, Torso 220, Arms 217, Legs 217, Undersuit 30, Backpack 29; weight on
878) + 176 FPS weapons (Pistol 36, Rifle 33, Sniper rifle 31, SMG 30, Shotgun 24, LMG 16,
Crossbow 4, HMG 2) + 37 FPS ammo + 377 ship components (Power plant 75, Cooler 73, Shield 62,
Radar 58, Quantum drive 57, Mining laser 17, Tractor beam 12, Refuelling nozzle 8, Salvage head
5, Ore pod 5, Mining module 5) + 99 ship weapons (Laser cannon 26, Laser repeater 17, Ballistic
cannon 14, Neutron cannon/repeater 6, Tachyon cannon 3, ...) + 4 mission items; maker on 1,584
(named 1,568), size on 652, grade on 354. Words in the record name (`_hmg_`, `crossbow`,
`shipmining_pod`, `mining_modules`, `tachyoncannon`, ...) name what `$templates` paths and bare
`vehiclegear/` do not; every ship and FPS blueprint now has a type (mission items have none). The owner's 531 owned entries became
271 rows: 269 matched to one blueprint and 2 ambiguous name rows (each one ambiguous import plus
one StarOrg pull of the same name), each listing its 2 candidates; those 4 blueprints show
"maybe owned".

**Merge.** `finder::build(channels)` keys rows by blueprint id (lower case) over the channels'
builds; an owned entry joins its row by matched id, else by a name only one blueprint has (in
its channel's build, else in any read build). Otherwise it gets a `name:<normalised>` row:
- **Ambiguous** when the name could be several blueprints: an import marked ambiguous (its
  candidate ids, else every blueprint of that name), or an entry without ids (log, StarOrg pull)
  whose name several blueprints have. The row lists the `candidates`, takes what they all agree
  on (kind, type, maker, size ...), and each candidate row gets the owner under `maybe`
  ("maybe owned"); nothing is marked owned, as on apply (§4 verification fixes).
- **Not in build** only when no read build has a blueprint of that name.

Each owner keeps `channel` + `character` + sources + earliest time; two entries of one set on one
blueprint merge into one owner. `unattributed.json` (blueprints seen before any login line) is
read as the pseudo-character `unattributed` ("Unattributed", last in the list). A set file that
cannot be read is logged (Activity, no ids) and counted in `skipped` (shown as a note); a set
whose file lacks its geid is read as its file name says and logged. A channel whose build was
not read has no catalogue: only what is owned there is listed, with a note. Reading creates
nothing: the incremental log scan runs only for channels that already have a `blueprints/`
folder (the current channel is scanned by `list_characters`), so "All channels" no longer makes
empty folders or `scan.json` files for PTU/HOTFIX.

TODO (after `blueprints-windows-fix` lands): `Characters.aliases` / `repair::consolidate` should
canonicalise geids in the All-channels character merge (marked `TODO(aliases)` in `finder.rs`).

**Contract (additive).** `blueprint_finder(BlueprintFinderRequest { install, channels })`
(empty = every channel) -> `BlueprintFinder { channels[] (catalog count or null, characters,
owned, skipped), characters[] (id, latest name, channels known), rows[] (key, name, record,
kind, type, class, manufacturer + name, size, grade, channels, candidates, owners[], maybe[]) }`; `BlueprintKind` union. Real
backend: cached game data only (never extracts); a build already in memory is reused, else the
cache entry is read, the catalogue (a few hundred KB) kept per build key and the game data
dropped, so "All channels" never holds every build. Rust mock: `mock_finder.rs` (39 synthetic
blueprints over LIVE and PTU, one only in each, two sharing the name "Sample Visor"; HOTFIX not
read; three characters with different sets per channel, one log name that is ambiguous, one
unattributed entry); the TS mock runs the same steps (`mock-finder.ts`: ids, ambiguity,
candidates, maybe, unattributed) and `tests/bpfinder.test.ts` replays it against both samples. `owned_blueprints` stays.

**UI (Blueprints screen).** The "Owned blueprints" card is now **Blueprints**, first and full
width (above the fold at 1024x700), with the [Owned]-tags and StarOrg cards side by side below:
- Scope: the current channel or **All channels** (rows side by side, every owner with its
  channel badge, "only in PTU" badge on blueprints one build lacks, a Channel facet).
- **All / Owned / Missing** with counts over the build's full catalogue. Owned / Missing mean
  owned or not by the picked characters (any character when none is picked) on the channels
  in view; picking characters with All keeps every row. Missing says how many of its rows are
  "maybe owned" (an ambiguous owned name could be them); those rows say so in Owned by.
- Ambiguous name rows show "Ambiguous" and "Could be: <name · type · weight/size · record>"
  per candidate; "Not in build" only when there is no candidate.
- Search: words over name, record, type and maker; only when no row matches as text, a 4+
  letter word also matches as letters in order in the name (`scnn` -> Synthetic Cannon).
- Count line: "N shown of M" (M includes name rows) and, separately, "LIVE's build has K
  blueprints".
- Source and Since show the picked characters only (others stay greyed in Owned by); sorting by
  them uses the same owners. Character numbers are how many blueprints each owns (lacks, with
  Missing), as the help line says.
- The import dialog starts on the first picked character only when it is known on the current
  channel (never a PTU-only character from the All view, never "Unattributed").
- Filters (n) does not count channel picks in the one-channel view (they do not apply there).
- Facet chips with counts: Category, Type, Manufacturer, Size (S0-S12 and armor weight),
  Source (Game log / Smart Citizen / StarOrg), Character (channel dots in All view), Channel.
  Each facet's counts apply every other filter; long facets show 8 and "+N more".
- Sort: name, category, manufacturer, size/grade, acquired, source, owned-by count; asc/desc,
  by header click (aria-sort) or the select; unknown values last in both directions.
- Group by none / category / manufacturer / character (one row per owning character plus
  "Not owned"). 200 rows, then "Show more".
- Remembered per channel (`starorg.blueprints.finder.v1:<install>/<channel>` in localStorage,
  validated on read); Clear filters keeps sort, group and scope.
- Keyboard: `/` focuses the search, Escape clears it, chips are toggle buttons
  (aria-pressed), headers are buttons; the owned/scope toggles carry no hint tooltip, because
  `LazyTip` mounting on focus took the focus away from arrow-key users (shared component, not
  changed here).

**Channel colours.** No shared helper existed; `lib/channels.ts` gives LIVE teal, PTU orange,
EPTU yellow, HOTFIX pink, TECH-PREVIEW lime, other channels a stable pick from four spares, each
with a dark and a light value at >= 4.5:1 on card and muted backgrounds of both themes (test).
`ChannelBadge` (badge / swatch / dot, always with the name or an sr-only name: never colour
alone) is used in the channel picker, the Home status card and other-channel rows, the
[Owned]-tags card title and the finder.

**Tests.** Core: classification from paths/items/categories, the strings grouping for armor,
merge over channels with scoped owners, name / id / ambiguous / unmatched rows, owner merge;
an ignored test prints counts for a real cache entry and data folder
(`STARORG_REAL_CACHE`, `STARORG_REAL_DATA`). App: mock finder per channel and after an import,
unknown channel; real backend after a Smart Citizen import (catalogue from the import's cache,
other channels without one, same rows from memory); TS: TS mock = Rust samples, owned / missing
counts, facet counts, character, source and channel semantics, search, every sort, groups,
prefs; channel colour contrast. Renders `app/scripts/render-blueprints.cjs` (1024x700,
1280x800, dark and light), keyboard `app/scripts/keyboard-blueprints.cjs`.

Renders: since main's "dark is the default theme" the mock's saved appearance wins over
`?theme=`, so `render-blueprints.cjs` pins `data-theme` before each shot.

**Not done.** No CLI view (`blueprints list` is unchanged); the finder does not read builds
that were never extracted (Apply or a dry run does); no favourite toggle or ownership edits
from the finder; tooltips in `Segmented` still steal focus on arrow keys elsewhere.

### 7.20 Model viewer: the Explorer's Model tab (branch `model-viewer`, 2026-11-15, local only)

Owner request: "do we have a way of viewing a low res version of the models too?", "maybe a
slider for details on model". Code: `components/assets/ModelViewer.svelte` (loaded on demand),
`lib/modelscene.ts` (three.js), `lib/modelview.ts` (slider, words, chunked read), the
`asset_model_*` commands in `contract/assets.rs` and `contract/real/assets.rs`,
`starorg_data::assets::model` (LODs) and `stl::glb_triangle_count`.

- **Where.** Developer mode, Explorer, an entity record (`hasModel`): a **Data | Model** tab
  pair above the record. Model shows the record's **Print shell** (geometry only, as 7.16) in 3D.
  The type, path and GUID rows hide on the Model tab (the view needs the height at 1024x700).
- **On demand.** Opening the tab reads only `asset_model_info` (per LOD: the memory estimate,
  the guard's verdict, whether it is cached; no worker). A cached step shows at once; any other
  waits for **Load LOD n**, which exports that LOD in a fresh `asset-worker` (stopped
  afterwards, as model exports) straight into the asset cache (`<record>-print-lod<n>.glb`, the
  same entries Export model uses, so "Export .glb" afterwards is a cache copy). Nothing is
  written outside the cache; never into the game folder.
- **Detail slider.** Least detail left: **Low (LOD 3) · Medium · High · Full (LOD 0)**, starting
  at LOD 3 or the least detailed LOD the mesh has below it. LOD 4 (**Lowest**) is not reliably
  lighter: StarBreaker loads LOD 0 for any part without a `_lod4` file, so on LIVE 4.10 LOD 4 has
  more triangles than LOD 3 (Gladius 104k vs 86k, Carrack 402k vs 232k, 890 Jump 571k vs 215k).
  It joins the slider only when the mesh has it and both GLBs are known (cached) with LOD 4 the
  lighter (Javelin: 58k vs 132k). Steps the mesh lacks (they would repeat the LOD below) are
  left out once its LODs are known. Under each step: its export memory (`~1.2 GB`) or cached
  size, and its triangles once exported (`AssetModelLod.triangles`, read from the cached GLB's
  JSON). The current step's line says what Load does (export, ask, refused, cached). Moving onto
  a cached step shows it after 150 ms (the timer is cleared on record change and close);
  dragging never starts exports. LOD 3 -> 0 checked monotonic on LIVE (table below).
- **LODs that do not exist.** StarBreaker also loads LOD 0 for a root mesh without the asked
  LOD, so exports read the entity's own mesh's LODs (`model::root_lods`: `<mesh>_lod<n>.<ext>m`
  in the archive) and use the most detailed one at or below the asked one (`effective_lod`);
  `ModelDone.lod`/`lods` report it and the cache keeps the GLB under that LOD. The LODs are
  recorded in the cache (`<record>-lods.lods`, beside the GLBs; `model::cached_lods`), so after a
  restart too, LOD 4 of a mesh with 0-3 finds the cached LOD 3 without a worker (app and CLI).
  A record never exported has unknown LODs: the first export reads them. A per-part clamp for
  loadout parts would have to live in StarBreaker's companion lookup (not changed here).
- **Memory.** The guard of 7.18 per step (over the limit refused even with force; over the free
  memory asks "Load anyway"); `model_estimate` is now per LOD for the Print shell: 1.4 GB at
  LOD 0 and 1.2 GB at LOD 1-4; capital ships 1.8 / 1.6 GB (Carrack and 890 Jump measured
  1.41-1.43 GB at LOD 1, 1.65-1.71 GB at LOD 0). A GLB larger than a quarter of the cache cap
  (512 MB by default) is refused for the viewer (Export model still writes it).
- **Stop.** The view cancels only its own job: the job id from its first progress event (never
  the app's current job in general). A Stop before the worker has the request is kept and sent
  as soon as the request id is known (`ExportJob.cancel` in `real/assets.rs`), so it never fails
  with "could not stop". Changing record or leaving the tab stops a running export. The TS mock
  honours `cancel_job` between its steps (`?delay=` makes it slow enough to press Stop).
- **WebGL first.** A throwaway canvas is probed (`webgl2`, else `webgl`; its context released)
  when the tab opens; without WebGL the message shows up front and Load stays off, so nothing
  is exported that cannot be drawn.
- **Transfer.** `asset_model_chunk` returns raw IPC binary (`tauri::ipc::Response`, an
  `ArrayBuffer` in the webview), 8 MiB per call, read from the cache file by record and LOD (the
  webview never sends a path); the webview fills one buffer and parses it with
  `GLTFLoader.parse` in memory: no base64, no URL, no blob, no asset protocol, so the CSP is
  unchanged (`connect-src ipc: http://ipc.localhost` already covers IPC).
- **Viewer.** three.js 0.187 (MIT, in THIRD_PARTY_NOTICES), its own chunk (656 kB, 166 kB
  gzip) fetched when the tab first opens; the main bundle grows 3 kB. Orbit, zoom, pan
  (OrbitControls), Wireframe, Reset view, Export .glb; auto-fit from the bounding sphere, kept
  clear of the readouts. Readouts: size (bounding box of the vertices the triangles use, with
  node transforms: StarBreaker's meshes share vertex buffers, so per-geometry boxes are far too
  large), triangles, meshes, file size, export time and peak memory. One shared grey material,
  per theme; draws only on change (no render loop while the game runs). States: reading,
  exporting (job progress with Stop), reading the cache (progress, Stop), building, errors with
  Try again, WebGL missing (`--safe-gpu` / `WEBKIT_DISABLE_COMPOSITING_MODE` named, Export .glb
  suggested), GPU context lost. The personal-use note stays on the view.
- **Contract.** `asset_model_info` (`AssetModelInfoRequest` -> `AssetModelInfo`,
  `AssetModelLod`, `MemoryFit`), `asset_model_view` (streamed job, cancellable;
  `AssetModelViewRequest` -> `AssetModelView`), `asset_model_chunk` (`AssetModelChunkRequest`
  -> bytes). Both mocks serve a synthetic GLB per LOD (a 22 m ship: 2,596 triangles at LOD 0,
  164 at LOD 3; items 0-1), from `mock-assets.gen.json`.

**Measured (LIVE 4.10, Print shell, release build, read only).** Export in its own worker,
whole view call including the worker start 0.8-1.0 s; chunks 0.4-3 ms.

| Ship | LOD | GLB | Triangles | Export | Peak RSS |
| --- | --- | --- | --- | --- | --- |
| AEGS_Gladius | 4 | 4.6 MB | 104,146 | 371 ms | 1.04 GB |
| AEGS_Gladius | 3 | 5.0 MB | 85,907 | 339 ms | 1.02 GB |
| AEGS_Gladius | 0 | 37.8 MB | 1,069,627 | 408 ms | 1.18 GB |
| AEGS_Javelin | 4 | 1.7 MB | 57,889 | 350 ms | 0.99 GB |
| AEGS_Javelin | 3 | 3.7 MB | 132,056 | 342 ms | 1.00 GB |
| AEGS_Javelin | 0 | 53.8 MB | 3,662,991 | 507 ms | 1.29 GB |

Also: LOD 1/2 0.98-1.06 GB; AEGS_Idris_M 1.18 GB at LOD 3 (14.7 MB, 0.5 M triangles), 1.67 GB
at LOD 0 (92 MB, 5.8 M triangles). Triangles per LOD 3/2/1/0/4 (the slider's order, then LOD
4): Gladius 86k/198k/476k/1.07M/104k; ANVL_Carrack 232k/559k/1.45M/3.41M/402k (0.8-1.0 s,
1.26-1.65 GB); ORIG_890Jump 215k/500k/1.43M/3.63M/571k (0.6-0.8 s, 1.23-1.71 GB). A Stop sent
with the job's first event ended a Javelin LOD 1 export as cancelled.
All four ships have LODs 0-4. Bounding boxes: Gladius 17.4 x
5.8 x 21.0 m, Javelin 218 x 89 x 493 m (debris prongs and engines included).

**Tests.** Data: LODs from mesh file names and the clamp, cache keys per record/preset/LOD,
GLB triangle counts (bytes and file). Core: `peek`, memory estimates per LOD. App: samples, mock
steps/guard/cache/chunks, raw binary over IPC, `dev-disabled`, the real backend on the
synthetic install (cache per LOD without a worker, chunks, guard before any worker, non-entity
refused in its own worker), and an ignored real-install test
(`SC_GAME_DIR=... cargo test -p starorg-codex-app --test real_model_view -- --ignored`). TS:
TS mock = Rust samples, slider steps (LOD 4 only when known lighter, missing LODs left out),
step states and words, chunked read with Stop and a short read, guard, mock Stop of its own
job only, WebGL probe, readouts, three.js only behind the lazy import. Data: the LODs kept in
the cache (LOD 4 asked finds LOD 3). The real-install test also checks LOD 3 -> 0 triangles
rise and an early Stop. Renders
`app/scripts/render-model.cjs` (SwiftShader WebGL; 1024x700, 1280x800, dark and light; lazy
chunk check; a Stop on the slow mock).

**Not done.** Textures (Full preset) are not viewable; the LODs of loadout parts are not
clamped (StarBreaker), hence LOD 4's rule above; no section/clipping planes or measurement tool.

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

### 8.11 Strings owners: readable names for keys (branch `strings-owners`, 2026-11-15)

User request: "in strings I also want the key but with the human readable names". The grid's
first column now says what a key *is*: the thing that uses it and which field, e.g.
`item_DescCOOL_AEGS_S01_Bracer` -> **Bracer · description** (Ship component · Aegis
Dynamics). The raw key stays under it (muted, mono) while **Show keys** is on.

- **Core** `starorg-core::groups::owners` (the Explorer sections of 7.17 were not on main when
  this started, so it is its own small index): `OwnerIndex::build(stock, game_data)` per stock
  key, in order:
  1. GameData records whose loc keys include the key: components, ship/FPS weapons,
     missiles, ships and ground vehicles (name, description, career, role), contracts
     (title, description), reputation standings. Owner name = the record's stock display
     text (record name when that is a placeholder); manufacturer label from
     `manufacturer_Name<code>` (not for `NONE`/"Unknown"). One record is one owner: a ship
     weapon or missile also listed among the components keeps its specific kind. Per key, one
     entry per record (its first field), and records that read the same (same name, same or
     compatible kind, same or missing maker) collapse into one (AI copies, makerless copies);
     templates and `LOC_*` filler keys own nothing. Namesake record first, then a specific kind
     over "component", then one with a maker.
  2. Key shapes, no record: `item_Name*`/`vehicle_Name*` name themselves and give their
     `Desc`/`ShortName` siblings the name (namesake stem); `manufacturer_Name|Desc<code>`;
     `X_desc` / `X_desc_N` take the text of `X`, `X_name`, `X_title` / `X_title_N`, which in turn
     name themselves. A variant with no name of its own (`_grey_red01`, `_mag`) takes its base
     item's name and the suffix: `S71 Rifle (mag)`; variants the game names (`_store01` ->
     `S71 "Infrared" Rifle`) read as their own item. `item_NameX_short` is the short-name field.
     A name is one line (literal `\n` trimmed at the ends), at most 120 bytes, not a
     placeholder (`<-=MISSING=->`, `PH`, `PH - ...`); game variables read `[target name]`.
  3. Nothing: `pretty_key` (`item desc · COOL_AEGS_S01_Bracer`, `hud · law trespass area`).
  Storage is CSR: `u32` offsets per row + `(u32 record, u8 field)` uses + distinct owners.
- **Lazy, per build, no Developer mode:** `LoadedData` holds a `OnceLock<Arc<OwnerIndex>>`
  beside the groups; the first Strings table of a build fills it from the cached GameData and
  stock file. No worker, no extraction.
- **Contract (additive, version stays 1):** `StringRow.owner: KeyOwner | null` (boxed in Rust;
  every page row has one, `kind: key` = readable-key fallback filled by
  `strings_query::with_fallback`) and `StringRow.owners` (count); `KeyOwner { name, field,
  kind, detail }` with `OwnerField` (name, short-name, description, title, career, role) and
  `OwnerKind` (ship, ground-vehicle, component, ship-weapon, fps-weapon, missile, mission,
  reputation, manufacturer, named, key); `StringDetail.owners` (all, primary first);
  `StringSort::Owner` (owned keys first, name A-Z case-insensitive, field, key). Search also
  matches every owner's name and manufacturer (secondary owners through a backend-only,
  unserialised `StringRow.other_owners`). Both mocks carry synthetic owners (every 5th ship
  weapon shared with a "Turret" copy; `ui_SYN_*` rows fall back); sample
  `query_strings_by_owner`.
- **UI:** column "String" = `name · field` (fallback muted), key line under it (Show keys,
  local storage `starorg.strings.show-keys`, default on). A shared key reads
  "N records · field" and opens a popover listing every owner with kind and maker (Esc closes,
  focus returns); **O** on the active row opens it from the keyboard, Esc or Tab closes it and
  focus goes back to the row (keys on a row's buttons never reach the grid, so Enter there does
  not open the detail). The detail sheet starts with "What this is" / "Used by N records" (8
  shown, then "Show all"). The copy button overlays the cell on row hover/focus (it takes no
  width), Ctrl+C copies on the active row. Sort "Owner A–Z". **Group tree:** the
  existing "Group by type" tree already is the owner category tree (Ships & vehicles, Ship
  components, Ship weapons, Missions & contracts, Locations, Interface, Other, …): rule 1 of
  8.2 uses the same GameData links, so a key's owner and its group agree.
- **Measured on LIVE 4.10.200.58746** (release, read only; app folders copied and pointed to
  with `XDG_*`): index build 55-67 ms, 1.4 MiB heap (13,960 distinct owners); 25,118 of 91,358
  keys owned (5,855 by a GameData record, the rest by key shape), 138 shared (primary kinds:
  57 ship, 41 component, 17 ship weapon, 15 FPS weapon, 5 mission, 3 ground vehicle;
  `vehicle_focus_combat`: 136 ships). Primary kinds over all owned keys include 510 ship
  weapon and 183 missile. Table rows add about 3 MiB. Real backend: first Strings query
  342 ms (game data, groups, owners, table; RSS 26 -> 194 MiB, peak 213 MiB), warm key sort
  31 ms, owner sort 125 ms, search 34 ms. The CLI `build --channel LIVE` output is
  byte-identical to main's (sha256 `216d2ab6…aaaebb8d7`).
  Real examples: `item_NameBEHR_LaserCannon_S1` -> M3A Cannon · name (Ship weapon · Behring);
  `item_NameGMISL_S03_EM_FSKI_Thunderbolt` -> Thunderbolt III-G Missile · name (Missile ·
  Talon); `item_DescGMNI_rifle_ballistic_01_grey_red01` -> S71 "Justified" Rifle ·
  description (FPS weapon · Gemini);
  `vehicle_NameAEGS_Gladius` -> Aegis Gladius · name (Ship · Aegis Dynamics);
  `manufacturer_DescAEGS` -> Aegis Dynamics · description; `Pyro1_L2_03_desc` -> PYR1 L2-C ·
  description; `item_Desc_fta_medium_helmet_01_04_10` -> Strata Helmet microTech Edition ·
  description; `hud_law_TrespassAreaRevokedAuthorisation` -> hud · law trespass area revoked
  authorisation.
- **Tests:** core `owners_tests.rs` (record owner + field, shared keys and namesake order, key
  shapes, fallback, filler keys; ignored `real_cache_entry` with `STARORG_OWNERS_DIR`),
  contract `strings_rows_carry_readable_owners`, real backend assertions plus ignored
  `measure_strings_owners_on_real_live`, `app/tests/strowner.test.ts` (prettyKey parity with
  the core, labels). Headless renders 1024x700 and 1280x800, dark and light.
- **Not done:** dialogue (`pu_*`, `human_*`, `dlg_*`, about 35,000 keys) and most UI keys
  have no owner and show the readable key; search matches the primary owner only; Explorer
  sections (7.17) could replace rule 1 once Strings may read them without the dev worker.

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
- **Transports:** `starorg-codex mcp [--channel]` over stdio in the CLI (the CLI links the
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

## 11. Stale reasons across app updates (branch `stale-reasons`)

**Bug (0.1.0).** Every channel read `Stale (app version)` after any update, so the app and the
launch hooks (`apply --if-stale`) asked for or did a re-apply although the output would be
byte-identical.

- **Reasons** (`apply::stale_reasons`, shown as words by `staleWords` in
  `app/src/lib/messages.ts`): `build`, `game data`, `profile`, `patches`, `overrides`,
  `owned blueprints`, `ship names`, `enhancer update`; plus `not the last recorded output`
  for an older output of ours. `app version` is gone.
- **Output version.** `starorg_core::OUTPUT_VERSION` (`u32`, now 5) is recorded in
  `applied.json` as `output_version`; a different recorded value is `Stale (enhancer update)`.
  Bump it **only when the generated text changes for the same inputs**: a new or changed
  enhancement, a changed default of a setting, a text or merge fix. `app_version` is still
  recorded, for information only. History: 1 = 0.1.0; 2 = no stats written into shared
  `LOC_*` sentinel keys or `<= ... =>` placeholder text (a radar's stats showed on PTU
  contract headers through `LOC_PLACEHOLDER`); 3 = same-named blueprints: ownership per
  blueprint, no false `[Owned]`/`[BP Owned]` (a merged list line is `[Owned]` only when all
  its blueprints are owned); 4 = `[Owned]` coloured by default; 5 = quest item hints (§13).
- **Old manifests** (0.1.0, no `output_version`) read as version 1, the version 0.1.0
  generated: no prompt on upgrade, and the first real bump still reaches them. (Treating a
  missing field as "always current" would hide that first bump.)
- **Profile hash** = hash of the generator settings (`[enhance]` over the preset) that differ
  from the Recommended defaults (`canon::settings_hash`: both sides to JSON, keep only values
  off the default, recurse into objects, arrays whole, keys sorted). A build adding a setting
  with its default leaves it unchanged; any user change, preset included, changes it. Keys
  that do not feed the generators no longer count: `patches.disabled` is in the patch-set
  hash, `[ship_names]` in the ship-names hash, unknown keys change nothing. An invalid
  `[enhance]` hashes as `invalid:` + the text hash.
- **Ship-names hash**: the same difference-from-default form of the effective settings
  (empty when off). Fields added later to `[[ship]]`/`[[group]]` entries need
  `skip_serializing_if` at their default, since array elements are compared whole.
- **Migration.** 0.1.0 recorded the profile hash of the whole `profile.toml` TOML text and the
  ship-names hash of all settings. Status computes those too (`legacy_hash`) and accepts a
  match with either form, so current users are not prompted once more; the next apply records
  the new form. The ship-names fallback holds while its shape is the 0.1.0 one.
- **Other inputs, reviewed for build stability.** Overrides hash = raw bytes of
  `overrides.ini`; owned hash = sorted names/ids of the selection: both change only with the
  user's data. Patch-set hash = id, SHA-256 of the pack text and enabled state per patch, plus
  load errors: it changes when a bundled pack's text changes (any byte, comments included) or
  a load error's wording changes. That is intended (a new pack may change the output); keep
  bundled packs free of per-build stamps.
- **Tests.** `canon` (new default field keeps the hash; key order, nesting), profile and
  state hash tests, `scope_and_hash` (legacy differs), core `fleet.rs` (old manifest reads
  `output_version` 1), CLI `apply.rs`: another app version with the same output version is
  `applied`; an older output version is `Stale (enhancer update)` and `--if-stale` re-applies;
  a 0.1.0 manifest (no `output_version`, text-hash profile) is `applied`, and a real settings
  change against it is `Stale (profile)`.

## 12. Collapsible blueprint sections and Blueprint markers (branch `blueprints-collapse`, 2026-11-15)

User report: the Blueprints screen is long (the finder, then the [Owned]-tags and StarOrg cards),
and the blueprint colour and marker options could not be found (they sat in Enhancements →
Missions among the contract options). No contract change: the profile fields exist.

- **Collapsible cards.** `CollapsibleCard` is a drop-in for `Card.Root`: the card title becomes
  the disclosure button inside its heading (ARIA accordion pattern; Enter/Space toggle, Left
  collapses, Right expands, `aria-expanded` / `aria-controls`), the description and content hide
  while collapsed and the card no longer stretches to its grid row. A `card/collapse.ts` context
  drives `Card.Title` / `Card.Description` / `Card.Content`; without it the card parts render as
  before. `bare` wraps a component that renders its own card: "Fix owned blueprints"
  (`ReimportCard`, key `blueprints.reimport`). The warnings sit above the cards, outside any of
  them, so collapsing never hides them: the auto-mode owner warning ("Check whose blueprints are
  marked", mock `&bphint=larger`) and the name notice ("Two characters, one name", moved out of
  ReimportCard, with "Use Any of my characters"; mock `&bpnotice=1`). `/` on a collapsed finder
  card opens it and focuses the search. State: `starorg.cards.collapsed.v1` in localStorage (a list of card keys,
  `blueprints.finder|character|starorg|markers`), validated on read, re-read before each write.
- **Collapsible finder groups.** With Group by set, each group header is a button with the
  group's count (Enter/Space, Left/Right, Up/Down/Home/End move between headers), plus
  "Collapse all" / "Expand all" and "N groups, M collapsed" in the count line (also with one
  group). Collapse all merges: groups collapsed outside the current search or filter stay
  collapsed. With a search or filter, matches only in collapsed groups are counted ("N matches
  in collapsed groups", `bpfinder.hiddenMatches`) with an Expand link for just those groups. The
  stored state is read while deriving, so the first render has it (no expanded flash). Remembered per
  install/channel, scope and group-by (`starorg.blueprints.groups.v1:<install>/<channel>/<scope>/<group>`).
  Paging (`bpfinder.pageRows`): a collapsed group keeps its header and its rows neither count
  toward the 200 nor toward "Show N more (M left)"; once a row is cut off, later groups wait for
  Show more.
- **Enhancements → Blueprints.** A tab right after Missions holds every blueprint option:
  [Owned] / [Missing] markers with their colours and "whole line" (plus "Choose the character"),
  the blueprint list (on/off, name tags on its lines, variant label colour, longest list), and
  the title tags [BP] / [BP Owned] with their colour. Missions keeps the contract-only options
  (reputation tag, "Highlight title tags", item rewards, details, section header / label /
  reward colours) and a note that links to the new tab; nothing is listed twice. With contracts
  off, the tab says so with "Turn on contracts". The in-game preview on this tab puts a
  contract with a blueprint list ([Owned] / [Missing]) first (`bpmarkers.previewOrder`); the
  mock previews (Rust and TS) show [BP Owned] in the title while `title_owned` is on.
- **Markers in game** (Blueprints screen): the saved profile's state in one list, e.g.
  `[Owned] blue · [Missing] off · [BP Owned] off` (`bpmarkers.markerSummary`; says "contracts
  off" / "no blueprint list" when that is why), and "Edit blueprint markers", which opens
  Enhancements on the Blueprints tab (`app.enhancementsTab`, read once by the screen).
- **Tests.** `tests/collapse.test.ts` (stored lists, persistence per key, blocked storage,
  arrow keys, paging with collapsed groups on synthetic and the sample finder, marker summary,
  preview order); `scripts/keyboard-blueprints.cjs` (groups and cards by keyboard, kept after a
  reload, Collapse/Expand all, the jump to Enhancements → Blueprints, no blueprint switch left
  under Missions); `scripts/render-blueprints.cjs` shots `bp-groups-collapsed`,
  `bp-cards-collapsed`, `bp-hint-collapsed`, `enh-blueprints`, `enh-missions`;
  `keyboard-check.cjs` walks the category tabs past the new Blueprints tab. Stale script
  selectors fixed on the way: the Strings search label ("Search keys, owners and text"), the
  Home foreign-file warning (`data-testid="foreign-home"`) and the two "Import from Smart
  Citizen" buttons on the Blueprints screen.

## 13. Quest item hints (branch `quest-item-hints`, 2026-11-15)

**Request.** Quest items should say what they are for, starting with the Nyx chain
Vanduul-Tech Smugglers (`Kaboos_*` keys). Read only on the game; checked on LIVE 4.10.200.58746.

**What the data has (LIVE, read only).** No DataCore record links a contract to the Kaboos items:
their objectives are in `Libs/Subsumption/.../Kaboos_MissionModule.xml`, which is not shipped.
Real mission-to-item references exist in two places only:

- `MissionBrokerEntry` spawn descriptions: `properties[].value.spawnDescriptions[].options[]
  .autoSpawnSettings.inventoryItems[].entityClass` (an item put on an NPC or in a container);
- contract hauling orders: `haulingOrderContent[].entityClass` anywhere in a contract
  (`paramOverrides.propertyOverrides`, `subContracts`).

A key that merely looks like a mission's (`Kaboos_*` next to a `Kaboos` title) is never used.

**Two parts, two switches** (Enhancements > Quest items; both on in Recommended and Everything).

1. **`quest_hints` (generator, category `quest-hints`).** Extraction pass 5 collects every
   entity those references name (`GameData.quest_items`, schema 3; `missionbroker/` joined the
   loaded record paths): loc keys, `AttachDef` type, each use (source, mission, title key,
   systems from `ContractPrerequisite_Locality` / `locationMissionAvailable`, unreleased), and
   the entities of its folder that share its description key. `enhance::quest` appends
   `Used in: <title> · <system>` (several titles: a `- ` list, at most 3 plus `and N more`;
   label in the mission label colour) and leaves an item alone when it is a general good (type
   not one of `RemovableChip`, `FPS_Consumable`, `Cargo`, `Misc`, `InventoryContainer`, or
   sub-type `Harvestable`/`Trophy`), its name is a placeholder, its description key is shared
   with a non-variant entity (variants: `_debug`, `_1_a`/`_2_a`), or no use survives (unreleased,
   Wikelo's `TheCollector` trades, titles that are runtime `~mission(...)` tokens or not plain
   text). Titles are read from stock, never from our output; EM tags in them are dropped.
   LIVE result: 6 items, all real references: `none_consumable_hackingchip_01` and
   `..._DataHeist_ID` (Overdrive Initiative: INTEL RAID; the Data Heist contracts' own titles
   are runtime tokens), `Carryable_2H_FL_MissionItem_prototype_ship_component_2_a` (Orison Relief
   fabrication orders, Stanton; its key is named after the `_1_a` variant),
   `Carryable_1H_CY_vial_specimen_1_asd_a`, and the two Vanduul corpse crates (Nyx). Skipped on
   LIVE among others: 19 datapads sharing `item_DescCommon_Datapad`, cryopods with `LOC_EMPTY`,
   every Wikelo turn-in, ores and trophies.
2. **`quest-items` (built-in loc patch, listed under Patches).** Hand-written facts from playing
   the mission (also matching a community guide; our own wording): one appended line each on
   `Kaboos_AccessItem_desc` (found on a lieutenant at the derelicts near the Auxiliary Station;
   opens its airlock), `Kaboos_DecryptioKeycard1_desc` (each of the 5 lieutenants carries one;
   use only one, after all the data is collected, at the Security office terminal) and
   `Kaboos_DecryptionKeycard2_desc` (encode at the 3 server racks, any order, to open the
   warehouse loot room), plus ` (QV)` on `Kaboos_DecryptioKeycard1` (used by that one entity
   only; `NYX_DecryptionKey` is another "Decryption Key"). The Passcode Disk key is shared by
   `CriptoKey_Prelude` and `CriptoKey_Generic`, so its line starts "In Vanduul-Tech Smugglers
   (Nyx):" and its name gets no suffix. Every entry is `append` with `expect_sha256` of the
   stock text (same on LIVE, HOTFIX and PTU) and the new `optional = true`: a build without the
   keys is `not-needed` with no warning; a changed text is `target-changed` and skipped. Its
   switch is separate from "Fix known CIG text bugs" in Enhancements (the setup wizard's
   "Fix known CIG text bugs" off still turns off every built-in patch, this one included).

`OUTPUT_VERSION` 5. The in-game preview gains a "Quest item" sample (an automatic hint) and,
from the real backend, the Decryption Key's patched description; the Quest items tab focuses
them.

**Tests.** `model::quest` (systems), `enhance::quest` (families, hint text, general goods and
shared keys), enhance golden + `quest_hints_on_and_off` (two hints, three items skipped, off
removes exactly them, no `LOC_*` key), `patch` `quest_items_pack_applies_and_skips_changed_text`
(applies on the LIVE texts, a changed text is `target-changed`, a build without the keys is
quietly `not-needed`), data `quest_items_from_spawns_and_hauling_orders` (fixture MBE + hauling
contract, shared key with a `_debug` variant and with an unrelated datapad).

**Open.** More hand-written packs as mission facts come in. Data Heist items name only the one
broker mission with a plain title. Loot-table and shop sources ("where to find") stay with the
MCP server.

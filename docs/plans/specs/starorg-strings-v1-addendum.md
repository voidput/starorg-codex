---
title: StarOrg Strings v1 — addendum (user requests 2026-11-14)
tags: [docs, plans, specs]
status: draft
updated: 2026-11-15
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

Section 8 is implemented; section 7 (Developer mode) is not (the user's request was the
grouping only; section 7 stays spec text).

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
  channel and session (Retry on failure, e.g. `busy`).
- **Not done:** sticky group headers (Q-G2, as recommended); CLI output of groups; dump
  sources still give approximate groups until `sc-gamedata` publishes loc keys (8.7).

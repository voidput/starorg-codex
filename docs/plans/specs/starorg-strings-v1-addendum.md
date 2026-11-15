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

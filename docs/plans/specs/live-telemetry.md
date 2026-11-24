---
title: "Live telemetry (experimental): the game's local gRPC service in the app"
tags: [docs, plans, specs]
status: active
updated: 2026-11-23
---

# Live telemetry (experimental): the game's local gRPC service in the app

Star Citizen 5.0 can host `sc.game.telemetry.v1.GameTelemetryService` on
`127.0.0.1:<port>`: a server stream of frames (actor vitals and state, vehicle state) and two
unary calls, `ListTriggerableActions` and `TriggerGameAction`. This spec brings the read path
and a guarded write path of [sc-telemetry](https://github.com/voidput/sc-telemetry) (same
owner, MIT, Python) into StarOrg Codex as an opt-in experimental feature. User guide:
[Live telemetry](../../user-guide.md#live-telemetry); privacy:
[privacy](../../privacy.md#live-telemetry-experimental-off-by-default).

## Decisions

- **Opt-in, off by default, 5.0+ only.** `[experimental] telemetry = true` in `config.toml`
  (`starorg_core::telemetry::Settings`). Offered for a channel whose `build_manifest.id`
  branch gives game version 5.0 or newer (`BuildInfo::game_version`, `eligible`): PTU 5.0
  today, LIVE by itself once LIVE reports 5.0. An unknown version is not offered. The screen
  is in the sidebar (Game group, `Exp` badge) only while the switch is on.
- **Loopback only, nothing leaves the computer.** The service is plaintext and has no
  authentication. The client connects to `127.0.0.1:<port>` and nothing else
  (`starorg_telemetry::client::LoopbackAddr` refuses any other address and every host name but
  `localhost`, without a lookup). The app never sets `GRPC_CLIENT_SERVER_EXTERNAL`, and the
  help text says never to.
- **No CIG file in the repo.** No `.proto`, no extracted descriptor, no recording. The schema
  (the `FileDescriptorProto`s embedded in `StarCitizen.exe`) is read at run time from the
  user's own `Bin64/StarCitizen.exe` (`starorg_telemetry::schema`, a port of sc-telemetry's
  `schema.py`: 8 MiB windows with 2 MiB overlap, the longest copy of each file kept) and cached
  as a `FileDescriptorSet` in `<cache>/telemetry/<Branch>-<change>.fds`, 3 builds kept. Tests,
  samples and the mock use a synthetic schema written in code (`starorg_telemetry::mock`).
- **Storage.** `<cache>/telemetry/` is its own area in Settings > Storage
  (`starorg_core::storage::Area::Telemetry`, "Live telemetry schemas"). It counts toward the
  cache folder's size and cap but is never evicted by the cap: it is a few hundred KB and its
  writer prunes it.
- **USER.cfg.** Switching on adopts `server.game.telemetry.enabled = 1` in the chosen
  channel's `USER.cfg` with the language line's ledger rules (`usercfg::adopt`, its own ledger
  `<hash>.telemetry.json`); "Metrics + actions" adds
  `server.game.telemetry.trigger.actions.enabled = 1` only on top of a telemetry line that is
  really there. An explicit other value (`= 0`) is replaced only after the user agrees, and
  comes back on undo. Switching off releases both lines on every registered channel.
- **Environment.** The game also needs `GRPC_CLIENT_SERVER_ENABLED=1` and
  `GRPC_CLIENT_SERVER_PORT=<port>` from the RSI Launcher's environment. The app never sets
  them; `env_help` gives per-OS steps (Windows `setx` plus a full launcher restart; Linux LUG
  or Wine script, Lutris, Steam `%command%`) with a copy button.
- **Write path, guarded twice.** `TriggerGameAction` only for actions the game lists in
  `ListTriggerableActions`, only while "Metrics + actions" is chosen, through
  `TriggerGate` (name and hold checks, allowlist, 1 s debounce per action, `confirmed` for the
  dangerous ones from sc-telemetry's `DANGEROUS`). The UI asks before the first press of each
  app session and before every dangerous press.
- **Stream lifetime.** One stream at a time, on Tauri's Tokio runtime. It starts when the Live
  telemetry screen opens and stops when it closes, when the switch goes off and when the app
  exits (`RunEvent::Exit`). It reconnects by itself with backoff (1 s doubling to 10 s; the
  game ends the stream with `UNAVAILABLE` when it quits).

## Design

| Layer | Where | What |
|---|---|---|
| Client | `crates/starorg-telemetry` | schema scan and cache, tonic channel and dynamic codec (prost-reflect), proto3 JSON, `Snapshot`, `live::run` (status on change, frames throttled to 10 Hz, latest wins), `TriggerGate`, synthetic schema and generator |
| Gate | `crates/starorg-core/src/telemetry.rs`, `usercfg.rs` | settings in `config.toml`, `eligible`, `game_exe`, loopback checks, managed `USER.cfg` lines, `env_help` |
| Host | `app/src-tauri/src/telemetry.rs` | `TelemetryHost`: the running stream (game or synthetic), sessions, the gate |
| Contract | `app/src-tauri/src/contract/telemetry.rs` (+ `real/telemetry.rs`, `mock_telemetry.rs`) | `telemetry_status`, `telemetry_configure`, `telemetry_start`, `telemetry_stop`, `telemetry_actions`, `telemetry_trigger`, `telemetry_gamelog`, `telemetry_people_clear`; event `telemetry` |
| UI | `app/src/lib/telemetry.ts`, `lib/stores/telemetry.svelte.ts`, `components/screens/TelemetryScreen.svelte`, `components/telemetry/`, `components/settings/TelemetrySection.svelte` | Settings section, screen panels, TS mock (`lib/contract/mock-telemetry.ts`) |

**Events.** `telemetry` carries `{session, status, frame, gamelog}`: a `LinkStatus` change
(`disabled`, `no_schema`, `connecting`, `connected`, `game_not_running`, `unreachable`) or a
frame `{seq, received_ms, snapshot, raw}`. `session` goes up at every start and stop; the
webview drops events of an older session. A start that cannot get a schema answers with a
`no_schema` status (and event) instead of an error.

**Rendering.** Frames arrive at most 10 a second into one `$state.raw` value. Each panel reads
only its fields, so a frame changes a few text nodes and bar widths. The raw frame inspector is
closed by default and, open, follows once a second (Pause holds it). The actions list is fetched
on open, when the switch changes, when the game connects, and on Refresh.

**Panels** (after sc-telemetry's `static/index.html`): status and connection (badge, channel,
endpoint, schema origin), key readouts (speed, vertical speed, G, master mode, gear), Pilot
(health, oxygen, heart rate, body temperature, gravity), Ship (hull, hydrogen and quantum fuel,
boost), Shields (level and faces, labelled by count as in `shield_faces.json`), Systems
(engines, shield generators, lights, gear, landed, spool, boost) and Annunciators (low health,
oxygen, hull, fuel, shields off or down, high G), Actions (by category, Press and Hold, the
current state for lights, gear and shield power from `action_state_map.json`'s verified
groups), Raw frame.

**Mock.** The mock backend and the in-browser mock stream the synthetic generator (about the
game's rate, throttled like a real stream) for the mock's PTU, which runs 5.0; presses flip its
lights, gear and shields. `?telemetry=on` (or `=triggers`) starts the browser mock with the
feature on.

## Tests

- Crate: synthetic descriptors found in noise across window edges, cache hit skips the scan,
  loopback refusal, gate refusals, JSON shape, an in-process tonic server for stream, list,
  trigger and the reconnecting loop (`tests/roundtrip.rs`).
- Core: settings round trip, the 5.0 gate, both managed lines with their own ledgers, shared
  symlinked `USER.cfg`, `NeedsDecision` and restore.
- App contract (`contract/tests_telemetry.rs`): off by default, PTU offered and LIVE not,
  configure writes and releases lines, the synthetic stream connects, throttles and stops,
  a new stream replaces the old, every refusal reason, IPC shapes, TS unions and commands.
- Real backend (`tests/real_telemetry.rs`): fake install, `config.toml` and `USER.cfg` written
  and restored, `= 0` replaced only after asking, `no_schema` without an exe, a cached synthetic
  schema streaming to a closed port reports `game_not_running`, Storage counts the schema.
- UI (`app/tests/telemetry.test.ts`, `nav.test.ts`): the TS snapshot equals the Rust one on
  the samples, readouts, systems and annunciators, action grouping and states, confirmation
  rule, the TS mock's stream and gate.
- Not covered: a real `StarCitizen.exe` (the crate's `#[ignore]` test `schema::tests::real_exe`
  reads `$STARORG_SC_EXE`) and a real game session.

## Phase 2 ideas

From sc-telemetry, not in this phase:

- **Alerts**: rules over frames and events (low fuel, shield faces down, hull, G) as toasts and
  sounds, with per-rule cooldowns (sc-telemetry `alerts.py`, `docs/telemetry/alerts.md`);
  speech off by default. **Notifications get their own choice, off by default**, separate from
  Metrics and actions (asked for 2026-11-23): turning on Metrics never turns on toasts or sounds.
- **Game.log signals**: done, see [Game.log events and People](#gamelog-events-and-people)
  below.
- **People**: the local part is done (below). Still open: the opt-in org lookup on the public
  RSI citizen page (sc-telemetry `people.py` `OrgLookup`, `docs/telemetry/people-and-lookup.md`).
  It needs its own network switch (off), its own row in [privacy](../../privacy.md) and the
  network table there, `starorg-net` as the transport (one request in flight, 3 s apart, a
  fixed User-Agent without personal data, backoff on 403/429/503), a clearable cache in the
  cache folder counted by Settings > Storage, and hand-written HTML fixtures only.
- **History and charts**: a short ring buffer of the headline series (`/api/history`), drawn
  as sparklines.
- **Ship capabilities** from the channel's game data (sc-telemetry `shipcaps.py`): which
  systems a hull has, to grey out what it lacks.
- **Full action/state map**: port `data/action_state_map.json` (VTOL, doors, weapons,
  countermeasures) once more links are verified in game.
- **Shield face layout** per ship class (`shield_faces.json` `overrides_by_class`) once face
  order is confirmed.
- A stream that keeps running in the background (for alerts) instead of only while the screen
  is open.

## Game.log events and People

A port of sc-telemetry's `gamelog.py` (session, signals, the quantum and spawn parsers of its
`gamelog-parser-plan.md`) and the local half of `people.py`.

- **Switches.** `[experimental] telemetry_gamelog` ("Read Game.log events") and
  `telemetry_people` ("People"), both off, each its own `SwitchRow` under Live telemetry in
  Settings. Game.log counts only with telemetry on (`Settings::gamelog_active`), People only
  with Game.log on (`people_active`). Neither changes `USER.cfg`; the IPC fields default to
  off when absent.
- **Reader.** `starorg_telemetry::gamelog`: `tail::Tail` finds the channel's `Game.log` with
  `resolve_ci` like the blueprint import, opens it read-only per poll (no handle kept between
  polls, so the game can still move it to `logbackups/` on Windows), reads only the new bytes
  (at most 4 MiB a poll, quicker polls while catching up) and starts over on a smaller size, a
  changed 128-byte head or, on Unix, a changed inode; a directory of that name is never
  opened. `GameLog` folds lines (partial lines kept across reads, lines over 1 MiB dropped)
  into `SessionState`, `SignalsState` and, with People on, `PeopleState`; events are a ring of
  200, nearby entities capped at 1000 and people at 300.
- **Lifetime.** The host (`TelemetryHost::set_gamelog`) runs the reader on a thread of its own
  for the running stream session, also when the stream says `no_schema`: it starts with
  `telemetry_start`, restarts when either switch changes, and stops with the stream, with the
  telemetry switch and at exit. The mock backend feeds `gamelog::mock` lines (fake ids,
  `TestPilot_*` handles) instead of a file.
- **Contract.** The view (`GameLogView`: session, signals, events, people) rides the
  `telemetry` event as `gamelog`, sent only when it changed and at most every 500 ms, so the
  panels never re-render at the frame rate. `telemetry_gamelog` reads it,
  `telemetry_people_clear` forgets every person (your own handle stays known so you are never
  listed as someone else).
- **UI.** `components/telemetry/EventsPanel.svelte` (where, shard, quantum, deaths and
  respawns, own ship, ships nearby, the newest 100 events) and `PeoplePanel.svelte` (by
  relation: crew, party, party?, nearby; Clear), helpers in `lib/telemetry.ts`.
- **Privacy.** Server address, port and session ids are not kept at all. Events carry class
  names, codes and counts, never an entity id or a handle (a killer's name goes only to
  People). Handles stay in memory, never in Activity; `diag::secrets` replaces every handle
  the reader holds with `<handle>` in the diagnostics report. No network request.
- **Deaths.** A corpse item list has no owner. It counts as your death only when your own
  `Spawned!` follows within 60 s (`DEATH_WINDOW_MS`); the first spawn after a join is the login.
- **Not ported yet:** the telemetry vehicle as an own-ship source (own ship comes from seat
  releases and quantum target selection only), multi-line HUD toasts for crime, injuries and
  contracts (parser plan F2 onward), recordings and replay, alerts on Game.log events, the org
  lookup (above).

Tests: `crates/starorg-telemetry/src/gamelog/tests.rs` (synthetic lines: session without
addresses, nearby bursts and own-ship exclusion, quantum route and timeout, zones, corpse and
respawn, toast rules and multi-line toasts across reads, killers and victims, bounds, the tail
over truncation, in-place rewrite, rotation and a directory named `Game.log`),
`contract/tests_telemetry.rs` (the switches on the mock, the reader's lifetime, TS unions),
`tests/real_telemetry.rs` (a fake install's `Game.log`, appended lines, nothing in Activity,
handles replaced in diagnostics), `app/tests/telemetry.test.ts` (helpers and the TS mock).

## Open

- The scan and field paths have not run against a real `StarCitizen.exe` in this repo's CI.
- `THIRD_PARTY_NOTICES.md` needs regenerating for the new dependencies (`scripts/gen-notices.sh`).

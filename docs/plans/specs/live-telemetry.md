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
| Contract | `app/src-tauri/src/contract/telemetry.rs` (+ `real/telemetry.rs`, `mock_telemetry.rs`) | `telemetry_status`, `telemetry_configure`, `telemetry_start`, `telemetry_stop`, `telemetry_actions`, `telemetry_trigger`; event `telemetry` |
| UI | `app/src/lib/telemetry.ts`, `lib/stores/telemetry.svelte.ts`, `components/screens/TelemetryScreen.svelte`, `components/telemetry/`, `components/settings/TelemetrySection.svelte` | Settings section, screen panels, TS mock (`lib/contract/mock-telemetry.ts`) |

**Events.** `telemetry` carries `{session, status, frame, alerts}` (alerts: see [Alerts](#alerts)): a `LinkStatus` change
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

## Alerts

A port of sc-telemetry's `alerts.py` engine and `data/alerts.default.json` (frame rules only;
`docs/telemetry/alerts.md` there).

- **Their own choice, off by default** (asked for 2026-11-23): Settings -> Experimental ->
  Live telemetry -> **Notifications**: Off / In app / In app + sound
  (`[experimental] telemetry_notifications`, `starorg_core::telemetry::AlertSettings`).
  Turning on Metrics never turns on toasts or sounds. **Spoken callouts**
  (`telemetry_speech`) is a further switch, off by default, used only with sound.
- **Engine** (`starorg_telemetry::alerts`, rules in `crates/starorg-telemetry/data/`):
  `threshold` (hysteresis via `rearm`), `transition`, `edge`, `composite`; severity, scope,
  cooldown (never drops an alert: it fires when the cooldown ends if the condition still
  holds), debounce, `when` guard (a gated rule baselines), `supersedes`, `on_entry`; derived
  `$oxygen`, `$shield_faces_down` (face shares learnt at full charge), `$quantum_seen`. The
  first frame, the first 1.5 s after a vehicle appears or changes, and a gap over 3 s are a
  baseline. Missing data never fires.
- **Overrides** in `[experimental.telemetry_alerts.<id>]`: `enabled`, `cooldown_s` (0 to
  3600), and for threshold rules `value` and `rearm` (a new value alone keeps the default's
  gap; a fraction stays 0..1; the re-arm point must be past the threshold). Values equal to
  the defaults are dropped on save; a misfit refuses the save; a saved misfit is skipped with
  a warning.
- **Where it runs**: in the host (`app/src-tauri/src/telemetry.rs` `AlertHub`) on the frames
  the stream forwards (at most 10 Hz), only while a stream runs and Notifications is not Off.
  Fired alerts ride on the frame's `telemetry` event (`alerts`), and the last 50 stay in memory
  for the screen's **Alerts** panel. The host strips `sound` and `speech` unless the user chose
  them. Commands: `telemetry_alerts`, `telemetry_alerts_configure`, `telemetry_alert_test`.
- **Delivery** (webview): a toast per alert (a newer one of the same rule replaces it; leaves
  after 5 to 12 s), and with sound one queue (`lib/telemetryAlerts.ts` `AlertAudioQueue`: one
  at a time, 2 s apart, severity first, critical cuts in, 6 waiting at most, 10 s old dropped)
  playing WebAudio tones made in code (`lib/alertSound.ts`; no audio file) and Web Speech with
  `localService` voices only. The recent list is a raw state replaced only when an alert fires.
- **Tests**: `alerts/tests.rs` (baseline, crossing, hysteresis, cooldown, debounce, guard,
  supersedes, vehicle change and gaps, missing data, transitions, edges with `on_entry`,
  composites, face shares, templates, overrides), host tests (off evaluates nothing, delivery
  stripping, bounded list, normalisation), contract tests (off by default, independent of
  Metrics, overrides checked, IPC), `app/tests/telemetryalerts.test.ts` (audio queue, recent
  list, threshold text, override patching, the TS mock against the Rust samples).

## Phase 2 ideas

From sc-telemetry, not in this phase:

- **Alerts**: done for frame rules, see [Alerts](#alerts). Still open: sc-telemetry's `event`
  rules (Game.log signals: armistice, shard, disconnects, quantum arrival, deaths), and alerts
  while the screen is closed (needs the background stream below).
- **Game.log signals**: session, location, quantum travel, deaths and respawns from the
  channel's `Game.log`, which the telemetry API lacks (`gamelog.py`,
  `docs/telemetry/gamelog-signals.md`). The app already reads `Game.log` for blueprints.
- **People**: handles seen in `Game.log`, grouped by relation, with an opt-in org lookup on
  the public RSI page (`people.py`). Needs its own network switch and the no-real-handles rule
  for fixtures.
- ~~**History and charts**~~: done, see *Phase 2: history, recording, replay* below.
- **Ship capabilities** from the channel's game data (sc-telemetry `shipcaps.py`): which
  systems a hull has, to grey out what it lacks.
- **Full action/state map**: port `data/action_state_map.json` (VTOL, doors, weapons,
  countermeasures) once more links are verified in game.
- **Shield face layout** per ship class (`shield_faces.json` `overrides_by_class`) once face
  order is confirmed.
- A stream that keeps running in the background (for alerts) instead of only while the screen
  is open.

## Phase 2: history, recording, replay

Ported from sc-telemetry `server.py` (`history`/`/api/history`, `--record` with
`session_recorder`, `--replay` with `replay_records`/`run_replay`).

- **History** (`starorg_telemetry::history`): a ring of one sample a second, 600 kept (10
  minutes), of speed, G, hydrogen and quantum fuel, hull, shield, health and oxygen. The host
  pushes every frame it hands to the UI (a replay: every frame it plays, at the recorded time).
  It survives a stop and start of the same channel and is cleared for another channel, a
  replay and a seek (an epoch). `telemetry_history {epoch, since_ms}` answers only what is new;
  another epoch gets the whole ring with `reset`. The UI polls once a second while the screen
  is open and redraws the graphs then: SVG paths, no chart library, at most 1 Hz.
- **Recording** (`starorg_telemetry::record`): "Record sessions" is its own switch
  (`[experimental] telemetry_record`, `telemetry_record` command), off by default, and counts
  only while telemetry is on; a configure keeps it, Metrics never turns it on. A file per live
  stream, opened at its first frame: `<YYYYMMDD-HHMMSS>-<CHANNEL>.jsonl` (UTC) in
  `<data>/telemetry/recordings/`, a `{"t", "starorg": {format, channel, schema}}` first line
  then `{"t", "frame"}` lines at most 4 a second (sc-telemetry's format). Caps: 64 MiB a file
  (it stops there), 20 files and 256 MiB in all (the oldest are pruned before a new file so a
  full one fits). Only names matching `[A-Za-z0-9_-]{1,80}.jsonl` are listed, counted or
  removed; the webview names a recording, never a path. Settings > Storage: *Live telemetry
  recordings* (data folder, outside the cache cap). The mock backend keeps its recordings in
  memory, seeded with two synthetic ones.
- **Replay** (`starorg_telemetry::replay`): one pass over the file builds an index of frame
  lines (time, offset, length), so a seek is one read and a file is never loaded whole. Lines
  without an object `frame`, a non-finite time, torn or over-long lines are skipped; times that
  run back are held. A thread plays frames at the recorded pace (gaps cut to 5 s) divided by the
  speed (0.25 to 16), at most 10 a second to the UI, through the same event with a
  `TelemetryReplayState`; a seek refills the history with the 10 minutes before it. Commands:
  `telemetry_replay_start`, `telemetry_replay_control {playing, seek_ms, speed}`,
  `telemetry_stop`. Play at the end starts over. The recording being written or played is not
  deletable (`busy`).

Not done here: alerts and their own notification switch, Game.log signals, people, ship
capabilities (other branches).

## Open

- The scan and field paths have not run against a real `StarCitizen.exe` in this repo's CI.
- `THIRD_PARTY_NOTICES.md` needs regenerating for the new dependencies (`scripts/gen-notices.sh`).

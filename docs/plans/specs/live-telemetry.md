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

**Events.** `telemetry` carries `{session, status, frame, replay, alerts, gamelog}` (alerts: see [Alerts](#alerts), gamelog: see [Game.log events and People](#gamelog-events-and-people)): a `LinkStatus` change
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

## Phase 2 status

Merged on `feat/telemetry-phase2` (history, alerts, Game.log branches plus the join history
and an integration pass). Each part is its own switch, off by default; none turns on another.

Done:

- ~~**History and charts**~~: [Phase 2: history, recording, replay](#phase-2-history-recording-replay).
- ~~**Recording and replay**~~: same section.
- ~~**Alerts** over frames~~ with their own Notifications choice: [Alerts](#alerts).
- ~~**Game.log signals**~~ and the local half of **People**:
  [Game.log events and People](#gamelog-events-and-people).
- ~~**Server and shard join history**~~: [Sessions](#sessions-the-shard-join-history).
- ~~**Screen and Settings layout**~~: the screen runs live readouts, then Alerts and Actions,
  History, Game.log (Events, People, Sessions; one "off" card with a Settings link when its
  switch is off), then Recordings and the raw frame. Every panel below the readouts is a
  `CollapsibleCard` (remembered per `telemetry.<panel>`). Settings groups Record sessions,
  Read Game.log events, People and Shard history under "More from Live telemetry", next to the Metrics /
  actions choice and the Notifications choice, with a table of the choices in the user guide.

Still open (sc-telemetry has them, not ported):

- **People org lookup** on the public RSI citizen page (sc-telemetry `people.py`
  `OrgLookup`, `docs/telemetry/people-and-lookup.md`). It needs its own network switch (off),
  its own row in [privacy](../../privacy.md) and the network table there, `starorg-net` as the
  transport (one request in flight, 3 s apart, a fixed User-Agent without personal data,
  backoff on 403/429/503), a clearable cache in the cache folder counted by Settings > Storage,
  and hand-written HTML fixtures only.
- **Alerts on Game.log events** (sc-telemetry `event` rules: armistice, shard change,
  disconnects, quantum arrival, death), on top of the Game.log reader.
- A stream that keeps running in the background (for alerts and the join history) instead
  of only while the screen is open.
- **Ship capabilities** from the channel's game data (sc-telemetry `shipcaps.py`): which
  systems a hull has, to grey out what it lacks.
- **Full action/state map**: port `data/action_state_map.json` (VTOL, doors, weapons,
  countermeasures) once more links are verified in game.
- **Shield face layout** per ship class (`shield_faces.json` `overrides_by_class`) once face
  order is confirmed.
- Game.log: the telemetry vehicle as an own-ship source, multi-line HUD toasts for crime,
  injuries and contracts (parser plan F2 onward).
- Not checked in a running app against a real game yet; all tests use synthetic data.

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
  contracts (parser plan F2 onward), alerts on Game.log events, the org lookup (above).

Tests: `crates/starorg-telemetry/src/gamelog/tests.rs` (synthetic lines: session without
addresses, nearby bursts and own-ship exclusion, quantum route and timeout, zones, corpse and
respawn, toast rules and multi-line toasts across reads, killers and victims, bounds, the tail
over truncation, in-place rewrite, rotation and a directory named `Game.log`),
`contract/tests_telemetry.rs` (the switches on the mock, the reader's lifetime, TS unions),
`tests/real_telemetry.rs` (a fake install's `Game.log`, appended lines, nothing in Activity,
handles replaced in diagnostics), `app/tests/telemetry.test.ts` (helpers and the TS mock).

## Sessions: the shard join history

Behind its own switch, `[experimental] telemetry_sessions` ("Shard history", off), a
`SwitchRow` under "Read Game.log events" like People: it counts only with Game.log on
(`Settings::sessions_active`). It is the only part of the Game.log family that writes a file
and the only one that opens `logbackups/`, so it is not bundled into the Game.log switch;
turning Game.log off turns it off too, turning Game.log on never turns it on. With it off
ended stints are dropped, no backup is planned or opened, and the file is not touched. Facts
from sc-telemetry's `gamelog.py`
and its session notes; the parsing is the [Game.log](#gamelog-events-and-people) reader's
`SessionState`, which never reads the address, port or session ids at all.

- **Rows** (`starorg_telemetry::gamelog::stints`): one `Stint` per shard stint: channel,
  build, shard, `shard_short`, region code and label, `log_started`, `joined_ms`, `left_ms`,
  `left_reason` (`leave` | `shard_change` | `disconnect` | `dropped` | `log_end`),
  `uptime_s`, the star systems seen (12 at most), disconnects and `dropped`. A stint starts at
  `<Join PU>` and ends at the menu's `Frontend` loading screen or another join (`leave`), a
  real `<Update Shard Id>` change (the usual repeat of the joined shard is not one), a new
  `Log started`, a rotation of the live file or the end of a backup (`log_end`). A
  `<Channel Disconnected>` inside it (not `_Frontend`; lines within 2 s are one) makes the
  reason `disconnect` (own quit: "Player requested disconnect", "disconnect light", "Nub
  destroyed", "before game shutdown") or `dropped`, with that line's time and `uptime_secs`.
  The raw line is never kept (it also names addresses, a session and the handle).
- **Identity.** (`log_started`, `joined_ms`, `shard`): reading the same log again (the live
  file at every reader start, then its copy in `logbackups/`) adds nothing.
- **Store** (`Store`): `<data>/telemetry/sessions/history.json`
  (`starorg_core::storage::telemetry_sessions_dir`, Storage area *Live telemetry session
  history*, outside the cache and its cap): `{format, scanned, stints}`, at most 2000 rows and
  2 MiB (the oldest go first), written through `history.json.tmp` and a rename, never held
  open. A file that does not parse reads as empty, says so, and is renamed to
  `history.json.bad` before the first write replaces it; a file of a newer `format` is shown
  but never written. **Clear** stays cleared: it empties the rows, keeps the list of backups
  read, and stores `cleared_ms` (wall time); `add` drops every stint with `joined_ms <=
  cleared_ms`, so neither the live `Game.log` folded again at the next reader start nor the
  same log later found in `logbackups/` under another name brings a row back. The mock backend keeps it in memory, seeded
  with four synthetic rows.
- **Backfill** (`Backfill`): at each reader start, the newest 40 `logbackups/*.log` (plain
  files only, by mtime) not marked as read (name and size), one 4 MiB read per 500 ms poll,
  each file opened and closed per read, 128 MiB of one file at most; a file's rows and its mark
  go in together when it is done. A file cut at that cap leaves out the stint still open at the
  cut (it did not end there). A read that fails (a sharing violation or a virus scanner on
  Windows, the file gone) drops what was read of it and marks nothing (`Step::Failed`); the
  next reader start tries again, at most 3 times per app run (counted in memory).
- **Host.** The Game.log thread drains ended stints into the store after every poll and reads
  one backup chunk (both only with Shard history on); the view carries the stint you are on
  (`stint`, with the reason and time of a disconnect nothing followed yet, so a closed game
  does not read as "Now") and the store's `sessions_rev`, so the UI reads `telemetry_sessions`
  again only when it moved, at most once a second (a backfill moves it up to 40 times in a
  row; `sessionsReload` in `lib/telemetry.ts`). Whole list each time (2000 rows at most): the
  channel and region filters work on all rows, so it is not cut to the 200 drawn. A long
  `Game.log` is caught up 4 MiB per 200 ms.
  `telemetry_sessions_clear` clears (Activity notes it; Storage measures again).
- **UI.** `components/telemetry/SessionsPanel.svelte`: the current stint, then the newest 200
  rows of the filter (channel, region), the reason as a badge (Dropped with a warning icon), a
  *same shard as before* mark (the previous stint of that channel was on the same shard),
  Clear. It also shows with Game.log off while it has rows. The store drops a list answer that
  a newer read or a Clear overtook (`Latest`), and Settings > Storage's Clear updates it too. Settings > Storage has **Clear
  session history** on its row.

Tests: `gamelog/stints/tests.rs` (synthetic lines: a stint and its systems, nothing private in
a row, the 2 s disconnect merge and the drop, the `_Frontend` skip and an own quit, the
shard-change repeat rule, a new log and a rotation, a rejoin, the store's dedupe, caps,
restart, clear and a broken file, a Clear that stays cleared across restarts and re-read logs,
a broken file set aside and a newer one left alone, a failed read retried and capped, a file cut
at the size cap, the current stint after a disconnect, the backfill's newest-once order with a
directory named `*.log`, a chunk boundary inside a line), `contract/tests_telemetry.rs` (the mock's history,
Storage, Clear, the TS `LeftReason` union), `tests/real_telemetry.rs` (a fake install's
`Game.log` and `logbackups/`, the file on disk without addresses or handles, no duplicate on a
second start, Storage and Clear, nothing written with Shard history off, still empty after a
Clear and a new start with the log copied into `logbackups/`), `app/tests/telemetry.test.ts` (rows, marks, filters,
durations, the TS mock).

## Open

- The scan and field paths have not run against a real `StarCitizen.exe` in this repo's CI.
- `THIRD_PARTY_NOTICES.md` needs regenerating for the new dependencies (`scripts/gen-notices.sh`).

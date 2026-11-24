---
title: Privacy policy
tags: [docs]
updated: 2026-11-23
---

# Privacy policy

*StarOrg Codex, desktop app and command-line tool. Applies to every build: Microsoft Store,
GitHub release downloads and builds from source.*

**Short version: StarOrg Codex collects nothing about you. It has no telemetry, no analytics,
no crash upload and no account of its own.**

## What the app does with data on your computer

- It reads your Star Citizen install (game files, `USER.cfg`) and, to find the install, the RSI
  Launcher's log files. On Linux it also reads the LUG Helper configuration.
- It writes the game's text file (`Data\Localization\english\global.ini`), one line in
  `USER.cfg`, and its own settings, backups and cache in your user folders
  (`%APPDATA%\StarOrg Tools\Codex\` and `%LOCALAPPDATA%\StarOrg Tools\Codex\` on Windows,
  `~/.config`, `~/.local/share` and `~/.cache` under `starorg-codex` on Linux).
- None of this leaves your computer. Removing the app's folders and using **Remove** in the app
  deletes everything it made. Settings -> Storage lists what is kept where and how much: the
  extracted game data cache, WHERE TO GET datasets, UEX market data, asset exports, temporary
  extraction folders, live telemetry schemas, recordings and session history (experimental,
  below) and `global.ini` backups
  (gzip-compressed; 5 per channel plus the newest of each of the last 3 game builds). The cache folder is capped (1 GB by default).
- Diagnostics exports are files you save and share yourself. They replace your user name and home
  folder with placeholders.
- Developer mode (Settings -> Advanced, off by default) only reads: `Data.p4k` (read only, as
  for extraction) and the JSON folders you pick. Record and type exports go only to a file you
  choose in a save dialog. It makes no network request; the folder list is kept in `app.json`.
  Its Files browser, Sounds and Export model read `Data.p4k` the same way, only when you open
  them; extracted files, sounds and models go only to a folder or file you pick, never into the
  game folder. Decoded sounds, exported models and texture previews are kept in an asset cache
  (`<cache>/assets`, or a `StarOrg Codex asset cache` subfolder of a folder you choose in
  Settings -> Advanced; 1 GB at most; only its own files are ever removed) that
  "Clear" empties. These are Cloud Imperium's assets: for your own use, not to redistribute.
- The memory limit of the Developer mode and MCP workers (Settings -> Advanced -> Memory limit,
  `[dev] memory_limit_gb` in `config.toml`) reads this computer's total and free RAM locally
  (`/proc/meminfo` on Linux, the system's memory status on Windows) to show it next to the slider
  and in the warning before Developer mode or the MCP server is turned on. It is not sent anywhere.

## Network

With the default settings the app's own code makes **no network request**. Each item below is
off until you turn it on in Settings -> Network (the setup wizard's Preferences step asks about
Check for updates and Connect StarOrg Tools, with both off unless you switch them on):

| Setting | What is sent, to whom |
|---|---|
| Check for updates (not in the Microsoft Store version) | One request to `api.github.com` for the list of releases, at most once a day plus when you press Check now. It sends only the app name and version as its user agent: no account, token or install id. Installing an update downloads the release files from `github.com`. |
| Connect StarOrg Tools | Nothing until you sign in, and then only to the environment chosen in Settings -> StarOrg Tools: `starorg.tools` (Production, the default) or `dev.starorg.tools` (Dev, the test server). Details below. |
| Download UEX market data | Nothing until you press **Fetch now** (Settings -> Network, or Enhancements -> Where to get -> Shops and sell prices). Then three requests to `api.uexcorp.space` (`terminals`, `items_prices_all`, `commodities_prices_all`: where items are sold and what commodity kiosks pay, player-reported), written to the UEX folder in the app's cache. They send the app name and version as the user agent and, only if you saved one, your UEX token (`Authorization: Bearer`); nothing else: no install id, no game data, no character. The token is kept in the system keyring (or, when there is none, a file only your user can read in the app's config folder), never in settings, logs or diagnostics, never shown again, and sent only to `api.uexcorp.space`. **Remove** deletes it. |
| Download published LIVE builds for Diff (Developer mode only) | Only for Developer mode's Diff, to compare an old LIVE build with a new one: the Datasets screen asks `api.github.com` for the release list of `voidput/sc-gamedata-dumps`, and **Download** fetches that one build's tag, file list and data files from `api.github.com` and `raw.githubusercontent.com`, each checked by checksum and read as data only. It sends only the app name and version as its user agent: nothing about you, no token, no install id. Downloaded builds stay in the app's cache until you delete them and work with the switch off. Applying never uses them: it always reads `Data.p4k` on this computer. |

Like any web request, these reveal your IP address to the server (GitHub, StarOrg Tools, UEX
Corp). GitHub's privacy statement covers what GitHub logs; UEX Corp's covers `uexcorp.space`.

### StarOrg Tools

Settings -> StarOrg Tools -> Environment picks the one host the app talks to: `starorg.tools` or
`dev.starorg.tools`. Switching signs you out of the other: its token is removed here, and revoked
there when "Connect StarOrg Tools" is on and the host answers. Changing the environment with
`starorg-codex sync set` signs the old one out the same way the next time the app opens StarOrg
Tools.

- **Sign-in.** Your browser opens StarOrg Tools' sign-in page, which signs you in with Discord
  there. The app only listens on `127.0.0.1` for the browser's return and then sends the one-time
  code with a check value (PKCE) to that host. It gets back a token (90 days, valid only for the
  routes below), your Discord user name and the orgs you belong to. The token is kept in the
  system keyring (Windows Credential Manager, the Secret Service on Linux), or, when there is
  none, in a file only your user can read in the app's config folder; never in its settings, logs
  or diagnostics. **Sign out** deletes it here, and revokes it on the server only while "Connect
  StarOrg Tools" is on and the host answers. Signed out with the switch off or offline, the token
  stays valid on the server until it expires; the app log says which happened. To end it there,
  sign out again with the switch on, or use "Sign out everywhere" on StarOrg Tools.
- **What to sync** (Settings -> StarOrg Tools), one switch each, also read by the command-line
  tool (`starorg-codex sync status`):

| Switch (default) | What is sent, to the chosen host |
|---|---|
| Import blueprints from Game.log (on) | Nothing: it reads the game's logs on this computer. Off, the app, the command-line tool and the post-exit hook do not read the logs at all. |
| Push owned blueprints (off) | The ids of the blueprints owned by the characters you tick, and when each was received when Game.log or StarOrg Tools says so (`POST …/my-blueprints`). First, the public blueprint list is downloaded without your token. Never character names, ids or log text; never removals. Names StarOrg Tools has no single blueprint for are not sent. |
| Pull owned blueprints (off) | Only the request with your token (`GET …/my-blueprints`); the list is added to one character here. |
| Pull fleet/hangar (off) | Only the request with your token (`GET /api/fleet/hangar`); ship names, counts and your ship names go into Ships. |
| Sync automatically after game exit (off) | Runs the switches above when the game closes, instead of only when you press Sync now. |

Requests go to the org you pick (the blueprint routes carry its Discord id). The account and
org names shown in the app are kept in `app.json`.

The Microsoft Store version never checks GitHub: the Store installs its updates.

**Support, Buy me a coffee and Patreon** (footer and Footer -> About) open `discord.gg`,
`buymeacoffee.com` or `patreon.com` in your browser, only when you click them. The app itself
sends nothing: your browser loads the page, under that site's privacy terms. The **uexcorp.space**
link next to the UEX token (Settings -> Network) works the same way. The app can open only these
four fixed addresses.

## Local MCP server (experimental, off by default)

Settings -> Experimental -> Local MCP server, and the command `starorg-codex mcp`, let an AI
assistant on your computer ask read-only questions about your game data ([details](mcp.md)).

- It makes no network request. The app's server listens on `127.0.0.1` only (other computers
  cannot reach it) and needs a token that is made fresh each time it starts. It is off at every
  start of the app.
- Answers can include your owned blueprints and character names. They go only to the local
  client you connected. If that client is an assistant that runs in the cloud, it sends the
  answers to its provider as part of your conversation, under that provider's privacy terms.
- It writes only its cache in the app's cache folder. Shop and commodity prices are used only
  if the UEX files are there: downloaded by the app's **Fetch now** (above), or put there by you. The MCP
  server never downloads them.

## Live telemetry (experimental, off by default)

Settings -> Experimental -> Live telemetry reads Star Citizen's own live data (vitals, ship,
fuel, shields) from the game's local service while you play, for 5.0 builds and newer
([details](user-guide.md#live-telemetry)). The app still collects nothing.

- It talks only to `127.0.0.1` on this computer (any other address is refused) and only while
  the switch is on and the Live telemetry screen is open. It makes no network request.
- Frames are shown and then dropped, unless you turn on **Record sessions** (below): nothing
  is logged or sent anywhere. The screen's graphs keep the last 10 minutes of a few values
  (speed, G, fuel, hull, shields, health, oxygen) in memory while the app runs; they are never
  written to disk. Activity notes only the switch changes (on or off, the port, whether game
  actions are allowed, Game.log events on or off, People and Shard history on, Record sessions
  on or off), each start and stop, which cached schema a start used or why there was none (that
  reason can name the path of the game's exe), that Game.log events are being read, each
  replay started and recording deleted (by file name), that the shard history was cleared, and
  the name and result of each game action pressed. None of these lines carries a log line,
  a shard, a handle or an address.
- **Record sessions** (its own switch, off by default; turning on Metrics never turns it on)
  saves each live stream, 4 frames a second, as a JSONL file in the app's data folder
  (`telemetry/recordings/` under `%APPDATA%\StarOrg Tools\Codex\data\` or
  `~/.local/share/starorg-codex/`). A file holds what the game sent in those frames: your
  in-game state (vitals, ship, position), the channel and the build's schema name; no account,
  no address. At most 20 files and 256 MB in all (64 MB each); the oldest go first. Settings ->
  Storage counts them, the Live telemetry screen lists, replays and deletes them, and turning
  the switch off ends a recording at once. They never leave the computer: share one only if you
  mean to.
- Turning it on writes one line to the chosen channel's `USER.cfg`
  (`server.game.telemetry.enabled = 1`; a second one only with "Metrics + actions" chosen) and
  saves the switches in `config.toml`. Only that one channel carries the lines: picking another
  channel, or going back to "Metrics", takes them out of the others. Turning it off
  takes them out everywhere. The app never sets the launcher's environment variables; it shows
  you how.
- To read the stream it reads the layout of the game's service from that channel's own
  `Bin64/StarCitizen.exe` and keeps it in the app's cache folder (Settings -> Storage, the last
  3 builds). No game file is shipped with the app.
- "Metrics + actions" (a separate choice, not the default) lets the app press game inputs the game lists
  as triggerable, only when you press a button. The first press of each app session asks
  first, and the dangerous actions (self destruct, eject, power off...) ask every time.
- Notifications (alerts on low fuel, shields, hull...) are their own choice, off by default;
  choosing Metrics does not turn them on. They are worked out on this computer from the frames
  above. The last 50 are kept in memory for the screen's Alerts panel and are gone when the app
  quits; Activity notes only a change of the choice (and a saved rule change that no longer fits). Tones are made by the app itself (no
  sound file, nothing from the game). Spoken callouts (a further switch, off by default) use
  only voices installed on this computer: a voice that would send the text to an online
  speech service is never used, so with no local voice nothing is spoken. The choice and your
  rule changes are saved in `config.toml`.
- **Read Game.log events** (its own switch, off by default) reads the streamed channel's
  `Game.log` on this computer while the Live telemetry screen is open: read only, the bytes
  the game added since the last read, twice a second. What it shows (session, shard and
  region, star system, quantum route, zone notices, ship classes nearby, deaths and respawns)
  lives in memory and goes when the screen closes; nothing of it is written or sent. The server address, port and session ids in the log are never read into the
  app at all, and a disconnect line (which also names addresses, a session and your handle) is
  only classified, never kept. This is separate from "Import blueprints from Game.log", which
  reads only the blueprint lines.
- **Shard history** (its own switch under Read Game.log events, off by default; turning on
  Game.log events never turns it on): one row per shard you joined (channel, build, shard id
  and region, join and leave times, how long, the star systems you went to, why it ended, how
  many disconnects and whether it dropped), saved in one file in the app's data folder
  (`telemetry/sessions/history.json`). To start it with your past sessions it also reads the
  channel's own `logbackups/*.log` (the newest 40, each once, read only, at most 128 MB of
  each, a few MB at a time, never held open). With it off, no row is written and no backup is
  opened. No address, port, session id, handle or raw log line is in it. At most 2000 rows and
  2 MB; the oldest go first. Settings -> Storage counts it; **Clear** (in the Sessions panel or
  Settings -> Storage) empties it for good: the file keeps only when you cleared it, and no
  session that started before then is added again, even when the same Game.log or one of its
  backups is read again. Activity notes only that it was cleared.
- **People** (another switch under it, off by default) also keeps the player handles Game.log
  names (crew channels, party notices, quantum calibration notices, players named in ship
  kills near you) and your own, to list them by how they relate to you. They stay in memory
  for the current Game.log only (**Clear** forgets them at once, People off or the screen
  closing too), are never written to a file, never go into Activity, and every handle held is
  replaced with `<handle>` in a diagnostics report. There is no online lookup of handles or
  orgs.

## Things outside the app

- **Windows:** the window is drawn by Microsoft Edge WebView2, which Microsoft updates and which
  has its own diagnostic settings. The Microsoft Store collects install and crash data under
  Microsoft's privacy statement. The app cannot switch these off.
- **Linux:** the window shows only the app's own pages.

## Children

The app does not collect personal information from anyone, including children.

## Changes and contact

Changes to this policy are made in this file; its history is public at
<https://github.com/voidput/starorg-codex/commits/main/docs/privacy.md>. Questions: open an issue
at <https://github.com/voidput/starorg-codex/issues>.

StarOrg Codex is a fan-made tool. It is not affiliated with or endorsed by Cloud Imperium Games
or Roberts Space Industries.

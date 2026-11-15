---
title: Privacy policy
tags: [docs]
updated: 2026-11-15
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
  (`%APPDATA%\StarOrg Tools\Strings\` and `%LOCALAPPDATA%\StarOrg Tools\Strings\` on Windows,
  `~/.config`, `~/.local/share` and `~/.cache` under `starorg-strings` on Linux).
- None of this leaves your computer. Removing the app's folders and using **Remove** in the app
  deletes everything it made.
- Diagnostics exports are files you save and share yourself. They replace your user name and home
  folder with placeholders.
- Developer mode (Settings -> Advanced, off by default) only reads: `Data.p4k` (read only, as
  for extraction) and the JSON folders you pick. Record and type exports go only to a file you
  choose in a save dialog. It makes no network request; the folder list is kept in `app.json`.

## Network

With the default settings the app's own code makes **no network request**. Each item below is
off until you turn it on in Settings -> Network:

| Setting | What is sent, to whom |
|---|---|
| Check for updates (not in the Microsoft Store version) | One request to `api.github.com` for the list of releases, at most once a day plus when you press Check now. It sends only the app name and version as its user agent: no account, token or install id. Installing an update downloads the release files from `github.com`. |
| Connect StarOrg Tools | Only after you sign in at starorg.tools in your browser: the owned-blueprint data you choose to pull or push. |
| Use published game data (when available) | Downloads public data files for your exact game build from github.com; nothing about you is sent. |

Like any web request, these reveal your IP address to the server (GitHub, starorg.tools). GitHub's
privacy statement covers what GitHub logs.

The Microsoft Store version never checks GitHub: the Store installs its updates.

## Local MCP server (experimental, off by default)

Settings -> Experimental -> Local MCP server, and the command `starorg-strings mcp`, let an AI
assistant on your computer ask read-only questions about your game data ([details](mcp.md)).

- It makes no network request. The app's server listens on `127.0.0.1` only (other computers
  cannot reach it) and needs a token that is made fresh each time it starts. It is off at every
  start of the app.
- Answers can include your owned blueprints and character names. They go only to the local
  client you connected. If that client is an assistant that runs in the cloud, it sends the
  answers to its provider as part of your conversation, under that provider's privacy terms.
- It writes only its cache in the app's cache folder. Shop prices are used only if you put UEX
  price files there yourself; the app never downloads them.

## Things outside the app

- **Windows:** the window is drawn by Microsoft Edge WebView2, which Microsoft updates and which
  has its own diagnostic settings. The Microsoft Store collects install and crash data under
  Microsoft's privacy statement. The app cannot switch these off.
- **Linux:** the window shows only the app's own pages.

## Children

The app does not collect personal information from anyone, including children.

## Changes and contact

Changes to this policy are made in this file; its history is public at
<https://github.com/voidput/starorg-strings/commits/main/docs/privacy.md>. Questions: open an issue
at <https://github.com/voidput/starorg-strings/issues>.

StarOrg Codex is a fan-made tool. It is not affiliated with or endorsed by Cloud Imperium Games
or Roberts Space Industries.

---
title: Windows test checklist (real install)
tags: [docs]
updated: 2026-11-14
---

# Windows test checklist (real install)

For the person testing StarOrg Codex on a real Windows PC with Star Citizen installed
(spec 13 and 14, Q19). Takes about 45 minutes plus one game start. Nothing here needs
administrator rights; please do **not** use "Run as administrator" at any point.

Before you start, note: Windows version (Settings -> System -> About, e.g. "Windows 11 24H2"),
where the game library is (e.g. `D:\Roberts Space Industries\StarCitizen`), which channels are
installed, and whether Smart Citizen or another localization tool is installed.

Tick each box; when something differs, write down what you saw (a screenshot helps) and go on.

## 1. Install

- [ ] Download `StarOrg Codex_<version>_x64-setup.exe` and `SHA256SUMS` from the release.
- [ ] Optional: in PowerShell, `Get-FileHash '.\StarOrg Codex_<version>_x64-setup.exe'` matches
      the line in `SHA256SUMS`.
- [ ] Run the installer. If SmartScreen says "Windows protected your PC", note it, then
      More info -> Run anyway. No administrator (UAC) prompt appears.
- [ ] If the build is signed: right-click the setup file -> Properties -> Digital Signatures shows
      the publisher.
- [ ] StarOrg Codex starts from the Start menu; the window is not blank.

## 2. First-run wizard

- [ ] Step "Find your game": the text mentions the RSI Launcher logs and drives (not Wine,
      LUG or `/mnt`).
- [ ] Your install is listed **without** pressing Browse. Note the path shown and the
      "found by" label (e.g. "RSI Launcher log", "default install path").
- [ ] Every installed channel (LIVE, PTU, ...) is listed with a version.
- [ ] If it was not found: Browse... to the folder that contains `LIVE`; it is then listed.
      Report this as a discovery miss and include the library path.
- [ ] Step "Choose channels": pick **LIVE** only.
- [ ] Steps "Game data source" and "Pick enhancements": keep the defaults (Recommended).

## 3. Dry run (Review step)

- [ ] The Review step builds a preview without writing to the game: note how long it takes
      (first build reads `Data.p4k`, can take a few minutes).
- [ ] Open the preview list: a component (e.g. a cooler or power plant) shows a tag like
      `[MIL-S1-B]`, a ship weapon a stats block, and the main-menu version line ends with
      "Text enhanced with StarOrg Codex v<version>".
- [ ] `<library>\LIVE\Data\Localization\english\global.ini` still does **not** exist (or is
      unchanged if another tool made it).

## 4. Apply on LIVE

- [ ] Make sure the game is not running. Press Apply. It finishes with "applied".
- [ ] `<library>\LIVE\Data\Localization\english\global.ini` now exists.
- [ ] `<library>\LIVE\USER.cfg` contains `g_language = english` and your other lines are
      unchanged (open it in Notepad).
- [ ] Home shows LIVE as applied; other channels are untouched.
- [ ] Start the game from the RSI Launcher, then try Apply again while it runs: it is skipped with
      "the game is running for LIVE". Quit the game afterwards if you want, or go on in game.

## 5. In game (LIVE)

- [ ] Main menu: the version text in the corner ends with **"Text enhanced with StarOrg Codex"**
      and the version number.
- [ ] A **component tag**: open a ship's components (or a shop list) and look at a component
      name, e.g. a power plant or cooler shows `[MIL-S1-B]`-style tag text.
- [ ] A **ship stats block**: a ship description (e.g. in a terminal or the mobiGlas) shows a
      `--- STATS ---` block with the ship's values.
- [ ] Optional: a mission description shows extra mission info.
- [ ] Nothing shows raw keys like `@item_Name...` or broken text where the game had text before.
- [ ] Quit the game.

## 6. Remove

- [ ] In the app, Remove for LIVE. It finishes; LIVE shows "not applied".
- [ ] `global.ini` is gone, or (if another tool's file was there before) it is that tool's file
      again.
- [ ] `USER.cfg` no longer has the line we added (lines you had before stay).
- [ ] Optional: start the game; the version text no longer mentions StarOrg Codex.

## 7. Restore a backup

- [ ] Apply LIVE again, then open Backups: there is a backup for LIVE.
- [ ] Restore it. The app shows the result; Home shows the state the backup was taken in.

## 8. With Smart Citizen (skip if you do not use it)

- [ ] With Smart Citizen's `global.ini` in place, start Apply on LIVE: the app says the file comes
      from another tool and backs it up before writing.
- [ ] Backups lists that file as from another tool (Smart Citizen).
- [ ] Remove (or restore that backup): Smart Citizen's file is back byte for byte (Smart Citizen
      still works in game).

## 9. Odds and ends

- [ ] Settings -> Network: everything is off by default. Turning on "Check for updates" and pressing
      Check now gives a result (or a clear "can't see releases" message).
- [ ] Activity -> Diagnostics: export a report. Open it: your Windows user name does not appear.
- [ ] Close and reopen the app: it does not ask to set up again; LIVE state is as you left it.
- [ ] Uninstall from Settings -> Apps, after Remove: the game folder has no `global.ini` from us.

## Report template

Copy this into a GitHub issue (title "Windows test: <version>, <Windows version>") and attach the
Diagnostics export and any screenshots.

```text
StarOrg Codex version:
Windows version and edition:
Installer: setup.exe / msi / Store / zip     Signed: yes / no
SmartScreen or Smart App Control prompt: none / "Run anyway" / blocked
Library path:
Channels installed:
Other tools installed (Smart Citizen, ...):

1 Install:            pass / fail  notes:
2 Wizard / discovery: pass / fail  found by:            notes:
3 Dry run:            pass / fail  time to build:       notes:
4 Apply LIVE:         pass / fail  notes:
5 In game:            version text: yes / no   component tag: yes / no   ship stats: yes / no
6 Remove:             pass / fail  notes:
7 Restore:            pass / fail  notes:
8 Smart Citizen:      pass / fail / skipped  notes:
9 Odds and ends:      notes:

Anything else that looked wrong or confusing:
```

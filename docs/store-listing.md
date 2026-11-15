---
title: Microsoft Store listing (draft)
tags: [docs]
updated: 2026-11-15
---

# Microsoft Store listing (draft)

Draft text for Partner Center -> the app -> Store listings -> English. Check each field's
current length limit in Partner Center when pasting. Setup steps: [windows](windows.md#microsoft-store).

## Product name

StarOrg Codex

## Short description

Clearer Star Citizen item, ship and mission text: component tags, stats, mission rewards with
[Owned] blueprint markers and quest item hints, applied to your own install and removable in one
click.

## Description

StarOrg Codex improves the in-game text of Star Citizen on your PC.

It builds a new text file for the game from the files of your own install: component names get
short tags such as size, class and grade, ship and weapon descriptions get a stats block, and
mission text gets the facts you would otherwise look up. You choose what is added, preview every
change before it is written, and can go back to the game's own text at any time.

Blueprints
- See which blueprints a contract can reward, marked [Owned] or [Missing] for your character,
  and [BP] / [BP Owned] in contract titles.
- A finder with every blueprint of the build and who owns it, per character and channel.
- Ownership comes from your game log, an import from Smart Citizen's blueprint tracker, or the
  optional StarOrg Tools sync.

More in game
- Quest items say which contract uses them, with hand-written notes for missions such as
  Vanduul-Tech Smugglers.
- Fixes for known text bugs, and your own edits to any line.
- Your ships first in game ship lists, in your order and with your names.

For the curious: Developer mode adds an item finder over the game's records, data tables and a
diff between builds, a game file browser, sounds and model export.

How it works
- Finds your game through the RSI Launcher logs or the folder you pick.
- Reads the game data from your install; nothing is downloaded unless you turn it on.
- Backs up any text file already there (for example from another tool) before writing.
- Writes only the game's localization text file and one line in USER.cfg, and checks the result.
- Never changes files while the game is running, and Remove puts everything back.

Privacy: no account, no telemetry, no analytics. With the default settings the app makes no
network request at all.

StarOrg Codex is an unofficial Star Citizen fan project, not affiliated with the Cloud Imperium
group of companies. All content in this app not authored by its host or users are property of
their respective owners. Star Citizen®, Roberts Space Industries® and Cloud Imperium® are
registered trademarks of Cloud Imperium Rights LLC. StarOrg Codex ships no game assets: it reads
the files of your own installation and changes only your own local files.

Requires a Star Citizen installation.

## Product features

- Component tags: size, class and grade in item names
- Stats blocks for ships, ship weapons, personal weapons and missiles
- Mission info in contract descriptions
- [Owned] / [Missing] blueprint markers and [BP] title tags, in colours you choose
- Blueprint finder per character and channel; game log and Smart Citizen import
- Quest item hints and fixes for known text bugs
- Ship order and names in game lists from your fleet
- Strings browser: every text key, what it belongs to, and your own edits
- Preview every changed line before applying
- Automatic backup of the existing text file; one-click restore
- Works with LIVE, PTU, EPTU, HOTFIX and other channels
- Skips changes while the game or a launcher update is running
- No account, no telemetry; offline by default

## Search terms

Star Citizen, localization, global.ini, item names, component tags, ship stats, mission info,
blueprints, owned blueprints, quest items, crafting

## Screenshots

From the headless renders (`app/scripts/render-screens.cjs`, mock backend, never a window;
output under `.tmp/renders/m3-screens/`). Use the 1280x800 dark set; Partner Center wants
PNG at 1366x768 or larger, so render the chosen shots at that size too:
`SIZES=1366x768 node app/scripts/render-screens.cjs '<name regex>'` against the mock dev server.
In order:

1. `home` - channels, build and status at a glance
2. `enhancements-tags` - choosing component tag elements
3. `enhancements-ships` - ship stats block options
4. `strings-filtered` - previewing changed strings
5. `strings-detail-rejected` - a line kept as stock, with the reason
6. `backups-foreign-restore` - another tool's file backed up and restorable
7. `home-running` - the "game is running" guard
8. `settings-updates-store` - Store build: "Updates come from the Microsoft Store."
9. `bp-live-owned` (`app/scripts/render-blueprints.cjs`, output `.tmp/renders/blueprints/`) -
   the blueprint finder with owners
10. `enhancements-colours` (`render-screens.cjs`) - [Owned] / [Missing] markers and their colours
    in the in-game preview
11. A Quest items shot: `enhancements` with the **Quest items** tab selected (add a
    `render-screens.cjs` entry like `enhancements-ships` that clicks the tab `/^Quest items/`)
12. `ships-imported` (`app/scripts/render-fleet.cjs`, output `.tmp/renders/fleet/`) - Ships: your
    fleet in order with names

Do not include the Star Citizen logo or game art.

## Other fields

- Category: Utilities and tools (secondary: none)
- Privacy policy URL: <https://github.com/voidput/starorg-codex/blob/main/docs/privacy.md>
- Website: <https://github.com/voidput/starorg-codex>
- Support contact: <https://github.com/voidput/starorg-codex/issues>
- Price: free; no in-app purchases
- Copyright: (c) voidput, MIT licence
- Age rating: see [windows](windows.md#microsoft-store) (IARC questionnaire answers)

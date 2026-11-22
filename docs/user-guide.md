---
title: User guide
tags: [docs]
updated: 2026-11-22
---

# User guide

How to use StarOrg Codex once it is installed. Installing, paths and platform problems:
[Linux](linux.md) · [Windows](windows.md). What goes over the network (by default nothing):
[privacy](privacy.md).

StarOrg Codex writes one file per game channel, `Data/Localization/english/global.ini`, plus
one `g_language = english` line in that channel's `USER.cfg`. The game reads them at start.
Everything else (settings, backups, caches) stays in the app's own folders, and **Remove** puts
the game back to stock.

## First run

1. Start the app. The setup wizard opens on the first start.
2. **Install:** it finds the game through the RSI Launcher's logs and the usual folders (on Linux
   inside your Wine prefix). Otherwise **Browse...** to the folder that contains `LIVE`.
3. **Channels:** tick the channels to set up (LIVE, PTU, EPTU, HOTFIX, ...). Setting up a channel
   is your consent to write into it; channels you leave out are never touched.
4. **Source:** where stats and mission details come from: the game data of your own install.
5. **Enhancements:** pick a starting point, **Recommended**, **Minimal** or **Everything**, and
   whether to fix known CIG text bugs. You can change every switch later.
6. **Preferences:** appearance, the update check and StarOrg Tools (both off unless you switch
   them on).
7. **Review**, then **Apply**. The first apply reads the game data from `Data.p4k`, which takes
   a while; later applies use the cache.
8. Restart the game if it was running: it reads the text file only at start.

**Run setup again** (Settings, or Ctrl+K) to add a channel; channels already set up keep their
settings.

## Applying and channel status

The channel switcher at the top picks the channel every screen works on, and **Apply to
<channel>** writes it. Home shows each channel's state:

| State | Meaning | What to do |
|---|---|---|
| Applied | The game file matches your settings and the game build | Nothing |
| Game updated / Stale | A patch, a settings change or a newer app changed what the file should be | Apply again |
| Changed by another tool | Another tool or the launcher replaced the file | Apply again (the other file is backed up first) or leave it |
| Not applied / Not set up | Nothing of ours is in that channel | Set it up, then apply |

**Dry run** on Home shows what an apply would do and writes nothing. An apply never runs while
the game of that channel is running or while the launcher installs it; it says so and waits for
you.

### After a game patch

A patch replaces `global.ini`, so the enhancements are gone until you apply again. Home shows
**Game updated**; press **Apply**. On Linux with LUG Helper, the launch hooks do this for you
before every start ([linux: launch hooks](linux.md#launch-hooks), Settings -> Launch hooks).
From the command line: `starorg-codex apply --all --if-stale`.

## Enhancements

Enhancements -> one tab per area; the **in-game preview** on the right shows before and after
for the open tab's samples only, with the game's own colours (EM styles) where they apply
("No example for ... with these settings" when that tab has none). It needs the build's game
data: when it is not read yet or the cache is from an older version (footer `cache stale`),
press **Read game data** in the preview (about 10 s; reads `Data.p4k` into the cache and
writes nothing to the game), no apply needed.

- **Ships, Components, Ship weapons, FPS weapons, Missiles:** a stats block in descriptions
  (speeds, shields, damage, fire rate, range, ...). Each line can be switched on or off.
- **Name tags:** short tags in item names, such as `[IND-S2-A]` (class, size, grade), built in
  the Tag Builder.
- **Missions:** tags in contract titles (reputation, `[BP]`) and sections in contract
  descriptions: item rewards, mission details, with colours for headers, labels and rewards.
- **Blueprints** (right after Missions): what a contract can reward and whether you own it.
  - The **blueprint list** under a contract's text, optionally with name tags.
  - **[Owned]** marks a blueprint the chosen character owns (blue, EM4, by default; *Colour the
    whole line* colours the full line); **[Missing]** marks one they do not own (off by
    default, only when an owned set is known).
  - In titles, **[BP]** when a contract can reward a blueprint, **[BP?]** when only some of its
    variants can, and **[BP Owned]** when you own every blueprint it can award. A **[BP?]**
    contract also says why under its BLUEPRINTS list: "Not every offer with this title awards
    one" (only some variants award a blueprint) or "Awarded by chance: 25–50%".
  - Which character counts is set on the Blueprints screen (below).
- **Quest items:** two switches. *Add "Used in" to items missions use* adds
  `Used in: <contract> · <system>` to items the game data ties to a mission (ores, food, armour
  and Wikelo trades are left alone). *Add mission hints the game data does not have* is the
  built-in `quest-items` text pack with hand-written hints, starting with Vanduul-Tech Smugglers
  (Nyx): where the access card, the decryption keys and the passcode disk are found and where to
  use them. Each hint is skipped when CIG changes that item's text.
- **Where to get:** a WHERE TO GET block after the description (and after its stats block) of
  items and commodities, from the game files: loot containers, crafting from a blueprint,
  contract rewards, mining places and ore quality. Names are never changed.
  - *Detail*: **Short** is one line, `Get: Loot (Rare) · Craft [BP] · Reward`; **Normal** is the
    header and one line per source with the top 3 places, `+N` for the rest
    (`Loot: Weapons rack (Rare) at Bunkers, Locker (Common) at Caves +3`); **Detailed** adds
    star systems, rough odds per box (`· 21%`, estimates), the quality spread and places with
    their own quality range.
  - Each line (*Loot containers*, *Crafting from a blueprint*, *Contract rewards*, *Mining
    places*, *Ore quality*) can be switched off; a line with nothing to say is left out, and an
    item with no source gets no block.
  - *Add shops from my UEX files* (off by default): shop stock is in no game file, so shop names
    come from UEX files in the UEX folder (`uex-item-prices.json` and `uex-terminals.json`; the
    tab shows the folder and each file's age). They are downloaded when you turn on Settings ->
    Network -> *Download UEX market data* and press **Fetch now** (there or on this tab), or you
    put them there yourself. Names only, never prices: prices go stale between applies.
  - Presets: Recommended has it on in Normal, Minimal in Short, Everything in Detailed with the
    shops on.
  - The first apply per game build extracts a few datasets from `Data.p4k` (seconds; the
    Explorer Sources card and the MCP server share that cache), later applies reuse them.

- **Fix missing item names** (General, on in every preset): some items show a code such as
  `@item_Name_nvs_pilot_flightsuit_01_01_02` instead of a name, because CIG's text file lacks
  that key. This adds the key with a name: the name its variants share when they all agree,
  else the base item's name with the variant (`P4-AR Rifle (LR01)`, `... Magazine`), else a
  readable one made from the code (`NVS Pilot Flightsuit 01-01-02`). No existing text
  changes. Whatever the switch, our own text never shows a raw `@key` or a placeholder.

Colours marked "look not verified" are our guess of how the game draws that style: switch on
the colour test line, check in game and tell us (Support). **Save** keeps the settings, **Save &
apply** also writes the game file. **Copy settings from...** copies another channel's
settings.

## Patches (text fixes)

The Patches screen lists the built-in fixes for known text bugs (on unless you switch them off;
*Fix known CIG text bugs* in Enhancements switches them all) and your own patches. Each patch
checks the original text first, so a fix CIG already made is skipped as *not needed* and a
changed text is skipped as *target changed* instead of overwritten. **Add patch** creates your
own edit; patches can be exported to a file and imported again (patch packs).

## Strings

Every text key of the build, its original and the result, with what each key belongs to
(**owner**, e.g. "Sample Hull 0 · description"). Search covers keys, owners and text; the
**Groups** tree on the left narrows to ships, components, missions and more, by type or by
manufacturer. Filter by source (*Unmodified*, *Enhanced*, *Patched*, *Edited*). Open a key
(Enter) to edit its result for this channel; edits are checked against the game's rules as you
type and are written on the next apply.

## Blueprints

- **Finder:** every blueprint of the build and who owns it, per character and channel (PTU
  progress is not LIVE progress). Search (`/`), filters (category, type, manufacturer, size,
  source, character), sort and group; groups and cards can be collapsed.
- **Where ownership comes from:**
  - **Game logs.** The game writes `Received Blueprint: ...` to `Game.log`. The app reads new
    lines when you open Blueprints; on Linux the post-exit hook does it after every session;
    the CLI with `starorg-codex blueprints scan`. On Windows, a scheduled task can scan and
    re-apply for you ([windows](windows.md#after-playing-blueprints-and-re-apply)).
  - **Smart Citizen import.** **Import from Smart Citizen** reads its blueprint tracker export
    (`owned-blueprints.json`) into the character you choose.
  - **StarOrg Tools** (optional, off by default, needs sign-in): pull your owned list into one
    character, push the characters you tick ([privacy](privacy.md#starorg-tools)).
- **Character for [Owned] tags:** *Newest session* (the character of the newest session in this
  channel's logs), *One character*, or *Any of my characters*. The CLI equivalent is
  `starorg-codex blueprints use`, or `apply --character` for one apply.
- **Fix owned blueprints:** after a character reset or wipe, **Re-scan game logs** reads every log
  again from the start and **Re-import from Smart Citizen** imports the last export again. Both
  show what changes before writing. Re-apply afterwards so the markers update.
- **Markers in game** sums up which markers are on and links to Enhancements -> Blueprints.

## Ships

Import your fleet (RSI+ "Export fleet", FleetYards, FleetView, hangar.link, or a plain list,
one ship per line), match the rows the app could not place, then put your ships in the order
you want them in game lists and give them names. *Write ship names into the game* adds this to
the next apply. A new import adds new ships to the end of My ships but never takes any away;
to start fresh, tick **Remove all ships first** in the import dialog: My ships then keeps only
the ships of that import (with their names, groups and places), and the check lists what goes
before anything is removed. CLI: `starorg-codex fleet import|list|match|clear` (`--prune` for
the same).

## Backups and restore

Before the first write into a channel, and whenever the game file was replaced by someone else,
the file there is backed up. **Backups** lists them per channel: **Restore** puts one back (a
backup made for another game build asks first), and **Remove changes** (Home) or
`starorg-codex remove --all` puts the game back to stock. Uninstalling the app does not do
that for you: run Remove first ([uninstall](uninstall.md)).

## Settings worth knowing

- **Appearance:** dark (default) or light theme, accent colour and palette.
- **Network:** every switch is off by default: *Check for updates* ([updates](updates.md)),
  *Connect StarOrg Tools*, *Download UEX market data*. With them off the app makes no network
  request.
- **Download UEX market data:** **Fetch now** downloads where items are sold (UEX Corp,
  player-reported) from `api.uexcorp.space` into the UEX folder (`<cache>/mcp/uex`), for the
  Buy line of Where to get, the Explorer Sources card and the MCP shop tools. Both files are
  public, so a token is optional; to use yours, sign in at uexcorp.space -> My Apps, paste the
  token and press Save. It is kept in the system keyring (or a private file when there is none),
  never shown again and sent only to `api.uexcorp.space`. **Remove** deletes it. Apply again
  after a fetch.
- **Launch hooks** (Linux): install, update or remove the LUG Helper hook blocks.
- **Advanced -> Developer mode:** adds the Dev group (below). **Memory limit** caps each
  Developer mode and MCP worker (default 5 GB or half the RAM, minimum 1.5 GB; a worker stops
  at 90% of it). **Asset cache:** where decoded sounds, exported models and previews go (2 GB,
  Clear, or a folder you choose).
- **Local MCP server** (experimental, off): read-only game data tools for Claude Code or Claude
  Desktop ([mcp](mcp.md)).
- **Diagnostics** (Activity): a report with user names and home paths replaced, for bug reports.

## Developer mode

For players who want to look things up in the game data. Read only on the game.

- **Explorer:** the item finder (sections such as Ships & vehicles, Ship weapons, Armor & clothing;
  search with prefixes like `type:cooler maker:alpha`, typo-tolerant; quick filters such as
  *Player-usable*, *Has model*, *Has sounds*; grouping and compare), the record's data with
  readable references, its sounds (play or export) and **Export model** (GLB; with the
  **Print shell** preset optionally an STL and, experimental, a 3MF for slicers such as
  PrusaSlicer, Bambu Studio and Cura, in real size in metres and Z-up). **Save as…** asks where
  the GLB goes; the STL and 3MF are written next to it with the same name. The 3MF is
  experimental: the mesh is the game's, not made for printing, so it may need repair in the
  slicer.
  With an STL or 3MF on, **3D print (experimental)** offers print processing of those files
  (the GLB stays as exported). All are off by default and experimental, and the result may
  still need repair in the slicer; the model always sits on the bed (lowest point at z 0,
  centred on x/y). Notes after the export say what each step did.
  - **Size**: longest side in mm or 1:N; a model larger than the bed (remembered) is turned
    90 degrees when that fits, else **shrink to the bed** or a note.
  - **Leave out tiny parts**: loose parts whose bounding-box diagonal is under the size.
  - **Solid** (0.1, 0.2 or 0.4 mm voxels; needs a size): rebuilds the model as one closed,
    watertight solid. Then optionally **thicken** thin parts, **hollow** (wall and drain hole),
    **split to fit the bed** (one STL per part, side by side, with 2 mm alignment pin holes)
    and **peg holes** (across the split cuts, plus a stand hole underneath).
  - **Multicolor**: hull, glass and engine as separate coloured parts of one 3MF object (from
    the node names, so a canopy frame may come out as glass); pick a filament per part.
  - **Simplify** to about N triangles; a solid is millions of triangles without it.
  - **No weapons**: guns, gimbals, missiles, mining lasers and tractor beams left out (changes
    the GLB too).
  CLI: the same flags on `dev export-model --preset print`, and `dev print-glb` for a GLB you
  already have.
  A **Full** export can wear any paint the ship has (**Paint**, listed with their in-game names;
  default: the one fitted) and can include **Layer materials (experimental)**, the textures of
  each material's layers (bigger files; some surfaces may look wrong).
  The record's **Data** tab has two views, and the one you pick is remembered. **Readable**
  (the default) shows the fields in sections with plain names ("Max health" for `maxHealth`):
  numbers with their unit, Yes/No, references as links to the record they name, `@keys` as
  their text (click to open it in Strings). DataCore's wrappers (`_RecordValue_`, `_Type_`)
  are left out; a section's type is the grey text beside its title. Empty values (none, blank,
  no text) are hidden until **Show empty**, right-click copies a field's raw value, and
  **Show IDs** puts the raw key and value beside each field. **Raw** is the JSON tree as
  stored, for exact keys and values.
  Ships, weapons and missiles also get a **Combat** card at the top of the Data tab (in both
  views), once the build's game data has been read: a ship's total weapon DPS and its stock
  guns (size, count, mount or turret, DPS each and in total) and missiles (rack, damage), with
  a total row; a weapon's DPS, alpha, fire rate, range, projectile speed, ammo, power, heat
  and EM/IR, plus its fire modes; a missile's damage, lock range and lock time. Names link to
  their record. Click the title to fold the card away; it stays folded until you open it again.
  Items, commodities and ores also get a **Sources** card, closed until you open it. For an
  item it first lists the shops that sell it, cheapest first, with the location, the price and
  the day the price was last confirmed (flagged when older than 30 days). Shop stock is in no
  game file, so these are UEX (player-reported) prices: turn on Settings -> Network ->
  *Download UEX market data* and press **Fetch now**, or put `uex-item-prices.json` and
  `uex-terminals.json` in the folder the card names (`<cache>/mcp/uex`). Without them the card
  says so. It then lists
  the loot containers that can hold the item, one row per area: the place, the container, what
  it is for (Weapons, Armour...), its size and rarity, and rough odds per box. It also lists
  the loot tables that roll the item but that no container uses (enemies, bosses). For an
  ore it lists where the ore is mined:
  - the bodies and belts whose spawn tables roll a rock that holds it, with the mining kind
    (ship, vehicle or hand), the tier and a spawn weight;
  - the spots in caves, asteroid bases and events that place such a rock;
  - the quality it comes in: the range it rolls in and where most rolls land, places with
    their own curve (Pyro System), and the values the game actually shows. A roll snaps to
    one of a few shown values (Aphorite rolled anywhere in 201–399 shows as 348), each with
    an estimated share. The share is an estimate: the game files give the curve's numbers but
    not how the game samples it.

  Some ores, such as Sadaryx, are in no body's spawn table. They come only from those spots:
  for Sadaryx, the Rock Cracker in Nyx. The first time you open the card for a build, the app
  reads the loot and mining data from Data.p4k, which takes a minute or two and about 1.3 GB
  of memory. That data is cached and shared with the MCP server. The odds assume an even pick
  inside a loot pool, which the game data does not state.
- **Datasets:** the game data as tables, with group trees; also any folder of JSON files.
- **Diff:** compare two builds, two dataset folders, or an old LIVE build against a new one.
  To compare LIVE builds: turn on Settings -> Network -> **Download published LIVE builds for
  Diff**, open **Datasets**, and press **Download** next to two builds under **Published LIVE
  builds** (about 20-25 MB each, from github.com/voidput/sc-gamedata-dumps, checked by
  checksum). Then on **Diff** pick the older one as **A** and the newer one as **B** (they are
  listed as "LIVE <version>") and press **Compare**. Downloaded builds stay usable with the
  switch off; the trash button deletes one. Applying never uses them: it always reads
  `Data.p4k` on this computer.
- **Files:** browse `Data.p4k`, preview textures and text, extract files.

Exports are for your own use; the game's assets belong to Cloud Imperium. CLI: `starorg-codex dev`.

## Command line quick reference

The `starorg-codex` CLI shares settings, backups and caches with the app. `--help` on any
command lists its options; `--json` gives one JSON object for scripts.

| Command | Does |
|---|---|
| `channels` | List installs and channels with branch, game version and change number |
| `setup --channel LIVE --preset recommended --yes` | Set up a channel with a starting profile; `--yes` is the consent to write (without it, setup only says what it would do, exit 6) |
| `apply --channel LIVE` / `apply --all --if-stale` | Build and apply; `--dry-run` writes nothing |
| `status` | Applied, Stale, Changed by another tool, or Not applied, per channel |
| `remove --all` | Back to stock |
| `backups --channel LIVE` / `restore --channel LIVE --backup <id>` | List and restore backups |
| `patches --channel LIVE` | Patches and their outcome |
| `blueprints scan\|characters\|list\|use\|import\|reimport\|rescan` | Owned blueprints per character |
| `fleet import\|list\|match\|clear` | Fleet for ship order and names |
| `hook install\|uninstall\|status` | LUG Helper launch hooks (Linux) |
| `sync` | StarOrg Tools environment and what to sync (the CLI itself never connects) |
| `doctor` | Installs, paths, casing, other tools' traces; changes nothing |
| `dev` | Records, type exports, game files, models and sounds |
| `mcp` | Experimental local MCP server on stdin/stdout ([mcp](mcp.md)) |
| `build`, `gamedata`, `version` | Write the output or the game data to a file; version details |

## Troubleshooting

| Symptom | Where to look |
|---|---|
| No install found, Wine prefix, `PATH`, AppImage, blank window, hooks | [linux: troubleshooting](linux.md#troubleshooting) |
| SmartScreen, elevated, `icacls`, file locks, WebView2 | [windows: troubleshooting](windows.md#troubleshooting) |
| In-game text unchanged | The game reads the file only at start: restart it. `USER.cfg` must have `g_language = english` |
| No `[Owned]` markers, or a character twice | Blueprints -> Character for [Owned] tags and Fix owned blueprints; then apply |
| Update check says it can't see releases | [updates](updates.md) |
| Developer mode or MCP refuses a dataset (memory) | Settings -> Advanced -> Memory limit ([mcp: memory limit](mcp.md#memory-limit)) |

Still stuck: the [Discord server](https://discord.gg/GCkYhbFYNq) or an
[issue](https://github.com/voidput/starorg-codex/issues), with the diagnostics report attached.

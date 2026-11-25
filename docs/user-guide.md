---
title: User guide
tags: [docs]
updated: 2026-11-25
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
    places*, *Ore quality*, and the shops and sell prices below) can be switched off; a line with nothing to say is left out, and an
    item with no source gets no block.
  - *Add shops from my UEX files* (off by default): shop stock is in no game file, so shop names
    come from UEX files in the UEX folder (`uex-item-prices.json` and `uex-terminals.json`; the
    tab shows the folder and each file's age). They are downloaded when you turn on Settings ->
    Network -> *Download UEX market data* and press **Fetch now** (there or on this tab), or you
    put them there yourself. Names only, never prices.
  - *Add best sell prices for commodities* (off by default, on in Everything): a **Sell** line on
    commodity and resource descriptions with the top 3 commodity kiosks that pay most, from
    `uex-commodity-prices.json` (downloaded by the same **Fetch now**):
    `Sell: ~9.6k aUEC/SCU at HUR-L2, ~9.2k at Rod's Fuel, ~9k at TDD Area 18 +4 (UEX, 21 Nov)`.
    Kiosk prices are player-reported and move every day, so the price is rounded to two digits
    and the line carries the day the prices were fetched; fetch and apply again for fresh ones.
    Kiosks that pay nothing, and kiosks UEX marks as not on LIVE, are left out. **Short** says
    only the best (`Get: Sell ~9.6k aUEC/SCU at HUR-L2 (UEX, 21 Nov)`), **Detailed** adds each
    kiosk's place and system.
  - Presets: Recommended has it on in Normal, Minimal in Short, Everything in Detailed with the
    shops and sell prices on.
  - The first apply per game build extracts a few datasets from `Data.p4k` (seconds; the
    Explorer Sources card and the MCP server share that cache), later applies reuse them.

- **Fix missing item names** (General, on in every preset): some items show a code such as
  `@item_Name_nvs_pilot_flightsuit_01_01_02` instead of a name, because CIG's text file lacks
  that key. This adds the key with a name: the name its variants share when they all agree,
  else the base item's name with the variant (`P4-AR Rifle (LR01)`, `... Magazine`), else a
  readable one made from the code (`NVS Pilot Flightsuit 01-01-02`). Items whose name in the
  file is a placeholder (`PLACEHOLDER - dmc_frontier_gloves_01_02_11`) get a name the same
  way; a real name from the game is never replaced. Where a public source confirms the real
  name, the built-in patch *Real names for items the game leaves unnamed* uses it instead,
  and its variants follow it (`Verdict LMG Magazine`). It is on the Patches screen and
  retires by itself once the game names the item. Mission rewards and blueprint lists show
  the same names. Whatever the switch, our own text never shows a raw `@key` or a placeholder.
  The switch is labelled **Fix missing item names and descriptions**: it also fills a
  description the file lacks (`@item_Desc_alb_jumpsuit_01_01_04`, mostly new colour variants
  of clothes and armour) with a sibling variant's real description from the game, without its
  last colour line (`The CFP edition is a deep fjord blue.`), so it never names the wrong
  colour. An item with no such sibling, or whose siblings' text names their own colour, keeps
  the code: the switch never invents a description. Stats and WHERE TO GET go on the filled
  text like on any other description.

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

The list shows at once. Working out each patch's state needs the build's full text table, so on
the first visit after a start or a change the badges read *Checking…* for a moment and then
fill in.

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
  source, character, missing for, missions, reputation), sort and group; groups and cards can be
  collapsed. It shows as soon as the build's blueprints are read and does not wait for the
  character list or StarOrg Tools; it reloads after an import, a pull or a re-scan.
- **Owned by one, missing for another:** **Owned** with a character picked lists what that
  character owns; add yourself under **Missing for** and only what you lack stays. The other
  character can be an alt, or a guild list pulled into a character through StarOrg Tools.
  Missing for counts any source (a blueprint you have from an import is not missing), and with
  several characters picked a blueprint must be missing for each of them. Like every filter it
  is remembered per channel and goes with **Clear filters**.
- **Earn it:** a blueprint a mission can award shows **Earn it: N missions** under its name;
  open it for each mission's in-game title (or its contract record when it has none), the
  reputation it pays (the faction's in-game name when the game data has one; every faction when
  the title pays several), the star system when the game data names one, the type (Chain, Once
  only), the standing it requires, and the odds in the words of the in-game MISSION DETAILS
  line: *Guaranteed*, *Some variants only* (not every contract with that title awards it) or a
  chance such as *30%*. That line is about the pool: the badge says what it means for this
  blueprint. **Guaranteed** is a sure pool that holds only this blueprint; **Guaranteed pool, 1
  of N** is a sure pool it is one draw of (its share of the draws follows when the pool has
  weights); **Chance** is everything else. With Developer mode on, **Find in Explorer** searches
  the Explorer for the contract's record. The **Missions** filter keeps guaranteed, from a
  guaranteed pool, by chance, not from missions or not known; **Reputation** keeps one faction's.
  In All channels every build's missions are listed, each with its channel when not every build
  has it, and the Channel filter narrows them too. Contracts come only from a local read of the
  game data: until a build has been read that way its blueprints say their missions are *not
  known* rather than none (after this update every build is read once more, for the star
  systems). The list is worked out once per build with the finder's catalogue.
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

## Shops

**Shops** (Game group, or Ctrl+K) shows what each shop sells. Shop stock is in no game file, so
the list is UEX market data (player-reported): turn on *Download UEX market data* in Settings
-> Network and press **Fetch now** (the screen offers both while the files are missing). The
line at the top says how old the data is ("UEX data from 21 Nov").

- **Left:** every shop by star system, then place, with how many items it lists and what it
  mostly sells. Search by shop or place name (`grim hex`, `area 18`, `ruin`), or pick a system.
- **Right:** the picked shop's items by category: FPS weapons, FPS attachments, armour &
  clothing, ship weapons, missiles & torpedoes, ship components (shields, coolers, power
  plants, quantum drives, ...), mining & salvage, consumables, other. The chips keep one
  category; *Filter items* narrows by name. **Buy** is what you pay, **Sell** what the shop pays
  you, **Updated** the day UEX last confirmed the row (⚠ after 30 days).
- An item the game data knows carries the game's name (UEX's name below it when it differs);
  with Developer mode on, the name opens its record in the Explorer. Personal items link once
  the channel's game data has been read (any apply or dry run). Ship guns, missiles and
  components come from a second dataset: while it is missing the screen says so and offers
  **Read game data** (the same job as in Enhancements; nothing is written to the game, the
  first read of a build takes a minute or two, under the memory limit). Rows no record matches
  still get their category from UEX.

- **Items:** the *Shops | Items* switch above the search turns the left column into an item
  search. Type part of a name (game or UEX name; small typos are fine, `hornet`, `fs9 lmg`) and
  pick one of the matches (at most 40; type more to narrow): the right side lists every shop
  that sells it, cheapest first, with the place, the price and the day UEX confirmed it.
  *Sell* lists the shops that pay for it instead, best price first. The system filter applies
  here too. Click a shop to open it in the shop list with *Filter items* set to that item. The
  screen keeps the mode and the search until the app closes. Ctrl+K offers **Where to buy
  "…"** for whatever you type, which opens this search.

To find which shops sell one item you can also open it in the Explorer (Sources card, Buy rows)
or ask the MCP server (`where_to_buy`); `shop_inventory` lists one shop's items there too.

## Commodities

**Commodities** and **Mining** (Game group, or Ctrl+K) are one screen with two tabs
(**Commodities | Mining** at the top of the list); each sidebar entry opens its tab, the search
box is shared, and both read the same cached game data, once per build.

### Commodities tab

**Commodities** lists every tradable resource of the channel's build
(ore, refined metal, gas, processed goods, ...), from its game data. Until that data is read
the screen offers **Read game data** (the same job as in Shops; nothing is written to the
game). Ship-system placeholders (heat, power, life support) are left out.

- **Left:** the resources by group (`Metal`, `Metal > Unrefined Ores`, `Mineral > Raw
  Minerals`, ...), searchable by name, record or group, with a chip per top group. *volatile*
  marks volatile cargo, *raw* a form that refines into something. Arrow keys move the pick.
- **Right:** the picked resource: what it refines into and what refines into it (both
  clickable), its quality (range, place overrides and the values the game shows, with
  estimated shares), volatility and what impacts and explosions do to volatile cargo, the
  cargo items that hold it (with SCU), where it is mined (bodies and belts, and spots in caves,
  asteroid bases and events) and the commodity kiosks that buy (**Sell**) and sell (**Buy**)
  it, per SCU. A refined form points at its raw form for where it is mined.
- Kiosk prices are UEX data (player-reported), read from the same UEX files as Shops: with
  *Download UEX market data* on, **Fetch now** gets them; the top line says how old they are.
  Anything the data does not cover shows a quiet "Not known" line.
- **Trade calculator** (under the kiosk prices, when UEX has any for the resource): type an
  amount in SCU, or pick one of your ships (the fleet the Ships tab lists) to fill in its stock
  cargo grid. A ship whose cargo the game data does not give shows without a number and leaves
  the amount alone. Pick the buy and the sell kiosk (the cheapest and the best paying of the
  listed ones by default) to see the cost, revenue, profit, profit per SCU and margin (profit as
  a share of the cost). Prices are UEX (player-reported), as old as the line above says. The
  amount is not capped: the UEX files have no stock to buy or demand to sell. Your fleet loads
  only when you first use the calculator; the last amount and ship are remembered on this
  computer.

The list carries names and groups only; a resource's detail loads when you pick it. Both come
from the build's cached data, built once per build, so switching screens does not rebuild.
A mineable resource's **Where it is mined** has a **Mining details** link to the Mining tab.

### Mining tab

**Mining** lists every mineable resource of the build: ores, gems and raw minerals, the
hand-mined and ROC-mined ones, and the harvestables with a quality curve. Chips narrow it by how
it is mined (**Hand**, **ROC**, **Ship**); **Best quality** sorts by the best place's chance of
a high roll. Pick one for:

- **Raw to refined:** what it refines into (click it for its Commodities row; the header links
  the resource's own row too). The refining yield is not in the game files (the server works it
  out), so it says "not known".
- **Quality:** the base curve, the values the game shows with estimated shares, and each place
  override (`Pyro System`). An override applies to a place only when it names that place's body
  or its system ("Pyro System"); one naming the body wins over one naming the system. An
  override naming a station or a place no spawn row is says so and is applied nowhere. A place
  whose system the data does not give (Lagrange points, asteroid clusters) shows "curve not
  known" where a system override exists, not "same as base".
- **Rock physics** (folded, open it to see): resistance, instability and the optimal charge window (where it sits, how far
  it moves, how thin it is) from the game's mineable element records. Rock mass is not known:
  the game rolls it when a rock spawns.
- **Where found:** every body or belt that spawns it, **Most common** first or **Best quality**
  first. Each row shows its mining kind, its quality range and where the curve comes from
  (*same as base*, the override, or not known), the estimated share of rolls at 700 or more
  (*est.*: a normal curve cut to its range), the best shown rolls with their shares, and the
  difficulty. A row whose
  ore has a quality scale below 1 in that rock (the records give ×0.49 or ×0.79 for many) is
  marked `*`: how the game applies the scale is not in the data, so the share is not changed,
  may be lower, and the row sorts after full-scale rows in **Best quality**. Names come from the
  body, else the provider key (`Yela Belt`, `Lagrange A`). Open a row for its quality first (range, share at 700 or
  more, best rolls with their shares, the scale note), then one rock line (tier, kind, how much
  of it is this ore, what else it can hold, scan signature) and the difficulty with its inputs.
- **Hand-placed spots:** caves, asteroid bases and events whose spawn points place the rock;
  they roll the base curve (the data does not say which system a spot is in).

**Estimates.** The quality shares and the difficulty are worked out, not read, and say so:

- **Difficulty** (easy, medium, hard) scores resistance (below 0.2, below 0.6, else), instability
  (below 100, below 500, else) and window thinness (below 0.6, below 1.5, else) 0, 1 or 2 points
  each: 0-1 of 6 easy, 2-3 medium, 4-6 hard. A missing input is left out and named.
- **No ship or laser pick.** The kind column says whether a rock is hand, ROC or ship mined
  (from the game data). Which laser head suits a ship rock depends on more than beam power
  (resistance and instability modifiers, window size, module slots), and the game files read
  here hold only power and range, so no head or ship is recommended.

The rock physics come from a dataset **Read game data** adds; a build read before it shows "not
known yet" and the top line offers **Read game data** again.

## Loot

**Loot** (Game group, or Ctrl+K) shows what drops where, from the channel's game data (loot
tables and where they spawn). It reads only what is already read: while the loot data of a build
is missing the screen says so and offers **Read game data** (the same job as in Enhancements and
Shops; nothing is written to the game). Nothing loads before you open the screen.

Both lists are collapsible trees, closed until you open a branch (the app remembers which
branches you opened, per list). Each branch shows how many rows it holds and the best odds
below it. Typing in the search filters the tree and opens every branch with a match; the
category filter keeps one kind of place or category. Up/Down move, Right opens a branch (or
enters it), Left closes it (or goes to its parent), Enter opens a branch or picks a row, and
typing a few letters jumps to the next row starting with them.

- **By area:** kind of place (Caves, ASD facilities, Distribution centres, Outposts, Stations,
  ...) -> a middle level from the area's id where several areas share one (a cave's planet or
  `Occupied`, a distribution-centre wing such as `Cargo wing`, `Colonial` outposts, the `Rund`
  station) -> the areas. A place with a single area is a row of its own (`Battaglia`). Names
  never show the game's preset words (`lootgeneration`, `v3slotpreset`); run-together and
  misspelt words are fixed (`Highsecurtiy` -> `High security`, `Foodanddrink` -> `Food and
  drink`), and an area with nothing more to say is `General`. Two areas that would read the
  same are told apart by their preset family: `General (v3)`, `General (multi)`, `General
  (classic)` are different slot presets of the same place. Pick an area to see every item that
  can drop there, grouped by where in it the item is (`Lying loose`, `Weapons rack`, `Large
  mining crate`, `Medical crate`, ...), each row with its rarity and `always in it` for a rack.
- **By item:** category (FPS weapons, FPS attachments, Armour & clothing, Consumables, Ship
  components, ...) -> what kind of thing (Rifles, Pistols, Magazines, Helmets, Jackets & coats,
  Power plants, ...) -> the items, searchable by name or class. Pick one to see every area it
  drops in, grouped by kind of place (Caves, Outposts, Stations, ...). With Developer mode on,
  **Open in Explorer** opens its record. Items that share a name (seven `Standard Container`s)
  show their class name beside it, in the tree and in the drops.
- **Group by** beside the filter picks how the drops are grouped: for an area **Where in it**
  (the default) or **None** (one list, likeliest first); for an item **Place** (the default, the
  top level of the area tree), **Where in it** or **None**. The app remembers the choice per
  view. Each group is a header with its size and best odds, over all its drops; groups are
  ordered by best odds, then by name, and the rows inside are likeliest first. Only the first
  group (or the first few small ones) is open at first; opening another loads its rows. A group
  shows at most its 500 likeliest drops (`N more in ...; filter to narrow`), an ungrouped list
  the 2,000 likeliest.
- A name in the drops list jumps to the other view with that row picked, its branches opened,
  scrolled into view and focused (an item's other areas, an area's other items).
- **Filter** above the drops narrows them by item or area name, tree path, container, rarity or
  id, across every group and every drop, so the drops past a cap are reachable; the group
  headers then count the matches, and the groups with matches open.
- **Per box** is a rough estimate: the chance one spawned crate, rack or loose spot of that kind
  holds the item, assuming an even pick inside each loot pool (the game data does not say how
  the pick is weighted). The WHERE TO GET line, the Explorer Sources card and the MCP
  `where_found` tool use the same estimate. Loot tables no area places (corpses, events) are not
  listed; test presets are left out.
- Item names are the game's; an item whose game name is missing or a placeholder shows readable
  words from its class name.

The index is built once per game build and kept in memory while the app runs, so switching views
or screens does not rebuild it. The lists carry only each row's summary and tree path; the
drops load when you pick a row.

## Live telemetry

**Experimental, off by default.** Star Citizen 5.0 can serve its own live data (your health,
oxygen, ship, fuel, shields, flight mode) to programs on the same computer, over a local
service on `127.0.0.1`. Live telemetry reads it while you play. It is offered only for a
channel whose build is 5.0 or newer: PTU today, LIVE by itself once LIVE reaches 5.0. Settings
lists every channel and why one is not offered (an older version, or no build manifest).

Nothing leaves the computer: the app talks to `127.0.0.1` only, refuses any other address,
records nothing unless you turn on **Record sessions**, and sends nothing anywhere
([privacy](privacy.md)). The screen and its Settings section say it in one line: "Live data
comes from your own game on this PC, through the game's local telemetry API new in 5.0. Nothing
leaves this PC." The Events, People and Sessions panels add the same for Game.log: "Read from
the game's own log file on this PC. Nothing leaves this PC."

**Each part is its own choice, off by default**, and turning one on never turns on another:

| Choice (Settings -> Experimental -> Live telemetry) | What it adds |
|---|---|
| **Off** / **Metrics** / **Metrics + actions** | the screen itself; actions also press game inputs ([below](#live-telemetry)) |
| **Record sessions** | a file per stream, for replay |
| **Read Game.log events** | the Events panel and the shard you are on (memory only) |
| **People** (under it) | the People panel (handles in memory only) |
| **Shard history** (under it) | the Sessions panel: a saved history of the shards you join |
| **Notifications**: Off / In app / In app + sound | toasts, tones and spoken callouts |

The last five sit under **More from Live telemetry** and **Notifications** once Metrics is on.

**Turning it on** (Settings -> Experimental -> Live telemetry):

1. Choose **Metrics** (or **Metrics + actions**) under **Live telemetry** for a 5.0 channel. The app adds one line to that channel's
   `USER.cfg`, `server.game.telemetry.enabled = 1`, and keeps every other line as it is. If
   the file already has another value there (`= 0`), it asks before replacing it; your line
   comes back when you choose **Off**. Choosing Off takes the line out again (unless you changed
   it since), the same way the language line works. Only one channel carries the lines: picking
   another **Channel** moves them there and takes them out of the one before.
2. **Port**: the port the game should use (default 50051, 1024 to 65535).
3. **Launcher setup**: the game also reads two environment variables from the RSI Launcher,
   `GRPC_CLIENT_SERVER_ENABLED=1` and `GRPC_CLIENT_SERVER_PORT=<port>`. The app never changes
   your environment or registry; Settings shows the steps for your system (Windows: `setx`,
   then quit the launcher from the tray and start it again; Linux: the LUG Helper or Wine
   script, Lutris, or Steam launch options) with **Copy steps**. Never set
   `GRPC_CLIENT_SERVER_EXTERNAL`: it opens the game's service, which has no password, to your
   whole network.

Then **Live telemetry** appears in the sidebar under Game (marked Exp). Opening it starts
reading; leaving it stops. It shows the connection (Connecting, Connected, Game not running:
it retries by itself every few seconds), then once you have spawned, top to bottom:

- key readouts (speed, vertical speed, G, master mode, gear),
- **Pilot**: health, oxygen, heart rate, body temperature, gravity,
- **Ship**: hull, hydrogen and quantum fuel, boost,
- **Shields**: the level and each face (labelled F1, F2... while the order is not confirmed
  for every ship),
- **Systems** (engines, shield generators, lights, gear, landed, quantum spool, boost) and
  **Annunciators** (low health, oxygen, hull or fuel, shields off, high G),
- **Alerts** and **Actions** (both below),
- **History**: small line graphs of speed, G, hydrogen and quantum fuel, hull, shields,
  health and oxygen over the last 1, 5 or 10 minutes (one sample a second, kept in memory
  while the app runs; a gap in the data breaks the line),
- **Events**, **People** and **Sessions** from Game.log (below); with Read Game.log events
  off, one card says so, with a **Settings** button,
- **Recordings** (below) and **Raw frame**: everything the game sent, as JSON, once a second
  while it is open, with Pause and Copy JSON.

Every panel below the live readouts folds to its title (click it); the app remembers which
are folded. A panel whose switch is off says how to turn it on.

The screen updates at most 10 times a second; the graphs once a second. The first time a build is read, the app reads
the layout of the game's service from that channel's `Bin64/StarCitizen.exe` (a few seconds)
and keeps it in the cache folder (Settings -> Storage, *Live telemetry schemas*); nothing of
the game is shipped with the app.

**Game actions**: the Live telemetry choice is **Off**, **Metrics** (read only) or **Metrics + actions**. Metrics + actions adds a second line,
`server.game.telemetry.trigger.actions.enabled = 1`, and lets the Actions panel press inputs
in the game for you: only the ones the game itself lists as triggerable (lights, landing
gear, shield power, ping...), with **Hold** for the holdable ones. The first press of each app
session asks first; self destruct, eject, emergency exit and power off ask every time. The app
also refuses a second press of the same action within a second. With **Metrics** only, the list is
shown but every button is disabled, and the second line is taken out of every channel's
`USER.cfg`.

**Record sessions and replay**: a switch of its own under Settings -> Experimental -> Live
telemetry, off by default (choosing Metrics never turns it on). While it is on, each stream
is saved from its first frame, 4 frames a second, to a file in the app's data folder
(`telemetry/recordings/`, *Live telemetry recordings* in Settings -> Storage): at most 20
files and 256 MB in all, 64 MB each, the oldest go first. The **Recordings** panel lists
them; **Replay** plays one into the same screen instead of the game (a *Replay* badge, Play
and Pause, a position slider, speed 0.25x to 16x, **Back to live**). Gaps longer than 5
seconds in a recording are shortened while it plays. The file being written and the one
playing cannot be deleted. The files use sc-telemetry's `--record` format, so either tool can
replay the other's.

**Notifications (alerts)**: a separate choice under Live telemetry, **Off** by default:
**Off**, **In app** (a toast and the screen's **Alerts** panel, silent) or **In app + sound**
(also a short tone the app makes itself). Choosing Metrics never turns them on. With sound,
**Spoken callouts** is one more switch, off by default: it speaks a short callout with your
computer's own voices only, and says so when it finds none. Alerts are checked only while the
Live telemetry screen is open and reading. The rules (after sc-telemetry): boost low and
empty, hydrogen low and critical (only with engines on), quantum fuel low, shields low, a
shield face down (a face you tuned away does not count), shield emitters lost, hull at 50 % and
25 %, missile, hit and proximity warnings, oxygen and health low, radiation, unbreathable air
on foot, gear down at speed, and high G with G-safe off. Each fires once when its condition
starts, never on what already holds when the stream starts or a ship appears, then waits out
its cooldown, and arms again only past its re-arm point (boost low fires at 25 % and again only
after going back above 35 %). Under **Rules** you can switch each one off, change a threshold
rule's threshold (the re-arm point moves with it) and any cooldown, test one with **Test**, or
go back with **Default**. The changes are saved in `config.toml`.

**Game.log events** (a switch of its own under Live telemetry, off by default): **Read
Game.log events** adds an **Events** panel fed by the channel's own `Game.log`, which tells
things the live service does not. It works even when the service is not reachable (no
environment variables set, no schema), as long as the screen is open:

- where you are: star system, the start of your last quantum route (planet, moon, station),
  armistice, monitored space and jurisdiction notices;
- the shard and region, the build, the login queue, the backend link and disconnects;
- quantum travel: the destination you picked, the fuel estimate, your arrival;
- ships streamed in near you (class names only: parked and AI ships count too, and there is
  no owner or distance);
- vehicles disabled or destroyed nearby, leaving the pilot seat;
- corpse item lists near you, spawns and respawns. A corpse list followed within a minute by
  your own respawn counts as your death (the log names no owner, so a corpse of someone you
  killed shows as a corpse only).

These are the game's unofficial debug lines: a patch can change or drop them. The app reads
only what the game added since the last read, twice a second, and keeps the newest 200 events.

**Sessions** (with **Shard history**, a switch of its own under Read Game.log events, off by
default): a history of the shards you joined, one row per
stint from the join to when you left: the channel, the shard (`USE1B · 020`) and region,
when you joined, how long you stayed, the star systems you went to, and why it ended: **Left**
(back at the menu, or joined another shard), **Shard change** (the game moved you), **Quit**
(you disconnected), **Dropped** (kicked for being idle, timed out, the server went: marked
with a warning sign) or **Log ended** (the game closed while on the shard). *Same shard as
before* marks a rejoin of the shard your previous session on that channel was on. The stint
you are on now is shown on top (after a disconnect, with how it ended, until the next line of
the log). Filter by channel and region; **Clear** forgets the whole history for good: a session
that started before the Clear never comes back, even when the same logs are read again. A row is saved when the stint ends; when the screen opens, the app also reads the
newest 40 older logs in the channel's `logbackups` folder (once each, a few MB at a time, at
most 128 MB of each; one that cannot be read right then is tried again the next time) so the
history starts with your past sessions. With Shard history off nothing is saved and no older
log is opened. It is one small file in the app's data folder
(`telemetry/sessions/history.json`, *Live telemetry session history* in Settings -> Storage,
which can clear it too): 2000 rows and 2 MB at most, the oldest go first. It holds no server
address, port, session id or player name.

**People** (under it, off by default) adds a **People** panel: the player handles Game.log
names, by how they relate to you: **Crew** (ship voice channels), **Party** (party notices),
**Party?** (quantum calibration notices, usually a party member) and **Nearby** (players named
in ship kills around you). You are shown once as *You*. **Clear** forgets everyone listed. The
handles stay in memory for the current Game.log only; nothing is looked up online
([privacy](privacy.md#live-telemetry-experimental-off-by-default)).

## Backups and restore

Before the first write into a channel, and whenever the game file was replaced by someone else,
the file there is backed up. **Backups** lists them per channel: **Restore** puts one back (a
backup made for another game build asks first), and **Remove changes** (Home) or
`starorg-codex remove --all` puts the game back to stock. Uninstalling the app does not do
that for you: run Remove first ([uninstall](uninstall.md)).

Kept per channel: the 5 newest backups, the newest one made under each of the last 3 game
builds, and the file from before StarOrg Codex; older ones are deleted after an apply. Each is
stored gzip-compressed (`global.ini.gz`, about a fifth of the 11 MB file); backups made by
older versions (plain `global.ini`) restore the same way.

## Storage

Settings -> Storage shows how much the app keeps and where:

| Area | Where (Linux; Windows: `%LOCALAPPDATA%\StarOrg Tools\Codex\cache\`) | Bound |
|---|---|---|
| Game data cache | `~/.cache/starorg-codex/<install>/<CHANNEL>/` | one entry per channel, gzip (LIVE about 3.8 MB) |
| WHERE TO GET datasets | `~/.cache/starorg-codex/mcp/<CHANNEL>/` | the last 2 builds per channel |
| UEX market data | `~/.cache/starorg-codex/mcp/uex/` | 3 files, replaced on download |
| Asset exports (Developer mode) | `~/.cache/starorg-codex/assets/` or your folder | 1 GB, least recently used first |
| Temporary extraction folders | `~/.cache/starorg-codex/work/` | removed after each read; leftovers older than an hour at the next start |
| Live telemetry schemas (experimental) | `~/.cache/starorg-codex/telemetry/` | the last 3 game builds, a few hundred KB each |
| Live telemetry recordings (experimental, Record sessions) | `~/.local/share/starorg-codex/telemetry/recordings/` (Windows: `%APPDATA%\StarOrg Tools\Codex\data\telemetry\recordings\`) | 20 files, 256 MB in all; not in the cache folder or its cap |
| Live telemetry session history (experimental, Shard history) | `~/.local/share/starorg-codex/telemetry/sessions/` (Windows: `%APPDATA%\StarOrg Tools\Codex\data\telemetry\sessions\`) | one file, 2000 rows and 2 MB; not in the cache folder or its cap; **Clear session history** |
| Backups | `~/.local/share/starorg-codex/channels/<install>/<CHANNEL>/backups/` | see [Backups](#backups-and-restore) |

**The cache folder's cap** (1 GB by default; `[storage] cache_max_mb` in `config.toml`):
after the app or the CLI writes to it (game data read, a dataset extracted, an asset export)
and at start, the least recently used goes first until it fits: asset exports, then folders
of older builds, then another channel's WHERE TO GET datasets. A channel's current game data,
anything used in the last 5 minutes, UEX files and backups are never removed by the cap, so it
can stay over when everything left is in use. Whatever goes is rebuilt when it is needed
again (a re-read of `Data.p4k`).

The sizes (here and in Settings -> Advanced -> Asset cache) are measured once and kept while
the app runs. They are measured again after the app changes those folders (a read, apply,
import, restore, download or asset export; an asset preview; Clear, Browse, Reset or Move of
the asset cache; a backup made or deleted) and when you press **Measure again**. What another
program writes there (the MCP server, a launch hook, the CLI) shows within 5 minutes, or at
once with **Measure again**.

## CPU and time

The game usually runs while StarOrg Codex works, so nothing it does runs at full priority:

| Work | Priority (Linux nice; Windows workers) | Threads at most | Time |
|---|---|---|---|
| You started it: Apply, Read game data, Developer mode, `apply` or `build` in a terminal | below normal (nice 10; `BELOW_NORMAL`) | all cores but two, at most 8 | until done |
| Nobody waits for it: the launch hooks, their post-exit background job, WHERE TO GET datasets an MCP client asks for (app or `starorg-codex mcp`) | lowest (nice 19, which also puts disk reads at the lowest level; `IDLE`) | a quarter of the cores, at most 4 | hooks: their `--timeout`; the background job stops when the game starts ([linux: launch hooks](linux.md#launch-hooks)) |

The extraction workers (`extract-worker`, `mcp-extract-worker`, the Developer mode workers) get
the same priority and thread cap as the job that started them; the app's own window does not
slow down. `STARORG_THREADS=<n>` sets the thread cap for both rows (at least 1), and
`STARORG_PRIORITY=background` makes a CLI run from a script or timer (for example
`apply --all --if-stale`) a background one. On Windows the app and the CLI keep their own
priority class (only their workers are lowered); the thread cap applies everywhere.

## Settings worth knowing

- **Appearance:** dark (default) or light theme, accent colour and palette.
- **Network:** every switch is off by default: *Check for updates* ([updates](updates.md)),
  *Connect StarOrg Tools*, *Download UEX market data*. With them off the app makes no network
  request.
- **Download UEX market data:** **Fetch now** downloads where items are sold and what
  commodity kiosks pay (UEX Corp, player-reported) from `api.uexcorp.space` into the UEX folder
  (`<cache>/mcp/uex`: `uex-item-prices.json`, `uex-commodity-prices.json`, `uex-terminals.json`),
  for the Buy and Sell lines of Where to get, the Shops and Commodities screens, the Explorer Sources card and
  the MCP shop tools. All three are public, so a token is optional; to use yours, sign in at
  uexcorp.space -> My Apps, paste the token and press Save. It is kept in the system keyring
  (or a private file when there is none),
  never shown again and sent only to `api.uexcorp.space`. **Remove** deletes it. Apply again
  after a fetch.
- **Launch hooks** (Linux): install, update or remove the LUG Helper hook blocks.
- **Advanced -> Developer mode:** adds the Dev group (below). **Memory limit** caps each
  Developer mode and MCP worker (default 5 GB or half the RAM, minimum 1.5 GB; a worker stops
  at 90% of it). **Asset cache:** where decoded sounds, exported models and previews go (1 GB,
  Clear, or a folder you choose).
- **Storage:** what the app keeps on this PC, per area, and the cache folder's cap
  ([below](#storage)); **Clear asset cache** (refused while an export runs).
- **Local MCP server** (experimental, off): read-only game data tools for Claude Code or Claude
  Desktop ([mcp](mcp.md)).
- **Live telemetry** (experimental, off): your vitals, ship and shields while you play, from the
  game's own local service, for 5.0 builds ([below](#live-telemetry)); Record sessions,
  Read Game.log events, People, Shard history and Notifications are more choices there, each
  off.
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
  says so. For a commodity or resource (or a cargo box holding one, or a rock whose ore UEX
  lists) a **Sell** section follows: the 3 commodity kiosks that pay most, with the location,
  the price per SCU, the day it was last confirmed and the day the prices were fetched, from
  `uex-commodity-prices.json` in the same folder. It then lists
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
- **Diff:** compare two builds, two dataset folders, or an old LIVE build against a new one;
  or compare a channel's current build with the previous one the app read (kept in the cache).
  That previous build is listed as "<CHANNEL> <version> (cached)" right after the channel's
  build, newest first, and Diff starts with it as **A** and the current build as **B**; only
  the datasets both have are compared, so after a game update use WHERE TO GET once on the
  new build first. It holds the WHERE TO GET datasets that were extracted for it (the last 2 builds per channel are
  kept, see [Storage](#storage)); it is read only and never extracted into, and a build whose
  cache was cut short shows on Datasets with the reason. To compare LIVE builds: turn on Settings -> Network -> **Download published LIVE builds for
  Diff**, open **Datasets**, and press **Download** next to two builds under **Published LIVE
  builds** (about 20-25 MB each, from github.com/voidput/sc-gamedata-dumps, checked by
  checksum). Then on **Diff** pick the older one as **A** and the newer one as **B** (they are
  listed as "LIVE <version>") and press **Compare**. Downloaded builds stay usable with the
  switch off; the trash button deletes one. The list from GitHub is kept for 15 minutes, so
  opening Datasets again or switching channel asks GitHub nothing; the refresh button next to
  **Published LIVE builds** asks again now. Applying never uses them: it always reads
  `Data.p4k` on this computer.
- **Files:** browse `Data.p4k`, preview textures and text, extract files. Long folders and
  searches scroll smoothly (only the rows in view are drawn); so does the Diff records list.

Explorer and Files read `Data.p4k` in a separate process that uses about 0.5-1 GB of memory
while open. Opening either screen does not start it: press **Open game data**. After that,
coming back to the screen (for the same channel, until the app closes) opens it again by itself,
and so does a hand-off such as Ctrl+K **Find record…** or **Open owning record**. Closing the
asset browser by hand (the power button on Files) makes Files ask again next time.

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

---
title: Local MCP server (experimental)
tags: [docs, mcp, experimental]
updated: 2026-11-25
---

# Local MCP server (experimental)

StarOrg Codex can answer questions about your game data for an AI assistant on the same
computer (Claude Code, Claude Desktop, or any other [Model Context Protocol](https://modelcontextprotocol.io)
client): "where do I get the FS-9?", "does Pilot_A own the Arrowhead blueprint?", "what changed
in ships between LIVE and PTU?". It is **read-only** and **local**: it reads the game install and
the app's own folders, writes only its cache, and makes no network request.

Two ways to run it, same tools:

| | Command line | App |
|---|---|---|
| Start | `starorg-codex mcp [--channel PTU]` | Settings -> Experimental -> **Local MCP server** |
| Transport | stdio (the client starts the process) | streamable HTTP on `127.0.0.1`, a free port |
| Auth | none needed (only the client that started it can talk to it) | bearer token, new at every start |
| Stops | when the client closes stdin | when switched off, and when the app exits |
| Default | - | **off**; the switch is not saved, so each app start begins off |

## Connect a client

**Claude Code, command line server:**

```sh
claude mcp add starorg-codex -- starorg-codex mcp
```

**Claude Code, the app's server** (Settings shows this line with the real URL and token; *Copy
for Claude Code*):

```sh
claude mcp add --transport http starorg-codex http://127.0.0.1:<port>/mcp --header "Authorization: Bearer <token>"
```

**Claude Desktop** starts stdio servers from `claude_desktop_config.json`. Either the command line
server:

```json
{ "mcpServers": { "starorg-codex": { "command": "starorg-codex", "args": ["mcp"] } } }
```

or the app's server through the `mcp-remote` bridge (*Copy for Claude Desktop*; Desktop's `npx`
downloads the bridge from npm, which is the client's network use, not ours):

```json
{ "mcpServers": { "starorg-codex": {
    "command": "npx",
    "args": ["-y", "mcp-remote", "http://127.0.0.1:<port>/mcp", "--header", "Authorization:${STARORG_MCP_AUTH}"],
    "env": { "STARORG_MCP_AUTH": "Bearer <token>" } } } }
```

The token and port change every time the app's server starts: copy the config again after a
restart. The command line server has neither.

## Tools

Every tool takes an optional `channel` (`LIVE`, `PTU`, ...; default the app's default channel,
else LIVE). Lists are paginated: `limit` (default 10, at most 50) and the `nextCursor` of the
previous page as `cursor`. Answers are compact JSON.

| Tool | Asks | Reads |
|---|---|---|
| `search` | items, ship weapons and components, and ships by name, class, maker, type (fuzzy, typos allowed) | `game-search-index` (from item-tags + ships), ship-components |
| `item` | one item or ship: identity, tags, loot eligibility, stats | item-tags; the stats dataset of its type (fps-weapons, weapons, armor, ship-components, consumables) or ships |
| `where_to_buy` | shops for an item (cheapest first), or what a place sells | item-tags + UEX files, if there |
| `shop_inventory` | everything one shop sells (`terminal`: a UEX terminal name or id), by category, with buy and sell prices, the day UEX last confirmed each row and the class name when linked; `category` keeps one category. A name several shops fit (a place) answers their list with ids | UEX files; item-tags (+ ship-components) to name and link rows |
| `where_found` | loot spawns: best areas with place, container, contents, size and rarity, odds per box, fixed racks, other tables | item-tags, loot, item-sources |
| `where_mined` | where an ore is mined: bodies and belts that roll a rock with it (most common first), and loot-area spots (caves, asteroid bases, the Rock Cracker) that place one, and the quality it comes in (range, place overrides, the values the game shows with estimated shares) | mining-spawns, item-sources, commodities |
| `how_to_obtain` | every source, easiest first: buy, contract/Wikelo reward, craft, loot; `owned_by` marks blueprints a character owns | item-tags, loot, item-sources, blueprints, wikelo-trades, UEX; the owned-blueprint store |
| `mission` | missions and contracts by name, giver, type, with the blueprints and items contracts award | missions, blueprints, item-sources |
| `compare` | 2 to 4 items or ships side by side, every numeric stat they share | as `item` |
| `string` | a localization key, or a text search: stock text, and the text StarOrg Codex applied to that channel | strings-english; the channel's `global.ini` |
| `diff` | a dataset between two channels (`to`), or against an older cached build (`from_build`) | that dataset, twice |
| `status` | channels, cached builds and datasets, what extracting each cost, server memory | nothing |

`owned_by` is a character name, `active` (the channel's character setting; default the newest
session's character, or the most recent one that owns something when that one owns nothing) or
`any`. The answer names the characters, never their ids; `any` lists
every character name in the owned-blueprint store.

### Names, typos and ship weapons

An item argument is a class name or a name. An exact name, or a name holding every word asked,
is a match. Anything else (a typo, or an item that does not exist) is answered about the
**closest** name, and the answer then carries an `inexact` object: `asked`, `used` (key and
name), up to four `others`, and a note. A client should tell the user it guessed rather than
answer as if it had found the item; with nothing close, the tool returns an error pointing at
`search`.

`game-item-tags` lists only the ship weapons and components that carry loot tags (7 of 241
ship guns on LIVE 4.10, no M5A or Deadbolt). The tools also look names up in
`game-ship-components` (named, non-template records item-tags lacks), so `search`, `item`,
`compare`, `where_to_buy` and `how_to_obtain` answer for ship guns, shields, coolers and the
other components; their UEX prices match by the game's item id. These rows have no tags, so
`where_found` and loot eligibility say they are not loot. sc-gamedata's own `find`/`obtain` do
not do this lookup, so for these items the MCP answers go beyond the parity check below.

The answers come from the ported sc-gamedata query layer (`starorg_gamedata::query`), which
gives the same answers as sc-gamedata's own `find`/`obtain`/`where`/`shop` on the same data
(`scripts/compare-query.sh`: 514 of 514 identical on LIVE 4.10.200.58746). Loot odds are
estimates: an even pick inside a loot pool is assumed; the game data does not say so. The
app's **Loot** screen inverts the same per-box odds for every item at once (every area with what
drops there; [user guide](user-guide.md#loot)), from the same cached datasets
(`starorg_mcp::loot`).

### Shop prices

Shop stock is server-side and in no game file. Prices come only from UEX (player-reported)
files in the cache folder, `<cache>/mcp/uex/uex-item-prices.json` and `uex-terminals.json`
(the `status` tool prints the folder). The app downloads them when Settings -> Network ->
*Download UEX market data* is on and you press **Fetch now**; or put the files
`sc-gamedata fetch uex-item-prices uex-terminals` writes there yourself (same shape). The MCP
server itself never downloads anything; it reads a file again once it changed, and answers
built from the files (shop rows, the app's WHERE TO GET hints) are rebuilt then too. Without them,
the shop tools say so and the other sources still answer.

`shop_inventory` sorts each row into one of nine categories (`fps-weapons`, `fps-attachments`,
`armour`, `ship-weapons`, `missiles`, `ship-components`, `mining`, `consumables`, `other`): by
the game's item type when the row is linked to a record, else by the UEX category id every
price row carries (`starorg_mcp::shops::categorise`). A row links by the game item id UEX
stores (`item_uuid`), else by a name only one item has. `terminal` takes an id, a terminal name
(`Dumper's GrimHEX`, or UEX's full name), or words of a shop or its place; when several shops
fit, the answer lists them (`shops`, with `id`, up to 25) and a note to call again with one id.
The app's **Shops** screen shows the same rows ([user guide](user-guide.md#shops)).

The same **Fetch now** also writes `uex-commodity-prices.json` (UEX `commodities_prices_all`:
what each commodity kiosk asks and pays per SCU, `{ _source, _fetched, prices, summary }` like
the item prices; sc-gamedata has no fetch for it yet). The app's Sources card and the WHERE TO
GET sell line read it through this server's store (matched to the game's resources by name);
no MCP tool answers from it yet.

## Data, cache and memory

A tool loads only the datasets it needs, on first use. The first question about a build
extracts each needed dataset from `Data.p4k` (read only) in a **separate short-lived worker
process** (`starorg-codex mcp-extract-worker` / `starorg-codex-app mcp-extract-worker`, one
dataset per process), which writes `<cache>/mcp/<CHANNEL>/<build>-<fingerprint>/<dataset>.json`
and records its cost in `measurements.json` beside it. Later questions read the JSON. The
fingerprint covers the archive's size and time and the cache format, so a game patch gets a
folder of its own.

**Stamps.** Beside each `<dataset>.json` is `<dataset>.stamp`: the key it was written under,
where it came from (`local`: extracted from this computer's `Data.p4k`;
`dump:<tag>@r<n>-<commit12>`, or `dump:<tag>@<commit12>` for a dump without a checked
revision: a prebuilt dataset of the same build, below, with `dumpRef`, the tag's ref it was read
under; `derived`: the search index) and the size and SHA-256 of the file. A replaced file is
swapped in as a pair with its stamp: the stamp first names the new file as `next`, then the
file is renamed into place, then the stamp names it alone, so a crash at any step leaves a stamp
that vouches for the file on disk (old or new). A dataset is served only while its stamp is current: its key is the one
this app expects now, and the file is the one stamped (a worker reports the SHA-256 of what it
wrote, and only that file is stamped, so two app versions sharing a folder never vouch for each
other's files). Anything else (no stamp, as in a folder from before stamps; another key; a
replaced file) is extracted again, and an older build's folder (Diff) serves only datasets
stamped for this app. The key of a dataset is a hash of the code that writes it and nothing
else (`crates/starorg-mcp/build.rs`): its domain module, the modules that module uses (followed
through `crate::` and `super::` paths, nested `use` groups included), the shared extraction
core (`domain`, `pipeline`, `source`, `output` and what they use, starorg-data's `extract` and
StarBreaker adapter, the starorg-core modules that path names, the manifests, the StarBreaker
pin, the worker body `starorg-mcp/src/extract.rs`) and the cache format. Test modules, the
query layer (`query/`), the golden compare, fixtures and overlays do not count. A prebuilt
dataset's key adds the rule it was taken under (its `DUMP_USE` row, `RULES_VERSION` for every
row and `ROW_RULES` for single rows); the
search index's adds the SHA-256 of the item-tags and ships files it was built from. So an app
update re-extracts only the datasets whose code changed: a change to `domains/ships.rs`
re-extracts `game-ships` (and so the search index), a change to `query/obtain.rs` re-extracts
nothing (it only rebuilds the obtain hints, which key on `HINTS_CODE` and the folder's
stamps), and a change to `records.rs` or the StarBreaker pin re-extracts everything. The
generated `dataset_codes.rs` (in the build's `OUT_DIR`) lists each key with the modules it
covers.

**Tidying.** After a dataset is read for the current build (once per run, under the
extraction lock), in the channel's cache:

- other folders of the same build go when they hold no stamp (the layout before stamps) or
  were not used for 7 days (another install of the same build keeps its folder while used);
- of the other builds, the most recently used folder stays (the previous build, for Diff) and
  the rest go;
- in the current folder, datasets stamped by other code go once a day old (another app version
  may still use the folder), unstamped datasets and leftover temp files after an hour.

A folder being read (Diff) is skipped until the next run. Only `<CHANNEL>/<build>-<fingerprint>`
folders are touched: never the UEX folder, another channel, user data or anything outside the
dataset cache. Activity names what was removed. The app cache folder's cap (1 GB by default) can
still remove an older build's folder first ([storage](user-guide.md#storage)).

A loose-file dataset is not reused across game builds even when its input files are
unchanged: `game-npc-loadouts` reads DataCore records as well as the loadout files, and the
string table carries the build in its `_build` stamp, so reusing either would not be clean.
The string table is cheap anyway (it reads `global.ini` only).

### Prebuilt datasets

In the app, while Settings -> Game data -> "Download prebuilt game data" is on (the default,
for people updating too; stored as `[data] prebuilt_dumps` in `config.toml`), a dataset the
cache lacks for build `B` of channel `C` is first looked for in
[voidput/sc-gamedata-dumps](https://github.com/voidput/sc-gamedata-dumps), which publishes one
tag and release per game build. This only saves the extraction: local extraction stays the
source of truth and the fallback for every dataset. The CLI and the MCP server never download
(the CLI links no HTTP client); they serve what the app put in the shared cache while the
setting is on, and extract it again locally while it is off (so does the app).

1. Only the release tagged exactly `B`, a build tag (`^\d+(\.\d+){2,3}$`). None: nothing is
   downloaded (never another build). A revision tag `B-r<n>` is never taken for a build (nor
   listed for Diff).
2. The tag's commit and its tree. When the tree lists `build.json`, it must match its blob id
   and read, or nothing of the tag is used: `build` must be `B`, `channel` (else the
   release's) must be `C` (same channel only), and a dump made by a dirty extractor checkout
   (`extractor.dirty`) is not used. Its `files` (`name` as `data/game-x.json` or
   `game-x.json`, `sha256`, `bytes`, `schema`) check each file; without them the tree's git
   blob id and size do. Unknown keys are ignored.
3. The revision. `B` moves when the build is published again; `build.json`'s `revision` (1 on
   the first publish, one more per republish) says which one it holds, and each publish also
   pushes the immutable tag `B-r<n>`. The revision counts only when that tag exists and points
   at `B`'s commit: `r<n>-<commit12>` (4.10.201.48085 is `r3-9a387f3b599e` since 2026-11-25).
   Without a revision, or with a revision tag missing or elsewhere, the short commit id names
   what was read.
4. Only the datasets in the table below, at most 32 MB each. The file must match its checksum,
   be a JSON object stamped `_build: B`, not be listed in `KNOWN_BAD` for that tag, not be
   older than the row's cut-off (`not_before`, against the file's own `_extracted`, else the
   release date: a republish of an old extraction does not pass it), carry a `_schema`
   (`<dataset>@<n>`, in the file or `build.json`) the row accepts, or, without one, be a tag
   the row lists as checked by hand (`ACCEPT_UNSCHEMAED`, one switch: off once sc-gamedata
   stamps every file). Every listed path must be present and not null, it must read with this
   app's typed reader where there is one, and `_redacted` may name only the prose the row lets
   go.
5. Redacted prose (`description`, refining's `rawDescription` and `refinedDescription`) is
   filled from this build's own string table, extracted locally from `Data.p4k`: every record
   with `<field>Key` gets the text it names, by the rule its domain writes that field with,
   in the place the domain writes it (`fill_text` / `fill_at` in `prebuilt.rs`): commodity
   descriptions and refining's `rawDescription` / `refinedDescription` by the commodities rule
   (markup stripped, the table's `\n` as line breaks, placeholders and "X description." stubs
   dropped); a refining process's `description` is the table's text trimmed; a manufacturer's
   the table's text as it is; the rest drop placeholders and unresolved keys. The file is
   marked `_filled` (fields, source, count) and written to a temp file beside the served one;
   then file and stamp (`dump:B@<revision>`, `dumpRef`) are swapped in as a pair. The served
   copy is not touched before the new one is accepted and filled.
6. Anything else (no release, a failed check, an incompatible shape, no network, the setting
   off): extracted from `Data.p4k` as before. A download asks GitHub for the tag once per
   build per app process (one prebuilt source serves every install and channel; a missing
   release is remembered for an hour; a network failure, the first failed download, or
   downloads over 60 s in all for ten minutes). Connections time out after 10 s; a download
   never holds up another dataset's extraction, and no lock is held across a request. Activity
   says once per build why a dump was not used, and each Read game data step what it downloaded
   (with the revision) and what it read locally.
7. A republish. Reading a dataset stamped from `B` never waits on the network: the cached copy
   is served at once. The first such read per build and app process starts a background check:
   one request for the tag's ref (`git/ref/tags/B`, 5 s timeouts). The stamp's ref: current,
   nothing more is asked until the app restarts. Another ref: a background resolve (steps 1-3)
   gives its revision; the same revision (the tag pushed again) only updates the stamp's ref,
   another one (or a stamp from before revisions) drops the dataset (and the values built from
   its folder) from memory, so the next read takes it again from the new revision, checked as
   any download. A new revision refused for the dataset is extracted locally, and should that
   fail too, the earlier copy is still served; a download that fails on the network leaves the
   tag unknown for ten minutes and serves the cached copy (Activity says so). Offline, the
   setting off or the CLI: the cached copy is served as it is. The search index ensures its
   item-tags and ships first, so it is built again from a republished input.

Where to get lines built from prebuilt datasets can be applied (they are the same build's data);
the game text itself is always read from `Data.p4k`.

The table (`DUMP_USE` in `crates/starorg-mcp/src/prebuilt.rs`; bump a schema, a path or a
cut-off there; bump `RULES_VERSION` when the checks change for every row, a row's `ROW_RULES`
entry when they change for that dataset only; refining, manufacturers and starmap are at 2):

| Dataset | `_schema` read | Shape checked by hand | Must carry (and typed reader) | May be redacted | Today |
|---|---|---|---|---|---|
| `game-ammo` | 1 | 4.10.200.58746 | `ammo`, `summary` | | compatible |
| `game-armor` | 1 | 4.10.200.58746 | `armor`, `clothing`, `bySlot`, `summary` | | compatible |
| `game-blueprints` | 1 | 4.10.200.58746 | `blueprints`, `rewardPools`, `resourceUse`, `summary` (blueprints) | | compatible |
| `game-cargo` | 1 | 4.10.200.58746 | `manifests`, `loading`, `summary` | | compatible |
| `game-commodities` | 1 | 4.10.200.58746 | `resources`, `items`, `damage`, `byGroup`, `summary` (quality) | `description` | compatible; redacted dumps filled |
| `game-consumables` | 1 | 4.10.200.58746 | `consumables`, `subtypes`, `summary` | | compatible |
| `game-containers` | 1 | 4.10.200.58746 | `containers`, `summary` | | compatible |
| `game-fps-weapons` | 1 | | `weapons`, `summary` | | not for 4.10.200.58746 (stale records) |
| `game-item-sources` | 1 | 4.10.200.58746 | `areas`, `contractAwards`, `summary` (loot areas, awards) | | compatible |
| `game-item-tags` | 1 | | `items`, `summary.withAmmo`, `summary.tags` (item tags) | | not for 4.10.200.58746: no `ammoItems`, stale records |
| `game-law` | 1 | 4.10.200.58746 | `crimes`, `jurisdictions`, `summary` | | compatible |
| `game-loot` | 1 | 4.10.200.58746 | `tables`, `archetypes`, `secondaryChoices`, `filters`, `specialEvents`, `summary` (loot) | | compatible |
| `game-manufacturers` | 1 | 4.10.200.58746 | `manufacturers`, `byCode`, `summary` | `description` | compatible; redacted dumps filled |
| `game-mining` | 1 | 4.10.200.58746 | `mineableElements`, `miningModules`, `elementSignatures`, `summary` | | compatible |
| `game-mining-spawns` | 1 | 4.10.200.58746 | `locations`, `rocks`, `oreSpawns`, `oreLocations`, `summary` (mining spawns) | | compatible |
| `game-missions` | 1 | 4.10.200.58746 | `missions`, `byGiver`, `summary` | `description` (not long titles: they change `name`) | compatible; redacted dumps filled |
| `game-npc-loadouts` | 1 | | `actors`, `byLootTable`, `summary` (NPC loadouts) | | not published yet |
| `game-quantum` | 1 | 4.10.200.58746 | `rules`, `summary` | | compatible |
| `game-refining` | 1 | 4.10.200.58746 | `processes`, `refinables`, `workloadTiers`, `settings`, `summary` | `description`, `rawDescription`, `refinedDescription` | compatible; redacted dumps filled |
| `game-reputation` | 1 | 4.10.200.58746 | `standings`, `byOrganisation`, `summary` | | compatible |
| `game-salvage` | 1 | 4.10.200.58746 | `structural`, `repairBeam`, `summary` | | compatible |
| `game-ship-components` | 1 | 4.10.200.58746 | `armor`, `missiles`, `mounts`, `quantumDrives`, `shields`, `weapons`, `summary` | | compatible |
| `game-ships` | 1 | | `vehicles`, `byManufacturer`, `byRole`, `summary` | `description` | not for 4.10.200.58746 (stale records) |
| `game-shops` | 1 | 4.10.200.58746 | `selling`, `commodities`, `buying`, `summary` | | compatible |
| `game-starmap` | 1 | | `locations`, `byType`, `summary`; extracted on or after 2026-11-26 | `description` | needs a dump with sc-gamedata's plain-container fix (350e7053b) |
| `game-weapons` | 1 | 4.10.200.58746 | `shipWeapons`, `fpsWeapons`, `missiles`, `summary` | | compatible |
| `game-wikelo-trades` | 1 | 4.10.200.58746 | `trades`, `totalDemand`, `summary` (Wikelo trades) | | compatible |
| `game-strings-english` | never | | | | published with name keys only |
| `game-search-index` | never | | | | built locally from item-tags and ships |

"Compatible" means: the published 4.10.200.58746 file has the shape this app writes (checked
against this app's output for 4.10.201.48085), and this repository's port was byte-identical to
sc-gamedata's output of that build for it (`crates/starorg-gamedata/README.md`). A later tag
without `_schema` is taken only after someone checks it the same way and adds it to the rows'
checked tags. The published LIVE build lags the installed one, so nothing is downloaded until a
matching, accepted tag exists.

LIVE 4.10.201.48085 as republished (commit `18cda1f3`, `build.json` with `files` and `_schema`
on every file, no `revision`; and again as revision 3, commit `9a387f3b`, with
`4.10.201.48085-r3` at that commit) was checked on 2026-11-25: every dataset in the table reads
(the fields it added, such as commodities' `minedAt` / `refinedBy` / `usedByBlueprints` /
`inManifests` / `qualityOverrideBelts`, the ship, component, weapon and ammo links and the
search index additions, are ignored by the checks and the typed readers). Filled from that
build's string table, its refining, manufacturers, ships, missions and starmap files are byte
for byte the unredacted ones sc-gamedata wrote (refining: all 69 description keys, beside its
9 name keys, give text); commodities
differs in one value, a `PH - ` description that sc-gamedata now drops as a placeholder and this
app's domain keeps (so the filled file is what this app extracts). The new `game-tag-index`
dataset is not in the table: nothing in this app reads tag indexes.

`<cache>` is `~/.cache/starorg-codex` on Linux and `%LOCALAPPDATA%\StarOrg Tools\Codex\cache`
on Windows. Nothing is written in the game folder (the worker refuses an output folder inside the
channel folder or the Wine prefix).

Why a worker: a domain holds the records it is handed in memory while it parses, and the
allocator keeps that memory. In its own process it is all returned when the worker exits; the
server holds only the JSON it serves.

Measured on LIVE 4.10.200.58746 (Linux, `VmHWM`):

| What | Peak RSS | Time |
|---|---|---|
| Server while answering all ten tools, LIVE and PTU loaded for a diff | **325 MB** | |
| Server after search, item, where_found, how_to_obtain, where_to_buy (LIVE) | 175 MB | |
| Worker: `game-ships` | 3.9 GB | 2.1 s |
| Worker: `game-ship-components` | 2.2 GB | 1.5 s |
| Worker: `game-item-sources` (pruned, was 9.0 GB) | 1.4 GB | 1.3 s |
| Worker: `game-blueprints` (pruned, was 6.5 GB) | 1.3 GB | 1.7 s |
| Worker: `game-item-tags` (pruned, was 8.7 GB) | 1.2 GB | 1.9 s |
| Worker: `game-armor` (pruned, was 3.0 GB) | 0.9 GB | 1.2 s |
| Worker: `game-npc-loadouts` (pruned; also the archive index, for the loadout files; LIVE 4.10.201, 4.3 MB) | 1.8 GB | 2.8 s |
| Worker: any other dataset (DataCore alone is most of it) | 0.85 to 1.3 GB | 0.6 to 4.7 s |

`search` needs `game-ships` and `game-ship-components` once per build, so the first search of
a build takes a 4 GB and a 2.2 GB worker (one after the other) for a few seconds; a name
item-tags lacks loads `game-ship-components` too. The server keeps every dataset it has loaded
in memory until it exits, so its memory grows with the channels and datasets asked about (a
few hundred MB for LIVE, PTU and HOTFIX). The pruning (`Domain::prune`) keeps the extracted datasets
byte-identical (`scripts/compare-gamedata.sh`).

### Memory limit

Each extraction worker is held to the memory limit (Settings > Advanced > Memory limit in the
app; `starorg-codex mcp --memory-limit <GB>` or `STARORG_MEMORY_LIMIT_GB` on the command line;
default the app's setting, else 5 GB or half the RAM; `0` = none). A worker is stopped at 90% of
the limit, so a dataset is refused before a worker starts, as a tool error, when its measured
peak (table above) is over that point or over the memory free at that moment: "Extracting
game-ships needs about 4.1 GB, more than the 1.5 GB memory limit allows ... Set the limit to at
least 5 GB ...". `search` needs `game-ships`, so it wants a limit of at least 5 GB (the default
on machines with 10 GB of RAM or more). A worker that grows past 90% anyway stops itself with
"Stopped: needed more than X GB ..."; a hard cap (`RLIMIT_DATA` on Linux) sits 10% (at least
512 MB) above as a last resort. The server process itself is not limited. Turning the server on
in the app shows these numbers first. Details: addendum 7.18.

## Security and privacy

- Read-only: no tool writes game files, settings or the owned-blueprint store. The only writes
  are the cache folder above.
- No network: the server opens no outgoing connection. The app's HTTP server binds `127.0.0.1`
  only (never `0.0.0.0`), needs `Authorization: Bearer <token>` on every request (a random
  256-bit token per start, compared in constant time), refuses a browser `Origin` that is not
  `localhost`/`127.0.0.1` (DNS rebinding), caps request bodies at 1 MiB, and answers one
  endpoint, `/mcp`.
- Your owned blueprints, character names and fleet are personal data. They are answered only to
  the local client you connected; StarOrg Codex sends them nowhere. What that client (an AI
  assistant) then does with an answer is up to the client and its provider: an assistant that
  runs in the cloud sends tool answers to its provider as part of the conversation. Leave the
  server off, or do not ask about owned blueprints, if you do not want that.
- The token is shown in Settings so you can copy it; it is not written to the log or to
  diagnostics. Requests in Activity show the tool and the result code, not the arguments.

## Protocol

A minimal implementation of the server side, no SDK (the official Rust SDK needs an async
runtime and, for HTTP, a web framework; this server answers one local client in order):
JSON-RPC 2.0, MCP revisions 2025-06-18 (preferred), 2025-03-26 and 2024-11-05; `initialize`,
`ping`, `tools/list`, `tools/call`. Streamable HTTP: `POST /mcp` answers with one
`application/json` body (`202` for notifications), `GET` is `405` (no server-initiated stream),
`DELETE` ends the session; `Mcp-Session-Id` is issued at `initialize`; `MCP-Protocol-Version`
is checked when sent. Code: `crates/starorg-mcp`; the CLI links it without the `http` feature.

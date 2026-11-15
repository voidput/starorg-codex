---
title: Local MCP server (experimental)
tags: [docs, mcp, experimental]
updated: 2026-11-15
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
| Start | `starorg-strings mcp [--channel PTU]` | Settings -> Experimental -> **Local MCP server** |
| Transport | stdio (the client starts the process) | streamable HTTP on `127.0.0.1`, a free port |
| Auth | none needed (only the client that started it can talk to it) | bearer token, new at every start |
| Stops | when the client closes stdin | when switched off, and when the app exits |
| Default | - | **off**; the switch is not saved, so each app start begins off |

## Connect a client

**Claude Code, command line server:**

```sh
claude mcp add starorg-strings -- starorg-strings mcp
```

**Claude Code, the app's server** (Settings shows this line with the real URL and token; *Copy
for Claude Code*):

```sh
claude mcp add --transport http starorg-strings http://127.0.0.1:<port>/mcp --header "Authorization: Bearer <token>"
```

**Claude Desktop** starts stdio servers from `claude_desktop_config.json`. Either the command line
server:

```json
{ "mcpServers": { "starorg-strings": { "command": "starorg-strings", "args": ["mcp"] } } }
```

or the app's server through the `mcp-remote` bridge (*Copy for Claude Desktop*; Desktop's `npx`
downloads the bridge from npm, which is the client's network use, not ours):

```json
{ "mcpServers": { "starorg-strings": {
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
| `where_to_buy` | shops for an item (cheapest first), or what a place sells | item-tags + UEX files, if you supplied them |
| `where_found` | loot spawns: best areas, container, odds per box, fixed racks, other tables | item-tags, loot, item-sources |
| `how_to_obtain` | every source, easiest first: buy, contract/Wikelo reward, craft, loot; `owned_by` marks blueprints a character owns | item-tags, loot, item-sources, blueprints, wikelo-trades, UEX; the owned-blueprint store |
| `mission` | missions and contracts by name, giver, type, with the blueprints and items contracts award | missions, blueprints, item-sources |
| `compare` | 2 to 4 items or ships side by side, every numeric stat they share | as `item` |
| `string` | a localization key, or a text search: stock text, and the text StarOrg Codex applied to that channel | strings-english; the channel's `global.ini` |
| `diff` | a dataset between two channels (`to`), or against an older cached build (`from_build`) | that dataset, twice |
| `status` | channels, cached builds and datasets, what extracting each cost, server memory | nothing |

`owned_by` is a character name, `active` (the channel's character setting; default the newest
session's character) or `any`. The answer names the characters, never their ids; `any` lists
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
estimates: an even pick inside a loot pool is assumed; the game data does not say so.

### Shop prices

Shop stock is server-side and in no game file. Prices come only from UEX (player-reported)
files you put in the cache folder yourself, `<cache>/mcp/uex/uex-item-prices.json` and
`uex-terminals.json` (the files `sc-gamedata fetch uex-item-prices uex-terminals` writes; the
`status` tool prints the folder). StarOrg Codex never downloads them. Without them, the shop
tools say so and the other sources still answer.

## Data, cache and memory

A tool loads only the datasets it needs, on first use. The first question about a build
extracts each needed dataset from `Data.p4k` (read only) in a **separate short-lived worker
process** (`starorg-strings mcp-extract-worker` / `starorg-strings-app mcp-extract-worker`, one
dataset per process), which writes `<cache>/mcp/<CHANNEL>/<build>-<fingerprint>/<dataset>.json`
and records its cost in `measurements.json` beside it. Later questions read the JSON. The
fingerprint covers the archive's size and time and the app version, so a game patch or an app
update never serves old answers; three builds per channel are kept.

`<cache>` is `~/.cache/starorg-strings` on Linux and `%LOCALAPPDATA%\StarOrg Tools\Strings\cache`
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
| Worker: any other dataset (DataCore alone is most of it) | 0.85 to 1.3 GB | 0.6 to 4.7 s |

`search` needs `game-ships` and `game-ship-components` once per build, so the first search of
a build takes a 4 GB and a 2.2 GB worker (one after the other) for a few seconds; a name
item-tags lacks loads `game-ship-components` too. The server keeps every dataset it has loaded
in memory until it exits, so its memory grows with the channels and datasets asked about (a
few hundred MB for LIVE, PTU and HOTFIX). The pruning (`Domain::prune`) keeps the extracted datasets
byte-identical (`scripts/compare-gamedata.sh`).

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

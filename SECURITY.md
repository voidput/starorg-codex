# Security policy

## Reporting a vulnerability

Report privately through GitHub: **Security -> Report a vulnerability** on
<https://github.com/voidput/starorg-codex/security/advisories/new>. Please do not open a public issue for
security problems.

Include the affected version, steps to reproduce, and the impact. You will get an acknowledgement as soon as
the maintainer sees it; fixes ship in the next release.

## Supported versions

Only the latest release receives security fixes.

## Scope

StarOrg Codex reads the game files of your own install (`Data.p4k`, `Game.log`) and writes only
local files: `Data/Localization/english/global.ini` in each channel it is set up for, one
`g_language` line in that channel's `USER.cfg` (each backed up first), and its own settings,
backups and cache folders. In scope, among others:

- **File handling:** path traversal or writes outside those places; unsafe handling of
  `Data.p4k`, DataCore, textures, models or other game data, including the extraction workers
  the app and CLI start as child processes; imported patch packs, blueprint imports (Smart
  Citizen) and fleet files.
- **Launch hooks:** the blocks `hook install` writes into LUG Helper's `sc-prelaunch.sh` /
  `sc-postexit.sh` (quoting, injection, unsafe edits of those scripts).
- **Local MCP server** (experimental, off by default): the app's HTTP server binds `127.0.0.1`
  only, needs a per-start bearer token and refuses foreign browser `Origin`s
  ([docs/mcp.md](docs/mcp.md)); a way around any of that is in scope.
- **StarOrg Tools sign-in:** the OAuth flow with PKCE, the token kept in the system keyring (or
  the fallback file), and what is sent ([docs/privacy.md](docs/privacy.md)).
- **Updates:** the release check and in-app install (minisign signature check, release URL
  allowlist; [docs/updates.md](docs/updates.md)).
- **Supply chain:** the build and release pipeline, signing and attestations. The CI hardening
  is described in [docs/ci.md](docs/ci.md).

Out of scope: the game itself, Star Citizen's launcher, and services StarOrg Codex does not run
(GitHub, StarOrg Tools servers; report those to their owners).

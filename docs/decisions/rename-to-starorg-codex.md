---
title: Rename starorg-strings to starorg-codex everywhere (0.3.0)
tags: [docs, decisions]
updated: 2026-11-15
---

# Rename starorg-strings to starorg-codex everywhere (0.3.0)

Status: accepted (2026-11-15)

## Context

0.2.0 renamed the product to StarOrg Codex but kept `starorg-strings` inside: the CLI, the app
binary `starorg-strings-app`, the release archives, the Tauri identifier
`tools.starorg.strings`, the app folders (`~/.config/starorg-strings`,
`%APPDATA%\StarOrg Tools\Strings`), the hook markers and the repo URL. Two installs exist (the
owner's Linux machine and one Windows tester), so a clean rename costs little now and more
with every release.

## Decision

Rename everything in 0.3.0, with a one-release bridge:

- **CLI** `starorg-codex` (crate `starorg-cli`, bin renamed). The tarball ships a
  `starorg-strings` link and the zip a copy; `scripts/install-linux.sh` makes the link. Run by
  the old name it works the same and prints one deprecation line on stderr. The packages
  (.deb/.rpm/AppImage/installers) do not contain the CLI, so they carry no alias.
- **App binary** `starorg-codex-app`, not `starorg-codex`: the .deb/.rpm put the app in
  `/usr/bin`, the CLI is usually in `~/.local/bin`, and two different programs with one name on
  `PATH` would make `starorg-codex` mean whichever comes first. The app crate is renamed to
  `starorg-codex-app` too (lib `starorg_codex_app`), so the crate, binary, window id
  (`StartupWMClass`, Wayland app_id), `.desktop` alias, icon name and Windows exe all agree;
  the library crates (`starorg-core`, ...) keep their names.
- **Identifier** `tools.starorg.codex`. On Windows it only names the webview folder and the
  taskbar AppUserModelID: the NSIS uninstall key and the MSI upgrade code come from the
  product name (unchanged since 0.2.0). The upgrade code is now pinned in `tauri.conf.json`
  so a later product-name change cannot split installs. The MSIX `Application Id` becomes
  `StarOrgCodex`; nothing was published to the Store yet, and the package identity still comes
  from the Partner Center variables.
- **App folders** move once, on the first start of the app or the CLI
  (`starorg_core::legacy`): rename; on another file system copy, verify every file (type,
  size, SHA-256), rename the copy into place and keep the old folder; any other failure moves
  nothing and keeps using the old folder (`appdirs::LegacyPair::current`). An OS lock beside
  the folders serialises concurrent first starts. When the new folder already exists the old
  one is never touched, only reported. The webview storage folder moves with them (UI
  layout and filters).
- **Hook markers** `# >>> starorg-codex hook >>>`. Blocks with the old markers count as the
  block: `Outdated`, replaced in place by `hook install`, removed by `hook uninstall`.
- **URLs** `github.com/voidput/starorg-codex` (GitHub redirects the old one). A saved
  update-check repo equal to the old name reads as the default.
- Kept: `CHANGELOG.md` history, the Strings *screen* names (`starorg.strings.*` storage keys
  belong to that screen), and the StarOrg Tools server's own wording in its mock.
- Kept: the StarOrg Tools server's wire values. The blueprint `source` stays
  `starorg_strings` (`starorg_net::starorg::SOURCE`; the server's `OwnedSource` literal accepts
  only that, anything else is HTTP 422) and the token audience stays `strings` (`AUDIENCE`).
  They change only together with the server.

## Consequences

- Remove the alias, the legacy markers and `starorg_core::legacy` in the release after 0.3
  (tracked in the backlog).
- One deprecation line per hook run until the user re-installs the hooks.
- Old docs links and `curl .../starorg-strings/main/scripts/install-linux.sh` keep working
  through GitHub's redirect after the repo is renamed.

Spec: [StarOrg Codex v1](../plans/specs/starorg-codex-v1.md), section 4.2 (app folders) and
section 8 (CLI).

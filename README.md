# StarOrg Codex

Clearer Star Citizen item, ship and mission text, built from the files of your own install and
removable in one click. Desktop app (Windows, Linux) plus the `starorg-codex` command-line tool.

StarOrg Codex builds a new text file (`global.ini`) for the game: component names get short tags
such as size, class and grade, ship and weapon descriptions get a stats block, and mission text
gets the facts you would otherwise look up. You choose what is added and preview every change
before it is written.

## Features

- Component tags: size, class and grade in item names
- Stats blocks for ships, ship weapons, personal weapons and missiles
- Mission info in contract descriptions
- Preview every changed line before applying; automatic backup and one-click restore
- LIVE, PTU, EPTU, HOTFIX and other channels
- Never writes while the game or a launcher update is running
- No account, no telemetry; offline by default

## Install

Downloads are on the [releases page](https://github.com/voidput/starorg-codex/releases).

- Windows: [docs/windows.md](docs/windows.md) (installer, Microsoft Store, CLI zip)
- Linux: [docs/linux.md](docs/linux.md) (AppImage, deb, rpm, CLI tarball)

Every release asset is listed in `SHA256SUMS` and carries a build provenance attestation.

Coming from 0.2? The CLI, app binary, release files and app folders were renamed from
`starorg-strings` to `starorg-codex` in 0.3.0; the folders move on the first start and the
old CLI name keeps working for one release. See
[Upgrading from 0.2](docs/linux.md#upgrading-from-02-the-rename-to-starorg-codex) (Windows:
[docs/windows.md](docs/windows.md#upgrading-from-02-the-rename)).

## Privacy

With the default settings the app makes no network request. What each optional network feature
sends: [docs/privacy.md](docs/privacy.md).

## Support

- Questions, bugs and ideas: the [Discord server](https://discord.gg/GCkYhbFYNq), or open an
  [issue](https://github.com/voidput/starorg-codex/issues).
- If the app saves you time: [Buy me a coffee](https://buymeacoffee.com/zedwil).

Both links are also in the app (footer and Footer -> About); they open your browser.

## Development

Rust workspace (`crates/`) plus the Tauri 2 + Svelte 5 app (`app/`). See
[CONTRIBUTING.md](CONTRIBUTING.md) for the checks and commit rules, [app/README.md](app/README.md)
for the app, and [docs/](docs/README.md) for specs, decisions and the release process. Security
issues: [SECURITY.md](SECURITY.md).

## License

MIT, see [LICENSE](LICENSE). Third-party software included in the builds, with its licenses:
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) (regenerate with `scripts/gen-notices.sh`). The
same notices are in the app under Footer -> About and in every release.

## Disclaimer

StarOrg Codex is an unofficial Star Citizen fan project, not affiliated with the Cloud Imperium
group of companies. All content in this app not authored by its host or users are property of
their respective owners. Star Citizen®, Roberts Space Industries® and Cloud Imperium® are
registered trademarks of Cloud Imperium Rights LLC.

StarOrg Codex ships no game assets. It reads the game files of your own installation and writes
only your own local files (the game text file, one line in `USER.cfg`, and its own settings,
backups and cache).

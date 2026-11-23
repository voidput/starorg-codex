# Contributing

Thanks for helping. Bugs and ideas go to [issues](https://github.com/voidput/starorg-codex/issues)
(templates provided); security problems go to [SECURITY.md](SECURITY.md), never a public issue.
Everyone taking part follows the [Code of Conduct](CODE_OF_CONDUCT.md).

Also read [AGENTS.md](AGENTS.md) (repo map, every CI gate, rules for AI agents and people) and
[PATCH.md](PATCH.md) (text patches for `global.ini` and how to send a code patch).

## Pull requests

- Branch from `main`; one topic per PR. Fill in the PR template.
- **Signed commits are required.** The `main` ruleset rejects unsigned commits. Set up SSH or GPG
  signing ([GitHub docs](https://docs.github.com/en/authentication/managing-commit-signature-verification))
  and commit with `git commit -S` (or `git config commit.gpgsign true`).
- **Conventional Commits** (`feat:`, `fix:`, `docs:`, `ci:`, `build:`, `chore:`, `refactor:`,
  `test:`, `perf:`; scope optional, e.g. `fix(app): ...`; `!` for breaking changes). release-please
  builds the changelog and the version from them.
- Do not commit game files, extracted game data, secrets, or `.tmp/` scratch output.

## Checks

Run these before pushing; CI runs the same.

```sh
# Rust (repo root)
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

# App (app/)
cd app
npm ci
npm run check
npm test
npm run build
```

### Disk space

- Debug builds keep line tables only (`[profile.dev]` in `Cargo.toml`); for a debugger session
  build with `CARGO_PROFILE_DEV_DEBUG=full`.
- Extra git worktrees should share one build folder: set `CARGO_TARGET_DIR` to the main
  checkout's `target/` (an absolute path). Otherwise each worktree builds its own `target/` of
  tens of GB.
- `scripts/clean-dev.sh` shows old build output, worktree `target/` folders and `.tmp/` scratch;
  `scripts/clean-dev.sh --yes` removes them (`DAYS=3` to keep less; uses `cargo sweep` when
  installed).

On Linux the Tauri crate needs the WebKitGTK development packages (see the `Tauri system
libraries` step in `.github/workflows/ci.yml`).

- Changed the command contract (`app/src-tauri/src/contract/`)? Regenerate the samples with
  `UPDATE_CONTRACT_SAMPLES=1 cargo test -p starorg-codex-app` and commit them.
- Changed `Cargo.lock` or `app/package-lock.json`? Regenerate the license notices with
  `scripts/gen-notices.sh` (needs `cargo install cargo-about --locked --features cli` and
  `npm ci` in `app/`) and commit `THIRD_PARTY_NOTICES.md`. New dependencies must use a license
  allowed in `deny.toml`.
- Screen loads in `app/src/`: keep `$effect` dependencies primitive (`app.targetKey`, ids), and
  guard every async load that a click or a channel switch can start again against stale answers:
  `latest()` for one `Load` value, `guard()` for loads that set several fields (both in
  `app/src/lib/loadstate.ts`). Long lists are windowed: `VirtualList.svelte`, or `windowRange` /
  `revealTop` from `app/src/lib/virtual.ts` for a list with its own keyboard handling. Heavy
  Data.p4k workers start on a click, a hand-off or a repeat visit, never on mount alone
  (`app/src/lib/lazyopen.ts`).
- Docs live in `docs/` (an Obsidian-compatible vault): kebab-case file names, wikilinks only in
  the two index notes (always with a plain link next to them). Link new notes from `docs/Home.md`
  and `docs/README.md`, give them frontmatter with `python3 scripts/obsidian_frontmatter.py`, and
  check with `python3 scripts/obsidian_frontmatter.py --check`. The script skips notes that already
  have frontmatter, so bump `updated:` by hand when you edit one.

## License

By contributing you agree that your contributions are licensed under the [MIT License](LICENSE).

# AGENTS.md

Working notes for AI coding agents and people new to the repo. [CONTRIBUTING.md](CONTRIBUTING.md)
holds the commit rules and checks; this file adds the map and the rules that are easy to miss.
For patches (text fixes and code changes) see [PATCH.md](PATCH.md).

## Layout

Rust workspace (`Cargo.toml`, edition 2024, toolchain pinned in `rust-toolchain.toml`) plus a
Tauri 2 + Svelte 5 app. `unsafe_code` is forbidden workspace-wide.

| Path | Owns |
|---|---|
| `crates/starorg-core` | Everything that does not read `Data.p4k`: discovery, `global.ini` parsing (`ini.rs`), enhancements (`enhance/`), patch engine (`patch.rs`, built-in patches in `patches/`), merge and validation (`merge.rs`, `validate.rs`), apply, backups, profile and state (`state.rs`), app folders (`appdirs.rs`), `OUTPUT_VERSION` (`src/lib.rs`) |
| `crates/starorg-data` | Reads `Data.p4k` via StarBreaker; the only crate that depends on StarBreaker (pinned git rev in the root `Cargo.toml`) |
| `crates/starorg-gamedata` | Rust port of sc-gamedata: game data domains as JSON (see its `README.md`) |
| `crates/starorg-mcp` | Experimental local MCP server, read-only ([docs/mcp.md](docs/mcp.md)) |
| `crates/starorg-net` | HTTPS transport for the opt-in network features |
| `crates/starorg-cli` | The `starorg-codex` binary. Must not pull in GTK or an HTTP client (CI `cli-deps` job) |
| `app/src-tauri` | Crate `starorg-codex-app`: the Tauri shell. Command contract in `src/contract/` (`types.rs`, `mod.rs`, real backend in `real/`, `mock.rs`) |
| `app/src` | Svelte 5 UI. `lib/contract/samples.gen.json` is generated from the Rust mock and type-checked against the TS types |
| `app/tests` | Node unit tests (`*.test.ts`); golden files in `app/tests/fixtures/` |
| `fixtures/` | Shared test vectors (coverage, ship names) |
| `crates/*/tests/fixtures/` | Per-crate fixtures. Synthetic only |
| `docs/` | Obsidian-compatible vault: user guide, specs, decisions, CI and release notes |
| `scripts/` | `gen-notices.sh`, `obsidian_frontmatter.py`, `install-linux.sh`, comparison helpers |

More on the app: [app/README.md](app/README.md).

## Gates (what CI runs)

Run from the repo root. CI (`.github/workflows/ci.yml`) runs the `rust` job on `ubuntu-latest`
and `windows-latest`.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test --release --locked -p starorg-cli --test worker    # release profile, panic=unwind
(cd app/src-tauri && cargo clippy --all-targets --locked -- -D warnings && cargo test --locked)
(cd app/src-tauri && cargo clippy --all-targets --locked --features store -- -D warnings \
  && cargo test --locked --features store --lib update)       # Microsoft Store flavour
cargo deny check --workspace
scripts/gen-notices.sh --check   # warning only in CI; needs cargo-about and npm ci in app/

cd app && npm ci && npm run check && npm test && npm run build
```

- Contract changed (`app/src-tauri/src/contract/`)? Run
  `UPDATE_CONTRACT_SAMPLES=1 cargo test -p starorg-codex-app`, then `git status --short`: commit
  the regenerated samples. Without that, `samples_match_the_committed_file` fails in CI.
- Lock file changed? Run `scripts/gen-notices.sh` and commit `THIRD_PARTY_NOTICES.md`. New
  dependencies need a licence allowed in `deny.toml`.
- New workspace crate? Add its `Cargo.lock` entry to `release-please-config.json`; CI checks it
  ([docs/release.md](docs/release.md)).
- Linux needs the WebKitGTK packages from the `Tauri system libraries` step in `ci.yml`.
- Windows: CI covers it. A local try under Wine is in [docs/windows.md](docs/windows.md)
  (throwaway `WINEPREFIX` only).
- Tests that need a real install are `#[ignore]` and read paths such as `STARORG_GAME_DIR`.
  They never run in CI. Do not un-ignore them.

## Commits and PRs

- Conventional Commits; release-please builds the changelog and version from them.
- Signed commits. The `main` ruleset rejects unsigned ones and blocks force pushes.
- No AI attribution: no `Co-Authored-By` trailers or "generated with" lines naming an AI tool.
- One topic per PR, PR template filled in ([.github/pull_request_template.md](.github/pull_request_template.md)).

## Rules that are easy to miss

- **`OUTPUT_VERSION`** (`crates/starorg-core/src/lib.rs`): bump it, and add a history line, when
  the generated text changes for the same inputs. That covers a new or changed enhancement, a
  changed setting default, a new built-in patch, or a fix in the text or merge. Do not bump it
  for releases that leave the output as it was. Users see `Stale (enhancer update)` after a bump.
- **Profile schema** (`PROFILE_SCHEMA` in `crates/starorg-core/src/state.rs`, now 3):
  `[enhance]` is sparse. A save holds only the settings that differ from the preset
  (`Profile::sparse_over` in `enhance/profile.rs`), so new defaults reach users who never
  changed them. Never write a full `[enhance]` table. A schema change needs a migration in
  `Profile::migrate` (`state.rs`) and its tests. A new `[enhance]` key must be named in the release notes
  (builds older than the mission colour settings reject unknown keys; see "Profile compatibility" in
  [docs/release.md](docs/release.md)).
- **Patch files never contain CIG's prose.** Guards are keys or `expect_sha256` of the stock
  value ([PATCH.md](PATCH.md)).
- **No game data in the repo.** No game files, extracted data, `Data.p4k` contents, or exported
  assets. No real player data either: handles, GEIDs, character names or account ids from
  `Game.log` or anywhere else. Fixtures and screenshots use synthetic values.
- **The game install is read-only for development.** Tests use fake install trees (see
  `crates/starorg-cli/tests/common/mod.rs`) and synthetic stock files. Never run `apply`,
  `remove` or `restore` against a real install from an agent session.
- **No secrets.** `.env` and keys are git-ignored; keep it that way.
- **Big temp files go on disk.** On some dev machines `/tmp` is RAM-backed. Put scratch output
  in the repo's `.tmp/` (git-ignored) and point `TMPDIR` there for large test or build runs
  (`scripts/gen-notices.sh` uses `mktemp -d`).
- **Docs**: kebab-case names in `docs/`, wikilinks only in `docs/Home.md` and
  `docs/README.md`, frontmatter via `python3 scripts/obsidian_frontmatter.py` (see
  CONTRIBUTING.md).

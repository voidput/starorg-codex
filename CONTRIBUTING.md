# Contributing

Thanks for helping. Bugs and ideas go to [issues](https://github.com/voidput/starorg-strings/issues)
(templates provided); security problems go to [SECURITY.md](SECURITY.md), never a public issue.
Everyone taking part follows the [Code of Conduct](CODE_OF_CONDUCT.md).

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

On Linux the Tauri crate needs the WebKitGTK development packages (see the `Tauri system
libraries` step in `.github/workflows/ci.yml`).

- Changed the command contract (`app/src-tauri/src/contract/`)? Regenerate the samples with
  `UPDATE_CONTRACT_SAMPLES=1 cargo test -p starorg-strings-app` and commit them.
- Changed `Cargo.lock` or `app/package-lock.json`? Regenerate the license notices with
  `scripts/gen-notices.sh` (needs `cargo install cargo-about --locked --features cli` and
  `npm ci` in `app/`) and commit `THIRD_PARTY_NOTICES.md`. New dependencies must use a license
  allowed in `deny.toml`.
- Docs live in `docs/` (an Obsidian-compatible vault): link new notes from `docs/Home.md` and
  `docs/README.md`.

## License

By contributing you agree that your contributions are licensed under the [MIT License](LICENSE).

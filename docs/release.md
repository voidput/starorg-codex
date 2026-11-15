---
title: Release process and v0.0.1 dry run
tags: [release, ci]
updated: 2026-11-14
---

# Release process

Spec section 12 is the design. This note covers how the files fit and the M0 dry run.

## Files

- `.github/workflows/ci.yml`: fmt, clippy `-D warnings`, `cargo test --workspace --locked` on Ubuntu and Windows,
  release-mode worker test, `cargo deny`, CLI dependency check (no GTK/HTTP), UI checks.
  Tauri and UI steps switch on once `app/package.json` / `app/src-tauri/Cargo.toml` exist.
- `.github/workflows/release.yml`: on push to `main`, release-please; on `release_created`, the CLI
  matrix (musl tar.gz, Windows zip), the Tauri matrix (Windows NSIS+MSI, Linux AppImage+deb+rpm, `latest.json`),
  SLSA attestations, then a `SHA256SUMS` upload.
- `release-please-config.json`, `.release-please-manifest.json`: version `0.0.0`, tags `vX.Y.Z`,
  minor bumps before 1.0.

## Version handling (known weak spot)

Root `Cargo.toml` is a virtual workspace, so release-please's `rust` strategy has no `[package]` to read.
Config uses `release-type: simple` with `toml` extra-files that bump `workspace.package.version` and the
three workspace crates in `Cargo.lock` together. When #8 lands, add to `extra-files`:

- `app/package.json` and `app/package-lock.json` (json, `$.version`, `$.packages[""].version`)
- the app crate's lock entry if `app/src-tauri` keeps its own `Cargo.lock`

`tauri.conf.json` must omit `version`; Tauri then reads the inherited `version.workspace = true`.
If the `simple` + extra-files route fails the dry run, fall back to `rust` with a `[package]`-bearing root
or a small bump script run in the release PR.

## Secrets and variables

`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (updater key; lose it and installs stop updating).
Repo setting: Actions may create PRs. The private repo cannot verify attestations publicly; see the
release-location decision in spec section 12.

## Throwaway v0.0.1 dry run

Do this on a scratch repo or a branch with Actions enabled, never against the real release history.

1. Create a throwaway private repo (e.g. `voidput/starorg-strings-dryrun`), push this tree to its `main`.
   Enable Settings > Actions > "Allow GitHub Actions to create and approve pull requests".
2. Push one `feat:` commit to `main`. The Release workflow opens a "release 0.1.0" PR. To land exactly
   `0.0.1`, add `Release-As: 0.0.1` in a commit body (`git commit --allow-empty -m "chore: release 0.0.1" -m "Release-As: 0.0.1"`).
3. In the release PR diff verify: `Cargo.toml` `[workspace.package] version`, all three crates in
   `Cargo.lock`, `.release-please-manifest.json`, `CHANGELOG.md` move to `0.0.1` together.
4. Merge it. Verify tag `v0.0.1`, release created, `cli` jobs attached `starorg-strings-v0.0.1-*.tar.gz|zip`,
   and `SHA256SUMS` is present and matches `sha256sum -c SHA256SUMS` after downloading.
5. Once #8 exists: confirm the Tauri bundle filenames carry `0.0.1` (proves `version.workspace = true` resolves).
6. Delete the throwaway repo (or the release and tag) afterwards.

Locally, without GitHub: `npx release-please release-pr --dry-run --repo-url=voidput/starorg-strings --token=$(gh auth token)`
prints the planned diff.

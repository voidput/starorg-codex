---
title: Release process and v0.0.1 dry run
tags: [release, ci]
updated: 2026-11-15
---

# Release process

Spec section 12 is the design. This note covers how the files fit and the M0 dry run.

## Files

- `.github/workflows/ci.yml`: fmt, clippy `-D warnings`, `cargo test --workspace --locked` on Ubuntu and Windows,
  release-mode worker test, `cargo deny`, CLI dependency check (no GTK/HTTP), UI checks, and a
  `THIRD_PARTY_NOTICES.md` freshness check (warning only).
  Tauri and UI steps switch on once `app/package.json` / `app/src-tauri/Cargo.toml` exist.
- `.github/workflows/release.yml`: on push to `main`, release-please; on `release_created`, the CLI
  matrix (musl tar.gz, Windows zip, each with `LICENSE` and `THIRD_PARTY_NOTICES.md`), the Tauri matrix (Windows NSIS+MSI, Linux AppImage+deb+rpm, `latest.json`),
  SLSA attestations, the `store` job (unsigned `.msix` for Microsoft Store submission, only with
  the `MSIX_*` variables), then a `SHA256SUMS` upload (which no longer waits on a successful store job).
- `scripts/gen-notices.sh` (`about.toml`, `scripts/notices/`): `THIRD_PARTY_NOTICES.md`, the license
  texts of every shipped Rust crate (cargo-about) and frontend npm package. The release's `notices`
  job regenerates it from the release commit, uploads it as a release asset and passes it to the
  `cli`, `app` and `store` jobs (CLI archives, the app's Footer -> About, installer resources).
- `.github/actions/windows-signing`: Azure Artifact Signing for the NSIS/MSI installers and the
  Windows CLI exe, only when the `AZURE_*` secrets and variables exist ([windows](windows.md#code-signing)).
- `app/scripts/msix.ts`: writes `AppxManifest.xml` and stages the Store package
  (`app/tests/msix.test.ts`, golden `app/tests/fixtures/AppxManifest.golden.xml`).
- `release-please-config.json`, `.release-please-manifest.json`: version `0.0.0`, tags `vX.Y.Z`,
  minor bumps before 1.0.

## Version handling (known weak spot)

Root `Cargo.toml` is a virtual workspace, so release-please's `rust` strategy has no `[package]` to read.
Config uses `release-type: simple` with extra-files that bump, together:

- `Cargo.toml` `$.workspace.package.version`
- `Cargo.lock` entries for every workspace member: `starorg-core`, `starorg-data`, `starorg-gamedata`, `starorg-cli`,
  `starorg-strings-app`. Filter on `@.name.value`, not `@.name`: release-please's TOML parser wraps
  each value in a tagged object, so `@.name=="x"` silently matches nothing ("No entries modified").
- `app/package.json` `$.version`, `app/package-lock.json` `$.version` and `$.packages[''].version`

**Adding a workspace crate means adding its `Cargo.lock` entry here**, or the release PR breaks every
`--locked` step. Check locally before pushing a config change: clone into `.tmp/`, install
`release-please` there, run its `GenericToml`/`GenericJson` updaters over each extra-file with version
`0.0.1`, then `cargo metadata --locked` and `npm ci --dry-run` in the clone (done 2026-11-14 with
release-please 17.11.2: all 8 entries changed, both checks pass).

`tauri.conf.json` must omit `version`; Tauri then reads the inherited `version.workspace = true`.
If the `simple` + extra-files route fails the dry run, fall back to `rust` with a `[package]`-bearing root
or a small bump script run in the release PR.

## Secrets and variables

`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (updater key; lose it and installs stop updating).
Optional: without them (or with the placeholder in `app/src-tauri/updater-public-key.txt`) releases build
without `.sig`/`latest.json` and the app offers download only. Key setup: [updates](updates.md).
Repo setting: Actions may create PRs. The private repo cannot verify attestations publicly; see the
release-location decision in spec section 12.

Windows extras, all optional (setup steps in [windows](windows.md)):

| Name | Kind | For |
|---|---|---|
| `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, `AZURE_TENANT_ID` | secrets | Artifact Signing app registration |
| `AZURE_SIGNING_ENDPOINT`, `AZURE_SIGNING_ACCOUNT`, `AZURE_SIGNING_PROFILE` | variables | Artifact Signing account and certificate profile |
| `MSIX_IDENTITY_NAME`, `MSIX_PUBLISHER`, `MSIX_PUBLISHER_DISPLAY_NAME` | variables | Store package identity (Partner Center -> Product identity) |
| `MS_STORE_SUBMIT` (`true`), `MS_STORE_ID` | variables | Optional draft submission with the msstore CLI |
| `MS_STORE_TENANT_ID`, `MS_STORE_SELLER_ID`, `MS_STORE_CLIENT_ID`, `MS_STORE_CLIENT_SECRET` | secrets | Partner Center API, only with `MS_STORE_SUBMIT` |

Each missing group gives a notice in the run and leaves that part out; a release never fails for
lack of signing or Store setup.

Windows release checks (after the run): the store job log names the `.msix` it uploaded (or the
notice); with signing on, the cli and app jobs already failed unless `Get-AuthenticodeSignature`
says `Valid` for every signed file; `SHA256SUMS` lists the `.msix` too.

Store versions: Partner Center rejects package versions starting with 0, so a 0.x release has no
`.msix` (the store job ends with a notice). The first Store release is 1.0.0, cut with a
`Release-As: 1.0.0` commit body; see [windows](windows.md#store-versions).

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

## Prereleases

A version with a SemVer pre-release suffix (`X.Y.Z-beta.1`, `-rc.1`; set it with `"release-as"` in
`release-please-config.json`) is a prerelease. The release job marks its GitHub release as a
prerelease (so `releases/latest` and the in-app updater skip it), builds the Windows installers and
the CLI exe **unsigned** (no Azure Artifact Signing) and skips the Store package. Plain `X.Y.Z`
releases are signed as before.

## Branch protection

Rulesets on GitHub (Settings -> Rules): `main` blocks deletion and force pushes and requires signed
commits; `release tags` (`refs/tags/v*`) blocks deleting, moving or force-updating a release tag.

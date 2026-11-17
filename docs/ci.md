---
title: CI and security checks
tags: [ci, security]
updated: 2026-11-15
---

# CI and security checks

Workflows live in `.github/workflows/`. Every `uses:` is pinned to a full commit SHA with the version in a trailing
comment; Dependabot (`github-actions`) moves the SHA and comment together. Each workflow sets least-privilege
`permissions` (top-level `contents: read`, escalated per job only where needed) and a concurrency group.

| Workflow | Triggers | Jobs |
|---|---|---|
| CI (`ci.yml`) | PR, push `main`/`dev` | `rust` (fmt, clippy, tests on Linux + Windows, release worker test, Tauri crate checks), `deny` (cargo-deny, all checks), `cli-deps` (CLI tree has no GTK/HTTP), `ui` (lint, check, unit tests in `app/`), `notices` (`THIRD_PARTY_NOTICES.md` up to date; warning only, so Dependabot lock bumps stay green), `notify-failure` |
| Security (`security.yml`) | push `main`, PR, weekly Mon 06:17 UTC, manual | `codeql` (rust, javascript-typescript, actions; `security-and-quality`; `build-mode: none` so no GTK build), `cargo-audit` (RustSec advisories via cargo-deny), `npm-audit` (`app/`: prod moderate+, all high+), `dependency-review` (PRs, fails on high+), `secrets` (gitleaks, full history), `notify-failure` |
| Scorecard (`scorecard.yml`) | push `main`, weekly Mon 06:41 UTC, manual | OpenSSF Scorecard; publishes the score and uploads SARIF to Code scanning. Never fails the build |
| Notify CI Failure (`notify-failure.yml`) | `workflow_call` only | Posts a red Discord embed. No-ops when the webhook secret is unset. GitHub-hosted runner only (public repo: never self-hosted) |
| Release (`release.yml`) | push `main` | release-please + build matrix; see [release process](release.md) |

`notify-failure` jobs in CI and Security run `if: failure()` on push/schedule only, never on PRs.

## Dependabot
`.github/dependabot.yml`: weekly (Monday) cargo (`/`), npm (`/app`) and github-actions (`/`). Minor/patch are grouped
per ecosystem, so CI and Security run once per batch; limit 5 open PRs each. `tauri`/`tauri-*` crates and
`@tauri-apps/*` npm packages form their own group so the Rust crate and JS API bump in lockstep.

Deferred majors (each has a targeted `ignore` in `dependabot.yml`; drop it when revisiting):
- `typescript` >= 7 (deferred 2026-11-15, 1.0.0 dependency pass). TypeScript 7 is the native (Go) compiler and
  ships no JS compiler API: the npm package's main export is only its version. `svelte-check` 4.7.8, which
  `npm run check` and CI use, loads that API and declares `typescript: ^5.0.0 || ^6.0.0`; with 7.0.2 installed
  it crashes on start. Revisit when svelte-check (or the Svelte language tools) support TS 7. 6.x stays allowed.

## Secrets
| Secret | Required | Use |
|---|---|---|
| `DISCORD_CI_WEBHOOK` | optional | Discord webhook for failure pings; unset means no notification |
| `GITHUB_TOKEN` | automatic | dependency review, CodeQL (gitleaks runs as the pinned CLI, no token or licence) |
| `TAURI_SIGNING_*` | handled elsewhere | release signing, see [release process](release.md) |

## Repo settings to enable (manual)
- Security -> Code security: Dependabot alerts and Dependabot security updates on.
- Secret scanning and push protection on.
- Private vulnerability reporting on (see `SECURITY.md`).
- CodeQL default setup OFF: `security.yml` is the advanced setup, and both together conflict.
- `main` ruleset (set 2026-11-17): signed commits, no force push or deletion, changes only through a
  PR with 1 approval and these checks passing: `rust (ubuntu-latest)`, `rust (windows-latest)`,
  `ui (app/)`, `deny`, `third-party notices`, `CLI has no GTK or HTTP client`,
  `gitleaks (full history)`, `cargo audit (RustSec via cargo-deny)`. Repository admins may bypass
  on a PR only (solo merges, and release-please PRs, which get no CI runs from `GITHUB_TOKEN`); a
  `Release-As:` commit therefore goes through a PR too. Renaming a CI job means updating the
  ruleset's check list.
- Optional: add the `DISCORD_CI_WEBHOOK` Actions secret.

## Scorecard

`scorecard.yml` publishes results (badge in the README). Known advisories in transitive
dependencies with no fix (Tauri's GTK3 stack, keyring's Secret Service backend, StarBreaker's UI
renderer) are listed with reasons in both `deny.toml` and `osv-scanner.toml`; review them on every
Tauri, keyring or StarBreaker bump. Releases carry an SLSA provenance bundle
(`starorg-codex-vX.Y.Z.intoto.jsonl`, from `actions/attest-build-provenance` over SHA256SUMS).

## Fuzzing

`fuzz/` holds cargo-fuzz targets for the parsers that read files users hand the app:
`global_ini`, `fleet_import`, `smart_citizen_export`. `fuzz.yml` runs each for 5 minutes weekly
(and 1 minute on PRs touching those parsers); a crash fails the run and uploads the input. Locally:
`cargo +nightly fuzz run global_ini -- -max_total_time=60` in `fuzz/`.

## Local
```sh
cargo deny check --workspace
(cd app && npm audit --omit=dev --audit-level=moderate && npm audit --audit-level=high)
uvx --from actionlint-py actionlint
```

---
title: CI and security checks
tags: [ci, security]
updated: 2026-11-14
---

# CI and security checks

Workflows live in `.github/workflows/`. Every `uses:` is pinned to a full commit SHA with the version in a trailing
comment; Dependabot (`github-actions`) moves the SHA and comment together. Each workflow sets least-privilege
`permissions` (top-level `contents: read`, escalated per job only where needed) and a concurrency group.

| Workflow | Triggers | Jobs |
|---|---|---|
| CI (`ci.yml`) | PR, push `main`/`dev` | `rust` (fmt, clippy, tests on Linux + Windows, release worker test, Tauri crate checks), `deny` (cargo-deny, all checks), `cli-deps` (CLI tree has no GTK/HTTP), `ui` (lint, check, unit tests in `app/`), `notify-failure` |
| Security (`security.yml`) | push `main`, PR, weekly Mon 06:17 UTC, manual | `codeql` (rust, javascript-typescript, actions; `security-and-quality`; `build-mode: none` so no GTK build), `cargo-audit` (RustSec advisories via cargo-deny), `npm-audit` (`app/`: prod moderate+, all high+), `dependency-review` (PRs, fails on high+), `secrets` (gitleaks, full history), `notify-failure` |
| Scorecard (`scorecard.yml`) | push `main`, weekly Mon 06:41 UTC, manual | OpenSSF Scorecard; publishes the score and uploads SARIF to Code scanning. Never fails the build |
| Notify CI Failure (`notify-failure.yml`) | `workflow_call` only | Posts a red Discord embed. No-ops when the webhook secret is unset. GitHub-hosted runner only (public repo: never self-hosted) |
| Release (`release.yml`) | push `main` | release-please + build matrix; see [release process](release.md) |

`notify-failure` jobs in CI and Security run `if: failure()` on push/schedule only, never on PRs.

## Dependabot
`.github/dependabot.yml`: weekly (Monday) cargo (`/`), npm (`/app`) and github-actions (`/`). Minor/patch are grouped
per ecosystem, so CI and Security run once per batch; limit 5 open PRs each. `tauri`/`tauri-*` crates and
`@tauri-apps/*` npm packages form their own group so the Rust crate and JS API bump in lockstep.

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
- Optional: branch protection on `main` requiring the CI and Security checks (raises the Scorecard score).
- Optional: add the `DISCORD_CI_WEBHOOK` Actions secret.

## Local
```sh
cargo deny check --workspace
(cd app && npm audit --omit=dev --audit-level=moderate && npm audit --audit-level=high)
uvx --from actionlint-py actionlint
```

---
title: "Updates: check, install, signing"
tags: [docs]
updated: 2026-11-15
---

# Updates: check, install, signing

How StarOrg Codex finds and installs new versions. Design: spec sections 11 (network policy)
and 12 (release, updater); this note is the as-built reference and the owner's checklist.

## What the user sees

- **Settings -> Network -> Check for updates**, off by default. The setup wizard's
  **Preferences** step asks the same question (off unless switched on, with the same "what is
  sent" text); Apply saves it before setup runs and, if it was turned on, runs the first check.
  With it off the app requests nothing and keeps whatever it last knew.
- With it on: one check when the app starts, **at most once per 24 hours** (a failed check
  counts too), plus **Check now**. Turning the switch on runs the first check right away.
- A newer version shows a **banner above every screen**: "StarOrg Codex X.Y.Z is available",
  What's new (the release notes), Install and restart (where supported) or Download from the
  release page, **Skip this version** (that exact version only; a newer one shows again; Undo is
  in Settings), and X (hide until the next start).
- **Include pre-releases** (off): also offer `-beta`/`-rc` versions. **Release repository**
  (`voidput/starorg-strings`): any `owner/name` on github.com, for a public mirror.
- Problems are reported in Settings in calm words, never as an error dialog:

| Answer | Shown as |
|---|---|
| HTTP 404, 401, or 403 with rate left | "Can't see releases for <repo>: the repo is private, renamed or has no public releases yet." |
| HTTP 403 with `x-ratelimit-remaining: 0`, or 429 | GitHub's limit for checks without sign-in is used up |
| No HTTP answer (DNS, TLS, timeout) | "Can't reach GitHub: you look offline." |
| 5xx, not JSON, over 2 MiB | "GitHub's answer could not be read (HTTP n)." |

## What is sent (privacy)

- One `GET https://api.github.com/repos/<repo>/releases?per_page=30` per check
  (`crates/starorg-net/src/releases.rs`), over the OS TLS stack.
- Headers: `User-Agent: StarOrg-Strings/<version>`, `Accept`, `X-GitHub-Api-Version`. No token,
  cookie, account or install id. GitHub sees the IP address, as with any web request.
- Installing (below) fetches `latest.json` and the installer from github.com release assets,
  only after the user presses Install and restart.
- The CLI has no HTTP client and never checks. `starorg-strings version [--json]` prints the
  version and build details only.

## How a release is chosen

`app/src-tauri/src/update/mod.rs`:

- Drafts are ignored. A release counts as a pre-release when GitHub flags it **or** its tag has a
  semver pre-release part (`v0.3.0-rc.1`); those count only with Include pre-releases.
- Tags are `vX.Y.Z` (or `X.Y.Z`); other tags are ignored. Order is semver precedence
  (`0.10.0` > `0.9.0`, `0.2.0` > `0.2.0-dev3`, build metadata ignored).
- Offered only if newer than the running version. After updating, a saved "available" for the
  running or an older version reads as up to date.
- The release page link is built as `https://github.com/<repo>/releases/tag/<tag>`, never taken
  from the response, and opened only if it passes the `https://github.com/<repo>/releases/...`
  allowlist (`allowed_release_url`). It opens in the system browser from Rust; the webview has
  no opener, shell or updater permission.
- Release notes are reduced to plain text blocks in Rust (`update/notes.rs`): HTML tags and
  comments, images, link targets, emphasis markers, control and bidi-override characters are
  removed; at most 40 blocks of 400 characters. The UI shows them as text.
- State lives in `app-update.json` in the app data folder (repo, pre-release choice, last check
  time and result, skipped version). A broken file is a fresh start.

## In-app install

| Install | Updates |
|---|---|
| AppImage, Windows NSIS (and MSI) | In the app: download, verify the minisign signature against our key, install, restart (`tauri-plugin-updater`) |
| deb, rpm | Download only: "update it with your package manager or from the release page" ([linux](linux.md)) |
| Windows portable zip, built from source, CLI | Download only |
| Any build whose `updater-public-key.txt` is still the placeholder | Download only |
| Microsoft Store build (feature `store`) | Store only: no check, no banner, no install; Settings says "Updates come from the Microsoft Store." ([windows](windows.md)) |

The bundle type is the one Tauri stamps into the binary at bundling. The manifest used is the
checked release's own `https://github.com/<repo>/releases/download/<tag>/latest.json`, and the
install is refused if it names another version.

## Signing setup (owner, once)

Never commit the private key. Nothing in this repo generates one.

1. `cd app && npx tauri signer generate -w ~/.tauri/starorg-strings.key` and choose a password.
   It prints the public key and writes `~/.tauri/starorg-strings.key` (private) and `.key.pub`.
2. Store the private key file content and the password in 1Password (losing them ends updates
   for every existing install).
3. GitHub repo -> Settings -> Secrets and variables -> Actions: add
   `TAURI_SIGNING_PRIVATE_KEY` (the private key file's content) and
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
4. Replace the single line in `app/src-tauri/updater-public-key.txt` with the public key (the
   base64 text starting `dW50cnVzdGVkIGNvbW1lbnQ6`) and commit it. The app reads it at compile
   time; release.yml merges it into the Tauri config.
5. The next release: the Release workflow's "Updater signing" step reports enabled, the AppImage
   and Windows installers get `.sig` files, and `latest.json` is uploaded. Without the secret or
   the key the step prints a notice and releases build exactly as before.

`tauri.conf.json` carries a placeholder `plugins.updater.pubkey` (the plugin needs the field) and
`bundle.createUpdaterArtifacts: false`; release.yml overrides both only when signing is on.

## Private repo caveat (spec Q1)

`voidput/starorg-strings` is private. Without credentials GitHub answers 404 for its releases
and its assets, so for everyone else the check reads "Can't see releases ..." and in-app install
cannot download. Nothing breaks, but updates do not work until Q1 is decided:

- (a) a public releases repo (spec's assumption): publish releases there, then either change
  `DEFAULT_REPO` in `update/mod.rs` before the first public build or tell users to set the
  Release repository; (b) static hosting needs a different fetcher; (c) making this repo public
  makes the default work as is.
- Whatever is chosen, the repo baked into v0.1.0 is what existing installs ask first.

## Testing

- `cargo test -p starorg-strings-app update::` (fake HTTP layer and clock): semver and
  pre-release rules, 24 h throttle, skip, repo prefs, error mapping, notes sanitising, URL
  allowlist, file state. `--test real_backend update_check` runs the real backend against a
  scripted GitHub. No test touches the network; `cargo test -p starorg-net -- --ignored` has one
  real request against a public repo.
- Contract samples include `update_status`, `check_for_updates` and `check_for_updates_failed`
  (Rust mock, fixed version 0.1.0 and clock). `npm test` checks the banner rules over them.
- Renders: browser mock with `?update=available|failed|skipped|store` (`&notes=1` opens What's new).
- Store flavour: `cargo test -p starorg-strings-app --features store --lib update` (CI runs it):
  `store_builds_never_request_or_install` checks no request and no updater call.

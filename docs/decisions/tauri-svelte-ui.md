---
title: "UI: Tauri 2 with Svelte 5, Tailwind 4 and shadcn-svelte"
tags: [docs, decisions]
updated: 2026-11-14
---

# UI: Tauri 2 with Svelte 5, Tailwind 4 and shadcn-svelte

Status: accepted (2026-11-14), revised the same day after review

## Context

StarOrg Codex needs a desktop UI on Windows and Linux, a headless CLI from the same code,
and the easiest possible install. The backend is Rust because game data is read through the
StarBreaker crates. We already have a design system and about 20 reusable components in RSI+
(Svelte 5, Tailwind 4, shadcn-svelte) and a visual identity in StarOrg Tools.

Smart Citizen, the tool we are replacing, is Python with PyQt6 and is Windows-only; on Linux
it had to run under Wine.

## Decision

- Tauri 2 shell with a Svelte 5 + Tailwind 4 + shadcn-svelte frontend.
- All logic lives in Rust crates (`starorg-core`, `starorg-data`). The Tauri layer is thin
  typed commands plus progress events.
- The CLI is the same Rust code without Tauri, shipped as its **own small binary on both
  systems** (`starorg-strings`; the GUI is `starorg-strings-app`). The first draft used one
  binary on Linux that skipped GTK when given a subcommand. That binary would still link
  WebKitGTK: slower to start from a launch hook, unable to start where WebKit is missing,
  and no use to AppImage users, who have nothing on `PATH`.
- The GUI does not extract game data itself; it runs the CLI's worker step as a child
  process.
- RSI+ token names and components are reused; StarOrg identity (void background, Orbitron
  headings, chamfer and glow in three places) is layered on top.

## Alternatives considered

- **Native Rust GUI (egui, iced, Slint).** One language and no webview, but we would rebuild
  every component, lose the RSI+ kit, and get weaker text rendering and accessibility for a
  text-heavy app.
- **Electron.** Same frontend reuse, but a 100 MB+ install and a bundled browser to keep
  patched. Tauri uses the system webview and gives a Rust backend directly.
- **Qt (PyQt or C++).** What Smart Citizen uses. Does not fit a Rust core and brings back the
  packaging problems we are leaving.

## Consequences

- Small installers; the Windows build depends on WebView2 (present on Windows 10 1803+ and
  11) and the Linux deb/rpm on `webkit2gtk-4.1`.
- WebKitGTK on NVIDIA with Wayland needs environment workarounds set before the window is
  created, and blur effects must degrade to solid surfaces. See spec section 10.4.
- The AppImage bundles WebKit and has known start-up crashes on Arch-based systems, so an AUR
  package is also shipped. Atomic systems (SteamOS, Bazzite) can only use the AppImage, so
  the CLI is also a static tarball and the app can install it to `~/.local/bin`.
- The release profile keeps `panic = "unwind"`, against the usual Tauri size advice.
- On Windows the window is drawn by Microsoft's WebView2 runtime, which has traffic of its
  own. "No network by default" is a promise about our code, and the app says so.
- Unsigned Windows builds do not run under Smart App Control. That is a signing question,
  not a UI one, but it limits who can use v0.x.
- The 91,000-row string table must be virtualised; filtering and sorting run in Rust.
- StarOrg's React components cannot be copied; the few we need are rebuilt in Svelte from the
  design spec.
- The webview is treated as untrusted: strict CSP, minimal capabilities, no raw paths from
  the frontend.

Spec: [StarOrg Codex v1](../plans/specs/starorg-strings-v1.md), sections 3 and 9.

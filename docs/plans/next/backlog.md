---
title: Next
tags: [docs, plans, next]
updated: 2026-11-17
---

# Next

M1–M4 shipped in 0.1.0–0.3.0 (apply core, hooks, GameData, enhancements, mission and blueprint
info, app screens, releases and the updater); see [CHANGELOG.md](../../../CHANGELOG.md).

- Release after 0.3: drop the `starorg-strings` CLI alias (tarball link, zip copy, install-linux.sh link), the legacy hook markers and `starorg_core::legacy` folder migration ([decision](../../decisions/rename-to-starorg-codex.md)) (tracking issue to be created)
- StarOrg Tools sign-in and blueprint pull/push: built in the app, waiting for the host to enable sign-in ([#18](https://github.com/voidput/starorg-codex/issues/18))
- Published dumps: rescoped 2026-11-17 to Developer mode's Diff only (past LIVE builds, Settings -> Network -> "Download published LIVE builds for Diff"), built on `feat/dump-diff`; apply never uses them ([#15](https://github.com/voidput/starorg-codex/issues/15))

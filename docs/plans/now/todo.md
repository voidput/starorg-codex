---
title: Now
tags: [docs, plans, now]
updated: 2026-11-17
---

# Now

Open items only; done work is in [CHANGELOG.md](../../../CHANGELOG.md) (v0.3.0 is released, `main`
is pushed). Spec: [[specs/starorg-codex-v1]] ([plain](../specs/starorg-codex-v1.md)) and its
addendum [[specs/starorg-codex-v1-addendum]] ([plain](../specs/starorg-codex-v1-addendum.md)).

Release and Windows

- [ ] Windows tester (Q19) ([#9](https://github.com/voidput/starorg-codex/issues/9)); the release location (Q1) is settled: the repo is public and the update check's default works ([updates](../../updates.md#release-location-spec-q1))
- [ ] Windows tester runs [windows-test-checklist](../../windows-test-checklist.md) on a real install and files the report ([#9](https://github.com/voidput/starorg-codex/issues/9))
- [ ] Owner: delete the duplicate Partner Center app "StarOrgCodex" (identity `StarOrgLabs.StarOrgCodex`, never submitted). Keep "StarOrg Codex" (Store ID 9NWHZD6MLMLD, identity `StarOrgLabs.293108C0BE3BF`), which the repo variables MSIX_* / MS_STORE_* point at
- [ ] Project board: check that closed issues (#1–#8, #10–#15) are Done and #9, #18 match their state (#16, #17 closed 2026-11-17)

Apply, hooks and blueprints

- [ ] M1/M2 in-game checks (spec 13): owner plays a session with an applied LIVE file ([#10](https://github.com/voidput/starorg-codex/issues/10), [#14](https://github.com/voidput/starorg-codex/issues/14))
- [ ] Owner: replace the Smart Citizen block in the real `sc-prelaunch.sh` with `hook install` (agents never edit the real prefix) ([#11](https://github.com/voidput/starorg-codex/issues/11))

Fleet import and ship ordering (addendum §6)

- [ ] In-game checks (spec §13, Fleet rows): list sort (Q23), long vs short name (Q25), where the prefix shows, symbols ([#15](https://github.com/voidput/starorg-codex/issues/15))
- [ ] Owner: try an RSI+ export (`fleet import --dry-run`) and report store names that need aliases ([#15](https://github.com/voidput/starorg-codex/issues/15))

Developer mode (addendum §7)

- [ ] Owner: download two published LIVE builds on Datasets and compare them on Diff in the real app (rescoped 2026-11-17: dumps are Diff-only, apply reads Data.p4k; built on `feat/dump-diff`) ([#15](https://github.com/voidput/starorg-codex/issues/15))
- [ ] Backlinks ("Referenced by", Q-D2) and the `starorg-gamedata` domain outputs as a source ([#15](https://github.com/voidput/starorg-codex/issues/15))
- [ ] Owner: open Explorer on LIVE in the real app and try a type export ([#15](https://github.com/voidput/starorg-codex/issues/15))

Model export 3D print processing, experimental — [[specs/model-print]] ([plain](../specs/model-print.md))

- [ ] Fill the stubs (scale: drop tiny + bed check; voxel: solid, thicken, hollow, pegs, extract; split; color; simplify; weapons) per the spec's module table
- [ ] Owner: print a Skylark from `dev print-glb` output and report what the slicer says

Local MCP server, experimental (addendum §10)

- [ ] Owner: connect Claude Code / Claude Desktop to the app's server on a real install and try the tools ([#15](https://github.com/voidput/starorg-codex/issues/15))

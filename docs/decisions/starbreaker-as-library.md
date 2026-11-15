---
title: Read game data with the StarBreaker crates, as a library
tags: [docs, decisions]
updated: 2026-11-14
---

# Read game data with the StarBreaker crates, as a library

Status: accepted (2026-11-14), revised the same day after review

## Context

To build an enhanced `global.ini` we need two files from the installed `Data.p4k`: the stock
English `global.ini` and the DataCore database (`Game2.dcb`), and then typed access to
DataCore records.

Smart Citizen gets there by running an external extractor that dumps DataCore to XML, then
parsing the XML. That path loses type tags (which is why its ship Pitch/Roll/Yaw never
appear), needs a large on-disk cache, and invites editing the cache in place.

[StarBreaker](https://github.com/diogotr7/StarBreaker) by diogotr7 (MIT) has Rust crates for
both jobs: `starbreaker-p4k`, `starbreaker-datacore`, `starbreaker-common`. We tested them as
a library, at upstream commit `041e559215e18e1b13b124cab0e3ecb0f3aa50d4` (origin/main,
2026-11-14), against the real LIVE, PTU and HOTFIX archives: open in about 0.4 s, `global.ini`
in 5 ms, DataCore read in about 110 ms and parsed in under 8 ms, typed queries across all
30,000 entity records in about 1 ms. They are pure Rust, thread-safe, and build on Windows
and Linux without a C toolchain. Its DataCore reader is correct; an earlier suspected
off-by-one was our own comparison error.

## Decision

- Depend on the three crates as a **git dependency pinned to the commit above**. They are not
  on crates.io.
- Point the dependency at **our own mirror**, `voidput/StarBreaker`, from day one. Upstream
  has one maintainer and no releases; a force-push or a deleted repo must not break our
  builds. The mirror carries no changes of ours. `cargo deny` allows only that git URL.
- The pin must be a commit whose `LICENSE` has the full MIT text. The pinned one does
  ("Copyright (c) 2026 Diogo Trindade"). Our local checkout `~/git/StarBreaker` is 804
  commits older (`08302fbd`); there the file is only the word "MIT", and the silent-stop
  behaviour below does not exist yet. Claims in the spec are about the pinned commit.
- Use them as a library. Do not shell out to the StarBreaker CLI and do not go through JSON
  or XML exports.
- Only one of our crates, `starorg-data`, may depend on StarBreaker. It turns DataCore into
  our own `GameData` model; everything else reads that model.
- Add our own checks around known gaps:
  - compare the archive's declared entry count with the parsed count, because
    `parse_entries` (since upstream `dd7a6095`) stops silently on a mid-directory error.
    StarBreaker keeps its count and locator code private, so this is a small reader of our
    own for the three fixed end-of-directory structs. It is a deliberate, tested duplicate;
  - run extraction in a **child process**, and DataCore work under `catch_unwind` inside it.
    Some accessors panic on malformed data, and a panic handler does not catch
    out-of-memory or stack-overflow aborts. The release profile is pinned to
    `panic = "unwind"` (Tauri size guides suggest `abort`), with a release-mode test;
  - check declared sizes and CRC-32 of the two files we read;
  - report query paths that fail to compile or return nothing as schema drift;
  - write our own install discovery (StarBreaker's skips HOTFIX and non-default roots).
- Ship StarBreaker's MIT notice in the app's licence screen.

## Alternatives considered

- **StarBreaker CLI plus JSON export** (what our sc-gamedata repo does). Works, but adds a
  second binary to install, a large intermediate export, and slower runs. Not acceptable for
  a launch hook.
- **unp4k/unforge XML** (Smart Citizen's approach). Loses type information and is
  Windows-centred.
- **Our own p4k and DataCore reader.** Months of work to reproduce something that already
  exists, is correct and is MIT-licensed.

## Consequences

- No semver promise upstream: the crates still say 0.3.2 while main is hundreds of commits
  ahead. A rev bump is a deliberate change, reviewed, and limited to `starorg-data`.
- The effective minimum Rust version (1.92 today) comes from StarBreaker's dependencies. We
  pin the toolchain and build with `--locked`.
- Peak memory is about 890 MB while the archive index is open (measured at the pinned
  commit). The worker process drops the index as soon as the two files are read and returns
  all memory when it exits; one worker runs at a time; `GameData` is cached per game build
  with the StarBreaker commit in the cache key.
- Upstream gets a pull request for an **opt-in** strict count mode. The lenient behaviour is
  right for embedded `.pak` archives whose stored count is wrong, so we do not ask for a
  revert, and we do not wait for the answer.
- The mirror becomes a real fork only if we want a low-memory "read these named entries"
  API. That would be a second decision.

Spec: [StarOrg Codex v1](../plans/specs/starorg-strings-v1.md), sections 3.3, 3.4 and 11.

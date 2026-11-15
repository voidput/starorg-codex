---
title: Our own patch format for CIG data bugs
tags: [docs, decisions]
updated: 2026-11-14
---

# Our own patch format for CIG data bugs

Status: accepted (2026-11-14), revised the same day after review

## Context

Star Citizen's data has bugs that show up as wrong text: one item pointing at another item's
name key, a contract pointing at the wrong description, a description field holding a title
key. An enhancer that trusts the data writes stats or mission info onto the wrong string.
Known cases today: the Bengal S6 gatling reusing the APAR S4 keys, Hockrow contract P2M4
pointing at the P2M1 description, and `hockrowagency_recoveritem` using its title as its
description.

Smart Citizen (Osiris DevWorks) had the idea of shipping such fixes as data files
(`*.patch.json`). We keep the idea and not the design. Its patches are XPath edits applied
**in place** to an extracted XML cache. That ties them to one extractor's file layout,
changes the cache other code reads (it caused our false "DataCore off-by-one" alarm), has no
link to a game build, and never retires a fix once CIG corrects the data.

## Decision

A patch is a TOML file (`<id>.patch.toml`, `schema = 1`) with two kinds of entry:

- **Data patch**: select one record by short name (or GUID) and set one named field of our
  `GameData` model, e.g. `desc_key`. Applied as an in-memory overlay before generators run.
  Nothing on disk is changed.
- **Loc patch**: change one output string after generators run (`set`, `append`, `prepend`,
  `replace`, `append_from`). Used when the game still follows the bugged pointer, so the
  player-visible text needs the correction.
  - `append_from` takes a named part of the other key. The default is `generated`: only the
    text our generators added, not CIG's prose.
  - All loc patches read from one snapshot taken before any of them writes, so there is no
    ordering between patches and no cycles.

Guards: a data entry carries `expect` (a loc key). A loc entry carries `expect_sha256` of the
stock value. Patch files never contain CIG's prose.

The engine reports one state per entry and never fails the build:

- `applies`: the guard matches;
- `not-needed`, shown as "CIG fixed it". Defined for every kind of entry: a data field that
  already has the target value; a loc entry tied (`requires`) to a data entry that is not
  needed; a `set`/`replace`/`append`/`prepend` whose result is already in the stock value;
- `target-changed`: it is something else, so the entry is skipped with a warning;
- `nothing-to-add`, `missing`, `out-of-range`, `conflict`.

**Build range.** `builds` compares the game version taken from `Data.Branch`
(`sc-alpha-4.10.0-hotfix` → 4.10.0) and, for exact builds, `RequestedP4ChangeNum`.
`Data.Version` is never compared: it is numbered per branch (`4.10.200.58746` on LIVE,
`1.0.191.55227` on HOTFIX), and a range on it would switch every patch off on HOTFIX.

Built-in patches are embedded in the binary. Users can add their own in the app data
directory and switch any patch off per channel. The patch-set hash is part of the applied
state, so changing patches marks a channel as needing re-apply.

## Alternatives considered

- **Reuse Smart Citizen's JSON/XPath schema.** Would need an XML cache we do not have and
  carries the problems above.
- **Address fields by DataCore query path.** More general, but exposes StarBreaker's query
  syntax and CIG's struct names in a user-facing file; both change. Named fields of our own
  model are stable and also work if the model is later filled from published data dumps.
- **Hard-code fixes in Rust.** Simple, but every fix needs a release, users cannot add their
  own, and there is no visible list of what was changed and why.
- **GUID as the primary selector.** Exact, but unreadable in review. Record names are
  readable and stable enough; GUID stays available for ambiguous names.
- **A patch for every shared-key case.** The Bengal S6 gatling was first handled by a patch
  that cleared its name and description keys. The namesake rule (the record named after a key
  owns it) already solves it, and clearing the key left the gun nameless in loadout lists.
  Patches are for wrong pointers, not for shared keys. v1 ships the two Hockrow patches.

## Consequences

- The set of patchable fields is a small documented list (loc key references in v1). Adding
  a field is a code change plus a schema note.
- Patches retire themselves visibly, so the built-in list stays short.
- Patch files are plain data: no expressions, no regular expressions, size-limited.
- Each built-in patch gets two tests: `applies` on a fixture with the bug, `not-needed` on a
  fixture without it.
- Patch output goes through the same validation as everything else. A patch that would break
  a rule (tokens, EM tags, format keys) is dropped alone, with a warning.

Spec: [StarOrg Codex v1](../plans/specs/starorg-strings-v1.md), section 4.5.

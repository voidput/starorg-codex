# PATCH.md

Two kinds of patch live here:

1. **Text patches**: the app's Patches feature. A `.patch.toml` file that fixes or adds to a
   string in the game's `global.ini`. You can keep one for yourself or send it upstream as a
   built-in patch.
2. **Code patches**: changes to this repo, sent as a pull request.

Rules for both: no game prose in the repo, no game files, no real player data
([AGENTS.md](AGENTS.md)).

## 1. Text patches

### What a patch is

A TOML file named `<id>.patch.toml` with `schema = 1`. The format and its reasons are in
[docs/decisions/own-patch-format.md](docs/decisions/own-patch-format.md); the parser and engine
are `crates/starorg-core/src/patch.rs`.

Top level:

| Key | Required | Notes |
|---|---|---|
| `schema` | yes | Must be `1` |
| `id` | yes | Letters, digits, `-`, `_`. Must equal the file name without `.patch.toml` |
| `title` | yes | Shown in the Patches screen |
| `description`, `reference` | no | Free text |
| `builds` | no | `game_min`, `game_max` (e.g. `"4.10.0"`, from `Data.Branch`), `change_min`, `change_max` |
| `[[data]]` | no | Changes one field of one record before generators run |
| `[[loc]]` | no | Changes one output string after generators run |

`[[loc]]` entries: `key` (matched without regard to case), `op`, optional `id`, and:

| `op` | Needs | Does |
|---|---|---|
| `set` | `value` | Replaces the value |
| `append` / `prepend` | `value` | Adds text at the end / start |
| `replace` | `find`, `value` | Swaps `find`, which must occur exactly once |
| `append_from` | `from`, plus `requires` or `expect_sha256` | Appends a part of another key (`from_part`: `generated` (default), `stock`, `final`) after `separator` |

Optional on any `[[loc]]` entry: `expect_sha256` (SHA-256 of the stock value; on a mismatch the
entry is skipped as `target-changed`), `requires` (the `id` of a `[[data]]` entry in the same
file), `optional = true` (a key missing from the build is `not-needed`, not a warning).

`fill_name = true` (with `op = "set"` only, no `expect_sha256` or `optional`): a curated item
name where the stock name is missing or a placeholder. Stock lacks the key: the entry replaces
the name *Fix missing item names* added (`no-data` when nothing added it, the switch is off).
Stock holds a placeholder: it sets that line. Stock holds a real name: `not-needed` (the game
names it now, stock wins). The generators read the same names, so a curated base names its
variants and magazine (`apply` and the app; `build` runs no patches). Built-in names of this kind live in `missing-names.patch.toml`,
one entry per key with a comment naming the source of the name. Never guess a name: an item
without a verifiable name keeps the readable fallback.

`name-fixes.patch.toml` (built in) fixes real stock names CIG copied from a sibling item
(`set` with `expect_sha256` of the wrong stock name and `optional`, so a fixed or missing line
retires the entry). The blueprint catalog reads these names and the curated ones too
(`patch::builtin_item_names`), so a Smart Citizen import matches by the fixed name; re-import
with `--replace` to drop links made under the old name.

`[[data]]` entries: `id`, exactly one of `record` (short record name) or `guid`, optional
`contract` (then `record` is the generator), `field`, `expect` (the loc key it has now) and
`set`. `field` is one of `name_key`, `short_name_key`, `desc_key`, `title_key`,
`description_key`.

Strings in TOML: a line break in `global.ini` is the two characters `\n`, so write `"\\n"`.

### How it is checked and applied

- Files over 256 KiB, files that are not UTF-8, unknown keys, a wrong schema, a bad id or a
  missing required key are refused with an error. The other patches still load.
- Every entry gets one outcome and never fails the apply: `applies`, `not-needed` (CIG fixed
  it, or the result is already there), `target-changed`, `nothing-to-add`, `missing`,
  `out-of-range`, `conflict` (two enabled entries write one key), `disabled`, `no-data`.
  `target-changed`, `missing` and `conflict` make the apply "applied with warnings".
- Each patched value goes through the same rules as every other edit
  (`crates/starorg-core/src/validate.rs`). A value that breaks them is dropped with a warning.
- Built-in patches are compiled in (`BUILTIN` in `patch.rs`). User patches load from the app
  data folder:
  - Linux: `$XDG_DATA_HOME/starorg-codex/patches/` (default `~/.local/share/starorg-codex/patches/`)
  - Windows: `%APPDATA%\StarOrg Tools\Codex\data\patches\`
- Switching a patch off is per channel: its id goes into `[patches] disabled` in that
  channel's `profile.toml`. Changing patches marks the channel as needing a re-apply.

### Make one in the app

Patches -> **Add patch** writes a one-entry loc patch (`set`, `append`, `prepend` or `replace`)
into the folder above. **Import patch file** reads a `.patch.toml`; **Export** saves one of
your own patches to a file (built-in ones cannot be exported). For `[[data]]` entries, `append_from` or guards, write the file by hand and
import it or drop it in the folder.

### Try one locally

Worked example. It uses a scratch app folder, so your real settings and the game are not
touched.

```sh
cargo build --locked -p starorg-cli
R=$PWD/.tmp/patch-example
mkdir -p "$R/data/starorg-codex/patches"
cat > "$R/data/starorg-codex/patches/my-first-fix.patch.toml" <<'EOF'
schema = 1
id = "my-first-fix"
title = "Example: add a note to one text key"

[[loc]]
id = "note"
key = "Example_Item_desc"
op = "append"
value = "\\n\\nExample note."
EOF
XDG_CONFIG_HOME=$R/config XDG_DATA_HOME=$R/data XDG_CACHE_HOME=$R/cache \
  target/debug/starorg-codex patches
```

Output: the four built-in patches, then
`my-first-fix  enabled  Example: add a note to one text key`. A file whose name does not match
its id is listed as `error in <file>: file name <file> does not match id <id>`. Add `--json`
for the full list with `errors`.

To see outcomes against a real build, copy the file into your real patch folder and run
`starorg-codex patches --channel LIVE` or `starorg-codex apply --channel LIVE --dry-run`, or
open the Patches screen. `patches` reads only a cached extraction, which an earlier real
`apply` leaves behind (a real `apply` does write `global.ini` into the game install).
`patches` and `apply --dry-run` do not write to the game.

### Guard it

A loc-only built-in patch must guard every entry with `expect_sha256`, or be a `fill_name`
entry, which is guarded by the stock name being missing or a placeholder (test
`builtin_patches_parse`). Hash the exact stock value, the text after `=` on the key's line,
with `\n` as two characters:

```sh
printf '%s' 'Old Name' | sha256sum
```

### Send it upstream as a built-in patch

1. Add `crates/starorg-core/patches/<id>.patch.toml`. Start with a comment saying which build
   you checked it against and how. Keys, record names and our own hint text only: no CIG
   prose.
2. Add it to `BUILTIN` in `crates/starorg-core/src/patch.rs`.
3. Add a test in the `tests` module of `patch.rs`. It runs the patch against a synthetic stock
   file and covers both cases: the guard matches (`applies`) and CIG changed the text
   (`target-changed`). `quest_items_pack_applies_and_skips_changed_text` is the model. A
   minimal one, using the module's helpers:

   ```rust
   #[test]
   fn my_first_fix_applies() {
       let s = snap(); // synthetic stock; has `name_a=Old Name`
       let sha = fsutil::sha256_hex(b"Old Name");
       let set = set_of(&[&format!(
           "{HDR}id = \"my-first-fix\"\n[[loc]]\nid = \"note\"\nkey = \"name_a\"\nop = \"append\"\nvalue = \" (fixed)\"\nexpect_sha256 = \"{sha}\"\n"
       )]);
       let (edits, outcomes, _) = run(&set, &GameData::default(), &s, None);
       assert_eq!(outcomes[0].entry, "loc:note");
       assert_eq!(outcomes[0].state, EntryState::Applies);
       assert_eq!(edits[0].value, "Old Name (fixed)");
   }
   ```

   Run it with `cargo test --locked -p starorg-core --lib patch::`.
4. Bump `OUTPUT_VERSION` in `crates/starorg-core/src/lib.rs` and add a history line: a new
   built-in patch changes the generated text.
5. Commit as `feat(patches): ...` and open a PR (below).

Not sure it is a CIG bug, or no way to write the guard? Open an
[issue](https://github.com/voidput/starorg-codex/issues) with the key and the build instead.

## 2. Code patches

1. Fork `voidput/starorg-codex` and branch from `main`. One topic per branch.
2. Make the change. Read [AGENTS.md](AGENTS.md) for the layout and the rules that are easy to
   miss.
3. Run the gates ([CONTRIBUTING.md](CONTRIBUTING.md#checks); the full CI list is in
   [AGENTS.md](AGENTS.md#gates-what-ci-runs)).
4. Commit with signed Conventional Commits (`fix(app): ...`, `feat: ...`, `docs: ...`). No AI
   attribution trailers.
5. Open a PR against `main` and fill in the template
   ([.github/pull_request_template.md](.github/pull_request_template.md)): what and why, the
   linked issue, how you tested it, and the checklist.

Review: CI must pass (`ci.yml` runs on every PR; `security.yml` too). A maintainer reviews
and merges. Unsigned commits cannot land on `main`, so sign them before you ask for review.

Licence: no CLA or DCO sign-off. By contributing you agree your work is under the
[MIT License](LICENSE) (CONTRIBUTING.md). Security problems go to [SECURITY.md](SECURITY.md),
not a PR or issue.

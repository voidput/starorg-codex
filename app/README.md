# StarOrg Strings app (`starorg-strings-app`)

Tauri 2 shell (`src-tauri/`, a workspace member) with a Svelte 5 + Tailwind 4 +
shadcn-svelte frontend (`src/`). Design tokens: spec section 9.2, in `src/app.css`.

```sh
cd app
npm ci
npm run tauri dev                        # hot-reload window (vite on 127.0.0.1:1420)
npm run tauri build -- --no-bundle       # release binary: <target>/release/starorg-strings-app
npm run check                            # svelte-check
npm test                                 # node --test: apply outcomes, message catalogue
cargo test -p starorg-strings-app        # gpu workaround, contract and IPC command tests (mock runtime)
```

- `--safe-gpu` forces `WEBKIT_DISABLE_DMABUF_RENDERER=1` and `WEBKIT_DISABLE_COMPOSITING_MODE=1`.
- The NVIDIA/WebKitGTK workaround (spec 10.4) lives in `src-tauri/src/gpu.rs`; programs the app
  starts must use `gpu::clean_command` so they get the user's original environment.
- A release `cargo build --workspace` needs `app/dist` (run `npm run build` first); debug builds
  and `cargo test` do not.
- Icons in `src-tauri/icons` are generated from `public/logo.png` (StarOrg Tools logo, 256 px
  source) with `npx tauri icon public/logo.png -o src-tauri/icons`.

## Command contract and mock backend (issue #15)

- **One contract:** `src-tauri/src/contract/` — `types.rs` (every request/response), `mod.rs`
  (`Backend` trait, one `#[tauri::command]` per method, `COMMAND_NAMES`), `mock.rs`
  (`MockBackend`: synthetic, stateful, no disk or network). The app runs the mock behind the
  default `mock-backend` feature until the M1/M2 backends implement `Backend`.
- **TS mirror:** `src/lib/contract/` — `types.ts`, `client.ts` (`api.*`, `ContractError`,
  transport chosen at runtime: Tauri inside the window, the in-browser mock otherwise or with
  `?backend=mock`), `mock.ts` (seeded from `samples.gen.json`).
- **Drift checks:** `UPDATE_CONTRACT_SAMPLES=1 cargo test -p starorg-strings-app` rewrites
  `samples.gen.json` from the Rust mock; `npm run check` type-checks it against `types.ts` in both
  directions (`samples.check.ts`). Rust tests also compare every string union and the command
  list with the TS files.
- **Browser-only dev:** `npm run dev` (or `npm run build && npx vite preview`) runs the whole UI on
  the mock. URL switches: `?wizard=<install|channels|source|enhancements|review|apply>` opens the
  setup wizard at a step, `?state=ready` starts with LIVE applied, HOTFIX behind a game update,
  backups, two overrides and launch-hook history (`&starorg=1` also signs in to StarOrg Tools),
  `?theme=dark|light`, `?delay=<ms>` per mock job step,
  `?screen=<home|enhancements|strings|patches|blueprints|backups|activity|settings>`. States for
  renders take channel names: `&running=LIVE`, `&interrupted=LIVE`, `&foreign=LIVE`,
  `&builderr=LIVE`, `&nobackups=LIVE`; also `&channels=5`, `&installs=0`, `&fail=overview`,
  `&log=empty`, `&accent=amber&palette=black`.
- **Outcomes, not errors:** M1 returns guards and refusals as a report (`skipped` + `reason`,
  `needs-decision`, `up-to-date`, `failed`), so every apply / remove / undo / restore goes through
  `app.write()` and `src/lib/outcome.ts`; `Err(AppError)` is only for calls that could not run.
- **Profile = M2's `enhance::Profile`:** field for field, plus `preset` and `patches_disabled`;
  `profile_schema` gives the labels for the core's line and tag element ids. Rule names are M1's
  `validate::Rule::name()`. `contract::tests::contract_matches_core_names` compares both with the
  core sources (skipped until they are in the workspace, or set `STARORG_CORE_SRC`).
- **Copy:** words for core values (rule names, stale reasons, hook log prefixes) live in
  `src/lib/messages.ts`; the raw value stays in a mono detail line.
- Keyboard: Ctrl/Cmd+K command palette, Alt+1..8 screens, arrows in the sidebar, channel
  switcher and Enhancements categories; Strings grid: arrows, Page Up/Down, Home/End, Enter opens
  a key; Tag Builder: Alt+Left/Right moves a focused part. Escape leaves the wizard (with a guard
  once choices were made) and closes sheets and dialogs.

## Screens (spec 9.4)

Home, Enhancements, Strings, Patches, Blueprints, Backups, Activity, Settings, all on the
contract. Shared pieces in `src/components/common/` (Sheet, ActionDialog, ToneBadge, StringDiff,
SelectField, Stat). Override rule checks run as you type (`src/lib/validate.ts`, spec 7.3); the
backend's `set_override` stays the authority. `cargo test` also checks every theme x palette x
accent in `src/app.css` for 4.5:1 text and 3:1 UI contrast (`src-tauri/src/contrast_tests.rs`).

## Headless checks

Never open a window: run `npm run build && npx vite preview --host 127.0.0.1 --port 4317`, then
with a `playwright-core` and its headless Chromium (`PLAYWRIGHT_CORE=/path/to/playwright-core`,
not a project dependency):

```sh
node scripts/render-screens.cjs   # every screen, dark + light, 1280x800 + 1024x700 -> ../.tmp/renders/m3-screens
node scripts/render-wizard.cjs    # wizard steps -> ../.tmp/renders/m3-wizard
node scripts/keyboard-check.cjs   # grid, sheet, tag builder, dialogs by keyboard
node scripts/keyboard-wizard.cjs  # the wizard end to end by keyboard
```

`RENDER_DIR` and `BASE_URL` override the output folder and server. Render scripts also report
page errors and horizontal overflow.

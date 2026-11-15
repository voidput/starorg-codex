# StarOrg Strings app (`starorg-strings-app`)

Tauri 2 shell (`src-tauri/`, a workspace member) with a Svelte 5 + Tailwind 4 +
shadcn-svelte frontend (`src/`). Design tokens: spec section 9.2, in `src/app.css`.

```sh
cd app
npm ci
npm run tauri dev                        # hot-reload window (vite on 127.0.0.1:1420)
npm run tauri build -- --no-bundle       # release binary: <target>/release/starorg-strings-app
npm run check                            # svelte-check
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
  setup wizard at a step, `?state=ready` starts with LIVE and HOTFIX set up, `?theme=dark|light`,
  `?delay=<ms>` per mock job step, `?screen=<id>`.
- Keyboard: Ctrl/Cmd+K command palette, Alt+1..7 screens, arrows in the sidebar and channel
  switcher, Escape leaves the wizard (with a guard once choices were made).

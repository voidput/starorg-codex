# StarOrg Strings app (`starorg-strings-app`)

Tauri 2 shell (`src-tauri/`, a workspace member) with a Svelte 5 + Tailwind 4 +
shadcn-svelte frontend (`src/`). Design tokens: spec section 9.2, in `src/app.css`.

```sh
cd app
npm ci
npm run tauri dev                        # hot-reload window (vite on 127.0.0.1:1420)
npm run tauri build -- --no-bundle       # release binary: <target>/release/starorg-strings-app
npm run check                            # svelte-check
cargo test -p starorg-strings-app        # gpu workaround + IPC command tests (mock runtime)
```

- `--safe-gpu` forces `WEBKIT_DISABLE_DMABUF_RENDERER=1` and `WEBKIT_DISABLE_COMPOSITING_MODE=1`.
- The NVIDIA/WebKitGTK workaround (spec 10.4) lives in `src-tauri/src/gpu.rs`; programs the app
  starts must use `gpu::clean_command` so they get the user's original environment.
- A release `cargo build --workspace` needs `app/dist` (run `npm run build` first); debug builds
  and `cargo test` do not.
- Icons in `src-tauri/icons` are generated from `public/logo.png` (StarOrg Tools logo, 256 px
  source) with `npx tauri icon public/logo.png -o src-tauri/icons`.

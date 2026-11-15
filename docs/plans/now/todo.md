---
title: Now
tags: [docs, plans, now]
updated: 2026-11-14
---

# Now

M0: skeleton and decisions — [[specs/starorg-strings-v1]] ([plain](../specs/starorg-strings-v1.md))

- [x] Cargo workspace, pinned toolchain, release profile (panic=unwind) ([#1](https://github.com/voidput/starorg-strings/issues/1))
- [x] StarBreaker mirror fork + pinned git dependency + adapter crate ([#2](https://github.com/voidput/starorg-strings/issues/2))
- [x] starorg-data worker with central-directory entry-count check (eocd.rs) ([#3](https://github.com/voidput/starorg-strings/issues/3))
- [x] INI reader/writer (byte-identical round trip) ([#4](https://github.com/voidput/starorg-strings/issues/4))
- [x] GameSource trait + install discovery (LUG prefix, Windows defaults) ([#5](https://github.com/voidput/starorg-strings/issues/5))
- [x] CI on Ubuntu + Windows (cargo test --workspace --locked) ([#6](https://github.com/voidput/starorg-strings/issues/6))
- [ ] release-please + throwaway v0.0.1 through the build matrix ([#7](https://github.com/voidput/starorg-strings/issues/7))
- [x] Tauri 2 + Svelte 5 app shell with StarOrg/RSI+ design tokens ([#8](https://github.com/voidput/starorg-strings/issues/8))
- [ ] decide release location (Q1) and Windows tester (Q19) ([#9](https://github.com/voidput/starorg-strings/issues/9))

M1 + M2 (merged into main; verification fixes 2026-11-14, local only, not pushed)

- [x] Shared `user.cfg` ledger race: re-read under the cfg lock, short wait on contention ([#10](https://github.com/voidput/starorg-strings/issues/10))
- [x] Built-in patches `hockrow-p2m4-desc`, `hockrow-recoveritem-desc` with data entries ([#14](https://github.com/voidput/starorg-strings/issues/14))
- [x] Ambiguous owned names never marked ([#17](https://github.com/voidput/starorg-strings/issues/17))
- [x] Hook block: no-final-newline round trip, continued `exit`, LF block in CRLF scripts ([#11](https://github.com/voidput/starorg-strings/issues/11))
- [x] One validator (`validate.rs`), BOM rule unconditional, test hooks only behind the `test-hooks` feature ([#10](https://github.com/voidput/starorg-strings/issues/10))
- [ ] M1/M2 in-game checks (spec 13): owner plays a session with an applied LIVE file ([#10](https://github.com/voidput/starorg-strings/issues/10), [#14](https://github.com/voidput/starorg-strings/issues/14))
- [ ] Push main and get the Ubuntu + Windows CI apply tests green ([#10](https://github.com/voidput/starorg-strings/issues/10))
- [ ] Owner: replace the Smart Citizen block in the real `sc-prelaunch.sh` with `hook install` (agents never edit the real prefix) ([#11](https://github.com/voidput/starorg-strings/issues/11))
- [ ] Project board: move #10, #11, #13, #14, #17 to match (needs a push first)

## What and why

<!-- What does this change, and why? Link the issue: Fixes #123 -->

## How it was tested

<!-- Commands run, screens checked, platforms. -->

## Checklist

- [ ] Commits are signed and follow Conventional Commits (CONTRIBUTING.md)
- [ ] `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked`
- [ ] `app/`: `npm run check`, `npm test`, `npm run build` (if the app changed)
- [ ] Contract samples regenerated (`UPDATE_CONTRACT_SAMPLES=1 cargo test -p starorg-strings-app`) if the contract changed
- [ ] `THIRD_PARTY_NOTICES.md` regenerated (`scripts/gen-notices.sh`) if a lock file changed
- [ ] Docs updated (`docs/`, linked from `docs/Home.md` and `docs/README.md`)
- [ ] No game files, extracted game data or secrets

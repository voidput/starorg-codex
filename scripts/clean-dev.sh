#!/usr/bin/env bash
# Frees disk space taken by builds and scratch while developing. Shows what it would remove
# unless --yes is given. Never touches the game folder, the app's user data or its cache.
#   scripts/clean-dev.sh               show what would go
#   scripts/clean-dev.sh --yes         remove it
#   DAYS=3 scripts/clean-dev.sh --yes  build output unused for 3 days (default 7)
# Removes:
#   - build output in target/ not used for DAYS days (cargo sweep when installed, else only
#     old incremental data)
#   - target/ folders inside agent worktrees (.claude/worktrees/*/target): worktrees should
#     build into the shared target/ (CARGO_TARGET_DIR), see CONTRIBUTING.md
#   - .tmp/ and tmp/ scratch (git-ignored)
#   - git worktrees whose folder is gone (git worktree prune)
set -euo pipefail
cd "$(dirname "$0")/.."

days=${DAYS:-7}
yes=false
[ "${1:-}" = --yes ] && yes=true

size() { du -sh "$@" 2>/dev/null | awk '{print $1"\t"$2}'; }
gone() {
  if $yes; then rm -rf -- "$@"; fi
}

echo "Disk use before:"
size target .tmp tmp .claude/worktrees || true

if cargo sweep --version >/dev/null 2>&1; then
  if $yes; then cargo sweep --time "$days"; else cargo sweep --dry-run --time "$days"; fi
else
  # Without cargo-sweep only incremental data goes: an old file in deps/ or build/ can still
  # belong to a fresh build (cargo would then fail to link), incremental data never does.
  echo "cargo-sweep not installed (cargo install cargo-sweep --locked): incremental data only"
  for d in target/*/incremental; do
    [ -d "$d" ] || continue
    n=$(find "$d" -mindepth 1 -maxdepth 1 -mtime +"$days" | wc -l)
    echo "$d: $n entries older than $days days"
    if $yes; then find "$d" -mindepth 1 -maxdepth 1 -mtime +"$days" -exec rm -rf -- {} +; fi
  done
fi

for t in .claude/worktrees/*/target; do
  [ -d "$t" ] || continue
  size "$t"
  gone "$t"
done
for d in .tmp tmp; do
  [ -d "$d" ] || continue
  size "$d"
  gone "$d"
done
if $yes; then git worktree prune; fi

if $yes; then
  echo "Disk use after:"
  size target .claude/worktrees || true
else
  echo "Nothing removed. Run with --yes to remove the above."
fi

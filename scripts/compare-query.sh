#!/usr/bin/env bash
# Answer the same queries with sc-gamedata's own query code (its compiled dist/query/, run by
# node) and with the Rust port (`gamedata-port query`), on the same datasets, and compare.
# Exits non-zero when any answer differs.
#
#   scripts/compare-query.sh [--sample N]
#
# Reads sc-gamedata's dist/ and data/ only (SC_GAMEDATA, default ~/git/sc-gamedata).
# Writes queries and answers to .tmp/query-golden/ in this repo.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
sample=60
while [ $# -gt 0 ]; do
  case "$1" in
    --sample) sample=$2; shift 2 ;;
    -h|--help) sed -n '2,9p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
src=${SC_GAMEDATA:-$HOME/git/sc-gamedata}
[ -d "$src/dist/query" ] || { echo "no $src/dist/query (build sc-gamedata first)" >&2; exit 2; }
out="$root/.tmp/query-golden"
mkdir -p "$out"
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$root/.tmp/target-port}
cargo build --quiet --release --locked --manifest-path "$root/Cargo.toml" -p starorg-gamedata --bin gamedata-port

node "$root/scripts/query-golden.mjs" --data "$src/data" --sample "$sample" > "$out/queries.json"
node "$root/scripts/query-golden.mjs" --dist "$src/dist" --data "$src/data" --queries "$out/queries.json" > "$out/golden.json"
"$CARGO_TARGET_DIR/release/gamedata-port" query --data "$src/data" --queries "$out/queries.json" > "$out/ours.json"
node "$root/scripts/query-golden.mjs" --compare "$out/ours.json" "$out/golden.json"

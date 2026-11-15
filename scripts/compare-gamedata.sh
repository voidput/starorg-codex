#!/usr/bin/env bash
# Extract the ported sc-gamedata domains from a game install and diff them against
# sc-gamedata's golden dumps. Exits non-zero when any dataset differs.
#
#   scripts/compare-gamedata.sh [--channel LIVE] [--p4k <Data.p4k>] [--domain <name>]...
#
# Reads the archive and the golden directory only. Writes to .tmp/gamedata/<channel>/ in
# this repo (wiped first) and builds into $CARGO_TARGET_DIR (default .tmp/target-port).
#   GOLDEN_DIR  golden dumps (default ~/git/sc-gamedata/data, build 4.10.200.58746 = LIVE)
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
channel=LIVE
p4k=()
domains=()
while [ $# -gt 0 ]; do
  case "$1" in
    --channel) channel=$2; shift 2 ;;
    --p4k) p4k=(--p4k "$2"); shift 2 ;;
    --domain) domains+=(--domain "$2"); shift 2 ;;
    -h|--help) sed -n '2,10p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

golden=${GOLDEN_DIR:-$HOME/git/sc-gamedata/data}
[ -d "$golden" ] || { echo "no golden directory at $golden (set GOLDEN_DIR)" >&2; exit 2; }
out="$root/.tmp/gamedata/$channel"
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$root/.tmp/target-port}

cargo build --quiet --release --locked --manifest-path "$root/Cargo.toml" -p starorg-gamedata --bin gamedata-port
bin="$CARGO_TARGET_DIR/release/gamedata-port"

rm -rf "$out"
"$bin" extract --channel "$channel" "${p4k[@]}" --out "$out" "${domains[@]}"
"$bin" compare "$out" "$golden" "${domains[@]}"

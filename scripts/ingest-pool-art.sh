#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
agni="$(cd "$here/../../../agni/agni" && pwd)"
pool="$agni/games/riftbound/rules/pool"

usage() {
  cat <<USAGE
usage: $(basename "$0") <store-dir> [--audit] [--ids <file|->] [--only <id>...] [pool.md ...]

Audits a spirit blob store for the art of every card the Riftbound pool
files name and fetches what is missing from Riftcodex (agni's User-Agent,
one request per second, journaled into <store-dir>/riftbound-images and
merged into the store's riftbound manifest). With no pool files, no --ids
and no --only, every file under
  $pool
is read. The store is never defaulted: name it every time.

  --audit      list the missing ids, fetch nothing, write nothing
  --ids F      one riftbound id per line (# comments), - for stdin
  --only ID..  ids on the command line

This is agni's ingest-riftbound bin (importers/src/bin/ingest_riftbound.rs)
run from the agni workspace with --pool for each file.
USAGE
}

if [ $# -eq 0 ] || [ "$1" = "-h" ] || [ "$1" = "--help" ]; then
  usage
  exit 2
fi

store="$1"
shift
case "$store" in
  /*) ;;
  ~*) store="${store/#\~/$HOME}" ;;
  *) store="$(pwd)/$store" ;;
esac

args=()
have_sources=0
mode=pool
for arg in "$@"; do
  case "$mode" in
    value) args+=("$arg"); mode=pool; continue ;;
    only) case "$arg" in -*) mode=pool ;; *) args+=("$arg"); continue ;; esac ;;
  esac
  case "$arg" in
    --ids) args+=("$arg"); have_sources=1; mode=value ;;
    --only) args+=("$arg"); have_sources=1; mode=only ;;
    -*) args+=("$arg") ;;
    *) args+=(--pool "$arg"); have_sources=1 ;;
  esac
done

if [ "$have_sources" -eq 0 ]; then
  for file in "$pool"/*.md; do
    args+=(--pool "$file")
  done
fi

cd "$agni"
exec cargo run --release -p agni-importers --features riftbound-native --bin ingest-riftbound -- "$store" "${args[@]}"

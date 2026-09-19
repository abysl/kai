#!/usr/bin/env bash
set -euo pipefail

run_dir="${1:?usage: web.sh <run-dir> <plan-json>}"
plan="${2:?usage: web.sh <run-dir> <plan-json>}"

mkdir -p "$run_dir"
run_dir="$(cd "$run_dir" && pwd)"

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
kai="$(cd "$here/../../.." && pwd)"
web_tests="$kai/tests/web"
dist="${KAI_WEB_DIST:-$kai/web/dist}"

fail() {
  printf 'web driver: %s\n' "$*" >&2
  mkdir -p "$run_dir"
  printf '2\n' >"$run_dir/exit"
  exit 2
}

http_answers() {
  python3 -c 'import sys, urllib.request
try:
    urllib.request.urlopen(sys.argv[1], timeout=10)
except urllib.error.HTTPError:
    pass
except Exception:
    sys.exit(1)' "$1"
}

check_bundle() {
  [ -f "$dist/hand.js" ] || fail "$dist/hand.js is missing: run devenv shell -- web-build"
  grep -q kai_autoplay "$dist/hand.js" || fail "$dist/hand.js has no kai_autoplay export: rebuild with devenv shell -- web-build"
  [ -f "$dist/engine.wasm" ] || fail "$dist/engine.wasm is missing: run devenv shell -- web-build"
}

check_node_modules() {
  [ -d "$web_tests/node_modules/@playwright/test" ] && return
  (cd "$web_tests" && npm ci --no-audit --no-fund --ignore-scripts >"$run_dir/npm.log" 2>&1) || fail "npm ci failed, see $run_dir/npm.log"
}

note_gateway() {
  local role
  role="$(printf '%s' "$plan" | python3 -c 'import json,sys; r=json.load(sys.stdin)["role"]; print("host" if r=="host" else "join")')"
  [ "$role" = host ] || return 0
  http_answers "${GATEWAY_PROBE_URL:-http://127.0.0.1:8787/gateway/status}" \
    || printf 'web driver: no gateway answers; the browser host serves its bundled plugin and joiners take modules from the host\n' >&2
}

main() {
  mkdir -p "$run_dir"
  check_bundle
  check_node_modules
  note_gateway
  source "$web_tests/env.sh"
  export KAI_PLAN="$plan"
  export KAI_RUN_DIR="$run_dir"
  export KAI_WEB_DIST="$dist"
  unset KAI_WEB_MODE
  cd "$web_tests"
  npx playwright test autoplay.spec.ts >"$run_dir/stdout.log" 2>"$run_dir/stderr.log" &
  local pid=$!
  printf '%s\n' "$pid" >"$run_dir/pid"
  trap 'kill "$pid" 2>/dev/null || true' TERM INT
  local status=0
  wait "$pid" || status=$?
  printf '%s\n' "$status" >"$run_dir/exit"
  exit "$status"
}

main

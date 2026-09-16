#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

usage() {
  cat <<USAGE
usage: tests/connectivity/run.sh [options]

  --only a:b[,c:d]     run these pairs (default: every pair in matrix.json except the extras)
  --skip a:b[,c:d]     leave these pairs out
  --seed N             host seed (joiner gets N+1); default 7
  --until U            winner | seated | turns:N; default winner
  --timeout S          plan timeout_s for both sides; default 600 (--timeout-s too)
  --out DIR            run directory; default \${CARGO_TARGET_DIR:-target}/connectivity/<stamp>
  --report PATH        where report.json goes; default <out>/report.json
  --keep               keep the scratch stores (logs are always kept)
  --allow-rejoin       a mid-game rejoin is a pass with a note, not a fail
  --list               print the matrix and exit

environment: KAI_BIN, KAI_WEB_DIST, KAI_APK, KAI_DISPLAY, KAI_WEB_URL, CARGO_TARGET_DIR
USAGE
}

ONLY=""
SKIP=""
SEED=7
UNTIL=winner
TIMEOUT_S=600
OUT=""
REPORT=""
KEEP=0
ALLOW_REJOIN=0

parse_args() {
  while (( $# )); do
    case $1 in
      --only) ONLY=$2; shift 2 ;;
      --only=*) ONLY=${1#*=}; shift ;;
      --skip) SKIP=$2; shift 2 ;;
      --skip=*) SKIP=${1#*=}; shift ;;
      --seed) SEED=$2; shift 2 ;;
      --seed=*) SEED=${1#*=}; shift ;;
      --until) UNTIL=$2; shift 2 ;;
      --until=*) UNTIL=${1#*=}; shift ;;
      --timeout|--timeout-s) TIMEOUT_S=$2; shift 2 ;;
      --timeout=*|--timeout-s=*) TIMEOUT_S=${1#*=}; shift ;;
      --out) OUT=$2; shift 2 ;;
      --out=*) OUT=${1#*=}; shift ;;
      --report) REPORT=$2; shift 2 ;;
      --report=*) REPORT=${1#*=}; shift ;;
      --keep) KEEP=1; shift ;;
      --allow-rejoin) ALLOW_REJOIN=1; shift ;;
      --list) python3 "$CONNECTIVITY_DIR/matrix.py" ids | tr ' ' '\n'; exit 0 ;;
      -h|--help) usage; exit 0 ;;
      *) usage >&2; die "unknown argument: $1" ;;
    esac
  done
  [[ -n $OUT ]] || OUT=${CARGO_TARGET_DIR:-$KAI_ROOT/target}/connectivity/$(date +%Y%m%d-%H%M%S)
  [[ -n $REPORT ]] || REPORT=$OUT/report.json
  mkdir -p "$(dirname "$REPORT")"
  REPORT=$(cd "$(dirname "$REPORT")" && pwd)/$(basename "$REPORT")
  export KAI_TIMEOUT_S=$TIMEOUT_S
}

selected_pairs() {
  local all pair skip list
  all=$(python3 "$CONNECTIVITY_DIR/matrix.py" ids)
  list=${ONLY//,/ }
  [[ -n $list ]] || list=$(python3 "$CONNECTIVITY_DIR/matrix.py" default)
  for pair in $list; do
    [[ " $all " == *" $pair "* ]] || die "unknown pair $pair (see --list)"
    skip=0
    [[ " ${SKIP//,/ } " == *" $pair "* ]] && skip=1
    (( skip )) || echo "$pair"
  done | awk '!seen[$0]++'
}

DISPLAY_READY=0
CURRENT_DIRS=()
cleanup() {
  local dir
  for dir in "${CURRENT_DIRS[@]}"; do
    stop_driver "$dir"
  done
  stop_web_server
  stop_xvfb
}

write_meta() {
  local pair_dir=$1
  shift
  python3 - "$pair_dir/meta.json" "$@" <<'PY'
import json, sys
path = sys.argv[1]
try:
    meta = json.load(open(path))
except (OSError, ValueError):
    meta = {}
for pair in sys.argv[2:]:
    key, value = pair.split("=", 1)
    try:
        meta[key] = json.loads(value)
    except ValueError:
        meta[key] = value
json.dump(meta, open(path, "w"), indent=1)
PY
}

jstr() {
  python3 -c 'import json,sys; sys.stdout.write(json.dumps(sys.argv[1]))' "$1"
}

make_plan() {
  python3 "$CONNECTIVITY_DIR/plan.py" "$@"
}

involves() {
  [[ $1 == "$3" || $2 == "$3" ]]
}

prune_stores() {
  local dir
  (( KEEP )) && return 0
  for dir in "$@"; do
    rm -rf "$dir/store" "$dir/xdg" "$dir/avd"
  done
}

run_pair() {
  local id=$1 host joiner needs offline spec pair_dir host_dir joiner_dir reason
  local host_plan joiner_plan ticket started error=""
  read -r host joiner needs offline spec < <(python3 "$CONNECTIVITY_DIR/matrix.py" show "$id")
  pair_dir=$OUT/${id//:/_}
  host_dir=$pair_dir/host
  joiner_dir=$pair_dir/joiner
  rm -rf "$pair_dir"
  mkdir -p "$pair_dir"
  write_meta "$pair_dir" "id=$(jstr "$id")" "host_platform=$(jstr "$host")" "joiner_platform=$(jstr "$joiner")" \
    "seeds=[$SEED,$((SEED + 1))]" "until=$(jstr "$UNTIL")" "enforced=true" "players=2" \
    "allow_rejoin=$([[ $ALLOW_REJOIN == 1 ]] && echo true || echo false)" \
    "build=$(build_identity)" "started_at=$(jstr "$(now_iso)")"
  log "== $id (host $host, joiner $joiner)"
  reason=$(preflight "$host" "$joiner" "$needs")
  write_meta "$pair_dir" "preflight=$(jstr "${reason:-ok}")"
  if [[ -n $reason ]]; then
    write_meta "$pair_dir" "skipped=$(jstr "$reason")" "seconds=0"
    python3 "$CONNECTIVITY_DIR/assess.py" "$pair_dir" "$pair_dir/meta.json" | tee -a "$OUT/summary.txt"
    return 0
  fi
  if involves "$host" "$joiner" desktop && (( ! DISPLAY_READY )); then
    start_xvfb "$OUT"
    DISPLAY_READY=1
  fi
  if involves "$host" "$joiner" web && [[ -z $WEB_SERVER_PID && -z ${KAI_WEB_URL:-} ]]; then
    start_web_server "$OUT"
  fi
  if [[ $offline == offline ]]; then
    export KAI_DEFAULT_PEERS=none
  else
    unset KAI_DEFAULT_PEERS
  fi
  started=$(now_ms)
  host_plan=$(make_plan --host --seed "$SEED" --name "$id-host" --until "$UNTIL" --timeout-s "$TIMEOUT_S")
  if [[ $spec == web-lease ]]; then
    error=$(run_lease "$pair_dir") || true
  else
    CURRENT_DIRS=("$host_dir" "$joiner_dir")
    start_driver host "$host" "$host_dir" "$host_plan"
    log "host $host started (driver pid $(cat "$host_dir/driver.pid")), waiting for hosting"
    if error=$(wait_event "$host_dir" hosting "$HOSTING_WAIT_S"); then
      ticket=$(event_field "$host_dir" hosting ticket)
      write_meta "$pair_dir" "ticket=$(jstr "$ticket")"
      log "host is up: node $(event_field "$host_dir" hosting node)"
      joiner_plan=$(make_plan --join "$ticket" --seed "$((SEED + 1))" --name "$id-joiner" --until "$UNTIL" --timeout-s "$TIMEOUT_S")
      write_meta "$pair_dir" "plans={\"host\":$host_plan,\"joiner\":$joiner_plan}"
      start_driver joiner "$joiner" "$joiner_dir" "$joiner_plan"
      log "joiner $joiner started (driver pid $(cat "$joiner_dir/driver.pid")), waiting for outcomes"
      if error=$(wait_outcomes "$host_dir" "$joiner_dir" "$((TIMEOUT_S + 30))"); then
        let_drivers_exit "$host_dir" "$joiner_dir"
      fi
    else
      write_meta "$pair_dir" "plans={\"host\":$host_plan}"
    fi
    stop_driver "$joiner_dir"
    stop_driver "$host_dir"
    CURRENT_DIRS=()
  fi
  write_meta "$pair_dir" "ended_at=$(jstr "$(now_iso)")" "seconds=$(( ($(now_ms) - started) / 1000 ))" "error=$(jstr "$error")"
  prune_stores "$host_dir" "$joiner_dir"
  python3 "$CONNECTIVITY_DIR/assess.py" "$pair_dir" "$pair_dir/meta.json" | tee -a "$OUT/summary.txt" || true
}

run_lease() {
  local pair_dir=$1 spec_dir=$KAI_ROOT/tests/web
  [[ -f $spec_dir/web-lease.spec.ts ]] || { echo "tests/web/web-lease.spec.ts is missing"; return 1; }
  mkdir -p "$pair_dir/host" "$pair_dir/joiner"
  (
    cd "$spec_dir"
    if [[ -f env.sh ]]; then
      source env.sh
    fi
    export KAI_RUN_DIR=$pair_dir
    export KAI_SEED=$SEED
    export KAI_UNTIL=$UNTIL
    export KAI_TIMEOUT_S
    unset KAI_PLAN KAI_WEB_MODE
    npx playwright test web-lease.spec.ts
  ) >"$pair_dir/driver.log" 2>&1 || echo "web-lease spec exited $? (see driver.log)"
}

main() {
  parse_args "$@"
  local pairs pair status=0
  pairs=$(selected_pairs)
  [[ -n $pairs ]] || die "no pairs selected"
  mkdir -p "$OUT"
  : >"$OUT/summary.txt"
  log "run dir $OUT"
  log "pairs: $(echo "$pairs" | tr '\n' ' ')"
  trap cleanup EXIT
  for pair in $pairs; do
    run_pair "$pair"
  done
  cleanup
  trap - EXIT
  echo
  echo "connectivity summary ($OUT)"
  cat "$OUT/summary.txt"
  "$CONNECTIVITY_DIR/report.sh" "$OUT" "$REPORT" || status=1
  exit "$status"
}

main "$@"

#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

plan='{"role":"host","brain":{"random":{"seed":7}},"until":"winner","timeout_s":600,"name":"paths"}'
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

resolved_run_dir_is_independent_of_driver_cwd() {
  local driver=$1 relative=$2
  mkdir -p "$work/$driver"
  (
    cd "$work/$driver"
    KAI_DRIVER_RESOLVE_ONLY=1 bash "$here/$driver.sh" "$relative" "$plan"
  ) >"$work/$driver/printed"
  [[ "$(cat "$work/$driver/printed")" == "$work/$driver/$relative" ]]
  [[ -d "$work/$driver/$relative" ]]
}

for driver in desktop web android; do
  resolved_run_dir_is_independent_of_driver_cwd "$driver" "target/connectivity/$driver-demo"
done

echo "driver run-dir resolution: ok"

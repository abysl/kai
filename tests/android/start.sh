#!/usr/bin/env bash
set -euo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/emulator.sh"
[[ $# -ge 2 ]] || die "usage: start.sh <serial> <plan-json>"
app_start "$1" "$2"

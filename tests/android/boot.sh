#!/usr/bin/env bash
set -euo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/emulator.sh"
[[ $# -ge 2 ]] || die "usage: boot.sh <avd-name> <port> [run-dir]"
emu_boot "$1" "$2" "${3:-$PWD}"

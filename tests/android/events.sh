#!/usr/bin/env bash
set -euo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/emulator.sh"
[[ $# -ge 2 ]] || die "usage: events.sh <serial> <run-dir>"
events_stream "$1" "$2"
echo "streaming $KAI_ANDROID_LOG_TAG from $1 into $2/events.jsonl (pid $(cat "$2/events.pid"))"

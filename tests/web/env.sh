#!/usr/bin/env bash
set -euo pipefail

kai_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

pinned_nixpkgs_rev() {
  node -e 'const fs = require("fs");
const lock = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
const rev = lock.nodes["nixpkgs-src"].locked.rev;
if (!/^[0-9a-f]{40}$/.test(rev)) throw new Error("devenv.lock has no nixpkgs-src revision");
process.stdout.write(rev);' "$kai_root/devenv.lock"
}

playwright_browsers() {
  if [ -n "${PLAYWRIGHT_BROWSERS_PATH:-}" ]; then
    printf '%s\n' "$PLAYWRIGHT_BROWSERS_PATH"
    return
  fi
  nix build "github:NixOS/nixpkgs/$(pinned_nixpkgs_rev)#playwright-driver.browsers" \
    --no-link --print-out-paths
}

PLAYWRIGHT_BROWSERS_PATH="$(playwright_browsers)"
export PLAYWRIGHT_BROWSERS_PATH
export PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS=true
export PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1

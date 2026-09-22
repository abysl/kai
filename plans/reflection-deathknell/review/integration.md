# LeBlanc Reflection / Deathknell integration

Kai 0.21.2 pins Agni 6fad1183, containing the fix and regression tests in
[abysl/agni#1](https://github.com/abysl/agni/pull/1). Merge that rules PR first.

## Behavior

LeBlanc's copy of Honest Broker and the original now each create one Gold when
both die in combat. Temporary kills the Reflection at its controller's
Beginning Phase before scoring, so the copied Deathknell also creates Gold.
Printed triggered and activated abilities survive their token source leaving
play; copied statics apply immediately. Rules, not Kai rendering code, own
this behavior.

## Changes

- Update all existing Agni Git pins together to the reviewed fix commit.
- Bump the application patch version from 0.21.1 to 0.21.2.
- Leave unrelated registry dependencies, the framework wire protocol, and UI
  unchanged.

Kai still builds the rules copy in Agni, not the extracted agni-rfb repository.
The new plugin is 0.9.1 with blob version 16. Existing supported states decode,
but old snapshots cannot reconstruct missing ability identity for a token
that had already disappeared. Existing matches retain their pinned module.

## Verification

- `bash ci/check.sh` passed with Python and native build dependencies supplied
  by a Nix shell: ten dependency-free tests, connectivity-driver path checks,
  and all-target headless compile against the exact updated Cargo.lock.
- Repository `treefmt --ci` passed.
- Agni's ten serialized Reflection regressions, full rules unit suite,
  compatibility tests, plugin tests, fast checks, portable release build, and
  hardening passed. See its PR for counts and an unrelated pre-existing broad
  replay fixture failure.
- No manual GUI/network match, full Kai application test run, or Android build
  was performed for this dependency-only application update.

## Release

Align the sibling Agni checkout using `ci/agni-revision.py` and rebuild/harden
bundled engine/game modules with the existing module-build helpers. No
compiled module or artwork is committed or published by this change. If the
Agni PR is squash-merged, update this lockfile to its final main commit before
merging/releasing Kai.

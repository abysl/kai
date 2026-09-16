# Developing Kai

Audience: programmers who know basic Git and terminal use but have not worked
on this project. Commands below start at the repository root after checkout.

## Requirements and checkout

The most direct development path is Linux. Install Git, Rust 1.98.1, Python 3.11
or later, and the native libraries Bevy needs. On Ubuntu 24.04:

```sh
sudo apt-get install clang mold pkg-config libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev libx11-dev libxi-dev libxrandr-dev libxcursor-dev libvulkan-dev libssl-dev
git clone https://github.com/abysl/kai.git
git clone https://github.com/abysl/agni.git
cd kai
```

Rust can be installed with rustup; select 1.98.1 with
`rustup toolchain install 1.98.1` and `rustup override set 1.98.1`.
The Nix/devenv environment is an alternative on supported systems, not a
requirement for the compile check.

## Related repositories

Keep `kai/` and `agni/` next to each other. Kai currently includes some pool
data from `../agni` at compile time. Its module-build helpers also read that
checkout. This is a remaining extraction dependency, not a Cargo feature.

Use the Agni revision recorded in Kai's lockfile:

```sh
git -C ../agni fetch origin
git -C ../agni checkout --detach "$(python3 ci/agni-revision.py)"
```

Do this only in the dependency checkout created above, not a worktree containing
your own uncommitted work. Cargo fetches Rust dependencies separately from their
public Git repositories; cloning a sibling does not override those dependencies.

## Build and run

A compile-only check does not require a display or downloaded art:

```sh
bash ci/check.sh
```

To run the application, install Nix and devenv, then from `kai/`:

```sh
devenv shell -- run
```

This helper builds the engine and game modules from `../agni`, then runs Kai.
The application needs a working graphical session and graphics driver.
The first build is substantially slower than the PR compile check.

`devenv shell -- dev` enables dynamic linking for faster iteration. Use
`devenv shell -- web-build` followed by `devenv shell -- web-serve` for the
browser build; the local server listens on port 8123.
Android has a separate environment under `android/`.

## Optional service configuration

Kai includes two public content-peer endpoint IDs so a fresh client can discover
content. These are public network identities, not server addresses or credentials.
On native builds, `KAI_DEFAULT_PEERS` replaces those defaults with comma-separated
endpoint tickets or IDs; set it to `none` to disable automatic seeding.
Join a multiplayer table through its invitation.

The [public browser app](https://kai.rae.blue) loads content through its
same-origin `/gateway/` API. Other browser hosts can provide the same API at
their own origin. Public runtime services belong in user-facing documentation;
private hostnames, credentials, and deployment configuration do not.

There is no default log collector. Desktop log shipping needs both
`KAI_INGEST_URL` and `KAI_INGEST_TOKEN`, or
equivalent `url` and `token` fields in the local telemetry configuration.
Android configuration and browser-origin defaults likewise need both values.
A token alone no longer enables shipping. Do not commit these values.

## Checks

The PR check is `bash ci/check.sh`: it runs the dependency-free network-default
tests, then compiles the library, binaries, and test targets with the `headless`
feature. It does not launch the GUI, execute the full application test suite,
build Android/WebAssembly, or run network playtests.
Despite its name, `headless` enables the command-line seat but does not remove
Bevy from the dependency graph.

For executed tests, use `cargo test --locked --lib` on a machine with the
native build dependencies. For transport changes, also use the
[connectivity harness](../tests/connectivity/README.md). A skipped connectivity
case is not a passing test.

## Formatting

treefmt runs the configured language formatters for this repository. With Nix
installed, the pinned environment supplies treefmt, rustfmt, taplo, and alejandra:

```sh
nix-shell ci/format.nix --run treefmt
nix-shell ci/format.nix --run 'treefmt --ci'
```

The first command applies formatting; the second fails if formatting changes
are needed. You can also install those tools yourself and run `treefmt`
directly. Rust, TOML, and Nix are covered; prose is reviewed for clarity.

## What GitHub checks

The `PR checks` workflow runs on pull requests targeting `main`, pushes to
`main`, and merge-queue events. Its `treefmt` and `fast-check` jobs feed the
single `pr-gate` result.

Rust dependency/build caches are reused; only pushes to `main` save shared
caches. A cold run still needs to fetch and compile dependencies.
The workflow uses read-only repository permissions and does not publish
packages or deploy applications.

Repository administrators must require `pr-gate` in the protection rule or
ruleset for `main` to prevent merging a failed check. A workflow file alone
does not enforce that rule.

## Common failures

If `--locked` refuses to proceed, a manifest and Cargo.lock disagree. Update
the lockfile intentionally, inspect the dependency changes, and commit it.
Do not remove `--locked` from CI to hide the mismatch.

A missing formatter means its executable is not on PATH; use the pinned Nix
environment. A native linker/pkg-config error usually means a required system
library or build tool is missing, not that a Rust test failed.

Run commands from the repository root unless a guide explicitly says otherwise.

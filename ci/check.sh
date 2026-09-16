set -euo pipefail

mkdir -p target/ci
rustc --edition 2021 --test src/net/defaults.rs -o target/ci/defaults-tests
env -u KAI_DEFAULT_PEERS target/ci/defaults-tests
rustc --edition 2021 --test src/table/undo_batch.rs -o target/ci/undo-tests
target/ci/undo-tests
rustc --edition 2021 --test src/engine/selection.rs -o target/ci/engine-selection-tests
target/ci/engine-selection-tests
cargo check --locked --all-targets --features headless

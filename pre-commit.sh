#!/usr/bin/env bash

set -euo pipefail

echo
echo 'Running all tests with Valgrind...'
host_target="$(rustc -vV | sed -n 's/^host: //p')"
runner_env="CARGO_TARGET_${host_target^^}_RUNNER"
runner_env="${runner_env//-/_}"
valgrind_runner='valgrind --quiet --error-exitcode=1 --leak-check=full --show-leak-kinds=definite,indirect --errors-for-leak-kinds=definite,indirect'
env "$runner_env=$valgrind_runner" \
    cargo test --all-features --bins --examples --lib --quiet --tests --workspace


echo 'Running doc-tests with Valgrind...'
env "$runner_env=$valgrind_runner" \
    cargo test --all-features --doc --quiet --workspace


echo
echo 'Linting with Clippy...'
cargo clippy --all-features --all-targets --quiet --workspace

echo
echo 'Checking the Nix flake...'
nix flake check --quiet --no-warn-dirty 2> >(
    grep -v -E \
        -e '^warning: The check omitted these incompatible systems: ' \
        -e "^Use '--all-systems' to check all\\.$" >&2
)

echo
echo 'Running all tests with Miri...'
MIRIFLAGS='-Zmiri-disable-isolation' cargo miri test --all-features --bins --examples --lib --quiet --tests --workspace # TODO: seems not to be available via Nix/Crane

echo
echo 'Running doc-tests with Miri...'
MIRIFLAGS='-Zmiri-disable-isolation' cargo miri test --all-features --doc --quiet --workspace # TODO: seems not to be available via Nix/Crane

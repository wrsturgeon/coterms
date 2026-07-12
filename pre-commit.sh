#!/usr/bin/env bash

set -euo pipefail


echo
echo 'Running tests...'
cargo test --all-features --bins --examples --lib --quiet --tests --workspace

echo 'Running doc-tests...'
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
echo 'Running tests with Miri...'
MIRIFLAGS='-Zmiri-disable-isolation' cargo miri test --all-features --bins --examples --lib --quiet --tests --workspace # TODO: `miri` seems not to be available via Nix/Crane

echo
echo 'Running doc-tests with Miri...'
MIRIFLAGS='-Zmiri-disable-isolation' cargo miri test --all-features --doc --quiet --workspace # TODO: `miri` seems not to be available via Nix/Crane


echo
echo 'Running mutation testing...'
PBT_CACHE_DIR="$(pwd)/.pbt" cargo mutants -j8

#!/usr/bin/env sh

cargo run --release --features serve -- serve-test &
cleanup_cargo=$!

# run go tests

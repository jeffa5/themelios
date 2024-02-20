#!/usr/bin/env bash

set -euxo pipefail

rm -f cargo-*.out

# run with default setup for tests, just dfs to reduce memory usage
MCO_CHECK_MODE=simulation cargo test --release -- --test-threads=1 --nocapture 2>&1 | tee cargo-default.out

# check with linearizability
MCO_CHECK_MODE=simulation MCO_CONSISTENCY=linearizable cargo test --release -- --test-threads=1 --nocapture 2>&1 | tee cargo-linearizable.out

# check with session reads consistency
MCO_CHECK_MODE=simulation MCO_CONSISTENCY=session cargo test --release -- --test-threads=1 --nocapture 2>&1 | tee cargo-session.out

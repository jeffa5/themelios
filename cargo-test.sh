#!/usr/bin/env bash

set -euxo pipefail

rm -f cargo-*.out

# check with linearizability
MCO_CHECK_MODE=simulation MCO_CONSISTENCY=linearizable cargo test --release -- --test-threads=1 --nocapture 2>&1 | tee cargo-linearizable.out

# check with monotonic (safe) session reads consistency
MCO_CHECK_MODE=simulation MCO_CONSISTENCY=monotonic-session cargo test --release -- --test-threads=1 --nocapture 2>&1 | tee cargo-monotonic-session.out

# check with session reads consistency
MCO_CHECK_MODE=simulation MCO_CONSISTENCY=session cargo test --release -- --test-threads=1 --nocapture 2>&1 | tee cargo-session.out

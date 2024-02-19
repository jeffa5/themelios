#!/usr/bin/env sh

set -ex

# run with default setup for tests, just dfs to reduce memory usage
MCO_CHECK_MODE=dfs cargo test --release -- --nocapture

# check with linearizability
MCO_CHECK_MODE=dfs MCO_CONSISTENCY=linearizable cargo test --release -- --nocapture

# check with session reads consistency
MCO_CHECK_MODE=dfs MCO_CONSISTENCY=session cargo test --release -- --nocapture

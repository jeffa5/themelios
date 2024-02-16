#!/usr/bin/env sh

set -x

# run with default setup for tests, just dfs to reduce memory usage
MCO_CHECK_MODE=dfs cargo test --release

# check with linearizability
MCO_CHECK_MODE=dfs MCO_CONSISTENCY=linearizable cargo test --release

# check with session reads consistency
MCO_CHECK_MODE=dfs MCO_CONSISTENCY=session cargo test --release

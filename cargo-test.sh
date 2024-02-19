#!/usr/bin/env bash

set -ex

rm -f cargo-*.{out,err}

# run with default setup for tests, just dfs to reduce memory usage
MCO_CHECK_MODE=dfs cargo test --release -- --nocapture > cargo-default.out 2> cargo-default.err

# check with linearizability
MCO_CHECK_MODE=dfs MCO_CONSISTENCY=linearizable cargo test --release -- --nocapture > cargo-linearizable.out 2> cargo-linearizable.err

# check with session reads consistency
MCO_CHECK_MODE=dfs MCO_CONSISTENCY=session cargo test --release -- --nocapture > cargo-session.out 2> cargo-session.err

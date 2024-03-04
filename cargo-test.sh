#!/usr/bin/env bash

set -uxo pipefail

rm -f cargo-test-*.out

function cargo_test(){
    MCO_REPORT_PATH=testout MCO_CHECK_MODE=simulation cargo test --release --test $1 -- --test-threads=1 --nocapture 2>&1 | tee cargo-test-$1.out
}

cargo_test deployment
cargo_test job
cargo_test replicaset
cargo_test statefulset

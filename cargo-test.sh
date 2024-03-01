#!/usr/bin/env bash

set -euxo pipefail

rm -f cargo-*.out

MCO_REPORT_PATH=testout MCO_CHECK_MODE=simulation cargo test --release -- --test-threads=1 --nocapture 2>&1 | tee cargo-test.out

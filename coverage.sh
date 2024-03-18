#!/usr/bin/env bash

set -euxo pipefail

rm -f coverage.out tarpaulin-report.*

MCO_REPORT_PATH=coverageout MCO_CHECK_MODE=simulation cargo tarpaulin --skip-clean --release --target-dir ctarget -o html -- --test-threads=1 --nocapture 2>&1 | tee coverage.out

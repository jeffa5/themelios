#!/usr/bin/env bash

set -euxo pipefail

./cargo-test.sh
./integration-test.sh
./cm-test.sh
./deploy-test.sh

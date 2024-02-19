#!/usr/bin/env bash

set -ex

./cargo-test.sh
./integration-test.sh
./cm-test.sh
./deploy-test.sh

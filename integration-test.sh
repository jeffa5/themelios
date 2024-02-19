#!/usr/bin/env bash

set -ex

# appends a command to a trap
#
# - 1st arg:  code to add
# - remaining args:  names of traps to modify
#
trap_add() {
    trap_add_cmd=$1; shift || fatal "${FUNCNAME} usage error"
    for trap_add_name in "$@"; do
        trap -- "$(
            # helper fn to get existing trap command from output
            # of trap -p
            extract_trap_cmd() { printf '%s\n' "$3"; }
            # print existing trap command with newline
            eval "extract_trap_cmd $(trap -p "${trap_add_name}")"
            # print the new trap command
            printf '%s\n' "${trap_add_cmd}"
        )" "${trap_add_name}" \
            || fatal "unable to add to trap ${trap_add_name}"
    done
}

cargo build --release --features serve
cargo run --release --features serve -- serve-test &
pid=$!
function cleanup_cargo {
  kill $pid
}
trap_add cleanup_cargo EXIT

sleep 5

# run go tests
cd ../kubernetes

make test-integration WHAT=./test/integration/deployment GOFLAGS="-v -failfast" > integration-deployment.out 2> integration-deployment.err
make test-integration WHAT=./test/integration/job GOFLAGS="-v -failfast" > integration-job.out 2> integration-job.err
make test-integration WHAT=./test/integration/replicaset GOFLAGS="-v -failfast" > integration-replicaset.out 2> integration-replicaset.err
make test-integration WHAT=./test/integration/scheduler GOFLAGS="-v -failfast" > integration-scheduler.out 2> integration-scheduler.err
make test-integration WHAT=./test/integration/statefulset GOFLAGS="-v -failfast" > integration-statefulset.out 2> integration-statefulset.err

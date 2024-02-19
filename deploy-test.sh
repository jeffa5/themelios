#!/usr/bin/env bash

set -euxo pipefail

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

cargo run --release --features serve -- serve-cluster &
pid=$!
function cleanup_cargo {
  kill $pid
}
trap_add cleanup_cargo EXIT

sleep 5

kubectl create deployment --image nginx:alpine nginx --replicas 1

kubectl rollout status deployment/nginx --watch

kubectl scale --replicas 3 deployment/nginx
kubectl rollout status deployment/nginx --watch

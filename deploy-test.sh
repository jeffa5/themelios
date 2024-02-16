#!/usr/bin/env sh

set -ex

cargo run --release --features serve -- serve-cluster &
pid=$!
# trap

kubectl create deployment --image nginx nginx --replicas 10

kubectl rollout status deployment/nginx --watch

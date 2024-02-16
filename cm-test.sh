#!/usr/bin/env sh

set -ex

# create a cluster
kind create cluster --wait 5m

function cleanup_kind {
  kind delete cluster
}
trap cleanup_kind EXIT

# remove the existing controller manager
docker exec kind-control-plane rm /etc/kubernetes/manifests/kube-controller-manager.yaml

# start our controller-manager
cargo run --release --features serve -- controller-manager &
pid=$!
function cleanup_cargo {
  kill $pid
}
trap cleanup_cargo EXIT

# create a resource
kubectl create deployment --image nginx nginx --replicas 10

# wait for it to finish deploying
kubectl rollout status deployment/nginx --watch

# TODO: scale the deployment down, change the image and wait again

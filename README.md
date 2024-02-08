# Themelios - Model checked orchestration

_Name derived from [θεμέλιος](https://en.wiktionary.org/wiki/%CE%B8%CE%B5%CE%BC%CE%AD%CE%BB%CE%B9%CE%BF%CF%82#Ancient_Greek)_

Each component is its own actor, can be run in duplicate and model checked individually (for local properties).

## Motivation

Current orchestration systems are substantial, complex distributed systems.
Assessing changes to them, at a fundamental level, is challenging and so also makes it hard to check their correctness formally.
Reducing them to actors exchanging messages in a system should enable more fine-grained property checking.

Things like the central datastore can be model checked separately for maintaining invariants, then different datastores can be model checked with the whole system to see how the guarantees hold up.
Additionally, using Rust for these actors and a Rust-based model checker we can directly spawn the actors in this system onto real servers and run the model-checked code.

Using this system we can add extra challenges and check things such as more adverse network conditions, and naturally partitions.
We can also check for liveness (being able to scale whilst partitioned, in particular).

Since we'll be able to spawn the actors directly, we can also run simulations on them to cover more of the state space and evaluate performance characteristics.
This further aids the usability of the system as a whole as not only will it be likely correct, but also performant.
These simulations will also form a great point of comparison between variations.

Of particular interest in the model checking is how running duplicates of a component affects the properties (do we need leader election for all of them).

## Progression

1. Scheduler
2. Faulty nodes
3. General node restarts
4. Multiple schedulers
5. Multiple datastore nodes (strong consistency)
6. Abstracting other bits

### Tertiary pieces

1. Watch streams from the datastore nodes

## Potential properties

1. When all pertubations to the system are done, it should be steady-state (no fighting controllers)
2. When a pod is created it eventually gets scheduled (scheduler acknowledges it)
3. When a pod is scheduled it eventually starts running (kubelet acknowledges it)
4. Replicasets eventually converge on a count (replicaset controller heals after partition)

### Kubernetes properties

#### [Statefulsets](https://kubernetes.io/docs/concepts/workloads/controllers/statefulset/#deployment-and-scaling-guarantees)

- For a StatefulSet with N replicas, when Pods are being deployed, they are created sequentially, in order from {0..N-1}.
- When Pods are being deleted, they are terminated in reverse order, from {N-1..0}.
- Before a scaling operation is applied to a Pod, all of its predecessors must be Running and Ready.
- Before a Pod is terminated, all of its successors must be completely shutdown.

This also relies on the numbering being sequential.

#### Other properties

Harder to find, especially on the documentation.

## Likely challenges

State space explosion...

Although we won't be able to run the system forever, or probably even that many rounds of messages it should be possible to check some properties.
Particularly as we can check that components have internal invariants separately, we need only check the combination / composition of components.

Model-checking the actual code may be too costly (lots of cloning).
May be able to mitigate this through persistent data-structures or just parameterise costly aspects and thoroughly test them independently.

Some forms of state-space reduction will be needed.
The stateright actor model already helps with this but we can still have cases where sending messages to different nodes ends in the same result so could be elided.

## Controller manager

To test the controller manager locally:
```sh
# create a cluster
kind create cluster
# remove the existing controller manager
docker exec kind-control-plane rm /etc/kubernetes/manifests/kube-controller-manager.yaml
# start our controller-manager
cargo run -- controller-manager
```

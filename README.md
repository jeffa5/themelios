# Model checked orchestration

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

## Likely challenges

State space explosion...

Although we won't be able to run the system forever, or probably even that many rounds of messages it should be possible to check some properties.
Particularly as we can check that components have internal invariants separately, we need only check the combination / composition of components.

Model-checking the actual code may be too costly (lots of cloning).
May be able to mitigate this through persistent data-structures or just parameterise costly aspects and thoroughly test them independently.

Some forms of state-space reduction will be needed.
The stateright actor model already helps with this but we can still have cases where sending messages to different nodes ends in the same result so could be elided.

## Components

### Scheduler

```rust
struct SchedulerState {
    // vector of nodes (max cpu, max mem, num pods)
    nodes: Vec<(u32, u32, u32)>,
}

enum SchedulerMsg {
    // request a pod to be scheduled
    SchedulePod,
    // bind a pod to a node
    BindPod
}
```

### Replicaset controller

```rust
struct ReplicasetState {
    // current replicasets (current scale, desired scale)
    replicasets: Vec<(u32, u32)>,
}

enum ReplicasetMsg {
    // pod in the management of this controller
    PodChanged,
    // create a new pod
    CreatePod,
}
```

### Datastore

```rust
struct DatastoreState {
    kvs: BTreeMap<String, String>,
}

enum DatastoreMsg {
    Range(String, String),
    RangeResponse(Vec<String>),
    Put(String, String),
    PutResponse(String, String),
    DeleteRange(String, String),
    DeleteRangeResponse(String, String),
}
```

### Clients

We'll need some clients to make changes to the system, at a high level, doing things like creating deployments, scaling deployments, creating pods, deleting pods.

```rust
struct ClientState {
}

enum ClientMsg {
    CreateDeployment,
    ScaleDeployment(u32),
    CreatePod(u32),
    DeletePod(u32),
}
```

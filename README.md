# Model checked orchestration

Each component is its own actor, can be run in duplicate and model checked individually (for local properties).

## Scheduler

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

## Replicaset controller

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

## Datastore

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

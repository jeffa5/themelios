use crate::{
    model::{Change, State},
    node::Node,
    scheduler::Scheduler,
};

pub trait Controller {
    fn step(&self, id: usize, state: &State) -> Vec<Change>;

    fn name(&self) -> String;
}

#[derive(Clone, Debug)]
pub enum Controllers {
    Node(Node),
    Scheduler(Scheduler),
    ReplicaSet(ReplicaSet),
}

impl Controller for Controllers {
    fn step(&self, id: usize, state: &State) -> Vec<Change> {
        match self {
            Controllers::Node(c) => c.step(id, state),
            Controllers::Scheduler(c) => c.step(id, state),
            Controllers::ReplicaSet(c) => c.step(id, state),
        }
    }

    fn name(&self) -> String {
        match self {
            Controllers::Node(c) => c.name(),
            Controllers::Scheduler(c) => c.name(),
            Controllers::ReplicaSet(c) => c.name(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ReplicaSet;

impl Controller for ReplicaSet {
    fn step(&self, id: usize, state: &State) -> Vec<Change> {
        let mut actions = Vec::new();
        if !state.replicaset_controllers.contains(&id) {
            actions.push(Change::ReplicasetJoin(id))
        }
        for replicaset in state.replica_sets.values() {
            for pod in replicaset.pods() {
                if !state.pods.contains_key(&pod) {
                    actions.push(Change::NewPod(pod));
                }
            }
        }
        actions
    }

    fn name(&self) -> String {
        "ReplicaSet".to_owned()
    }
}

use crate::model::{Change, State};
use node::Node;
use replicaset::ReplicaSet;
use scheduler::Scheduler;

pub mod node;
pub mod replicaset;
pub mod scheduler;

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

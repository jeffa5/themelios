use crate::abstract_model::Change;
use crate::state::StateView;
pub use node::Node;
pub use replicaset::ReplicaSet;
pub use scheduler::Scheduler;

mod node;
mod replicaset;
mod scheduler;

pub trait Controller {
    fn step(&self, id: usize, state: &StateView) -> Vec<Change>;

    fn register(&self, id: usize) -> Change;

    fn name(&self) -> String;
}

#[derive(Clone, Debug)]
pub enum Controllers {
    Node(Node),
    Scheduler(Scheduler),
    ReplicaSet(ReplicaSet),
}

impl Controller for Controllers {
    fn step(&self, id: usize, state: &StateView) -> Vec<Change> {
        match self {
            Controllers::Node(c) => c.step(id, state),
            Controllers::Scheduler(c) => c.step(id, state),
            Controllers::ReplicaSet(c) => c.step(id, state),
        }
    }

    fn register(&self, id: usize) -> Change {
        match self {
            Controllers::Node(c) => c.register(id),
            Controllers::Scheduler(c) => c.register(id),
            Controllers::ReplicaSet(c) => c.register(id),
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

use crate::abstract_model::Change;
use crate::state::StateView;
pub use node::Node;
pub use replicaset::ReplicaSet;
pub use scheduler::Scheduler;

mod node;
mod replicaset;
mod scheduler;

pub trait Controller {
    /// Take a step, generating changes, based on the current view of the state.
    fn step(&self, id: usize, state: &StateView) -> Vec<Change>;

    /// Name of this controller.
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

    fn name(&self) -> String {
        match self {
            Controllers::Node(c) => c.name(),
            Controllers::Scheduler(c) => c.name(),
            Controllers::ReplicaSet(c) => c.name(),
        }
    }
}

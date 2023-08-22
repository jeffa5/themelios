use crate::abstract_model::Operation;
use crate::state::StateView;

pub use deployment::Deployment;
pub use node::Node;
pub use replicaset::ReplicaSet;
pub use scheduler::Scheduler;
pub use statefulset::StatefulSet;

mod deployment;
mod node;
mod replicaset;
mod scheduler;
mod statefulset;

pub trait Controller {
    /// Take a step, generating changes, based on the current view of the state.
    fn step(&self, id: usize, state: &StateView) -> Option<Operation>;

    /// Name of this controller.
    fn name(&self) -> String;
}

#[derive(Clone, Debug)]
pub enum Controllers {
    Node(Node),
    Scheduler(Scheduler),
    ReplicaSet(ReplicaSet),
    Deployment(Deployment),
    StatefulSet(StatefulSet),
}

impl Controller for Controllers {
    fn step(&self, id: usize, state: &StateView) -> Option<Operation> {
        match self {
            Controllers::Node(c) => c.step(id, state),
            Controllers::Scheduler(c) => c.step(id, state),
            Controllers::ReplicaSet(c) => c.step(id, state),
            Controllers::Deployment(c) => c.step(id, state),
            Controllers::StatefulSet(c) => c.step(id, state),
        }
    }

    fn name(&self) -> String {
        match self {
            Controllers::Node(c) => c.name(),
            Controllers::Scheduler(c) => c.name(),
            Controllers::ReplicaSet(c) => c.name(),
            Controllers::Deployment(c) => c.name(),
            Controllers::StatefulSet(c) => c.name(),
        }
    }
}

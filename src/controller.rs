use std::hash::Hash;

use crate::abstract_model::Operation;
use crate::state::StateView;

pub use deployment::Deployment;
pub use node::Node;
pub use replicaset::ReplicaSet;
pub use scheduler::Scheduler;
pub use statefulset::StatefulSet;

pub use self::deployment::DeploymentState;
pub use self::node::NodeState;
pub use self::replicaset::ReplicaSetState;
pub use self::scheduler::SchedulerState;
pub use self::statefulset::StatefulSetState;

mod deployment;
mod node;
mod replicaset;
mod scheduler;
pub mod statefulset;
mod util;

pub trait Controller {
    type State: Clone + Hash + PartialEq + std::fmt::Debug + Default;

    /// Take a step, generating changes, based on the current view of the state.
    fn step(
        &self,
        id: usize,
        global_state: &StateView,
        local_state: &mut Self::State,
    ) -> Option<Operation>;

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

#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub enum ControllerStates {
    Node(NodeState),
    Scheduler(SchedulerState),
    ReplicaSet(ReplicaSetState),
    Deployment(DeploymentState),
    StatefulSet(StatefulSetState),
}

impl Default for ControllerStates {
    fn default() -> Self {
        Self::Node(NodeState::default())
    }
}

impl Controller for Controllers {
    type State = ControllerStates;

    fn step(
        &self,
        id: usize,
        global_state: &StateView,
        local_state: &mut Self::State,
    ) -> Option<Operation> {
        match (self, local_state) {
            (Controllers::Node(c), ControllerStates::Node(s)) => c.step(id, global_state, s),
            (Controllers::Scheduler(c), ControllerStates::Scheduler(s)) => {
                c.step(id, global_state, s)
            }
            (Controllers::ReplicaSet(c), ControllerStates::ReplicaSet(s)) => {
                c.step(id, global_state, s)
            }
            (Controllers::Deployment(c), ControllerStates::Deployment(s)) => {
                c.step(id, global_state, s)
            }
            (Controllers::StatefulSet(c), ControllerStates::StatefulSet(s)) => {
                c.step(id, global_state, s)
            }
            _ => unreachable!(),
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

impl Controllers {
    pub fn new_state(&self) -> ControllerStates {
        match self {
            Controllers::Node(_) => ControllerStates::Node(NodeState::default()),
            Controllers::Scheduler(_) => ControllerStates::Scheduler(SchedulerState),
            Controllers::ReplicaSet(_) => ControllerStates::ReplicaSet(ReplicaSetState),
            Controllers::Deployment(_) => ControllerStates::Deployment(DeploymentState),
            Controllers::StatefulSet(_) => ControllerStates::StatefulSet(StatefulSetState),
        }
    }
}

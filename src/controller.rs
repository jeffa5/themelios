use std::hash::Hash;

use crate::abstract_model::ControllerAction;
use crate::state::StateView;

pub use deployment::DeploymentController;
pub use node::NodeController;
pub use replicaset::ReplicaSetController;
pub use scheduler::SchedulerController;
pub use statefulset::StatefulSetController;

pub use self::deployment::DeploymentControllerState;
pub use self::node::NodeControllerState;
pub use self::replicaset::ReplicaSetControllerState;
pub use self::scheduler::SchedulerControllerState;
pub use self::statefulset::StatefulSetControllerState;

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
    ) -> Option<ControllerAction>;

    /// Name of this controller.
    fn name(&self) -> String;
}

#[derive(Clone, Debug)]
pub enum Controllers {
    Node(NodeController),
    Scheduler(SchedulerController),
    ReplicaSet(ReplicaSetController),
    Deployment(DeploymentController),
    StatefulSet(StatefulSetController),
}

#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub enum ControllerStates {
    Node(NodeControllerState),
    Scheduler(SchedulerControllerState),
    ReplicaSet(ReplicaSetControllerState),
    Deployment(DeploymentControllerState),
    StatefulSet(StatefulSetControllerState),
}

impl Default for ControllerStates {
    fn default() -> Self {
        Self::Node(NodeControllerState::default())
    }
}

impl Controller for Controllers {
    type State = ControllerStates;

    fn step(
        &self,
        id: usize,
        global_state: &StateView,
        local_state: &mut Self::State,
    ) -> Option<ControllerAction> {
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
            Controllers::Node(_) => ControllerStates::Node(NodeControllerState::default()),
            Controllers::Scheduler(_) => ControllerStates::Scheduler(SchedulerControllerState),
            Controllers::ReplicaSet(_) => ControllerStates::ReplicaSet(ReplicaSetControllerState),
            Controllers::Deployment(_) => ControllerStates::Deployment(DeploymentControllerState),
            Controllers::StatefulSet(_) => {
                ControllerStates::StatefulSet(StatefulSetControllerState)
            }
        }
    }
}

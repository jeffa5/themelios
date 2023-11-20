use std::hash::Hash;

use crate::abstract_model::ControllerAction;
use crate::state::StateView;

pub use deployment::DeploymentController;
pub use node::NodeController;
pub use replicaset::ReplicaSetController;
pub use scheduler::SchedulerController;
pub use statefulset::StatefulSetController;

pub use self::deployment::DeploymentControllerState;
use self::job::{JobController, JobControllerState};
pub use self::node::NodeControllerState;
pub use self::replicaset::ReplicaSetControllerState;
pub use self::scheduler::SchedulerControllerState;
pub use self::statefulset::StatefulSetControllerState;

pub mod deployment;
pub mod client;
pub mod job;
pub mod node;
pub mod replicaset;
pub mod scheduler;
pub mod statefulset;
pub mod util;

pub trait Controller {
    type State: Clone + Hash + PartialEq + std::fmt::Debug + Default;

    type Action: Into<ControllerAction>;

    /// Take a step, generating changes, based on the current view of the state.
    fn step(
        &self,
        id: usize,
        global_state: &StateView,
        local_state: &mut Self::State,
    ) -> Option<Self::Action>;

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
    Job(JobController),
}

#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub enum ControllerStates {
    Node(NodeControllerState),
    Scheduler(SchedulerControllerState),
    ReplicaSet(ReplicaSetControllerState),
    Deployment(DeploymentControllerState),
    StatefulSet(StatefulSetControllerState),
    Job(JobControllerState),
}

impl Default for ControllerStates {
    fn default() -> Self {
        Self::Node(NodeControllerState::default())
    }
}

impl Controller for Controllers {
    type State = ControllerStates;

    type Action = ControllerAction;

    fn step(
        &self,
        id: usize,
        global_state: &StateView,
        local_state: &mut Self::State,
    ) -> Option<ControllerAction> {
        match (self, local_state) {
            (Controllers::Node(c), ControllerStates::Node(s)) => {
                c.step(id, global_state, s).map(|a| a.into())
            }
            (Controllers::Scheduler(c), ControllerStates::Scheduler(s)) => {
                c.step(id, global_state, s).map(|a| a.into())
            }
            (Controllers::ReplicaSet(c), ControllerStates::ReplicaSet(s)) => {
                c.step(id, global_state, s).map(|a| a.into())
            }
            (Controllers::Deployment(c), ControllerStates::Deployment(s)) => {
                c.step(id, global_state, s).map(|a| a.into())
            }
            (Controllers::StatefulSet(c), ControllerStates::StatefulSet(s)) => {
                c.step(id, global_state, s).map(|a| a.into())
            }
            (Controllers::Job(c), ControllerStates::Job(s)) => {
                c.step(id, global_state, s).map(|a| a.into())
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
            Controllers::Job(c) => c.name(),
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
            Controllers::Job(_) => ControllerStates::Job(JobControllerState),
        }
    }
}

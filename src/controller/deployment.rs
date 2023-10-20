use crate::{abstract_model::Operation, state::StateView};

use super::Controller;

#[derive(Clone, Debug)]
pub struct Deployment;

pub struct DeploymentState;

impl Controller for Deployment {
    type State = DeploymentState;

    fn step(
        &self,
        id: usize,
        global_state: &StateView,
        local_state: &mut Self::State,
    ) -> Option<Operation> {
        if !global_state.controllers.contains(&id) {
            return Some(Operation::ControllerJoin(id));
        } else {
            for deployment in global_state.deployments.values() {
                for replicaset in deployment.replicasets() {
                    if !global_state.replica_sets.contains_key(&replicaset) {
                        return Some(Operation::NewReplicaset(replicaset));
                    }
                }
            }
        }
        None
    }

    fn name(&self) -> String {
        "Deployment".to_owned()
    }
}

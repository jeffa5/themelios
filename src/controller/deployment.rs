use crate::{abstract_model::Operation, state::StateView};

use super::Controller;

#[derive(Clone, Debug)]
pub struct Deployment;

impl Controller for Deployment {
    fn step(&self, id: usize, state: &StateView) -> Option<Operation> {
        if !state.controllers.contains(&id) {
            return Some(Operation::ControllerJoin(id));
        } else {
            for deployment in state.deployments.values() {
                for replicaset in deployment.replicasets() {
                    if !state.replica_sets.contains_key(&replicaset) {
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

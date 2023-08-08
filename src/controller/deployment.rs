use crate::{abstract_model::Operation, state::StateView};

use super::Controller;

#[derive(Clone, Debug)]
pub struct Deployment;

impl Controller for Deployment {
    fn step(&self, id: usize, state: &StateView) -> Vec<Operation> {
        let mut actions = Vec::new();
        if !state.controllers.contains(&id) {
            actions.push(Operation::ControllerJoin(id));
        } else {
            for deployment in state.deployments.values() {
                for replicaset in deployment.replicasets() {
                    if !state.replica_sets.contains_key(&replicaset) {
                        actions.push(Operation::NewReplicaset(replicaset));
                    }
                }
            }
        }
        actions
    }

    fn name(&self) -> String {
        "Deployment".to_owned()
    }
}

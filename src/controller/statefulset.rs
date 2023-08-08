use super::Controller;
use crate::{abstract_model::Operation, state::StateView};

#[derive(Clone, Debug)]
pub struct StatefulSet;

impl Controller for StatefulSet {
    fn step(&self, id: usize, state: &StateView) -> Vec<Operation> {
        let mut actions = Vec::new();
        if !state.statefulset_controllers.contains(&id) {
            actions.push(Operation::StatefulSetJoin(id));
        } else {
            for statefulset in state.statefulsets.values() {
                for replicaset in statefulset.replicasets() {
                    if !state.replica_sets.contains_key(&replicaset) {
                        actions.push(Operation::NewReplicaset(replicaset));
                    }
                }
            }
        }
        actions
    }

    fn name(&self) -> String {
        "StatefulSet".to_owned()
    }
}

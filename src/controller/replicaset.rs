use crate::abstract_model::Operation;
use crate::controller::Controller;
use crate::state::StateView;

#[derive(Clone, Debug)]
pub struct ReplicaSet;

impl Controller for ReplicaSet {
    fn step(&self, id: usize, state: &StateView) -> Vec<Operation> {
        let mut actions = Vec::new();
        if !state.controllers.contains(&id) {
            actions.push(Operation::ControllerJoin(id))
        } else {
            for replicaset in state.replica_sets.values() {
                for pod in replicaset.pods() {
                    if !state.pods.contains_key(&pod) {
                        actions.push(Operation::NewPod(pod));
                    }
                }
            }
        }
        actions
    }

    fn name(&self) -> String {
        "ReplicaSet".to_owned()
    }
}

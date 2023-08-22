use crate::abstract_model::Operation;
use crate::controller::Controller;
use crate::state::StateView;

#[derive(Clone, Debug)]
pub struct ReplicaSet;

impl Controller for ReplicaSet {
    fn step(&self, id: usize, state: &StateView) -> Option<Operation> {
        if !state.controllers.contains(&id) {
            return Some(Operation::ControllerJoin(id));
        } else {
            for replicaset in state.replica_sets.values() {
                for pod in replicaset.pods() {
                    if !state.pods.contains_key(&pod) {
                        return Some(Operation::NewPod(pod));
                    }
                }
            }
        }
        None
    }

    fn name(&self) -> String {
        "ReplicaSet".to_owned()
    }
}

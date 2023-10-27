use crate::abstract_model::Operation;
use crate::controller::Controller;
use crate::state::StateView;

#[derive(Clone, Debug)]
pub struct ReplicaSet;

#[derive(Debug, Default, Hash, Clone, PartialEq, Eq)]
pub struct ReplicaSetState;

impl Controller for ReplicaSet {
    type State = ReplicaSetState;
    fn step(
        &self,
        id: usize,
        global_state: &StateView,
        _local_state: &mut Self::State,
    ) -> Option<Operation> {
        if !global_state.controllers.contains(&id) {
            return Some(Operation::ControllerJoin(id));
        } else {
            for replicaset in global_state.replica_sets.values() {
                for pod in replicaset.pods() {
                    if !global_state.pods.contains_key(&pod) {
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

use crate::abstract_model::Operation;
use crate::state::StateView;
use crate::{abstract_model::Change, controller::Controller};

#[derive(Clone, Debug)]
pub struct ReplicaSet;

impl Controller for ReplicaSet {
    fn step(&self, id: usize, state: &StateView) -> Vec<Change> {
        let mut actions = Vec::new();
        if !state.replicaset_controllers.contains(&id) {
            actions.push(Change {
                revision: state.revision,
                operation: Operation::ReplicasetJoin(id),
            })
        } else {
            for replicaset in state.replica_sets.values() {
                for pod in replicaset.pods() {
                    if !state.pods.contains_key(&pod) {
                        actions.push(Change {
                            revision: state.revision,
                            operation: Operation::NewPod(pod),
                        });
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

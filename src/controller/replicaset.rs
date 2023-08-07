use crate::state::StateView;
use crate::{abstract_model::Change, controller::Controller};

#[derive(Clone, Debug)]
pub struct ReplicaSet;

impl Controller for ReplicaSet {
    fn step(&self, id: usize, state: &StateView) -> Vec<Change> {
        let mut actions = Vec::new();
        if !state.replicaset_controllers.contains(&id) {
            actions.push(Change::ReplicasetJoin(id))
        }
        for replicaset in state.replica_sets.values() {
            for pod in replicaset.pods() {
                if !state.pods.contains_key(&pod) {
                    actions.push(Change::NewPod(pod));
                }
            }
        }
        actions
    }

    fn register(&self, id: usize) -> Change {
        Change::ReplicasetJoin(id)
    }

    fn name(&self) -> String {
        "ReplicaSet".to_owned()
    }
}

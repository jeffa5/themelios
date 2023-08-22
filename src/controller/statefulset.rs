use super::Controller;
use crate::{abstract_model::Operation, state::StateView};

#[derive(Clone, Debug)]
pub struct StatefulSet;

impl Controller for StatefulSet {
    fn step(&self, id: usize, state: &StateView) -> Option<Operation> {
        if !state.controllers.contains(&id) {
            return Some(Operation::ControllerJoin(id));
        } else {
            for statefulset in state.statefulsets.values() {
                for pod in statefulset.pods() {
                    if !state.pods.contains_key(&pod) {
                        return Some(Operation::NewPod(pod));
                    }
                }
            }
        }
        None
    }

    fn name(&self) -> String {
        "StatefulSet".to_owned()
    }
}

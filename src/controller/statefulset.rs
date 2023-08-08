use super::Controller;
use crate::{abstract_model::Operation, state::StateView};

#[derive(Clone, Debug)]
pub struct StatefulSet;

impl Controller for StatefulSet {
    fn step(&self, id: usize, state: &StateView) -> Vec<Operation> {
        let mut actions = Vec::new();
        if !state.controllers.contains(&id) {
            actions.push(Operation::ControllerJoin(id));
        } else {
            for statefulset in state.statefulsets.values() {
                for pod in statefulset.pods() {
                    if !state.pods.contains_key(&pod) {
                        actions.push(Operation::NewPod(pod));
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

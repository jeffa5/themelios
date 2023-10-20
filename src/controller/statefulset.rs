use super::Controller;
use crate::{abstract_model::Operation, state::StateView};

#[derive(Clone, Debug)]
pub struct StatefulSet;

pub struct StatefulSetState;

impl Controller for StatefulSet {
    type State = StatefulSetState;

    fn step(
        &self,
        id: usize,
        global_state: &StateView,
        local_state: &mut Self::State,
    ) -> Option<Operation> {
        if !global_state.controllers.contains(&id) {
            return Some(Operation::ControllerJoin(id));
        } else {
            for statefulset in global_state.statefulsets.values() {
                for pod in statefulset.pods() {
                    if !global_state.pods.contains_key(&pod) {
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

use crate::abstract_model::Operation;
use crate::controller::Controller;
use crate::state::StateView;

#[derive(Clone, Debug)]
pub struct Scheduler;

impl Controller for Scheduler {
    fn step(&self, id: usize, state: &StateView) -> Option<Operation> {
        if !state.controllers.contains(&id) {
            return Some(Operation::ControllerJoin(id));
        } else {
            let mut nodes = state
                .nodes
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<Vec<_>>();
            // TODO: sort nodes by load
            nodes.sort_by_key(|(_, node)| node.running.len());

            for pod in state.pods.values() {
                // find a pod that needs scheduling
                if pod.node_name.is_none() {
                    // try to find a node suitable
                    if let Some((node, _)) = nodes.first() {
                        return Some(Operation::SchedulePod(pod.id.clone(), *node));
                    }
                }
            }
        }
        None
    }

    fn name(&self) -> String {
        "Scheduler".to_owned()
    }
}

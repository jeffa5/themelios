use crate::abstract_model::Operation;
use crate::controller::Controller;
use crate::state::StateView;

#[derive(Clone, Debug)]
pub struct Scheduler;

impl Controller for Scheduler {
    fn step(&self, id: usize, state: &StateView) -> Vec<Operation> {
        let mut actions = Vec::new();
        if !state.controllers.contains(&id) {
            actions.push(Operation::ControllerJoin(id))
        } else {
            let mut nodes = state
                .nodes
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<Vec<_>>();
            // TODO: sort nodes by load
            nodes.sort_by_key(|(_, node)| node.running.len());
            if let Some((_, pod)) = state.pods.first_key_value() {
                if let Some((node, _)) = nodes.first() {
                    if pod.node_name.is_none() {
                        actions.push(Operation::SchedulePod(pod.id.clone(), *node));
                    }
                }
            }
        }
        actions
    }

    fn name(&self) -> String {
        "Scheduler".to_owned()
    }
}

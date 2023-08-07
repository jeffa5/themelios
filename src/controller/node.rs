use crate::abstract_model::Operation;
use crate::controller::Controller;
use crate::state::StateView;

#[derive(Clone, Debug)]
pub struct Node;

impl Controller for Node {
    fn step(&self, id: usize, state: &StateView) -> Vec<Operation> {
        let mut actions = Vec::new();
        if let Some(node) = state.nodes.get(&id) {
            if node.ready {
                for pod in state
                    .pods
                    .values()
                    .filter(|p| p.node_name.map_or(false, |n| n == id))
                {
                    if !node.running.contains(&pod.id) {
                        actions.push(Operation::RunPod(pod.id, id));
                    }
                }
            }
        } else {
            actions.push(Operation::NodeJoin(id));
        }
        actions
    }

    fn name(&self) -> String {
        "Node".to_owned()
    }
}

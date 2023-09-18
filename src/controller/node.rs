use crate::abstract_model::Operation;
use crate::controller::Controller;
use crate::resources::ResourceQuantities;
use crate::state::StateView;

#[derive(Clone, Debug)]
pub struct Node {
    pub name: String,
}

impl Controller for Node {
    fn step(&self, id: usize, state: &StateView) -> Option<Operation> {
        if let Some(node) = state.nodes.get(&id) {
            if node.ready {
                for pod in state
                    .pods
                    .values()
                    .filter(|p| p.node_name.as_ref().map_or(false, |n| n == &self.name))
                {
                    if !node.running.contains(&pod.id) {
                        return Some(Operation::RunPod(pod.id.clone(), id));
                    }
                }
            }
        } else {
            return Some(Operation::NodeJoin(
                id,
                ResourceQuantities {
                    cpu_cores: Some(4),
                    memory_mb: Some(4000),
                },
            ));
        }
        None
    }

    fn name(&self) -> String {
        "Node".to_owned()
    }
}

use crate::{
    controller::Controller,
    model::{Change, State},
};

#[derive(Clone, Debug)]
pub struct Node;

impl Controller for Node {
    fn step(&self, id: usize, state: &State) -> Vec<Change> {
        let mut actions = Vec::new();
        if let Some(node) = state.nodes.get(&id) {
            if node.ready {
                for pod in state
                    .pods
                    .values()
                    .filter(|p| p.node_name.map_or(false, |n| n == id))
                {
                    if !node.running.contains(&pod.id) {
                        actions.push(Change::RunPod(pod.id, id));
                    }
                }
            }
        } else {
            actions.push(Change::NodeJoin(id));
        }
        actions
    }

    fn name(&self) -> String {
        "Node".to_owned()
    }
}

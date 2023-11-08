use std::collections::BTreeMap;

use crate::abstract_model::Operation;
use crate::controller::Controller;
use crate::resources::ResourceQuantities;
use crate::state::StateView;

#[derive(Clone, Debug)]
pub struct Node {
    pub name: String,
}

#[derive(Debug, Default, Hash, Clone, PartialEq, Eq)]
pub struct NodeState {
    pub running: Vec<String>,
}

impl Controller for Node {
    type State = NodeState;

    fn step(
        &self,
        id: usize,
        global_state: &StateView,
        local_state: &mut Self::State,
    ) -> Option<Operation> {
        if let Some(_node) = global_state.nodes.get(&id) {
            for pod in global_state
                .pods
                .values()
                .filter(|p| p.spec.node_name.as_ref().map_or(false, |n| n == &self.name))
            {
                if !local_state.running.contains(&pod.metadata.name) {
                    return Some(Operation::RunPod(pod.metadata.name.clone(), id));
                }
            }
        } else {
            return Some(Operation::NodeJoin(
                id,
                ResourceQuantities {
                    others: BTreeMap::new(),
                },
            ));
        }
        None
    }

    fn name(&self) -> String {
        "Node".to_owned()
    }
}

use std::collections::BTreeMap;

use crate::abstract_model::ControllerAction;
use crate::controller::Controller;
use crate::resources::ResourceQuantities;
use crate::state::StateView;

#[derive(Clone, Debug)]
pub struct NodeController {
    pub name: String,
}

#[derive(Debug, Default, Hash, Clone, PartialEq, Eq)]
pub struct NodeControllerState {
    pub running: Vec<String>,
}

#[derive(Debug)]
pub enum NodeControllerAction {
    NodeJoin(usize, ResourceQuantities),
}

impl From<NodeControllerAction> for ControllerAction {
    fn from(val: NodeControllerAction) -> Self {
        match val {
            NodeControllerAction::NodeJoin(id, q) => ControllerAction::NodeJoin(id, q),
        }
    }
}

impl Controller for NodeController {
    type State = NodeControllerState;

    type Action = NodeControllerAction;

    fn step(
        &self,
        id: usize,
        global_state: &StateView,
        local_state: &mut Self::State,
    ) -> Option<NodeControllerAction> {
        if let Some(_node) = global_state.nodes.get(&id) {
            for pod in global_state
                .pods
                .iter()
                .filter(|p| p.spec.node_name.as_ref().map_or(false, |n| n == &self.name))
            {
                if !local_state.running.contains(&pod.metadata.name) {
                    local_state.running.push(pod.metadata.name.clone());
                }
            }
        } else {
            return Some(NodeControllerAction::NodeJoin(
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

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

impl Controller for NodeController {
    type State = NodeControllerState;

    fn step(
        &self,
        id: usize,
        global_state: &StateView,
        local_state: &mut Self::State,
    ) -> Option<ControllerAction> {
        if let Some(_node) = global_state.nodes.get(&id) {
            for pod in global_state
                .pods
                .values()
                .filter(|p| p.spec.node_name.as_ref().map_or(false, |n| n == &self.name))
            {
                if !local_state.running.contains(&pod.metadata.name) {
                    local_state.running.push(pod.metadata.name.clone());
                }
            }
        } else {
            return Some(ControllerAction::NodeJoin(
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

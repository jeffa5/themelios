use std::collections::BTreeMap;

use crate::abstract_model::ControllerAction;
use crate::controller::Controller;
use crate::resources::{
    ConditionStatus, Pod, PodCondition, PodConditionType, PodPhase, ResourceQuantities,
};
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

    UpdatePod(Pod),
}

impl From<NodeControllerAction> for ControllerAction {
    fn from(val: NodeControllerAction) -> Self {
        match val {
            NodeControllerAction::NodeJoin(id, q) => ControllerAction::NodeJoin(id, q),
            NodeControllerAction::UpdatePod(pod) => ControllerAction::UpdatePod(pod),
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
                if pod.status.phase != PodPhase::Running {
                    let mut pod = pod.clone();
                    pod.status.phase = PodPhase::Running;
                    return Some(NodeControllerAction::UpdatePod(pod));
                }
                if !pod.status.conditions.iter().any(|c| {
                    c.r#type == PodConditionType::Ready && c.status == ConditionStatus::True
                }) {
                    let mut pod = pod.clone();
                    pod.status.conditions.push(PodCondition {
                        status: ConditionStatus::True,
                        r#type: PodConditionType::Ready,
                        last_probe_time: None,
                        last_transition_time: None,
                        message: None,
                        reason: None,
                    });
                    return Some(NodeControllerAction::UpdatePod(pod));
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

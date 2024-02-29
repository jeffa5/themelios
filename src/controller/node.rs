use std::collections::BTreeMap;

use crate::abstract_model::ControllerAction;
use crate::controller::Controller;
use crate::resources::{
    ConditionStatus, Pod, PodCondition, PodConditionType, PodPhase, ResourceQuantities,
};
use crate::state::revision::Revision;
use crate::state::StateView;

#[derive(Clone, Debug)]
pub struct NodeController {
    pub name: String,
}

#[derive(Debug, Default, Hash, Clone, PartialEq, Eq)]
pub struct NodeControllerState {
    pub running: Vec<String>,
    revision: Revision,
}

#[derive(Debug)]
pub enum NodeControllerAction {
    NodeJoin(String, ResourceQuantities),

    UpdatePod(Pod),
    DeletePod(Pod),
}

impl From<NodeControllerAction> for ControllerAction {
    fn from(val: NodeControllerAction) -> Self {
        match val {
            NodeControllerAction::NodeJoin(id, q) => ControllerAction::NodeJoin(id, q),
            NodeControllerAction::UpdatePod(pod) => ControllerAction::UpdatePod(pod),
            NodeControllerAction::DeletePod(pod) => ControllerAction::HardDeletePod(pod),
        }
    }
}

impl Controller for NodeController {
    type State = NodeControllerState;

    type Action = NodeControllerAction;

    fn step(
        &self,
        global_state: &StateView,
        local_state: &mut Self::State,
    ) -> Option<NodeControllerAction> {
        local_state.revision = global_state.revision.clone();
        if let Some(_node) = global_state.nodes.get(&self.name) {
            let pods_for_this_node = global_state
                .pods
                .iter()
                .filter(|p| p.spec.node_name.as_ref().map_or(false, |n| n == &self.name))
                .collect::<Vec<_>>();

            // quickly start up all local pods
            for pod in &pods_for_this_node {
                if !local_state.running.contains(&pod.metadata.name) {
                    local_state.running.push(pod.metadata.name.clone());
                }
            }

            for pod in pods_for_this_node {
                if pod.metadata.deletion_timestamp.is_some() {
                    // pod has been marked for deletion and is running on this node, forget about
                    // it locally and delete it for good in the API
                    local_state.running.remove(
                        local_state
                            .running
                            .iter()
                            .position(|r| r == &pod.metadata.name)
                            .unwrap(),
                    );
                    return Some(NodeControllerAction::DeletePod(pod.clone()));
                }

                let mut new_pod = pod.clone();
                if new_pod.status.phase != PodPhase::Running {
                    new_pod.status.phase = PodPhase::Running;
                }
                if !new_pod.status.conditions.iter().any(|c| {
                    c.r#type == PodConditionType::Ready && c.status == ConditionStatus::True
                }) {
                    new_pod.status.conditions.push(PodCondition {
                        status: ConditionStatus::True,
                        r#type: PodConditionType::Ready,
                        last_probe_time: None,
                        last_transition_time: None,
                        message: None,
                        reason: None,
                    });
                }
                if new_pod.status != pod.status {
                    return Some(NodeControllerAction::UpdatePod(new_pod));
                }
            }
        } else {
            return Some(NodeControllerAction::NodeJoin(
                self.name.clone(),
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

    fn min_revision_accepted<'a>(&self, state: &'a Self::State) -> &'a Revision {
        &state.revision
    }
}

use std::collections::BTreeMap;

use crate::abstract_model::ControllerAction;
use crate::controller::Controller;
use crate::resources::{
    ConditionStatus, ContainerState, ContainerStateRunning, ContainerStateTerminated,
    ContainerStateWaiting, ContainerStatus, Pod, PodCondition, PodConditionType, PodPhase,
    ResourceQuantities,
};
use crate::state::revision::Revision;
use crate::state::StateView;
use crate::utils::now;

use super::util::is_pod_active;

#[derive(Clone, Debug)]
pub struct NodeController {
    pub name: String,
}

#[derive(Debug, Default, Hash, Clone, PartialEq, Eq)]
pub struct NodeControllerState {
    pub running: Vec<String>,
    revision: Option<Revision>,
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
        local_state.revision = Some(global_state.revision.clone());
        if let Some(_node) = global_state.nodes.get(&self.name) {
            let pods_for_this_node = global_state
                .pods
                .iter()
                .filter(|p| p.spec.node_name.as_ref().map_or(false, |n| n == &self.name))
                .collect::<Vec<_>>();

            for pod in pods_for_this_node {
                if is_pod_active(pod) {
                    if !local_state.running.contains(&pod.metadata.name) {
                        local_state.running.push(pod.metadata.name.clone());
                        let mut new_pod = pod.clone();
                        new_pod.status.container_statuses.clear();
                        for c in &new_pod.spec.containers {
                            new_pod.status.container_statuses.push(ContainerStatus {
                                name: c.name.clone(),
                                state: ContainerState::Running(ContainerStateRunning {
                                    started_at: Some(now()),
                                }),
                                last_state: ContainerState::Waiting(
                                    ContainerStateWaiting::default(),
                                ),
                                ready: true,
                                image: c.image.clone(),
                                started: true,
                                ..Default::default()
                            })
                        }
                        new_pod.status.phase = PodPhase::Running;
                        return Some(NodeControllerAction::UpdatePod(new_pod));
                    } else {
                        // already running it, monitor it
                        let mut new_pod = pod.clone();
                        if pod.status.container_statuses.iter().any(|cs| {
                            matches!(
                                cs.state,
                                ContainerState::Terminated(ContainerStateTerminated { exit_code, .. }) if exit_code > 0
                            )
                        }) {
                            new_pod.status.phase = PodPhase::Failed;
                            new_pod.status.conditions.clear();

                            if let Some(pos) = local_state
                                .running
                                .iter()
                                .position(|r| r == &pod.metadata.name)
                            {
                                local_state.running.remove(pos);
                            }
                            return Some(NodeControllerAction::UpdatePod(new_pod));
                        } else if pod.status.container_statuses.iter().all(|cs| {
                            matches!(
                                cs.state,
                                ContainerState::Terminated(ContainerStateTerminated {
                                    exit_code: 0,
                                    ..
                                })
                            )
                        }) {
                            new_pod.status.phase = PodPhase::Succeeded;
                            new_pod.status.conditions.clear();
                            if let Some(pos) = local_state
                                .running
                                .iter()
                                .position(|r| r == &pod.metadata.name)
                            {
                                local_state.running.remove(pos);
                            }
                            return Some(NodeControllerAction::UpdatePod(new_pod));
                        }else if pod.status.phase == PodPhase::Running
                            && !new_pod.status.conditions.iter().any(|c| {
                                c.r#type == PodConditionType::Ready
                                    && c.status == ConditionStatus::True
                            })
                        {
                            // TODO: should have an arbitrary action to mark pods running, then this relies on
                            // that.
                            new_pod.status.conditions.push(PodCondition {
                                status: ConditionStatus::True,
                                r#type: PodConditionType::Ready,
                                last_probe_time: None,
                                last_transition_time: None,
                                message: None,
                                reason: None,
                            });
                            return Some(NodeControllerAction::UpdatePod(new_pod));
                        }
                    }
                } else if pod.metadata.deletion_timestamp.is_some() {
                    // TODO: ensure we mark containers as shutdown first?

                    // pod has been marked for deletion and is running on this node, forget about
                    // it locally and delete it for good in the API
                    if let Some(pos) = local_state
                        .running
                        .iter()
                        .position(|r| r == &pod.metadata.name)
                    {
                        local_state.running.remove(pos);
                    }
                    return Some(NodeControllerAction::DeletePod(pod.clone()));
                } else {
                    // suceeded or failed, not sure what to do here?
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

    fn min_revision_accepted<'a>(&self, state: &'a Self::State) -> Option<&'a Revision> {
        state.revision.as_ref()
    }
}

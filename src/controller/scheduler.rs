use tracing::debug;

use crate::abstract_model::Operation;
use crate::controller::Controller;
use crate::resources::{NodeResource, PodResource};
use crate::state::StateView;

#[derive(Clone, Debug)]
pub struct Scheduler;

#[derive(Debug, Default, Hash, Clone, PartialEq, Eq)]
pub struct SchedulerState;

impl Controller for Scheduler {
    type State = SchedulerState;

    fn step(
        &self,
        id: usize,
        global_state: &StateView,
        _local_state: &mut Self::State,
    ) -> Option<Operation> {
        if !global_state.controllers.contains(&id) {
            return Some(Operation::ControllerJoin(id));
        } else {
            let mut nodes = global_state
                .nodes
                .values()
                .map(|v| (v, global_state.pods_for_node(&v.metadata.name)))
                .collect::<Vec<_>>();
            // TODO: sort nodes by load
            nodes.sort_by_key(|(_, pods)| pods.len());

            let pods_to_schedule = global_state
                .pods
                .values()
                .filter(|p| p.spec.node_name.is_none());

            for pod in pods_to_schedule {
                if let Some(op) = schedule(pod, &nodes) {
                    return Some(op);
                }
            }
        }
        None
    }

    fn name(&self) -> String {
        "Scheduler".to_owned()
    }
}

fn schedule(pod: &PodResource, nodes: &[(&NodeResource, Vec<&PodResource>)]) -> Option<Operation> {
    let requests = pod
        .spec
        .resources
        .as_ref()
        .and_then(|r| r.requests.as_ref())
        .cloned()
        .unwrap_or_default();
    // try to find a node suitable
    for (node, pods) in nodes {
        debug!(node = node.metadata.name, "Seeing if node fits");

        if !tolerates_taints(pod, node) {
            continue;
        }

        let mut remaining_capacity = node.status.capacity.clone();
        for running_pod in pods {
            if let Some(resources) = &running_pod.spec.resources {
                if let Some(requests) = &resources.requests {
                    remaining_capacity -= requests.clone();
                }
            }
        }
        debug!(?remaining_capacity, ?requests, "Checking if node has space");
        if remaining_capacity >= requests {
            debug!(
                pod = pod.metadata.name,
                node = node.metadata.name,
                "Did have space, scheduling pod"
            );
            return Some(Operation::SchedulePod(
                pod.metadata.name.clone(),
                node.metadata.name.clone(),
            ));
        } else {
            debug!(node = node.metadata.name, "Node does not have space");
        }
    }
    None
}

fn tolerates_taints(pod: &PodResource, node: &NodeResource) -> bool {
    for taint in &node.spec.taints {
        if pod.spec.tolerations.iter().any(|t| t.key == taint.key) {
            // this pod tolerates this taint and so is immune to its effects
        } else {
            // this pod does not tolerate this taint, apply the effect
            return false;
        }
    }
    true
}

use tracing::debug;

use crate::abstract_model::Operation;
use crate::controller::Controller;
use crate::resources::{NodeResource, PersistentVolumeClaim, PodResource, ResourceQuantities};
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

            let pvcs = global_state
                .persistent_volume_claims
                .values()
                .collect::<Vec<_>>();

            for pod in pods_to_schedule {
                if let Some(op) = schedule(pod, &nodes, &pvcs) {
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

fn schedule(
    pod: &PodResource,
    nodes: &[(&NodeResource, Vec<&PodResource>)],
    pvcs: &[&PersistentVolumeClaim],
) -> Option<Operation> {
    // try to find a node suitable
    for (node, pods) in nodes {
        debug!(node = node.metadata.name, "Seeing if node fits");

        if node.spec.unschedulable {
            debug!("Node is not schedulable");
            continue;
        }

        if !tolerates_taints(pod, node) {
            debug!("Pod doesn't tolerate node's taints");
            continue;
        }

        if !volumes_exist(pod, pvcs) {
            debug!("Pod requires volumes that don't exist");
            continue;
        }

        if !fits_resources(pod, node, pods) {
            debug!("Pod requires more resources than the node has available");
            continue;
        }

        return Some(Operation::SchedulePod(
            pod.metadata.name.clone(),
            node.metadata.name.clone(),
        ));
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

fn volumes_exist(pod: &PodResource, pvcs: &[&PersistentVolumeClaim]) -> bool {
    for volume in &pod.spec.volumes {
        if !pvcs.iter().any(|pvc| pvc.metadata.name == volume.name) {
            return false;
        }
    }
    true
}

fn fits_resources(pod: &PodResource, node: &NodeResource, pods_for_node: &[&PodResource]) -> bool {
    let requests = pod
        .spec
        .containers
        .iter()
        .filter_map(|c| c.resources.requests.as_ref())
        .sum();

    // use allocatable from node status, or capacity if it is missing
    let mut remaining_allocatable = node
        .status
        .allocatable
        .as_ref()
        .unwrap_or(&node.status.capacity)
        .clone();

    for running_pod in pods_for_node {
        let requests: ResourceQuantities = running_pod
            .spec
            .containers
            .iter()
            .filter_map(|c| c.resources.requests.as_ref())
            .sum();
        remaining_allocatable -= requests.clone();
    }

    debug!(
        ?remaining_allocatable,
        ?requests,
        "Checking if node has space"
    );
    if remaining_allocatable >= requests {
        debug!(
            pod = pod.metadata.name,
            node = node.metadata.name,
            "Did have space, scheduling pod"
        );
        true
    } else {
        debug!(node = node.metadata.name, "Node does not have space");
        false
    }
}

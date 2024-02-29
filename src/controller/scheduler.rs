use tracing::debug;

use crate::abstract_model::ControllerAction;
use crate::controller::Controller;
use crate::resources::{Node, PersistentVolumeClaim, Pod, ResourceQuantities};
use crate::state::revision::Revision;
use crate::state::StateView;

#[derive(Clone, Debug)]
pub struct SchedulerController;

#[derive(Debug, Default, Hash, Clone, PartialEq, Eq)]
pub struct SchedulerControllerState {
    revision: Revision,
}

#[derive(Debug)]
pub enum SchedulerControllerAction {
    UpdatePod(Pod),
}

impl From<SchedulerControllerAction> for ControllerAction {
    fn from(value: SchedulerControllerAction) -> Self {
        match value {
            SchedulerControllerAction::UpdatePod(p) => ControllerAction::UpdatePod(p),
        }
    }
}

impl Controller for SchedulerController {
    type State = SchedulerControllerState;

    type Action = SchedulerControllerAction;

    fn step(
        &self,
        global_state: &StateView,
        local_state: &mut Self::State,
    ) -> Option<SchedulerControllerAction> {
        local_state.revision = global_state.revision.clone();
        let mut nodes = global_state
            .nodes
            .iter()
            .map(|v| (v, global_state.pods_for_node(&v.metadata.name)))
            .collect::<Vec<_>>();
        // TODO: sort nodes by load
        nodes.sort_by_key(|(_, pods)| pods.len());

        let pods_to_schedule = global_state
            .pods
            .iter()
            .filter(|p| p.spec.node_name.is_none());

        let pvcs = global_state
            .persistent_volume_claims
            .iter()
            .collect::<Vec<_>>();

        for pod in pods_to_schedule {
            if let Some(op) = schedule(pod, &nodes, &pvcs) {
                return Some(op);
            }
        }
        None
    }

    fn name(&self) -> String {
        "Scheduler".to_owned()
    }

    fn min_revision_accepted<'a>(&self, state: &'a Self::State) -> &'a Revision {
        &state.revision
    }
}

fn schedule(
    pod: &Pod,
    nodes: &[(&Node, Vec<&Pod>)],
    pvcs: &[&PersistentVolumeClaim],
) -> Option<SchedulerControllerAction> {
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

        let mut pod = pod.clone();
        pod.spec.node_name = Some(node.metadata.name.clone());
        return Some(SchedulerControllerAction::UpdatePod(pod));
    }
    None
}

fn tolerates_taints(pod: &Pod, node: &Node) -> bool {
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

fn volumes_exist(pod: &Pod, pvcs: &[&PersistentVolumeClaim]) -> bool {
    for volume in &pod.spec.volumes {
        if !pvcs.iter().any(|pvc| pvc.metadata.name == volume.name) {
            return false;
        }
    }
    true
}

fn fits_resources(pod: &Pod, node: &Node, pods_for_node: &[&Pod]) -> bool {
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

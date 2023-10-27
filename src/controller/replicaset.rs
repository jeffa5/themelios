use std::time::Duration;

use tracing::debug;

use crate::abstract_model::Operation;
use crate::controller::util::new_controller_ref;
use crate::controller::Controller;
use crate::resources::{
    ConditionStatus, GroupVersionKind, LabelSelector, PodConditionType, PodPhase, PodResource,
    ReplicaSetCondition, ReplicaSetConditionType, ReplicaSetResource, ReplicaSetStatus, Time,
};
use crate::state::StateView;
use crate::utils::now;

use super::util::ResourceOrOp;

const CONTROLLER_KIND: GroupVersionKind = GroupVersionKind {
    group: "apps",
    version: "v1",
    kind: "ReplicaSet",
};

#[derive(Clone, Debug)]
pub struct ReplicaSet;

#[derive(Debug, Default, Hash, Clone, PartialEq, Eq)]
pub struct ReplicaSetState;

impl Controller for ReplicaSet {
    type State = ReplicaSetState;
    fn step(
        &self,
        id: usize,
        global_state: &StateView,
        _local_state: &mut Self::State,
    ) -> Option<Operation> {
        if !global_state.controllers.contains(&id) {
            return Some(Operation::ControllerJoin(id));
        } else {
            for replicaset in global_state.replica_sets.values() {
                let pods = global_state.pods.values().collect::<Vec<_>>();
                if let Some(op) = reconcile(replicaset, &pods) {
                    return Some(op);
                }
            }
        }
        None
    }

    fn name(&self) -> String {
        "ReplicaSet".to_owned()
    }
}

fn reconcile(replicaset: &ReplicaSetResource, all_pods: &[&PodResource]) -> Option<Operation> {
    let filtered_pods = filter_active_pods(all_pods);
    let filtered_pods = claim_pods(replicaset, &filtered_pods);

    let filtered_pods = match filtered_pods {
        ResourceOrOp::Resource(r) => r,
        ResourceOrOp::Op(op) => return Some(op),
    };

    if replicaset.metadata.deletion_timestamp.is_none() {
        if let Some(op) = manage_replicas(filtered_pods, replicaset) {
            return Some(op);
        }
    }

    let new_status = calculate_status(replicaset, &filtered_pods);
    if let Some(op) = update_replicaset_status(replicaset, new_status) {
        return Some(op);
    }

    None
}

fn claim_pods<'a>(
    replicaset: &ReplicaSetResource,
    filtered_pods: &[&'a PodResource],
) -> ResourceOrOp<Vec<&'a PodResource>> {
    for pod in filtered_pods {
        // try and disown things that aren't ours
        // TODO: should we check that this is a replicaset kind?
        if pod
            .metadata
            .owner_references
            .iter()
            .any(|or| or.name == replicaset.metadata.name)
        {
            debug!("Updating pod to remove ourselves as an owner");
            let mut pod = pod.clone();
            pod.metadata
                .owner_references
                .retain(|or| or.uid != replicaset.metadata.uid);
            return ResourceOrOp::Op(Operation::UpdatePod(pod.clone()));
        }
    }

    let mut pods = Vec::new();
    for pod in filtered_pods {
        // claim any that don't have the owner reference set with controller
        // TODO: should we check that this is a replicaset kind?
        let owned = pod.metadata.owner_references.iter().any(|or| or.controller);
        if !owned {
            // our ref isn't there, set it
            debug!("Claiming pod");
            let mut rs = (*pod).clone();
            if let Some(us) = pod
                .metadata
                .owner_references
                .iter_mut()
                .find(|or| or.uid == replicaset.metadata.uid)
            {
                us.block_owner_deletion = true;
                us.controller = true;
            } else {
                pod.metadata
                    .owner_references
                    .push(new_controller_ref(&replicaset.metadata, &CONTROLLER_KIND));
            }
            return ResourceOrOp::Op(Operation::UpdatePod((*pod).clone()));
        }

        // collect the ones that we actually own
        let ours = pod
            .metadata
            .owner_references
            .iter()
            .find(|or| or.uid == replicaset.metadata.uid);
        if ours.is_some() {
            pods.push(*pod)
        }
    }
    ResourceOrOp::Resource(pods)
}

fn filter_active_pods<'a>(pods: &[&'a PodResource]) -> Vec<&'a PodResource> {
    pods.iter()
        .filter_map(|pod| if is_pod_active(pod) { Some(*pod) } else { None })
        .collect()
}

fn calculate_status(replicaset: &ReplicaSetResource, pods: &[&PodResource]) -> ReplicaSetStatus {
    let mut new_status = replicaset.status.clone();

    // Count the number of pods that have labels matching the labels of the pod
    // template of the replica set, the matching pods may have more
    // labels than are in the template. Because the label of podTemplateSpec is
    // a superset of the selector of the replica set, so the possible
    // matching pods must be part of the filteredPods.
    let mut fully_labeled_replicas_count = 0;
    let mut ready_replicas_count = 0;
    let mut available_replicas_count = 0;
    let template_label_selector = LabelSelector {
        match_labels: replicaset.spec.template.metadata.labels,
    };
    for pod in pods {
        if template_label_selector.matches(&pod.metadata.labels) {
            fully_labeled_replicas_count += 1;
        }
        if is_pod_ready(pod) {
            ready_replicas_count += 1;
            if is_pod_available(pod, replicaset.spec.min_ready_seconds, now()) {
                available_replicas_count += 1;
            }
        }
    }

    if let Some(failure_condition) =
        get_condition(&replicaset.status, ReplicaSetConditionType::ReplicaFailure)
    {
        remove_condition(&mut new_status, ReplicaSetConditionType::ReplicaFailure)
    } else {
        let diff = pods.len() as isize - replicaset.spec.replicas.unwrap_or_default() as isize;
        let reason = if diff < 0 {
            "FailedCreate"
        } else {
            "FailedDelete"
        };
        let cond = new_replicaset_condition(
            ReplicaSetConditionType::ReplicaFailure,
            ConditionStatus::True,
            reason.to_owned(),
            "TODO some manage replicas err?".to_owned(),
        );
        set_condition(&mut new_status, cond);
    }

    new_status.replicas = pods.len() as u32;
    new_status.fully_labeled_replicas = fully_labeled_replicas_count;
    new_status.ready_replicas = ready_replicas_count;
    new_status.available_replicas = available_replicas_count;
    new_status
}

fn get_condition(
    status: &ReplicaSetStatus,
    cond_type: ReplicaSetConditionType,
) -> Option<&ReplicaSetCondition> {
    status.conditions.iter().find(|c| c.r#type == cond_type)
}

fn set_condition(status: &mut ReplicaSetStatus, condition: ReplicaSetCondition) {
    if let Some(cc) = get_condition(status, condition.r#type) {
        if cc.status == condition.status && cc.reason == condition.reason {
            return;
        }
    }
    remove_condition(status, condition.r#type);
    status.conditions.push(condition);
}

fn remove_condition(status: &mut ReplicaSetStatus, cond_type: ReplicaSetConditionType) {
    status.conditions.retain(|c| c.r#type != cond_type)
}

fn new_replicaset_condition(
    cond_type: ReplicaSetConditionType,
    status: ConditionStatus,
    reason: String,
    message: String,
) -> ReplicaSetCondition {
    ReplicaSetCondition {
        status,
        r#type: cond_type,
        last_transition_time: Some(now()),
        message: Some(message),
        reason: Some(reason),
    }
}

fn is_pod_ready(pod: &PodResource) -> bool {
    pod.status
        .conditions
        .iter()
        .find(|c| c.r#type == PodConditionType::Ready)
        .map_or(false, |c| c.status == ConditionStatus::True)
}

fn is_pod_available(pod: &PodResource, min_ready_seconds: u32, now: Time) -> bool {
    if let Some(c) = pod
        .status
        .conditions
        .iter()
        .find(|c| c.r#type == PodConditionType::Ready)
    {
        if min_ready_seconds == 0
            || c.last_transition_time.map_or(false, |ltt| {
                ltt.0 + Duration::from_secs(min_ready_seconds.into()) < now.0
            })
        {
            return true;
        }
    }
    false
}

fn is_pod_active(pod: &PodResource) -> bool {
    pod.status.phase != PodPhase::Succeeded
        && pod.status.phase != PodPhase::Failed
        && pod.metadata.deletion_timestamp.is_none()
}

// updateReplicaSetStatus attempts to update the Status.Replicas of the given ReplicaSet, with a single GET/PUT retry.
fn update_replicaset_status(
    rs: &ReplicaSetResource,
    mut new_status: ReplicaSetStatus,
) -> Option<Operation> {
    if rs.status.replicas == new_status.replicas
        && rs.status.fully_labeled_replicas == new_status.fully_labeled_replicas
        && rs.status.ready_replicas == new_status.ready_replicas
        && rs.status.available_replicas == new_status.available_replicas
        && rs.metadata.generation == rs.status.observed_generation
        && rs.status.conditions == new_status.conditions
    {
        return None;
    }

    new_status.observed_generation = rs.metadata.generation;

    let mut rs = rs.clone();
    rs.status = new_status;
    Some(Operation::UpdateReplicaSetStatus(rs))
}

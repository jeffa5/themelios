use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::time::Duration;

use tracing::debug;

use crate::abstract_model::Operation;
use crate::controller::util::new_controller_ref;
use crate::controller::Controller;
use crate::resources::{
    ConditionStatus, GroupVersionKind, LabelSelector, Pod, PodConditionType, PodPhase, ReplicaSet,
    ReplicaSetCondition, ReplicaSetConditionType, ReplicaSetStatus, Time,
};
use crate::state::StateView;
use crate::utils::now;

use super::util::get_pod_from_template;
use super::util::ValOrOp;

const CONTROLLER_KIND: GroupVersionKind = GroupVersionKind {
    group: "apps",
    version: "v1",
    kind: "ReplicaSet",
};

const POD_DELETION_COST: &str = "controller.kubernetes.io/pod-deletion-cost";

#[derive(Clone, Debug)]
pub struct ReplicaSetController;

#[derive(Debug, Default, Hash, Clone, PartialEq, Eq)]
pub struct ReplicaSetControllerState;

impl Controller for ReplicaSetController {
    type State = ReplicaSetControllerState;
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

fn reconcile(replicaset: &ReplicaSet, all_pods: &[&Pod]) -> Option<Operation> {
    let filtered_pods = filter_active_pods(all_pods);
    let filtered_pods = claim_pods(replicaset, &filtered_pods);

    let filtered_pods = match filtered_pods {
        ValOrOp::Resource(r) => r,
        ValOrOp::Op(op) => return Some(op),
    };

    if replicaset.metadata.deletion_timestamp.is_none() {
        if let Some(op) = manage_replicas(&filtered_pods, replicaset) {
            return Some(op);
        }
    }

    let new_status = calculate_status(replicaset, &filtered_pods);
    if let Some(op) = update_replicaset_status(replicaset, new_status) {
        return Some(op);
    }

    None
}

fn claim_pods<'a>(replicaset: &ReplicaSet, filtered_pods: &[&'a Pod]) -> ValOrOp<Vec<&'a Pod>> {
    for pod in filtered_pods {
        if replicaset.spec.selector.matches(&pod.metadata.labels) {
            continue;
        }
        // try and disown things that aren't ours
        // TODO: should we check that this is a replicaset kind?
        if pod
            .metadata
            .owner_references
            .iter()
            .any(|or| or.name == replicaset.metadata.name)
        {
            debug!("Updating pod to remove ourselves as an owner");
            let mut pod = (*pod).clone();
            pod.metadata
                .owner_references
                .retain(|or| or.uid != replicaset.metadata.uid);
            return ValOrOp::Op(Operation::UpdatePod(pod));
        }
    }

    let mut pods = Vec::new();
    for pod in filtered_pods {
        if !replicaset.spec.selector.matches(&pod.metadata.labels) {
            continue;
        }
        // claim any that don't have the owner reference set with controller
        // TODO: should we check that this is a replicaset kind?
        let owned = pod.metadata.owner_references.iter().any(|or| or.controller);
        if !owned {
            // our ref isn't there, set it
            debug!("Claiming pod");
            let mut pod = (*pod).clone();
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
            return ValOrOp::Op(Operation::UpdatePod(pod));
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
    ValOrOp::Resource(pods)
}

fn filter_active_pods<'a>(pods: &[&'a Pod]) -> Vec<&'a Pod> {
    pods.iter()
        .filter_map(|pod| if is_pod_active(pod) { Some(*pod) } else { None })
        .collect()
}

fn calculate_status(replicaset: &ReplicaSet, pods: &[&Pod]) -> ReplicaSetStatus {
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
        match_labels: replicaset.spec.template.metadata.labels.clone(),
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

    if let Some(_failure_condition) =
        get_condition(&replicaset.status, ReplicaSetConditionType::ReplicaFailure)
    {
        remove_condition(&mut new_status, ReplicaSetConditionType::ReplicaFailure)
    } else {
        // We never get a manage replicas error so ignore adding this condition.
        // let diff = pods.len() as isize - replicaset.spec.replicas.unwrap_or_default() as isize;
        // let reason = if diff < 0 {
        //     "FailedCreate"
        // } else {
        //     "FailedDelete"
        // };
        // let cond = new_replicaset_condition(
        //     ReplicaSetConditionType::ReplicaFailure,
        //     ConditionStatus::True,
        //     reason.to_owned(),
        //     "TODO some manage replicas err?".to_owned(),
        // );
        // set_condition(&mut new_status, cond);
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

fn is_pod_ready(pod: &Pod) -> bool {
    pod.status
        .conditions
        .iter()
        .find(|c| c.r#type == PodConditionType::Ready)
        .map_or(false, |c| c.status == ConditionStatus::True)
}

fn is_pod_available(pod: &Pod, min_ready_seconds: u32, now: Time) -> bool {
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

fn is_pod_active(pod: &Pod) -> bool {
    pod.status.phase != PodPhase::Succeeded
        && pod.status.phase != PodPhase::Failed
        && pod.metadata.deletion_timestamp.is_none()
}

// updateReplicaSetStatus attempts to update the Status.Replicas of the given ReplicaSet, with a single GET/PUT retry.
fn update_replicaset_status(
    rs: &ReplicaSet,
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

// manageReplicas checks and updates replicas for the given ReplicaSet.
// Does NOT modify <filteredPods>.
// It will requeue the replica set in case of an error while creating/deleting pods.
fn manage_replicas(filtered_pods: &[&Pod], replicaset: &ReplicaSet) -> Option<Operation> {
    match filtered_pods
        .len()
        .cmp(&(replicaset.spec.replicas.unwrap_or_default() as usize))
    {
        Ordering::Less => {
            // if diff > burst_replicas {
            //     diff = burst_replicas;
            // }

            // Batch the pod creates. Batch sizes start at SlowStartInitialBatchSize
            // and double with each successful iteration in a kind of "slow start".
            // This handles attempts to start large numbers of pods that would
            // likely all fail with the same error. For example a project with a
            // low quota that attempts to create a large number of pods will be
            // prevented from spamming the API service with the pod create requests
            // after one of its pods fails.  Conveniently, this also prevents the
            // event spam that those failures would generate.
            // TODO: batch size??
            let pod = get_pod_from_template(
                &replicaset.metadata,
                &replicaset.spec.template,
                &CONTROLLER_KIND,
            );
            Some(Operation::CreatePod(pod))
        }
        Ordering::Greater => {
            // if diff > burst_replicas {
            //     diff = burst_replicas;
            // }

            let related_pods = get_indirectly_related_pods(replicaset, filtered_pods);

            let diff = filtered_pods.len() as u32 - replicaset.spec.replicas.unwrap_or_default();
            // Choose which Pods to delete, preferring those in earlier phases of startup.
            let pods_to_delete = get_pods_to_delete(filtered_pods, &related_pods, diff);

            pods_to_delete
                .first()
                .map(|pod| Operation::DeletePod((*pod).clone()))
        }
        Ordering::Equal => None,
    }
}

fn get_pods_to_delete<'a>(
    filtered_pods: &[&'a Pod],
    related_pods: &[&Pod],
    diff: u32,
) -> Vec<&'a Pod> {
    if diff < filtered_pods.len() as u32 {
        let mut pods_with_ranks =
            get_pods_ranked_by_related_pods_on_same_node(filtered_pods, related_pods);
        pods_with_ranks.sort_by(|(r1, p1), (r2, p2)| {
            // Corresponds to ActivePodsWithRanks

            // 1. Unassigned < assigned
            // If only one of the pods is unassigned, the unassigned one is smaller
            if p1.spec.node_name != p2.spec.node_name
                && (p1.spec.node_name.as_ref().map_or(true, |n| n.is_empty())
                    || p2.spec.node_name.as_ref().map_or(true, |n| n.is_empty()))
            {
                if p1.spec.node_name.as_ref().map_or(true, |n| n.is_empty()) {
                    return Ordering::Less;
                } else {
                    return Ordering::Greater;
                }
            }

            // 2. PodPending < PodUnknown < PodRunning
            if p1.status.phase as u8 != p2.status.phase as u8 {
                return (p1.status.phase as u8).cmp(&(p2.status.phase as u8));
            }

            // 3. Not ready < ready
            // If only one of the pods is not ready, the not ready one is smaller
            if is_pod_ready(p1) != is_pod_ready(p2) {
                if !is_pod_ready(p1) {
                    return Ordering::Less;
                } else {
                    return Ordering::Greater;
                }
            }

            // 4. lower pod-deletion-cost < higher pod-deletion cost
            let d1 = get_deletion_cost_from_pod_annotations(&p1.metadata.annotations);
            let d2 = get_deletion_cost_from_pod_annotations(&p2.metadata.annotations);
            if d1 != d2 {
                return d1.cmp(&d2);
            }

            // 5. Doubled up < not doubled up
            // If one of the two pods is on the same node as one or more additional
            // ready pods that belong to the same replicaset, whichever pod has more
            // colocated ready pods is less
            if r1 != r2 {
                return r1.cmp(r2).reverse();
            }

            // TODO: take availability into account when we push minReadySeconds information from deployment into pods,
            //       see https://github.com/kubernetes/kubernetes/issues/22065
            // 6. Been ready for empty time < less time < more time
            // If both pods are ready, the latest ready one is smaller
            if is_pod_ready(p1) && is_pod_ready(p2) {
                // TODO
            }

            // 7. Pods with containers with higher restart counts < lower restart counts
            if max_container_restarts(p1) != max_container_restarts(p2) {
                return max_container_restarts(p1)
                    .cmp(&max_container_restarts(p2))
                    .reverse();
            }

            // 8. Empty creation time pods < newer pods < older pods
            if p1.metadata.creation_timestamp != p2.metadata.creation_timestamp {
                // TODO
            }

            Ordering::Equal
        });

        pods_with_ranks[..diff as usize]
            .iter()
            .map(|(_, p)| *p)
            .collect()
    } else {
        filtered_pods[..diff as usize].to_vec()
    }
}

fn get_pods_ranked_by_related_pods_on_same_node<'a>(
    filtered_pods: &[&'a Pod],
    related_pods: &[&Pod],
) -> Vec<(usize, &'a Pod)> {
    let mut pods_on_node = BTreeMap::new();
    for pod in related_pods {
        if is_pod_active(pod) {
            *pods_on_node.entry(pod.spec.node_name.clone()).or_default() += 1;
        }
    }

    let mut ranks = Vec::new();
    for pod in filtered_pods.iter() {
        if let Some(n) = pods_on_node.get(&pod.spec.node_name) {
            ranks.push((*n, *pod));
        } else {
            ranks.push((0, *pod));
        }
    }

    ranks
}

// getIndirectlyRelatedPods returns all pods that are owned by any ReplicaSet
// that is owned by the given ReplicaSet's owner.
fn get_indirectly_related_pods<'a>(replicaset: &ReplicaSet, pods: &[&'a Pod]) -> Vec<&'a Pod> {
    let mut seen = BTreeSet::new();
    let mut related_pods = Vec::new();
    for rs in get_replicasets_with_same_controller(replicaset, &[]) {
        for pod in pods
            .iter()
            .filter(|p| rs.spec.selector.matches(&p.metadata.labels))
        {
            if seen.contains(&pod.metadata.uid) {
                continue;
            }

            seen.insert(&pod.metadata.uid);
            related_pods.push(*pod);
        }
    }
    related_pods
}

// getReplicaSetsWithSameController returns a list of ReplicaSets with the same
// owner as the given ReplicaSet.
fn get_replicasets_with_same_controller<'a>(
    replicaset: &ReplicaSet,
    replicasets: &[&'a ReplicaSet],
) -> Vec<&'a ReplicaSet> {
    let mut matched = Vec::new();
    for rs in replicasets {
        if replicaset.metadata.owner_references.iter().any(|or| {
            rs.metadata
                .owner_references
                .iter()
                .any(|or2| or.uid == or2.uid)
        }) {
            matched.push(*rs);
        }
    }
    matched
}

fn get_deletion_cost_from_pod_annotations(annotations: &BTreeMap<String, String>) -> i32 {
    annotations
        .get(POD_DELETION_COST)
        .and_then(|s| s.parse().ok())
        .unwrap_or_default()
}

fn max_container_restarts(pod: &Pod) -> u32 {
    pod.status
        .container_statuses
        .iter()
        .map(|c| c.restart_count)
        .max()
        .unwrap_or_default()
}

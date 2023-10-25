use std::collections::BTreeMap;

use crate::{
    abstract_model::Operation,
    resources::{
        DeploymentCondition, DeploymentResource, DeploymentStatus, DeploymentStrategyType,
        PodTemplateSpec, ReplicaSetResource,
    },
    state::StateView,
    utils::now,
};
use diff::Diff;
use tracing::debug;

use super::Controller;

// Progressing means the deployment is progressing. Progress for a deployment is
// considered when a new replica set is created or adopted, and when new pods scale
// up or old pods scale down. Progress is not estimated for paused deployments or
// when progressDeadlineSeconds is not specified.
const DEPLOYMENT_PROGRESSING: &str = "Progressing";

// FoundNewRSReason is added in a deployment when it adopts an existing replica set.
const FOUND_NEW_RSREASON: &str = "FoundNewReplicaSet";

const DEPRECATED_ROLLBACK_TO: &str = "deprecated.deployment.rollback.to";

// const KUBE_CTL_PREFIX: &str = "kubectl.kubernetes.io/";
// TODO: should use a const format thing with KUBE_CTL_PREFIX
const LAST_APPLIED_CONFIG_ANNOTATION: &str = "kubectl.kubernetes.io/last-applied-configuration";

// DefaultDeploymentUniqueLabelKey is the default key of the selector that is added
// to existing ReplicaSets (and label key that is added to its pods) to prevent the existing ReplicaSets
// to select new pods (and old pods being select by new ReplicaSet).
const DEFAULT_DEPLOYMENT_UNIQUE_LABEL_KEY: &str = "pod-template-hash";

// Available means the deployment is available, ie. at least the minimum available
// replicas required are up and running for at least minReadySeconds.
const DEPLOYMENT_AVAILABLE: &str = "Available";

// RevisionAnnotation is the revision annotation of a deployment's replica sets which records its rollout sequence
const REVISION_ANNOTATION: &str = "deployment.kubernetes.io/revision";

// RevisionHistoryAnnotation maintains the history of all old revisions that a replica set has served for a deployment.
const REVISION_HISTORY_ANNOTATION: &str = "deployment.kubernetes.io/revision-history";

// DesiredReplicasAnnotation is the desired replicas for a deployment recorded as an annotation
// in its replica sets. Helps in separating scaling events from the rollout process and for
// determining if the new replica set for a deployment is really saturated.
const DESIRED_REPLICAS_ANNOTATION: &str = "deployment.kubernetes.io/desired-replicas";

// MaxReplicasAnnotation is the maximum replicas a deployment can have at a given point, which
// is deployment.spec.replicas + maxSurge. Used by the underlying replica sets to estimate their
// proportions in case the deployment has surge replicas.
const MAX_REPLICAS_ANNOTATION: &str = "deployment.kubernetes.io/max-replicas";

// MinimumReplicasAvailable is added in a deployment when it has its minimum replicas required available.
const MINIMUM_REPLICAS_AVAILABLE: &str = "MinimumReplicasAvailable";

// MinimumReplicasUnavailable is added in a deployment when it doesn't have the minimum required replicas
// available.
const MINIMUM_REPLICAS_UNAVAILABLE: &str = "MinimumReplicasUnavailable";

// limit revision history length to 100 element (~2000 chars)
const MAX_REV_HISTORY_LENGTH_IN_CHARS: usize = 2000;

enum ResourceOrOp<R> {
    Resource(R),
    Op(Operation),
}

#[derive(Clone, Debug)]
pub struct Deployment;

pub struct DeploymentState;

impl Controller for Deployment {
    type State = DeploymentState;

    fn step(
        &self,
        id: usize,
        global_state: &StateView,
        _local_state: &mut Self::State,
    ) -> Option<Operation> {
        if !global_state.controllers.contains(&id) {
            return Some(Operation::ControllerJoin(id));
        } else {
            for deployment in global_state.deployments.values() {
                let replicasets = global_state.replica_sets.values().collect::<Vec<_>>();
                if let Some(op) = sync(&mut deployment.clone(), &replicasets) {
                    return Some(op);
                }
                // for replicaset in deployment.replicasets() {
                //     if !global_state.replica_sets.contains_key(&replicaset) {
                //         return Some(Operation::NewReplicaset(replicaset));
                //     }
                // }
            }
        }
        None
    }

    fn name(&self) -> String {
        "Deployment".to_owned()
    }
}

#[tracing::instrument(skip_all)]
fn sync(
    deployment: &mut DeploymentResource,
    replicasets: &[&ReplicaSetResource],
) -> Option<Operation> {
    debug!("Syncing deployment");
    let (new_replicaset, old_replicasets) =
        get_all_replicasets_and_sync_revision(deployment, replicasets);
    let new_replicaset = match new_replicaset {
        Some(ResourceOrOp::Resource(r)) => Some(r),
        Some(ResourceOrOp::Op(op)) => return Some(op),
        None => None,
    };

    if let Some(op) = scale(deployment, &new_replicaset, &old_replicasets) {
        return Some(op);
    }

    if deployment.spec.paused && get_rollback_to(deployment).is_none() {
        debug!("Found paused deployment");
        if let Some(op) = cleanup_deployment(&old_replicasets, deployment) {
            return Some(op);
        }
    }

    let mut all_replicasets = old_replicasets;
    if let Some(new_rs) = &new_replicaset {
        all_replicasets.push(&new_rs);
    }
    if let Some(op) = sync_deployment_status(&all_replicasets, &new_replicaset, deployment) {
        return Some(op);
    }
    None
}

// getAllReplicaSetsAndSyncRevision returns all the replica sets for the provided deployment (new and all old), with new RS's and deployment's revision updated.
//
// rsList should come from getReplicaSetsForDeployment(d).
//
//  1. Get all old RSes this deployment targets, and calculate the max revision number among them (maxOldV).
//  2. Get new RS this deployment targets (whose pod template matches deployment's), and update new RS's revision number to (maxOldV + 1),
//     only if its revision number is smaller than (maxOldV + 1). If this step failed, we'll update it in the next deployment sync loop.
//  3. Copy new RS's revision number to deployment (update deployment's revision). If this step failed, we'll update it in the next deployment sync loop.
//
// Note that currently the deployment controller is using caches to avoid querying the server for reads.
// This may lead to stale reads of replica sets, thus incorrect deployment status.
fn get_all_replicasets_and_sync_revision<'a>(
    deployment: &mut DeploymentResource,
    replicasets: &[&'a ReplicaSetResource],
) -> (
    Option<ResourceOrOp<ReplicaSetResource>>,
    Vec<&'a ReplicaSetResource>,
) {
    debug!("getting all replicasets and sync revision");
    let (_, all_old_replicasets) = find_old_replicasets(deployment, replicasets);

    let new_replicaset = get_new_replicaset(deployment, replicasets, &all_old_replicasets);

    (new_replicaset, all_old_replicasets)
}

// FindOldReplicaSets returns the old replica sets targeted by the given Deployment, with the given slice of RSes.
// Note that the first set of old replica sets doesn't include the ones with no pods, and the second set of old replica sets include all old replica sets.
fn find_old_replicasets<'a>(
    deployment: &DeploymentResource,
    replicasets: &[&'a ReplicaSetResource],
) -> (Vec<&'a ReplicaSetResource>, Vec<&'a ReplicaSetResource>) {
    let new_replicaset = find_new_replicaset(deployment, replicasets);
    let mut all_replicasets = Vec::new();
    let mut required_replicasets = Vec::new();
    for rs in replicasets {
        // filter out new replicaset
        if new_replicaset
            .as_ref()
            .map_or(false, |nrs| nrs.metadata.uid == rs.metadata.uid)
        {
            continue;
        }
        all_replicasets.push(*rs);
        if rs.spec.replicas.map_or(false, |r| r > 0) {
            required_replicasets.push(*rs);
        }
    }
    (required_replicasets, all_replicasets)
}

// FindNewReplicaSet returns the new RS this given deployment targets (the one with the same pod template).
#[tracing::instrument(skip_all)]
fn find_new_replicaset(
    deployment: &DeploymentResource,
    replicasets: &[&ReplicaSetResource],
) -> Option<ReplicaSetResource> {
    let mut replicasets = replicasets.to_vec();
    replicasets.sort_by_key(|r| r.metadata.creation_timestamp);

    for rs in replicasets {
        if equal_ignore_hash(&rs.spec.template, &deployment.spec.template) {
            debug!("found new replicaset");
            return Some(rs.clone());
        }
    }
    debug!("Didn't find new replicaset");
    None
}

// Returns a replica set that matches the intent of the given deployment. Returns nil if the new replica set doesn't exist yet.
// 1. Get existing new RS (the RS that the given deployment targets, whose pod template is the same as deployment's).
// 2. If there's existing new RS, update its revision number if it's smaller than (maxOldRevision + 1), where maxOldRevision is the max revision number among all old RSes.
// 3. If there's no existing new RS and createIfNotExisted is true, create one with appropriate revision number (maxOldRevision + 1) and replicas.
// Note that the pod-template-hash will be added to adopted RSes and pods.
fn get_new_replicaset(
    deployment: &mut DeploymentResource,
    replicasets: &[&ReplicaSetResource],
    old_replicasets: &[&ReplicaSetResource],
) -> Option<ResourceOrOp<ReplicaSetResource>> {
    let existing_new_rs = find_new_replicaset(deployment, replicasets);

    // Calculate the max revision number among all old RSes
    let max_old_revision = max_revision(old_replicasets);
    // Calculate revision number for this new replica set
    let new_revision = max_old_revision + 1;
    debug!(?max_old_revision, ?new_revision, "Got max old revision");

    // Latest replica set exists. We need to sync its annotations (includes copying all but
    // annotationsToSkip from the parent deployment, and update revision, desiredReplicas,
    // and maxReplicas) and also update the revision annotation in the deployment with the
    // latest revision.
    if let Some(existing_new_rs) = existing_new_rs {
        // Set existing new replica set's annotation
        let mut rs_copy = existing_new_rs.clone();
        debug!("Got existing replicaset");

        let annotations_updated = set_new_replicaset_annotations(
            deployment,
            &mut rs_copy,
            new_revision.to_string(),
            true,
            MAX_REV_HISTORY_LENGTH_IN_CHARS,
        );
        let min_ready_seconds_need_update =
            rs_copy.spec.min_ready_seconds != deployment.spec.min_ready_seconds;
        if annotations_updated || min_ready_seconds_need_update {
            rs_copy.spec.min_ready_seconds = deployment.spec.min_ready_seconds;
            return Some(ResourceOrOp::Op(Operation::UpdateReplicaSet(rs_copy)));
        }

        let mut needs_update = set_deployment_revision(
            deployment,
            rs_copy
                .metadata
                .annotations
                .get(REVISION_ANNOTATION)
                .cloned()
                .unwrap_or_default(),
        );

        let cond = get_deployment_condition(&deployment.status, DEPLOYMENT_PROGRESSING);
        if has_progress_deadline(deployment) && cond.is_none() {
            let message = format!("Found new replica set {}", rs_copy.metadata.name);
            let condition = new_deployment_condition(
                DEPLOYMENT_PROGRESSING.to_owned(),
                "True".to_owned(),
                FOUND_NEW_RSREASON.to_owned(),
                message,
            );
            set_deployment_condition(&mut deployment.status, condition);
            needs_update = true;
        }

        if needs_update {
            return Some(ResourceOrOp::Op(Operation::UpdateDeploymentStatus(
                deployment.clone(),
            )));
        }
        return Some(ResourceOrOp::Resource(rs_copy));
    }
    debug!("no existing replicaset match, not creating a new one");
    None
}

// syncDeploymentStatus checks if the status is up-to-date and sync it if necessary
fn sync_deployment_status(
    all_replicasets: &[&ReplicaSetResource],
    new_replicaset: &Option<ReplicaSetResource>,
    deployment: &DeploymentResource,
) -> Option<Operation> {
    debug!("Syncing deployment status");
    let new_status = calculate_status(all_replicasets, new_replicaset, deployment);
    if deployment.status != new_status {
        debug!(
            status_diff = ?deployment.status.diff(&new_status),
            "Setting new status"
        );
        let mut new_deployment = deployment.clone();
        new_deployment.status = new_status;
        Some(Operation::UpdateDeploymentStatus(new_deployment))
    } else {
        None
    }
}

// calculateStatus calculates the latest status for the provided deployment by looking into the provided replica sets.
fn calculate_status(
    all_replicasets: &[&ReplicaSetResource],
    new_replicaset: &Option<ReplicaSetResource>,
    deployment: &DeploymentResource,
) -> DeploymentStatus {
    let available_replicas = get_available_replica_count_for_replicasets(all_replicasets);
    let total_replicas = get_replica_count_for_replicasets(all_replicasets);
    // If unavailableReplicas would be negative, then that means the Deployment has more available replicas running than
    // desired, e.g. whenever it scales down. In such a case we should simply default unavailableReplicas to zero.
    let unavailable_replicas = total_replicas.saturating_sub(available_replicas);

    let mut status = DeploymentStatus {
        observed_generation: deployment.metadata.generation,
        replicas: get_actual_replica_count_for_replicasets(all_replicasets),
        updated_replicas: get_actual_replica_count_for_replicasets(
            &new_replicaset.iter().collect::<Vec<_>>(),
        ),
        ready_replicas: get_ready_replica_count_for_replicasets(all_replicasets),
        available_replicas,
        unavailable_replicas,
        collision_count: deployment.status.collision_count,
        conditions: deployment.status.conditions.clone(),
    };

    let max_unavailable = max_unavailable(deployment);
    if available_replicas >= deployment.spec.replicas - max_unavailable {
        debug!(
            available_replicas,
            deployment.spec.replicas, max_unavailable, "minimum replicas available"
        );
        let min_availability = new_deployment_condition(
            DEPLOYMENT_AVAILABLE.to_owned(),
            "True".to_owned(),
            MINIMUM_REPLICAS_AVAILABLE.to_owned(),
            "Deployment has minimum availability.".to_owned(),
        );
        set_deployment_condition(&mut status, min_availability);
    } else {
        debug!(
            available_replicas,
            deployment.spec.replicas, max_unavailable, "minimum replicas not available"
        );
        let no_min_availability = new_deployment_condition(
            DEPLOYMENT_AVAILABLE.to_owned(),
            "False".to_owned(),
            MINIMUM_REPLICAS_UNAVAILABLE.to_owned(),
            "Deployment does not have minimum availability.".to_owned(),
        );
        set_deployment_condition(&mut status, no_min_availability);
    }
    status
}

// scale scales proportionally in order to mitigate risk. Otherwise, scaling up can increase the size
// of the new replica set and scaling down can decrease the sizes of the old ones, both of which would
// have the effect of hastening the rollout progress, which could produce a higher proportion of unavailable
// replicas in the event of a problem with the rolled out template. Should run only on scaling events or
// when a deployment is paused and not during the normal rollout process.
fn scale(
    deployment: &DeploymentResource,
    new_replicaset: &Option<ReplicaSetResource>,
    old_replicasets: &[&ReplicaSetResource],
) -> Option<Operation> {
    debug!("Scaling");

    // If there is only one active replica set then we should scale that up to the full count of the
    // deployment. If there is no active replica set, then we should scale up the newest replica set.
    let active_or_latest = find_active_or_latest(new_replicaset, old_replicasets);
    if let Some(active_or_latest) = active_or_latest {
        if active_or_latest.spec.replicas == Some(deployment.spec.replicas) {
            debug!(deployment.spec.replicas, "already fully scaled");
            return None;
        }
        return scale_replicaset_and_record_event(
            &active_or_latest,
            deployment.spec.replicas,
            deployment,
        );
    }

    // If the new replica set is saturated, old replica sets should be fully scaled down.
    // This case handles replica set adoption during a saturated new replica set.
    if is_saturated(deployment, new_replicaset) {
        for old in filter_active_replicasets(old_replicasets) {
            if let Some(op) = scale_replicaset_and_record_event(old, 0, deployment) {
                return Some(op);
            }
        }
    }

    // There are old replica sets with pods and the new replica set is not saturated.
    // We need to proportionally scale all replica sets (new and old) in case of a
    // rolling deployment.
    if is_rolling_update(deployment) {
        let mut all_replicasets = old_replicasets.to_vec();
        if let Some(rs) = new_replicaset {
            all_replicasets.push(rs);
        }
        let mut all_replicasets = filter_active_replicasets(&all_replicasets);
        let all_replicasets_replicas = get_replica_count_for_replicasets(&all_replicasets);

        let mut allowed_size = 0;
        if deployment.spec.replicas > 0 {
            allowed_size = deployment.spec.replicas + max_surge(deployment);
        }

        // Number of additional replicas that can be either added or removed from the total
        // replicas count. These replicas should be distributed proportionally to the active
        // replica sets.
        let deployment_replicas_to_add = allowed_size as i32 - all_replicasets_replicas as i32;

        // The additional replicas should be distributed proportionally amongst the active
        // replica sets from the larger to the smaller in size replica set. Scaling direction
        // drives what happens in case we are trying to scale replica sets of the same size.
        // In such a case when scaling up, we should scale up newer replica sets first, and
        // when scaling down, we should scale down older replica sets first.
        if deployment_replicas_to_add > 0 {
            // sort replicasets by size newer
            all_replicasets.sort_by(|l, r| {
                if l.spec.replicas == r.spec.replicas {
                    l.metadata
                        .creation_timestamp
                        .cmp(&r.metadata.creation_timestamp)
                        .reverse()
                } else {
                    l.spec.replicas.cmp(&r.spec.replicas).reverse()
                }
            });
        } else if deployment_replicas_to_add < 0 {
            // sort replicasets by size older
            all_replicasets.sort_by(|l, r| {
                if l.spec.replicas == r.spec.replicas {
                    l.metadata
                        .creation_timestamp
                        .cmp(&r.metadata.creation_timestamp)
                } else {
                    l.spec.replicas.cmp(&r.spec.replicas).reverse()
                }
            });
        }

        // Iterate over all active replica sets and estimate proportions for each of them.
        // The absolute value of deploymentReplicasAdded should never exceed the absolute
        // value of deploymentReplicasToAdd.
        let mut deployment_replicas_added = 0;
        let mut name_to_size = BTreeMap::new();
        for rs in &all_replicasets {
            // Estimate proportions if we have replicas to add, otherwise simply populate
            // nameToSize with the current sizes for each replica set.
            if deployment_replicas_to_add != 0 {
                let proportion = get_proportion(
                    rs,
                    deployment,
                    deployment_replicas_to_add,
                    deployment_replicas_added,
                );
                let new_size = if proportion < 0 {
                    rs.spec.replicas.unwrap().saturating_sub(proportion as u32)
                } else {
                    rs.spec.replicas.unwrap() + proportion as u32
                };
                name_to_size.insert(&rs.metadata.name, new_size);
                deployment_replicas_added += proportion;
            } else {
                name_to_size.insert(&rs.metadata.name, rs.spec.replicas.unwrap());
            }
        }

        let mut updated_rss = Vec::new();
        // Update all replicasets
        for (i, rs) in all_replicasets.iter().enumerate() {
            // Add/remove any leftovers to the largest replica set.
            if i == 0 && deployment_replicas_to_add != 0 {
                let leftover = deployment_replicas_to_add - deployment_replicas_added;
                if leftover < 0 {
                    *name_to_size.get_mut(&rs.metadata.name).unwrap() -= leftover as u32;
                } else {
                    *name_to_size.get_mut(&rs.metadata.name).unwrap() += leftover as u32;
                }
            }

            // TODO: Use transactions when we have them.
            if let Some(Operation::UpdateReplicaSet(rs)) = scale_replicaset(
                rs,
                name_to_size.get(&rs.metadata.name).copied().unwrap_or(0),
                deployment,
            ) {
                updated_rss.push(rs);
            }
        }
        if !updated_rss.is_empty() {
            return Some(Operation::UpdateReplicaSets(updated_rss));
        }
    }
    None
}

fn max_revision(all_replicasets: &[&ReplicaSetResource]) -> u64 {
    all_replicasets
        .iter()
        .filter_map(|rs| {
            rs.metadata
                .annotations
                .get(REVISION_ANNOTATION)
                .and_then(|r| r.parse().ok())
        })
        .max()
        .unwrap_or(0)
}

// FindActiveOrLatest returns the only active or the latest replica set in case there is at most one active
// replica set. If there are more active replica sets, then we should proportionally scale them.
#[tracing::instrument(skip_all)]
fn find_active_or_latest(
    new_replicaset: &Option<ReplicaSetResource>,
    old_replicasets: &[&ReplicaSetResource],
) -> Option<ReplicaSetResource> {
    if new_replicaset.is_none() && old_replicasets.is_empty() {
        debug!("no replicasets to work on");
        return None;
    }

    let mut old_replicasets = old_replicasets.to_vec();
    old_replicasets.sort_by_key(|rs| rs.metadata.creation_timestamp);
    old_replicasets.reverse();

    let mut all_replicasets = old_replicasets.clone();
    if let Some(new_rs) = new_replicaset {
        all_replicasets.push(&new_rs);
    }
    let active_replicasets = filter_active_replicasets(&all_replicasets);

    match active_replicasets.len() {
        0 => {
            // If there is no active replica set then we should return the newest.
            if let Some(new_replicaset) = new_replicaset {
                debug!("using new replicaset");
                Some(new_replicaset.clone())
            } else {
                debug!("using old replicaset");
                Some(old_replicasets[0].clone())
            }
        }
        1 => {
            debug!("using first active replicaset");
            Some(active_replicasets[0].clone())
        }
        _ => None,
    }
}

fn filter_active_replicasets<'a>(
    replicasets: &[&'a ReplicaSetResource],
) -> Vec<&'a ReplicaSetResource> {
    replicasets
        .iter()
        .filter_map(|rs| {
            if rs.spec.replicas.unwrap() > 0 {
                Some(*rs)
            } else {
                None
            }
        })
        .collect()
}

// IsSaturated checks if the new replica set is saturated by comparing its size with its deployment size.
// Both the deployment and the replica set have to believe this replica set can own all of the desired
// replicas in the deployment and the annotation helps in achieving that. All pods of the ReplicaSet
// need to be available.
fn is_saturated(deployment: &DeploymentResource, replicaset: &Option<ReplicaSetResource>) -> bool {
    let Some(rs) = replicaset else {
        return false
    };
    let Some(desired_string) = rs
        .metadata
        .annotations
        .get(DESIRED_REPLICAS_ANNOTATION)
        .and_then(|ds| ds.parse::<u32>().ok()) else{ return false};
    rs.spec.replicas == Some(deployment.spec.replicas)
        && desired_string == deployment.spec.replicas
        && rs.status.available_replicas == deployment.spec.replicas
}

#[tracing::instrument(skip_all)]
fn scale_replicaset_and_record_event(
    replicaset: &ReplicaSetResource,
    new_scale: u32,
    deployment: &DeploymentResource,
) -> Option<Operation> {
    if replicaset.spec.replicas == Some(new_scale) {
        debug!("already scaled");
        return None;
    } else {
        scale_replicaset(replicaset, new_scale, deployment)
    }
}

#[tracing::instrument(skip_all)]
fn scale_replicaset(
    replicaset: &ReplicaSetResource,
    new_scale: u32,
    deployment: &DeploymentResource,
) -> Option<Operation> {
    let size_needs_update = replicaset.spec.replicas != Some(new_scale);

    let annotations_need_update = replicas_annotations_need_update(
        replicaset,
        deployment.spec.replicas,
        deployment.spec.replicas + max_surge(deployment),
    );
    if size_needs_update || annotations_need_update {
        debug!(
            from = replicaset.spec.replicas,
            to = new_scale,
            "Scaling replicaset"
        );
        let mut new_rs = replicaset.clone();
        new_rs.spec.replicas = Some(new_scale);
        set_replicas_annotations(
            &mut new_rs,
            deployment.spec.replicas,
            deployment.spec.replicas + max_surge(deployment),
        );
        return Some(Operation::UpdateReplicaSet(new_rs));
    } else {
        debug!("Not scaling replicaset");
        None
    }
}

fn replicas_annotations_need_update(
    replicaset: &ReplicaSetResource,
    desired_replicas: u32,
    max_replicas: u32,
) -> bool {
    let annotations = &replicaset.metadata.annotations;

    if annotations
        .get(DESIRED_REPLICAS_ANNOTATION)
        .map_or(true, |a| a != &desired_replicas.to_string())
    {
        return true;
    }

    if annotations
        .get(MAX_REPLICAS_ANNOTATION)
        .map_or(true, |a| a != &max_replicas.to_string())
    {
        return true;
    }
    false
}

fn set_replicas_annotations(
    replicaset: &mut ReplicaSetResource,
    desired_replicas: u32,
    max_replicas: u32,
) -> bool {
    let mut updated = false;
    let annotations = &mut replicaset.metadata.annotations;
    updated |= annotations
        .insert(
            DESIRED_REPLICAS_ANNOTATION.to_owned(),
            desired_replicas.to_string(),
        )
        .is_some();
    updated |= annotations
        .insert(MAX_REPLICAS_ANNOTATION.to_owned(), max_replicas.to_string())
        .is_some();
    updated
}

fn max_surge(deployment: &DeploymentResource) -> u32 {
    if is_rolling_update(deployment) {
        0
    } else {
        1
    }
}

fn is_rolling_update(deployment: &DeploymentResource) -> bool {
    deployment
        .spec
        .strategy
        .as_ref()
        .map_or(true, |s| s.r#type == DeploymentStrategyType::RollingUpdate)
}

fn set_deployment_revision(deployment: &mut DeploymentResource, revision: String) -> bool {
    if deployment
        .metadata
        .annotations
        .get(REVISION_ANNOTATION)
        .map_or(true, |r| r != &revision)
    {
        deployment
            .metadata
            .annotations
            .insert(REVISION_ANNOTATION.to_owned(), revision);
        true
    } else {
        false
    }
}

// cleanupDeployment is responsible for cleaning up a deployment ie. retains all but the latest N old replica sets
// where N=d.Spec.RevisionHistoryLimit. Old replica sets are older versions of the podtemplate of a deployment kept
// around by default 1) for historical reasons and 2) for the ability to rollback a deployment.
fn cleanup_deployment(
    old_replicasets: &[&ReplicaSetResource],
    deployment: &DeploymentResource,
) -> Option<Operation> {
    debug!("Cleaning up deployment");
    if has_revision_history_limit(deployment) {
        return None;
    }

    // Avoid deleting replica set with deletion timestamp set
    let mut cleanable_replicasets = old_replicasets
        .iter()
        .filter(|rs| rs.metadata.deletion_timestamp.is_none())
        .collect::<Vec<_>>();

    let diff = cleanable_replicasets.len() - deployment.spec.revision_history_limit as usize;
    if diff <= 0 {
        return None;
    }

    cleanable_replicasets.sort_by_key(|rs| {
        rs.metadata
            .annotations
            .get(REVISION_ANNOTATION)
            .and_then(|r| r.parse().ok())
            .unwrap_or(0)
    });

    for rs in cleanable_replicasets.iter().take(diff) {
        // Avoid delete replica set with non-zero replica counts
        if rs.status.replicas != 0
            || (!rs.spec.replicas.is_none() && rs.spec.replicas != Some(0))
            || rs.metadata.generation > rs.status.observed_generation
            || rs.metadata.deletion_timestamp.is_some()
        {
            continue;
        }
        debug!("Trying to cleanup replica set for deployment");
        return Some(Operation::DeleteReplicaSet((**rs).clone()));
    }
    None
}

// HasRevisionHistoryLimit checks if the Deployment d is expected to keep a specified number of
// old replicaSets. These replicaSets are mainly kept with the purpose of rollback.
// The RevisionHistoryLimit can start from 0 (no retained replicasSet). When set to math.MaxInt32,
// the Deployment will keep all revisions.
fn has_revision_history_limit(deployment: &DeploymentResource) -> bool {
    deployment.spec.revision_history_limit != u32::MAX
}

fn get_available_replica_count_for_replicasets(replicasets: &[&ReplicaSetResource]) -> u32 {
    replicasets
        .iter()
        .map(|rs| rs.status.available_replicas)
        .sum()
}

fn get_replica_count_for_replicasets(replicasets: &[&ReplicaSetResource]) -> u32 {
    replicasets.iter().filter_map(|rs| rs.spec.replicas).sum()
}

// ResolveFenceposts resolves both maxSurge and maxUnavailable. This needs to happen in one
// step. For example:
//
// 2 desired, max unavailable 1%, surge 0% - should scale old(-1), then new(+1), then old(-1), then new(+1)
// 1 desired, max unavailable 1%, surge 0% - should scale old(-1), then new(+1)
// 2 desired, max unavailable 25%, surge 1% - should scale new(+1), then old(-1), then new(+1), then old(-1)
// 1 desired, max unavailable 25%, surge 1% - should scale new(+1), then old(-1)
// 2 desired, max unavailable 0%, surge 1% - should scale new(+1), then old(-1), then new(+1), then old(-1)
// 1 desired, max unavailable 0%, surge 1% - should scale new(+1), then old(-1)
fn max_unavailable(deployment: &DeploymentResource) -> u32 {
    if is_rolling_update(deployment) || deployment.spec.replicas == 0 {
        return 0;
    }

    let max_unavailable = deployment
        .spec
        .strategy
        .as_ref()
        .and_then(|s| {
            s.rolling_update.as_ref().map(|r| {
                r.max_unavailable
                    .scaled_value(deployment.spec.replicas, true)
            })
        })
        .unwrap_or(0);
    if max_unavailable > deployment.spec.replicas {
        deployment.spec.replicas
    } else {
        max_unavailable
    }
}

fn new_deployment_condition(
    cond_type: String,
    status: String,
    reason: String,
    message: String,
) -> DeploymentCondition {
    DeploymentCondition {
        r#type: cond_type,
        status,
        last_update_time: Some(now()),
        last_transition_time: Some(now()),
        reason: Some(reason),
        message: Some(message),
    }
}

// SetDeploymentCondition updates the deployment to include the provided condition. If the condition that
// we are about to add already exists and has the same status and reason then we are not going to update.
fn set_deployment_condition(status: &mut DeploymentStatus, mut condition: DeploymentCondition) {
    let current_condition = get_deployment_condition(status, &condition.r#type);
    if let Some(cc) = current_condition {
        if cc.status == condition.status && cc.reason == condition.reason {
            return;
        }

        // Do not update lastTransitionTime if the status of the condition doesn't change.
        if cc.status == condition.status {
            condition.last_transition_time = cc.last_transition_time;
        }

        debug!(current_condition=?cc, new_condition=?condition, "Setting deployment condition");

        let mut new_conditions = filter_out_condition(&status.conditions, &condition.r#type);
        new_conditions.push(&condition);
        status.conditions = new_conditions.into_iter().cloned().collect();
    }
}

fn get_actual_replica_count_for_replicasets(replicasets: &[&ReplicaSetResource]) -> u32 {
    replicasets.iter().map(|rs| rs.status.replicas).sum()
}

fn get_ready_replica_count_for_replicasets(replicasets: &[&ReplicaSetResource]) -> u32 {
    replicasets.iter().map(|rs| rs.status.ready_replicas).sum()
}

// GetDeploymentCondition returns the condition with the provided type.
fn get_deployment_condition<'a>(
    status: &'a DeploymentStatus,
    cond_type: &str,
) -> Option<&'a DeploymentCondition> {
    status.conditions.iter().find(|c| c.r#type == cond_type)
}

// filterOutCondition returns a new slice of deployment conditions without conditions with the provided type.
fn filter_out_condition<'a>(
    conditions: &'a [DeploymentCondition],
    cond_type: &str,
) -> Vec<&'a DeploymentCondition> {
    conditions
        .iter()
        .filter(|c| c.r#type != cond_type)
        .collect()
}

fn equal_ignore_hash(t1: &PodTemplateSpec, t2: &PodTemplateSpec) -> bool {
    let mut t1 = t1.clone();
    let mut t2 = t2.clone();
    t1.metadata
        .labels
        .remove(DEFAULT_DEPLOYMENT_UNIQUE_LABEL_KEY);
    t2.metadata
        .labels
        .remove(DEFAULT_DEPLOYMENT_UNIQUE_LABEL_KEY);
    t1 == t2
}

// SetNewReplicaSetAnnotations sets new replica set's annotations appropriately by updating its revision and
// copying required deployment annotations to it; it returns true if replica set's annotation is changed.
fn set_new_replicaset_annotations(
    deployment: &DeploymentResource,
    new_replicaset: &mut ReplicaSetResource,
    new_revision: String,
    exists: bool,
    rev_history_limit_in_chars: usize,
) -> bool {
    // First, copy deployment's annotations (except for apply and revision annotations)
    let mut annotation_changed =
        copy_deployment_annotations_to_replica_set(deployment, new_replicaset);
    // Then, update replica set's revision annotation
    let old_revision = new_replicaset
        .metadata
        .annotations
        .get(REVISION_ANNOTATION)
        .cloned();
    // The newRS's revision should be the greatest among all RSes. Usually, its revision number is newRevision (the max revision number
    // of all old RSes + 1). However, it's possible that some of the old RSes are deleted after the newRS revision being updated, and
    // newRevision becomes smaller than newRS's revision. We should only update newRS revision when it's smaller than newRevision.
    let old_revision_int = match old_revision.as_ref().map(|r| r.parse().ok()) {
        Some(Some(r)) => r,
        Some(None) => {
            debug!("Updating replica set revision oldrevision not int");
            return false;
        }
        None => 0,
    };
    let Ok(new_revision_int) = new_revision.parse() else {
        debug!("Updating replica set revision newrevision not int");
        return false;
    };

    if old_revision_int < new_revision_int {
        new_replicaset
            .metadata
            .annotations
            .insert(REVISION_ANNOTATION.to_owned(), new_revision.clone());
        annotation_changed = true;
        debug!(new_revision, "Updating replica set revision");
    }
    // If a revision annotation already existed and this replica set was updated with a new revision
    // then that means we are rolling back to this replica set. We need to preserve the old revisions
    // for historical information.
    if old_revision.is_some() && old_revision_int < new_revision_int {
        let revision_history_annotation = new_replicaset
            .metadata
            .annotations
            .get(REVISION_HISTORY_ANNOTATION);
        let mut old_revisions = revision_history_annotation
            .cloned()
            .unwrap_or_default()
            .split(",")
            .map(|s| s.to_owned())
            .collect::<Vec<String>>();
        if old_revisions[0].is_empty() {
            new_replicaset.metadata.annotations.insert(
                REVISION_HISTORY_ANNOTATION.to_owned(),
                old_revision.unwrap().clone(),
            );
        } else {
            let mut total_len = revision_history_annotation.map_or(0, |a| a.len())
                + old_revision.as_ref().map_or(0, |r| r.len())
                + 1;
            // index for the starting position in oldRevisions
            let mut start = 0;
            while total_len > rev_history_limit_in_chars && start < old_revisions.len() {
                total_len -= old_revisions[start].len() - 1;
                start += 1;
            }
            if total_len <= rev_history_limit_in_chars {
                old_revisions = old_revisions[start..].to_vec();
                old_revisions.push(old_revision.unwrap());
                new_replicaset.metadata.annotations.insert(
                    REVISION_HISTORY_ANNOTATION.to_owned(),
                    old_revisions.join(","),
                );
            } else {
                debug!(
                    rev_history_limit_in_chars,
                    "Not appending revision due to revision history length limit reached"
                );
            }
        }
    }
    // If the new replica set is about to be created, we need to add replica annotations to it.
    if !exists
        && set_replicas_annotations(
            new_replicaset,
            deployment.spec.replicas,
            deployment.spec.replicas + max_surge(deployment),
        )
    {
        annotation_changed = true;
    }
    annotation_changed
}

fn copy_deployment_annotations_to_replica_set(
    deployment: &DeploymentResource,
    replicaset: &mut ReplicaSetResource,
) -> bool {
    let mut annotations_changed = false;
    for (k, v) in &deployment.metadata.annotations {
        // newRS revision is updated automatically in getNewReplicaSet, and the deployment's revision number is then updated
        // by copying its newRS revision number. We should not copy deployment's revision to its newRS, since the update of
        // deployment revision number may fail (revision becomes stale) and the revision number in newRS is more reliable.
        if skip_copy_annotation(&k)
            || replicaset
                .metadata
                .annotations
                .get(k)
                .map_or(false, |av| av == v)
        {
            continue;
        }
        replicaset.metadata.annotations.insert(k.clone(), v.clone());
        annotations_changed = true;
    }
    annotations_changed
}

fn skip_copy_annotation(key: &str) -> bool {
    [
        LAST_APPLIED_CONFIG_ANNOTATION,
        REVISION_ANNOTATION,
        REVISION_HISTORY_ANNOTATION,
        DESIRED_REPLICAS_ANNOTATION,
        MAX_REPLICAS_ANNOTATION,
        DEPRECATED_ROLLBACK_TO,
    ]
    .contains(&key)
}

fn has_progress_deadline(deployment: &DeploymentResource) -> bool {
    deployment.spec.progress_deadline_seconds != Some(u32::MAX)
}

fn get_rollback_to(deployment: &DeploymentResource) -> Option<RollbackConfig> {
    // Extract the annotation used for round-tripping the deprecated RollbackTo field.
    let revision = deployment.metadata.annotations.get(DEPRECATED_ROLLBACK_TO);
    if let Some(revision) = revision {
        if revision == "" {
            return None;
        }
        let Ok(revision64) = revision.parse::<u64>() else {return None};
        Some(RollbackConfig {
            revision: revision64,
        })
    } else {
        None
    }
}

pub struct RollbackConfig {
    revision: u64,
}

// GetProportion will estimate the proportion for the provided replica set using 1. the current size
// of the parent deployment, 2. the replica count that needs be added on the replica sets of the
// deployment, and 3. the total replicas added in the replica sets of the deployment so far.
fn get_proportion(
    replicaset: &ReplicaSetResource,
    deployment: &DeploymentResource,
    deployment_replicas_to_add: i32,
    deployment_replicas_added: i32,
) -> i32 {
    if replicaset.spec.replicas.map_or(true, |r| r == 0)
        || deployment_replicas_to_add == 0
        || deployment_replicas_to_add == deployment_replicas_added
    {
        return 0;
    }

    let rs_fraction = get_replicaset_fraction(replicaset, deployment);
    let allowed = deployment_replicas_to_add - deployment_replicas_added;

    if deployment_replicas_to_add > 0 {
        // Use the minimum between the replica set fraction and the maximum allowed replicas
        // when scaling up. This way we ensure we will not scale up more than the allowed
        // replicas we can add.
        return rs_fraction.min(allowed);
    }

    // Use the maximum between the replica set fraction and the maximum allowed replicas
    // when scaling down. This way we ensure we will not scale down more than the allowed
    // replicas we can remove.
    rs_fraction.max(allowed)
}

fn get_replicaset_fraction(
    replicaset: &ReplicaSetResource,
    deployment: &DeploymentResource,
) -> i32 {
    // If we are scaling down to zero then the fraction of this replica set is its whole size (negative)
    if deployment.spec.replicas == 0 {
        return -(replicaset.spec.replicas.unwrap() as i32);
    }

    let deployment_replicas = deployment.spec.replicas + max_surge(deployment);
    // If we cannot find the annotation then fallback to the current deployment size. Note that this
    // will not be an accurate proportion estimation in case other replica sets have different values
    // which means that the deployment was scaled at some point but we at least will stay in limits
    // due to the min-max comparisons in getProportion.
    let annotated_replicas =
        get_max_replicas_annotation(replicaset).unwrap_or(deployment.status.replicas);

    // We should never proportionally scale up from zero which means rs.spec.replicas and annotatedReplicas
    // will never be zero here.
    let new_replicaset_size = (replicaset.spec.replicas.unwrap() * deployment_replicas) as f64
        / annotated_replicas as f64;
    new_replicaset_size.round() as i32 - replicaset.spec.replicas.unwrap() as i32
}

fn get_max_replicas_annotation(replicaset: &ReplicaSetResource) -> Option<u32> {
    replicaset
        .metadata
        .annotations
        .get(MAX_REPLICAS_ANNOTATION)
        .and_then(|r| r.parse().ok())
}

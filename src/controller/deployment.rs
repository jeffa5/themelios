use crate::{
    abstract_model::Operation,
    resources::{DeploymentResource, DeploymentStatus, ReplicaSetResource},
    state::StateView,
};
use tracing::debug;

use super::Controller;

// RevisionAnnotation is the revision annotation of a deployment's replica sets which records its rollout sequence
const REVISION_ANNOTATION: &str = "deployment.kubernetes.io/revision";
// DesiredReplicasAnnotation is the desired replicas for a deployment recorded as an annotation
// in its replica sets. Helps in separating scaling events from the rollout process and for
// determining if the new replica set for a deployment is really saturated.
const DESIRED_REPLICAS_ANNOTATION: &str = "deployment.kubernetes.io/desired-replicas";
// MaxReplicasAnnotation is the maximum replicas a deployment can have at a given point, which
// is deployment.spec.replicas + maxSurge. Used by the underlying replica sets to estimate their
// proportions in case the deployment has surge replicas.
const MAX_REPLICAS_ANNOTATION: &str = "deployment.kubernetes.io/max-replicas";

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

    // TODO: Clean up the deployment when it's paused and no rollback is in flight.

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
        if rs.spec.template == deployment.spec.template {
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
        let annotations_updated = false;
        //     set_new_replicaset_annotations(
        //     deployment,
        //     rs_copy,
        //     new_revision,
        //     true,
        //     maxRevHistoryLengthInChars,
        // );
        let min_ready_seconds_need_update =
            rs_copy.spec.min_ready_seconds != deployment.spec.min_ready_seconds;
        if annotations_updated || min_ready_seconds_need_update {
            rs_copy.spec.min_ready_seconds = deployment.spec.min_ready_seconds;
            return Some(ResourceOrOp::Op(Operation::UpdateReplicaSet(rs_copy)));
        }

        let needs_update = set_deployment_revision(
            deployment,
            rs_copy
                .metadata
                .annotations
                .get(REVISION_ANNOTATION)
                .cloned()
                .unwrap_or_default(),
        );
        // TODO: apply a condition
        // let cond = get_deployment_condition(deployment.status, DEPLOYMENT_PROGRESSING);
        // if has_progress_deadline(deployment) && cond.is_none() {
        // }

        if needs_update {
            return Some(ResourceOrOp::Op(Operation::UpdateDeployment(
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
    let new_status = calculate_status(all_replicasets, new_replicaset, deployment);
    if deployment.status != new_status {
        let mut new_deployment = deployment.clone();
        new_deployment.status = new_status;
        Some(Operation::UpdateDeployment(new_deployment))
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
    // TODO
    deployment.status.clone()
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
            debug!("already fully scaled");
            // already fully scaled
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

    None
    // TODO
    // There are old replica sets with pods and the new replica set is not saturated.
    // We need to proportionally scale all replica sets (new and old) in case of a
    // rolling deployment.
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
    if let Some(binding)  = new_replicaset {
        all_replicasets.push(&binding);
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
    let mut scaled = false;
    if size_needs_update || annotations_need_update {
        debug!(from=replicaset.spec.replicas, to=new_scale, "Scaling replicaset");
        let oldscale = replicaset.spec.replicas;
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
    false
}

fn set_deployment_revision(deployment: &mut DeploymentResource, revision: String) -> bool {
    let mut updated = false;
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

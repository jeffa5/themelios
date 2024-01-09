use std::{collections::BTreeMap, hash::Hash};

use crate::{
    abstract_model::ControllerAction,
    controller::util::new_controller_ref,
    hasher::FnvHasher,
    resources::{
        ConditionStatus, Deployment, DeploymentCondition, DeploymentConditionType,
        DeploymentStatus, DeploymentStrategyType, LabelSelector, Pod, PodTemplateSpec, ReplicaSet,
        ReplicaSetCondition, ReplicaSetConditionType,
    },
    state::StateView,
    utils::now,
};
use diff::Diff;
use tracing::debug;

use super::Controller;

// PausedDeployReason is added in a deployment when it is paused. Lack of progress shouldn't be
// estimated once a deployment is paused.
const PAUSED_DEPLOY_REASON: &str = "DeploymentPaused";

// ResumedDeployReason is added in a deployment when it is resumed. Useful for not failing accidentally
// deployments that paused amidst a rollout and are bounded by a deadline.
const RESUMED_DEPLOY_REASON: &str = "DeploymentResumed";

// ReplicaSetUpdatedReason is added in a deployment when one of its replica sets is updated as part
// of the rollout process.
const REPLICASET_UPDATED_REASON: &str = "ReplicaSetUpdated";

// NewRSAvailableReason is added in a deployment when its newest replica set is made available
// ie. the number of new pods that have passed readiness checks and run for at least minReadySeconds
// is at least the minimum available pods that need to run for the deployment.
const NEW_RSAVAILABLE_REASON: &str = "NewReplicaSetAvailable";
// TimedOutReason is added in a deployment when its newest replica set fails to show any progress
// within the given deadline (progressDeadlineSeconds).
const TIMED_OUT_REASON: &str = "ProgressDeadlineExceeded";

// FoundNewRSReason is added in a deployment when it adopts an existing replica set.
const FOUND_NEW_RSREASON: &str = "FoundNewReplicaSet";

const DEPRECATED_ROLLBACK_TO: &str = "deprecated.deployment.rollback.to";

// const KUBE_CTL_PREFIX: &str = "kubectl.kubernetes.io/";
// TODO: should use a const format thing with KUBE_CTL_PREFIX
pub const LAST_APPLIED_CONFIG_ANNOTATION: &str = "kubectl.kubernetes.io/last-applied-configuration";

// DefaultDeploymentUniqueLabelKey is the default key of the selector that is added
// to existing ReplicaSets (and label key that is added to its pods) to prevent the existing ReplicaSets
// to select new pods (and old pods being select by new ReplicaSet).
pub const DEFAULT_DEPLOYMENT_UNIQUE_LABEL_KEY: &str = "pod-template-hash";

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

#[derive(Clone, Debug)]
pub struct DeploymentController;

#[derive(Debug, Default, Hash, Clone, PartialEq, Eq)]
pub struct DeploymentControllerState;

#[derive(Debug)]
pub enum DeploymentControllerAction {
    RequeueDeployment(Deployment),
    UpdateDeployment(Deployment),
    UpdateDeploymentStatus(Deployment),

    CreateReplicaSet(ReplicaSet),
    UpdateReplicaSet(ReplicaSet),
    DeleteReplicaSet(ReplicaSet),
    UpdateReplicaSets(Vec<ReplicaSet>),
}

impl From<DeploymentControllerAction> for ControllerAction {
    fn from(value: DeploymentControllerAction) -> Self {
        match value {
            DeploymentControllerAction::RequeueDeployment(d) => {
                ControllerAction::RequeueDeployment(d)
            }
            DeploymentControllerAction::UpdateDeployment(d) => {
                ControllerAction::UpdateDeployment(d)
            }
            DeploymentControllerAction::UpdateDeploymentStatus(d) => {
                ControllerAction::UpdateDeploymentStatus(d)
            }
            DeploymentControllerAction::CreateReplicaSet(rs) => {
                ControllerAction::CreateReplicaSet(rs)
            }
            DeploymentControllerAction::UpdateReplicaSet(rs) => {
                ControllerAction::UpdateReplicaSet(rs)
            }
            DeploymentControllerAction::DeleteReplicaSet(rs) => {
                ControllerAction::DeleteReplicaSet(rs)
            }
            DeploymentControllerAction::UpdateReplicaSets(rss) => {
                ControllerAction::UpdateReplicaSets(rss)
            }
        }
    }
}

type ValOrOp<V> = super::util::ValOrOp<V, DeploymentControllerAction>;

impl Controller for DeploymentController {
    type State = DeploymentControllerState;

    type Action = DeploymentControllerAction;

    fn step(
        &self,
        _id: usize,
        global_state: &StateView,
        _local_state: &mut Self::State,
    ) -> Option<DeploymentControllerAction> {
        for deployment in global_state.deployments.iter() {
            let replicasets = global_state.replicasets.iter().collect::<Vec<_>>();
            let pod_map = BTreeMap::new();
            debug!(rev = ?global_state.revision, "Reconciling state");
            if let Some(op) = reconcile(deployment, &replicasets, &pod_map) {
                return Some(op);
            }

            // for replicaset in deployment.replicasets() {
            //     if !global_state.replicasets.contains_key(&replicaset) {
            //         return Some(Operation::NewReplicaset(replicaset));
            //     }
            // }
        }
        None
    }

    fn name(&self) -> String {
        "Deployment".to_owned()
    }
}

fn reconcile(
    deployment: &Deployment,
    all_replicasets: &[&ReplicaSet],
    pod_map: &BTreeMap<String, Vec<Pod>>,
) -> Option<DeploymentControllerAction> {
    let everything = LabelSelector::default();
    if deployment.spec.selector == everything {
        debug!("Found selector matching everything");
        if deployment.status.observed_generation < deployment.metadata.generation {
            let mut deployment = deployment.clone();
            deployment.status.observed_generation = deployment.metadata.generation;
            return Some(DeploymentControllerAction::UpdateDeploymentStatus(
                deployment,
            ));
        }
        return None;
    }

    // TODO: handle podmap thing

    let replicasets = match claim_replicasets(deployment, all_replicasets) {
        ValOrOp::Resource(r) => r,
        ValOrOp::Op(op) => return Some(op),
    };

    if deployment.metadata.deletion_timestamp.is_some() {
        return sync_status_only(&mut deployment.clone(), &replicasets, all_replicasets);
    }

    // Update deployment conditions with an Unknown condition when pausing/resuming
    // a deployment. In this way, we can be sure that we won't timeout when a user
    // resumes a Deployment with a set progressDeadlineSeconds.
    if let Some(op) = check_paused_conditions(&mut deployment.clone()) {
        return Some(op);
    }

    if deployment.spec.paused {
        return sync(&mut deployment.clone(), &replicasets, all_replicasets);
    }

    // rollback is not re-entrant in case the underlying replica sets are updated with a new
    // revision so we should ensure that we won't proceed to update replica sets until we
    // make sure that the deployment has cleaned up its rollback spec in subsequent enqueues.
    if get_rollback_to(deployment).is_some() {
        return rollback(&mut deployment.clone(), &replicasets, all_replicasets);
    }

    let scaling_event = is_scaling_event(&mut deployment.clone(), &replicasets, all_replicasets);
    let scaling_event = match scaling_event {
        ValOrOp::Resource(r) => r,
        ValOrOp::Op(op) => return Some(op),
    };
    if scaling_event {
        return sync(&mut deployment.clone(), &replicasets, all_replicasets);
    }

    match deployment
        .spec
        .strategy
        .as_ref()
        .map(|s| s.r#type)
        .unwrap_or_default()
    {
        DeploymentStrategyType::Recreate => rollout_recreate(
            &mut deployment.clone(),
            &replicasets,
            all_replicasets,
            pod_map,
        ),
        DeploymentStrategyType::RollingUpdate => {
            rollout_rolling(&mut deployment.clone(), &replicasets, all_replicasets)
        }
    }
}

fn claim_replicasets<'a>(
    deployment: &Deployment,
    all_replicasets: &[&'a ReplicaSet],
) -> ValOrOp<Vec<&'a ReplicaSet>> {
    // trim down replicasets to those for this deployment
    let (replicaset_matches, not_our_replicasets): (Vec<_>, Vec<_>) = all_replicasets
        .iter()
        .copied()
        .partition(|rs| deployment.spec.selector.matches(&rs.metadata.labels));

    for rs in not_our_replicasets {
        // try and disown things that aren't ours
        // TODO: should we check that this is a deployment kind?
        if rs
            .metadata
            .owner_references
            .iter()
            .any(|or| or.name == deployment.metadata.name)
        {
            debug!("Updating replicaset to remove ourselves as an owner");
            let mut rs = rs.clone();
            rs.metadata
                .owner_references
                .retain(|or| or.uid != deployment.metadata.uid);
            return ValOrOp::Op(DeploymentControllerAction::UpdateReplicaSet(rs));
        }
    }

    let mut replicasets = Vec::new();
    for rs in &replicaset_matches {
        // claim any that don't have the owner reference set with controller
        // TODO: should we check that this is a deployment kind?
        let owned = rs.metadata.owner_references.iter().any(|or| or.controller);
        if !owned {
            // our ref isn't there, set it
            debug!("Claiming replicaset");
            let mut rs = (*rs).clone();
            if let Some(us) = rs
                .metadata
                .owner_references
                .iter_mut()
                .find(|or| or.uid == deployment.metadata.uid)
            {
                us.block_owner_deletion = true;
                us.controller = true;
            } else {
                rs.metadata
                    .owner_references
                    .push(new_controller_ref(&deployment.metadata, &Deployment::GVK));
            }
            return ValOrOp::Op(DeploymentControllerAction::UpdateReplicaSet(rs));
        }

        // collect the ones that we actually own
        let ours = rs
            .metadata
            .owner_references
            .iter()
            .find(|or| or.uid == deployment.metadata.uid);
        if ours.is_some() {
            replicasets.push(*rs)
        }
    }
    ValOrOp::Resource(replicasets)
}

fn sync_status_only(
    deployment: &mut Deployment,
    replicasets: &[&ReplicaSet],
    replicasets_in_ns: &[&ReplicaSet],
) -> Option<DeploymentControllerAction> {
    let (new_replicaset, old_replicasets) =
        get_all_replicasets_and_sync_revision(deployment, replicasets, replicasets_in_ns, false);
    let new_replicaset = match new_replicaset {
        Some(ValOrOp::Resource(r)) => Some(r),
        Some(ValOrOp::Op(op)) => return Some(op),
        None => None,
    };
    let mut all_rss = old_replicasets.clone();
    if let Some(new_replicaset) = &new_replicaset {
        all_rss.push(new_replicaset);
    }
    sync_deployment_status(&all_rss, &new_replicaset, deployment)
}

// checkPausedConditions checks if the given deployment is paused or not and adds an appropriate condition.
// These conditions are needed so that we won't accidentally report lack of progress for resumed deployments
// that were paused for longer than progressDeadlineSeconds.
fn check_paused_conditions(deployment: &mut Deployment) -> Option<DeploymentControllerAction> {
    debug!("Checking paused conditions");
    if has_progress_deadline(deployment) {
        return None;
    }
    let cond = get_deployment_condition(&deployment.status, DeploymentConditionType::Progressing);
    if cond.map_or(false, |c| c.reason.as_ref().unwrap() == TIMED_OUT_REASON) {
        // If we have reported lack of progress, do not overwrite it with a paused condition.
        return None;
    }

    let paused_cond_exists = cond.map_or(false, |c| {
        c.reason.as_ref().unwrap() == PAUSED_DEPLOY_REASON
    });
    if deployment.spec.paused && !paused_cond_exists {
        let cond = new_deployment_condition(
            DeploymentConditionType::Progressing,
            ConditionStatus::Unknown,
            PAUSED_DEPLOY_REASON.to_owned(),
            "Deployment is paused".to_owned(),
        );
        set_deployment_condition(&mut deployment.status, cond);
        Some(DeploymentControllerAction::UpdateDeploymentStatus(
            deployment.clone(),
        ))
    } else if !deployment.spec.paused && paused_cond_exists {
        let cond = new_deployment_condition(
            DeploymentConditionType::Progressing,
            ConditionStatus::Unknown,
            RESUMED_DEPLOY_REASON.to_owned(),
            "Deployment is resumed".to_owned(),
        );
        set_deployment_condition(&mut deployment.status, cond);
        Some(DeploymentControllerAction::UpdateDeploymentStatus(
            deployment.clone(),
        ))
    } else {
        None
    }
}

#[tracing::instrument(skip_all)]
fn sync(
    deployment: &mut Deployment,
    replicasets: &[&ReplicaSet],
    replicasets_in_ns: &[&ReplicaSet],
) -> Option<DeploymentControllerAction> {
    debug!("Syncing deployment");
    let (new_replicaset, old_replicasets) =
        get_all_replicasets_and_sync_revision(deployment, replicasets, replicasets_in_ns, false);
    let new_replicaset = match new_replicaset {
        Some(ValOrOp::Resource(r)) => Some(r),
        Some(ValOrOp::Op(op)) => return Some(op),
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
        all_replicasets.push(new_rs);
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
    deployment: &mut Deployment,
    replicasets: &[&'a ReplicaSet],
    replicasets_in_ns: &[&ReplicaSet],
    create_if_not_existed: bool,
) -> (Option<ValOrOp<ReplicaSet>>, Vec<&'a ReplicaSet>) {
    debug!("getting all replicasets and sync revision");
    let (_, all_old_replicasets) = find_old_replicasets(deployment, replicasets);

    let new_replicaset = get_new_replicaset(
        deployment,
        replicasets,
        &all_old_replicasets,
        replicasets_in_ns,
        create_if_not_existed,
    );

    (new_replicaset, all_old_replicasets)
}

// FindOldReplicaSets returns the old replica sets targeted by the given Deployment, with the given slice of RSes.
// Note that the first set of old replica sets doesn't include the ones with no pods, and the second set of old replica sets include all old replica sets.
pub fn find_old_replicasets<'a>(
    deployment: &Deployment,
    replicasets: &[&'a ReplicaSet],
) -> (Vec<&'a ReplicaSet>, Vec<&'a ReplicaSet>) {
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
fn find_new_replicaset(deployment: &Deployment, replicasets: &[&ReplicaSet]) -> Option<ReplicaSet> {
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
    deployment: &mut Deployment,
    replicasets: &[&ReplicaSet],
    old_replicasets: &[&ReplicaSet],
    replicasets_in_ns: &[&ReplicaSet],
    create_if_not_existed: bool,
) -> Option<ValOrOp<ReplicaSet>> {
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
            return Some(ValOrOp::Op(DeploymentControllerAction::UpdateReplicaSet(
                rs_copy,
            )));
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

        let cond =
            get_deployment_condition(&deployment.status, DeploymentConditionType::Progressing);
        if has_progress_deadline(deployment) && cond.is_none() {
            let message = format!("Found new replica set {}", rs_copy.metadata.name);
            let condition = new_deployment_condition(
                DeploymentConditionType::Progressing,
                ConditionStatus::True,
                FOUND_NEW_RSREASON.to_owned(),
                message,
            );
            set_deployment_condition(&mut deployment.status, condition);
            needs_update = true;
        }

        if needs_update {
            debug!("Existing replicaset needs update");
            return Some(ValOrOp::Op(
                DeploymentControllerAction::UpdateDeploymentStatus(deployment.clone()),
            ));
        }
        return Some(ValOrOp::Resource(rs_copy));
    }

    if !create_if_not_existed {
        return None;
    }

    debug!("no existing replicaset match, not creating a new one");

    // new ReplicaSet does not exist, create one.
    let mut new_rs_template = deployment.spec.template.clone();
    let pod_template_spec_hash = compute_hash(&new_rs_template, deployment.status.collision_count);
    new_rs_template.metadata.labels = deployment.spec.template.metadata.labels.clone();
    new_rs_template.metadata.labels.insert(
        DEFAULT_DEPLOYMENT_UNIQUE_LABEL_KEY.to_owned(),
        pod_template_spec_hash.clone(),
    );

    // Add podTemplateHash label to selector
    let mut new_rs_selector = deployment.spec.selector.clone();
    new_rs_selector.match_labels.insert(
        DEFAULT_DEPLOYMENT_UNIQUE_LABEL_KEY.to_owned(),
        pod_template_spec_hash.clone(),
    );

    // Create new ReplicaSet
    let mut new_rs = ReplicaSet {
        metadata: crate::resources::Metadata {
            // Make the name deterministic, to ensure idempotence
            name: format!("{}-{}", deployment.metadata.name, pod_template_spec_hash),
            namespace: deployment.metadata.namespace.clone(),
            owner_references: vec![new_controller_ref(&deployment.metadata, &Deployment::GVK)],
            labels: new_rs_template.metadata.labels.clone(),
            ..Default::default()
        },
        spec: crate::resources::ReplicaSetSpec {
            min_ready_seconds: deployment.spec.min_ready_seconds,
            selector: new_rs_selector,
            template: new_rs_template.clone(),
            ..Default::default()
        },
        status: crate::resources::ReplicaSetStatus::default(),
    };
    let mut all_rss = old_replicasets.to_vec();
    all_rss.push(&new_rs);

    let new_replicas_count = new_rs_new_replicas(deployment, &all_rss, &new_rs);

    new_rs.spec.replicas = Some(new_replicas_count);
    // Set new replica set's annotation
    set_new_replicaset_annotations(
        deployment,
        &mut new_rs,
        new_revision.to_string(),
        false,
        MAX_REV_HISTORY_LENGTH_IN_CHARS,
    );
    // Create the new ReplicaSet. If it already exists, then we need to check for possible
    // hash collisions. If there is any other error, we need to report it in the status of
    // the Deployment.

    // DIFFERENT from kubernetes as we can't get error codes back
    // check hash collision
    let has_hash_collision = replicasets_in_ns
        .iter()
        .any(|rs| rs.metadata.name == new_rs.metadata.name);
    if has_hash_collision {
        // found a hash collision, update our status and then we'll try again next time
        deployment.status.collision_count += 1;
        debug!(
            deployment.status.collision_count,
            "Found hash collision with new replicaset, bumping collision count"
        );
        Some(ValOrOp::Op(
            DeploymentControllerAction::UpdateDeploymentStatus(deployment.clone()),
        ))
    } else {
        Some(ValOrOp::Op(DeploymentControllerAction::CreateReplicaSet(
            new_rs,
        )))
    }

    // TODO: work out handling errors of creating the replicaset here.
    // TODO: do we need to update the deployment status here? or does it get handled in another
    // loop?
}

// syncDeploymentStatus checks if the status is up-to-date and sync it if necessary
fn sync_deployment_status(
    all_replicasets: &[&ReplicaSet],
    new_replicaset: &Option<ReplicaSet>,
    deployment: &Deployment,
) -> Option<DeploymentControllerAction> {
    debug!("Syncing deployment status");
    let new_status = calculate_status(all_replicasets, new_replicaset, deployment);
    if deployment.status != new_status {
        debug!(
            status_diff = ?deployment.status.diff(&new_status),
            "Setting new status"
        );
        let mut new_deployment = deployment.clone();
        new_deployment.status = new_status;
        Some(DeploymentControllerAction::UpdateDeploymentStatus(
            new_deployment,
        ))
    } else {
        None
    }
}

// calculateStatus calculates the latest status for the provided deployment by looking into the provided replica sets.
fn calculate_status(
    all_replicasets: &[&ReplicaSet],
    new_replicaset: &Option<ReplicaSet>,
    deployment: &Deployment,
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
            DeploymentConditionType::Available,
            ConditionStatus::True,
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
            DeploymentConditionType::Available,
            ConditionStatus::False,
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
    deployment: &Deployment,
    new_replicaset: &Option<ReplicaSet>,
    old_replicasets: &[&ReplicaSet],
) -> Option<DeploymentControllerAction> {
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
                let val = name_to_size.get_mut(&rs.metadata.name).unwrap();
                if leftover < 0 {
                    *val = val.saturating_sub(leftover as u32);
                } else {
                    *val += leftover as u32;
                }
            }

            // TODO: Use transactions when we have them.
            if let Some(DeploymentControllerAction::UpdateReplicaSet(rs)) = scale_replicaset(
                rs,
                name_to_size.get(&rs.metadata.name).copied().unwrap_or(0),
                deployment,
            ) {
                updated_rss.push(rs);
            }
        }
        if !updated_rss.is_empty() {
            return Some(DeploymentControllerAction::UpdateReplicaSets(updated_rss));
        }
    }
    None
}

fn max_revision(all_replicasets: &[&ReplicaSet]) -> u64 {
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
    new_replicaset: &Option<ReplicaSet>,
    old_replicasets: &[&ReplicaSet],
) -> Option<ReplicaSet> {
    if new_replicaset.is_none() && old_replicasets.is_empty() {
        debug!("no replicasets to work on");
        return None;
    }

    let mut old_replicasets = old_replicasets.to_vec();
    old_replicasets.sort_by_key(|rs| rs.metadata.creation_timestamp);
    old_replicasets.reverse();

    let mut all_replicasets = old_replicasets.clone();
    if let Some(new_rs) = new_replicaset {
        all_replicasets.push(new_rs);
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

fn filter_active_replicasets<'a>(replicasets: &[&'a ReplicaSet]) -> Vec<&'a ReplicaSet> {
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
fn is_saturated(deployment: &Deployment, replicaset: &Option<ReplicaSet>) -> bool {
    let Some(rs) = replicaset else { return false };
    let Some(desired_string) = rs
        .metadata
        .annotations
        .get(DESIRED_REPLICAS_ANNOTATION)
        .and_then(|ds| ds.parse::<u32>().ok())
    else {
        return false;
    };
    rs.spec.replicas == Some(deployment.spec.replicas)
        && desired_string == deployment.spec.replicas
        && rs.status.available_replicas == deployment.spec.replicas
}

#[tracing::instrument(skip_all)]
fn scale_replicaset_and_record_event(
    replicaset: &ReplicaSet,
    new_scale: u32,
    deployment: &Deployment,
) -> Option<DeploymentControllerAction> {
    if replicaset.spec.replicas == Some(new_scale) {
        debug!("already scaled");
        None
    } else {
        scale_replicaset(replicaset, new_scale, deployment)
    }
}

#[tracing::instrument(skip_all)]
fn scale_replicaset(
    replicaset: &ReplicaSet,
    new_scale: u32,
    deployment: &Deployment,
) -> Option<DeploymentControllerAction> {
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
        Some(DeploymentControllerAction::UpdateReplicaSet(new_rs))
    } else {
        debug!("Not scaling replicaset");
        None
    }
}

fn replicas_annotations_need_update(
    replicaset: &ReplicaSet,
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
    replicaset: &mut ReplicaSet,
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

fn max_surge(deployment: &Deployment) -> u32 {
    if is_rolling_update(deployment) {
        0
    } else {
        1
    }
}

fn is_rolling_update(deployment: &Deployment) -> bool {
    deployment
        .spec
        .strategy
        .as_ref()
        .map_or(true, |s| s.r#type == DeploymentStrategyType::RollingUpdate)
}

fn set_deployment_revision(deployment: &mut Deployment, new_revision: String) -> bool {
    let old_revision = deployment
        .metadata
        .annotations
        .get(REVISION_ANNOTATION)
        .cloned()
        .unwrap_or_default();
    if old_revision != new_revision {
        debug!(old_revision, new_revision, "Updating deployment revision");
        deployment
            .metadata
            .annotations
            .insert(REVISION_ANNOTATION.to_owned(), new_revision);
        true
    } else {
        false
    }
}

// cleanupDeployment is responsible for cleaning up a deployment ie. retains all but the latest N old replica sets
// where N=d.Spec.RevisionHistoryLimit. Old replica sets are older versions of the podtemplate of a deployment kept
// around by default 1) for historical reasons and 2) for the ability to rollback a deployment.
fn cleanup_deployment(
    old_replicasets: &[&ReplicaSet],
    deployment: &Deployment,
) -> Option<DeploymentControllerAction> {
    debug!("Cleaning up deployment");
    if has_revision_history_limit(deployment) {
        return None;
    }

    // Avoid deleting replica set with deletion timestamp set
    let mut cleanable_replicasets = old_replicasets
        .iter()
        .filter(|rs| rs.metadata.deletion_timestamp.is_none())
        .collect::<Vec<_>>();

    let diff = cleanable_replicasets
        .len()
        .saturating_sub(deployment.spec.revision_history_limit as usize);
    if diff == 0 {
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
            || (rs.spec.replicas.is_some() && rs.spec.replicas != Some(0))
            || rs.metadata.generation > rs.status.observed_generation
            || rs.metadata.deletion_timestamp.is_some()
        {
            continue;
        }
        debug!("Trying to cleanup replica set for deployment");
        return Some(DeploymentControllerAction::DeleteReplicaSet((**rs).clone()));
    }
    None
}

// HasRevisionHistoryLimit checks if the Deployment d is expected to keep a specified number of
// old replicaSets. These replicaSets are mainly kept with the purpose of rollback.
// The RevisionHistoryLimit can start from 0 (no retained replicasSet). When set to math.MaxInt32,
// the Deployment will keep all revisions.
fn has_revision_history_limit(deployment: &Deployment) -> bool {
    deployment.spec.revision_history_limit != u32::MAX
}

fn get_available_replica_count_for_replicasets(replicasets: &[&ReplicaSet]) -> u32 {
    replicasets
        .iter()
        .map(|rs| rs.status.available_replicas)
        .sum()
}

fn get_replica_count_for_replicasets(replicasets: &[&ReplicaSet]) -> u32 {
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
fn max_unavailable(deployment: &Deployment) -> u32 {
    if is_rolling_update(deployment) || deployment.spec.replicas == 0 {
        return 0;
    }

    let max_unavailable = deployment
        .spec
        .strategy
        .as_ref()
        .and_then(|s| {
            s.rolling_update.as_ref().and_then(|r| {
                r.max_unavailable
                    .as_ref()
                    .map(|mu| mu.scaled_value(deployment.spec.replicas, true))
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
    cond_type: DeploymentConditionType,
    status: ConditionStatus,
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
    let current_condition = get_deployment_condition(status, condition.r#type);
    if let Some(cc) = current_condition {
        if cc.status == condition.status && cc.reason == condition.reason {
            return;
        }

        // Do not update lastTransitionTime if the status of the condition doesn't change.
        if cc.status == condition.status {
            debug!("Updating last_transition_time as status changed");
            condition.last_transition_time = cc.last_transition_time;
        }
    }

    debug!(new_condition=?condition, "Setting deployment condition");

    let mut new_conditions = filter_out_condition(&status.conditions, condition.r#type);
    new_conditions.push(&condition);
    status.conditions = new_conditions.into_iter().cloned().collect();
}

fn get_actual_replica_count_for_replicasets(replicasets: &[&ReplicaSet]) -> u32 {
    replicasets.iter().map(|rs| rs.status.replicas).sum()
}

fn get_ready_replica_count_for_replicasets(replicasets: &[&ReplicaSet]) -> u32 {
    replicasets.iter().map(|rs| rs.status.ready_replicas).sum()
}

// GetDeploymentCondition returns the condition with the provided type.
fn get_deployment_condition(
    status: &DeploymentStatus,
    cond_type: DeploymentConditionType,
) -> Option<&DeploymentCondition> {
    let o = status.conditions.iter().find(|c| c.r#type == cond_type);
    debug!(found=o.is_some(), ?cond_type, ?status.conditions,  "Got deployment condition");
    o
}

fn remove_deployment_condition(status: &mut DeploymentStatus, cond_type: DeploymentConditionType) {
    status.conditions.retain(|c| c.r#type != cond_type)
}

// filterOutCondition returns a new slice of deployment conditions without conditions with the provided type.
fn filter_out_condition(
    conditions: &[DeploymentCondition],
    cond_type: DeploymentConditionType,
) -> Vec<&DeploymentCondition> {
    conditions
        .iter()
        .filter(|c| {
            if c.r#type == cond_type {
                debug!(condition=?c, "Filtering out condition");
                false
            } else {
                true
            }
        })
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
    deployment: &Deployment,
    new_replicaset: &mut ReplicaSet,
    new_revision: String,
    exists: bool,
    rev_history_limit_in_chars: usize,
) -> bool {
    // First, copy deployment's annotations (except for apply and revision annotations)
    let mut annotation_changed =
        copy_deployment_annotations_to_replicaset(deployment, new_replicaset);
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
    if let Some(old_revision) = old_revision {
        if old_revision_int < new_revision_int {
            let revision_history_annotation = new_replicaset
                .metadata
                .annotations
                .get(REVISION_HISTORY_ANNOTATION);
            let mut old_revisions = revision_history_annotation
                .cloned()
                .unwrap_or_default()
                .split(',')
                .map(|s| s.to_owned())
                .collect::<Vec<String>>();
            if old_revisions[0].is_empty() {
                new_replicaset
                    .metadata
                    .annotations
                    .insert(REVISION_HISTORY_ANNOTATION.to_owned(), old_revision.clone());
            } else {
                let mut total_len =
                    revision_history_annotation.map_or(0, |a| a.len()) + old_revision.len() + 1;
                // index for the starting position in oldRevisions
                let mut start = 0;
                while total_len > rev_history_limit_in_chars && start < old_revisions.len() {
                    total_len -= old_revisions[start].len() - 1;
                    start += 1;
                }
                if total_len <= rev_history_limit_in_chars {
                    old_revisions = old_revisions[start..].to_vec();
                    old_revisions.push(old_revision);
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

fn copy_deployment_annotations_to_replicaset(
    deployment: &Deployment,
    replicaset: &mut ReplicaSet,
) -> bool {
    let mut annotations_changed = false;
    for (k, v) in &deployment.metadata.annotations {
        // newRS revision is updated automatically in getNewReplicaSet, and the deployment's revision number is then updated
        // by copying its newRS revision number. We should not copy deployment's revision to its newRS, since the update of
        // deployment revision number may fail (revision becomes stale) and the revision number in newRS is more reliable.
        if skip_copy_annotation(k)
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

fn has_progress_deadline(deployment: &Deployment) -> bool {
    deployment.spec.progress_deadline_seconds != Some(u32::MAX)
}

fn get_rollback_to(deployment: &Deployment) -> Option<RollbackConfig> {
    // Extract the annotation used for round-tripping the deprecated RollbackTo field.
    let revision = deployment.metadata.annotations.get(DEPRECATED_ROLLBACK_TO);
    if let Some(revision) = revision {
        if revision.is_empty() {
            return None;
        }
        let Ok(revision64) = revision.parse::<u64>() else {
            return None;
        };
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
    replicaset: &ReplicaSet,
    deployment: &Deployment,
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

fn get_replicaset_fraction(replicaset: &ReplicaSet, deployment: &Deployment) -> i32 {
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

fn get_max_replicas_annotation(replicaset: &ReplicaSet) -> Option<u32> {
    replicaset
        .metadata
        .annotations
        .get(MAX_REPLICAS_ANNOTATION)
        .and_then(|r| r.parse().ok())
}

fn rollback(
    deployment: &mut Deployment,
    replicasets: &[&ReplicaSet],
    replicasets_in_ns: &[&ReplicaSet],
) -> Option<DeploymentControllerAction> {
    let (new_rs, all_old_rss) =
        get_all_replicasets_and_sync_revision(deployment, replicasets, replicasets_in_ns, true);

    let new_rs = match new_rs {
        Some(ValOrOp::Resource(r)) => Some(r),
        Some(ValOrOp::Op(op)) => return Some(op),
        None => None,
    };

    let mut all_rss = all_old_rss.to_vec();
    if let Some(new_rs) = &new_rs {
        all_rss.push(new_rs);
    }

    let mut rollback_to = get_rollback_to(deployment).unwrap();

    // If rollback revision is 0, rollback to the last revision
    if rollback_to.revision == 0 {
        rollback_to.revision = last_revision(&all_rss);
        if rollback_to.revision == 0 {
            // If we still can't find the last revision, gives up rollback
            // Gives up rollback
            return Some(update_deployment_and_clear_rollback_to(deployment));
        }
    }

    for rs in all_rss {
        let v = rs
            .metadata
            .annotations
            .get(REVISION_ANNOTATION)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or_default();
        if v == rollback_to.revision {
            // rollback by copying podTemplate.Spec from the replica set
            // revision number will be incremented during the next getAllReplicaSetsAndSyncRevision call
            // no-op if the spec matches current deployment's podTemplate.Spec
            let op = rollback_to_template(deployment, rs);
            return Some(op);
        }
    }
    // gives up rollback
    Some(update_deployment_and_clear_rollback_to(deployment))
}

// NewRSNewReplicas calculates the number of replicas a deployment's new RS should have.
// When one of the followings is true, we're rolling out the deployment; otherwise, we're scaling it.
// 1) The new RS is saturated: newRS's replicas == deployment's replicas
// 2) Max number of pods allowed is reached: deployment's replicas + maxSurge == all RSs' replicas
#[tracing::instrument(skip_all)]
fn new_rs_new_replicas(
    deployment: &Deployment,
    all_replicasets: &[&ReplicaSet],
    new_replicaset: &ReplicaSet,
) -> u32 {
    match deployment
        .spec
        .strategy
        .as_ref()
        .map(|s| s.r#type)
        .unwrap_or_default()
    {
        DeploymentStrategyType::RollingUpdate => {
            // Check if we can scale up.
            let max_surge = deployment
                .spec
                .strategy
                .as_ref()
                .and_then(|s| {
                    s.rolling_update.as_ref().and_then(|ru| {
                        ru.max_surge
                            .as_ref()
                            .map(|ms| ms.scaled_value(deployment.spec.replicas, true))
                    })
                })
                .unwrap_or_default();
            // Find the total number of pods
            let current_pod_count = get_replica_count_for_replicasets(all_replicasets);
            let max_total_pods = deployment.spec.replicas + max_surge;
            if current_pod_count >= max_total_pods {
                // cannot scale up
                debug!("Cannot scale replicaset up");
                return new_replicaset.spec.replicas.unwrap_or_default();
            }
            // scale up
            let scale_up_count = max_total_pods - current_pod_count;
            // do not exceed the number of desired replicas
            let scale_up_count = scale_up_count
                .min(deployment.spec.replicas - new_replicaset.spec.replicas.unwrap_or_default());
            let to = new_replicaset.spec.replicas.unwrap_or_default() + scale_up_count;
            debug!(by = scale_up_count, to, "Can scale replicaset up");
            to
        }
        DeploymentStrategyType::Recreate => deployment.spec.replicas,
    }
}

// LastRevision finds the second max revision number in all replica sets (the last revision)
fn last_revision(all_rss: &[&ReplicaSet]) -> u64 {
    let mut max = 0;
    let mut sec_max = 0;
    for v in all_rss.iter().filter_map(|rs| {
        rs.metadata
            .annotations
            .get(REVISION_ANNOTATION)
            .and_then(|v| v.parse().ok())
    }) {
        if v >= max {
            sec_max = max;
            max = v;
        } else if v > sec_max {
            sec_max = v;
        }
    }
    sec_max
}

// isScalingEvent checks whether the provided deployment has been updated with a scaling event
// by looking at the desired-replicas annotation in the active replica sets of the deployment.
//
// rsList should come from getReplicaSetsForDeployment(d).
fn is_scaling_event(
    deployment: &mut Deployment,
    replicasets: &[&ReplicaSet],
    replicasets_in_ns: &[&ReplicaSet],
) -> ValOrOp<bool> {
    let (new_rs, old_rss) =
        get_all_replicasets_and_sync_revision(deployment, replicasets, replicasets_in_ns, false);
    let new_rs = match new_rs {
        Some(ValOrOp::Resource(r)) => Some(r),
        Some(ValOrOp::Op(op)) => return ValOrOp::Op(op),
        None => None,
    };
    let mut all_rss = old_rss.to_vec();
    if let Some(new_rs) = &new_rs {
        all_rss.push(new_rs);
    }

    for rs in filter_active_replicasets(&all_rss) {
        let desired = rs
            .metadata
            .annotations
            .get(DESIRED_REPLICAS_ANNOTATION)
            .and_then(|v| v.parse::<u32>().ok());
        if let Some(desired) = desired {
            if desired != deployment.spec.replicas {
                return ValOrOp::Resource(true);
            }
        }
    }
    ValOrOp::Resource(false)
}

// updateDeploymentAndClearRollbackTo sets .spec.rollbackTo to nil and update the input deployment
// It is assumed that the caller will have updated the deployment template appropriately (in case
// we want to rollback).
fn update_deployment_and_clear_rollback_to(deployment: &Deployment) -> DeploymentControllerAction {
    let mut d = deployment.clone();
    set_rollback_to(&mut d, None);
    DeploymentControllerAction::UpdateDeployment(d)
}

fn set_rollback_to(deployment: &mut Deployment, rollback_to: Option<RollbackConfig>) {
    if let Some(rb) = rollback_to {
        deployment
            .metadata
            .annotations
            .insert(DEPRECATED_ROLLBACK_TO.to_owned(), rb.revision.to_string());
    } else {
        deployment
            .metadata
            .annotations
            .remove(DEPRECATED_ROLLBACK_TO);
    }
}

// rollbackToTemplate compares the templates of the provided deployment and replica set and
// updates the deployment with the replica set template in case they are different. It also
// cleans up the rollback spec so subsequent requeues of the deployment won't end up in here.
fn rollback_to_template(
    deployment: &mut Deployment,
    replicaset: &ReplicaSet,
) -> DeploymentControllerAction {
    if equal_ignore_hash(&deployment.spec.template, &replicaset.spec.template) {
        set_from_replicaset_template(deployment, &replicaset.spec.template);
        // set RS (the old RS we'll rolling back to) annotations back to the deployment;
        // otherwise, the deployment's current annotations (should be the same as current new RS) will be copied to the RS after the rollback.
        //
        // For example,
        // A Deployment has old RS1 with annotation {change-cause:create}, and new RS2 {change-cause:edit}.
        // Note that both annotations are copied from Deployment, and the Deployment should be annotated {change-cause:edit} as well.
        // Now, rollback Deployment to RS1, we should update Deployment's pod-template and also copy annotation from RS1.
        // Deployment is now annotated {change-cause:create}, and we have new RS1 {change-cause:create}, old RS2 {change-cause:edit}.
        //
        // If we don't copy the annotations back from RS to deployment on rollback, the Deployment will stay as {change-cause:edit},
        // and new RS1 becomes {change-cause:edit} (copied from deployment after rollback), old RS2 {change-cause:edit}, which is not correct.
        set_deployment_annotations_to(deployment, replicaset);
    } else {
        // same template, skip
    }
    update_deployment_and_clear_rollback_to(deployment)
}

fn set_deployment_annotations_to(deployment: &mut Deployment, replicaset: &ReplicaSet) {
    deployment.metadata.annotations = get_skipped_annotations(&deployment.metadata.annotations);
    for (k, v) in &replicaset.metadata.annotations {
        if !skip_copy_annotation(k) {
            deployment.metadata.annotations.insert(k.clone(), v.clone());
        }
    }
}

fn get_skipped_annotations(annotations: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    annotations
        .iter()
        .filter(|(k, _)| skip_copy_annotation(k))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

fn set_from_replicaset_template(deployment: &mut Deployment, template: &PodTemplateSpec) {
    deployment.spec.template.metadata = template.metadata.clone();
    deployment.spec.template.spec = template.spec.clone();
    deployment
        .spec
        .template
        .metadata
        .labels
        .remove(DEFAULT_DEPLOYMENT_UNIQUE_LABEL_KEY);
}

// ComputeHash returns a hash value calculated from pod template and
// a collisionCount to avoid hash collision. The hash will be safe encoded to
// avoid bad words.
fn compute_hash(template: &PodTemplateSpec, collision_count: u32) -> String {
    let mut hasher = FnvHasher::new_32a();
    template.hash(&mut hasher);

    // Add collisionCount in the hash
    let bytes = collision_count.to_le_bytes();
    hasher.write(&bytes);

    safe_encode_string(&hasher.finish_32().to_string())
}

fn safe_encode_string(s: &str) -> String {
    const ALPHA_NUMS: &[char] = &[
        'b', 'c', 'd', 'f', 'g', 'h', 'j', 'k', 'l', 'm', 'n', 'p', 'q', 'r', 's', 't', 'v', 'w',
        'x', 'z', '2', '4', '5', '6', '7', '8', '9',
    ];
    s.chars()
        .map(|c| ALPHA_NUMS[c as usize % ALPHA_NUMS.len()])
        .collect()
}

// rolloutRolling implements the logic for rolling a new replica set.
fn rollout_rolling(
    deployment: &mut Deployment,
    replicasets: &[&ReplicaSet],
    replicasets_in_ns: &[&ReplicaSet],
) -> Option<DeploymentControllerAction> {
    let (new_replicaset, old_replicasets) =
        get_all_replicasets_and_sync_revision(deployment, replicasets, replicasets_in_ns, true);
    let new_replicaset = match new_replicaset {
        Some(ValOrOp::Resource(r)) => r,
        Some(ValOrOp::Op(op)) => return Some(op),
        None => unreachable!(),
    };
    let mut all_rss = old_replicasets.clone();
    all_rss.push(&new_replicaset);

    // Scale up, if we can.
    if let Some(scaled_up_op) = reconcile_new_replicaset(&all_rss, &new_replicaset, deployment) {
        return Some(scaled_up_op);
        // update deploymentstatus
        // TODO: handle this as it should be done but might be done on reconciliation anyway?
        // return sync_rollout_status(all_rss, new_replicaset, deployment);
    }

    // scale down, if we can
    let scaled_down = reconcile_old_replicasets(
        &all_rss,
        &filter_active_replicasets(&old_replicasets),
        &new_replicaset,
        deployment,
    );
    if let Some(op) = scaled_down {
        return Some(op);
        // TODO: work out where to handle this
        // return sync_rollout_status(all_rss, new_replicaset, deployment);
    }

    if deployment_complete(deployment, &deployment.status) {
        if let Some(op) = cleanup_deployment(&old_replicasets, deployment) {
            return Some(op);
        }
    }

    sync_rollout_status(&all_rss, &Some(new_replicaset.clone()), deployment)
}

// syncRolloutStatus updates the status of a deployment during a rollout. There are
// cases this helper will run that cannot be prevented from the scaling detection,
// for example a resync of the deployment after it was scaled up. In those cases,
// we shouldn't try to estimate any progress.
#[tracing::instrument(skip_all)]
fn sync_rollout_status(
    all_rss: &[&ReplicaSet],
    new_rs: &Option<ReplicaSet>,
    deployment: &Deployment,
) -> Option<DeploymentControllerAction> {
    let mut new_status = calculate_status(all_rss, new_rs, deployment);
    debug!(status_diff = ?deployment.status.diff(&new_status), "Checking new status");

    if !has_progress_deadline(deployment) {
        remove_deployment_condition(&mut new_status, DeploymentConditionType::Progressing);
    }

    let current_cond =
        get_deployment_condition(&deployment.status, DeploymentConditionType::Progressing);
    let is_complete_deployment = new_status.replicas == new_status.updated_replicas
        && current_cond.is_some()
        && current_cond.unwrap().reason.as_ref().unwrap() == NEW_RSAVAILABLE_REASON;

    if has_progress_deadline(deployment) && !is_complete_deployment {
        if deployment_complete(deployment, &new_status) {
            let msg = format!(
                "Deployment {} has successfully progressed.",
                deployment.metadata.name
            );
            let condition = new_deployment_condition(
                DeploymentConditionType::Progressing,
                ConditionStatus::True,
                NEW_RSAVAILABLE_REASON.to_owned(),
                msg,
            );
            set_deployment_condition(&mut new_status, condition);
        } else if deployment_progressing(deployment, &new_status) {
            let msg = format!("Deployment {} is progressing.", deployment.metadata.name);
            let mut condition = new_deployment_condition(
                DeploymentConditionType::Progressing,
                ConditionStatus::True,
                REPLICASET_UPDATED_REASON.to_owned(),
                msg,
            );
            if let Some(current_cond) = current_cond {
                if current_cond.status == ConditionStatus::True {
                    condition.last_transition_time = current_cond.last_transition_time;
                }
                remove_deployment_condition(&mut new_status, DeploymentConditionType::Progressing);
            }
            set_deployment_condition(&mut new_status, condition);
        } else if deployment_timed_out(deployment, &new_status) {
            let msg = format!(
                "Deployment {} has timed out progressing.",
                deployment.metadata.name
            );
            let condition = new_deployment_condition(
                DeploymentConditionType::Progressing,
                ConditionStatus::False,
                TIMED_OUT_REASON.to_owned(),
                msg,
            );
            set_deployment_condition(&mut new_status, condition);
        }
    }

    let replica_failure_cond = get_replica_failures(all_rss, new_rs);
    if !replica_failure_cond.is_empty() {
        set_deployment_condition(&mut new_status, replica_failure_cond[0].clone())
    } else {
        remove_deployment_condition(&mut new_status, DeploymentConditionType::ReplicaFailure)
    }

    if deployment.status == new_status {
        return requeue_stuck_deployment(deployment, new_status);
    }

    debug!("Deployment status was different at the end, updating");
    let mut new_deployment = deployment.clone();
    new_deployment.status = new_status;
    Some(DeploymentControllerAction::UpdateDeploymentStatus(
        new_deployment,
    ))
}

#[tracing::instrument(skip_all)]
fn reconcile_new_replicaset(
    all_replicasets: &[&ReplicaSet],
    new_rs: &ReplicaSet,
    deployment: &Deployment,
) -> Option<DeploymentControllerAction> {
    if new_rs.spec.replicas == Some(deployment.spec.replicas) {
        debug!("New replicaset already at correct size");
        // scaling not required
        return None;
    }
    if new_rs.spec.replicas > Some(deployment.spec.replicas) {
        // scale down
        if let Some(op) =
            scale_replicaset_and_record_event(new_rs, deployment.spec.replicas, deployment)
        {
            debug!("scaling down new replicaset");
            return Some(op);
        }
    }
    let new_replicas_count = new_rs_new_replicas(deployment, all_replicasets, new_rs);
    scale_replicaset_and_record_event(new_rs, new_replicas_count, deployment)
}

#[tracing::instrument(skip_all)]
fn reconcile_old_replicasets(
    all_replicasets: &[&ReplicaSet],
    old_replicasets: &[&ReplicaSet],
    new_rs: &ReplicaSet,
    deployment: &Deployment,
) -> Option<DeploymentControllerAction> {
    let old_pods_count = get_replica_count_for_replicasets(old_replicasets);
    if old_pods_count == 0 {
        return None;
    }

    let all_pods_count = get_replica_count_for_replicasets(all_replicasets);
    let max_unavailable = max_unavailable(deployment);

    let min_avilable = deployment.spec.replicas - max_unavailable;
    let new_rs_unavailable_pod_count =
        new_rs.spec.replicas.unwrap() - new_rs.status.available_replicas;
    let max_scaled_down = all_pods_count
        .saturating_sub(min_avilable)
        .saturating_sub(new_rs_unavailable_pod_count);
    if max_scaled_down == 0 {
        debug!("can't scale below zero");
        return None;
    }

    // Clean up unhealthy replicas first, otherwise unhealthy replicas will block deployment
    // and cause timeout. See https://github.com/kubernetes/kubernetes/issues/16737
    let res = cleanup_unhealthy_replicas(old_replicasets, deployment, max_scaled_down);
    let old_replicasets = match res {
        Some(res_or_op) => match res_or_op {
            ValOrOp::Resource(res) => res,
            ValOrOp::Op(op) => return Some(op),
        },
        None => return None,
    };

    // Scale down old replica sets, need check maxUnavailable to ensure we can scale down
    let mut all_rss = old_replicasets.to_vec();
    all_rss.push(new_rs);

    if let Some(scaled_down_op) =
        scale_down_old_replicasets_for_rolling_update(&all_rss, &old_replicasets, deployment)
    {
        debug!("Scaling down old replicasets");
        return Some(scaled_down_op);
    }
    None

    // let total_scaled_down = cleanup_count + scaled_down_count;
    // total_scaled_down > 0
}

// cleanupUnhealthyReplicas will scale down old replica sets with unhealthy replicas, so that all unhealthy replicas will be deleted.
fn cleanup_unhealthy_replicas<'a>(
    old_replicasets: &'a [&ReplicaSet],
    deployment: &Deployment,
    max_cleanup_count: u32,
) -> Option<ValOrOp<Vec<&'a ReplicaSet>>> {
    let mut old_replicasets = old_replicasets.to_vec();
    old_replicasets.sort_by_key(|rs| rs.metadata.creation_timestamp);

    // Safely scale down all old replica sets with unhealthy replicas. Replica set will sort the pods in the order
    // such that not-ready < ready, unscheduled < scheduled, and pending < running. This ensures that unhealthy replicas will
    // been deleted first and won't increase unavailability.
    let mut total_scaled_down = 0;
    let mut updated_rss = Vec::new();
    for target_rs in old_replicasets.iter() {
        if total_scaled_down >= max_cleanup_count {
            break;
        }
        if target_rs.spec.replicas.unwrap() == 0 {
            // cannot scale down this replica set
            continue;
        }
        if target_rs.spec.replicas.unwrap() == target_rs.status.available_replicas {
            // no unhealthy replicas found, no scaling required
            continue;
        }

        debug!(max_cleanup_count, total_scaled_down, ?target_rs.spec.replicas, ?target_rs.status.available_replicas, "calculating scaled_down_count");
        let scaled_down_count = (max_cleanup_count.saturating_sub(total_scaled_down)).min(
            target_rs
                .spec
                .replicas
                .unwrap()
                .saturating_sub(target_rs.status.available_replicas),
        );
        let new_replicas_count = target_rs.spec.replicas.unwrap() - scaled_down_count;
        if new_replicas_count > target_rs.spec.replicas.unwrap() {
            return None;
        }
        match scale_replicaset_and_record_event(target_rs, new_replicas_count, deployment) {
            Some(DeploymentControllerAction::UpdateReplicaSet(rs)) => {
                updated_rss.push(rs);
            }
            Some(_) => {
                unreachable!()
            }
            None => {
                // no changes otherwise we would have had an update op
            }
        }
        total_scaled_down += scaled_down_count;
    }
    if !updated_rss.is_empty() {
        Some(ValOrOp::Op(DeploymentControllerAction::UpdateReplicaSets(
            updated_rss,
        )))
    } else {
        Some(ValOrOp::Resource(old_replicasets))
    }
}

#[tracing::instrument(skip_all)]
fn scale_down_old_replicasets_for_rolling_update(
    all_replicasets: &[&ReplicaSet],
    old_replicasets: &[&ReplicaSet],
    deployment: &Deployment,
) -> Option<DeploymentControllerAction> {
    let max_unavailable = max_unavailable(deployment);
    let min_available = deployment.spec.replicas - max_unavailable;
    let available_pod_count = get_available_replica_count_for_replicasets(all_replicasets);
    if available_pod_count <= min_available {
        debug!(
            available_pod_count,
            min_available,
            ?all_replicasets,
            "available pod count less or equal than the min available, not scaling down"
        );
        // cannot scale down
        return None;
    }

    let mut old_replicasets = old_replicasets.to_vec();
    old_replicasets.sort_by_key(|rs| rs.metadata.creation_timestamp);

    let mut total_scaled_down = 0;
    let total_scale_down_count = available_pod_count - min_available;
    let mut updated_rss = Vec::new();
    for target_rs in old_replicasets {
        debug!(
            total_scaled_down,
            total_scale_down_count, "Scaling down old replicaset"
        );
        if total_scaled_down >= total_scale_down_count {
            // no further scaling required
            break;
        }
        if target_rs.spec.replicas.unwrap() == 0 {
            // cannot scale down this replicaset
            continue;
        }

        // scale down
        let scaled_down_count = target_rs
            .spec
            .replicas
            .unwrap()
            .min(total_scale_down_count - total_scaled_down);
        let new_replicas_count = target_rs.spec.replicas.unwrap() - scaled_down_count;
        if new_replicas_count > target_rs.spec.replicas.unwrap() {
            return None;
        }
        if let Some(DeploymentControllerAction::UpdateReplicaSet(rs)) =
            scale_replicaset_and_record_event(target_rs, new_replicas_count, deployment)
        {
            updated_rss.push(rs);
        }
        total_scaled_down += scaled_down_count
    }
    if !updated_rss.is_empty() {
        Some(DeploymentControllerAction::UpdateReplicaSets(updated_rss))
    } else {
        None
    }
}

// rolloutRecreate implements the logic for recreating a replica set.
fn rollout_recreate(
    deployment: &mut Deployment,
    replicasets: &[&ReplicaSet],
    replicasets_in_ns: &[&ReplicaSet],
    pod_map: &BTreeMap<String, Vec<Pod>>,
) -> Option<DeploymentControllerAction> {
    // Don't create a new RS if not already existed, so that we avoid scaling up before scaling down.
    let (new_replicaset, old_replicasets) =
        get_all_replicasets_and_sync_revision(deployment, replicasets, replicasets_in_ns, false);
    let new_replicaset = match new_replicaset {
        Some(ValOrOp::Resource(r)) => Some(r),
        Some(ValOrOp::Op(op)) => return Some(op),
        None => None,
    };
    let mut all_rss = old_replicasets
        .iter()
        .map(|rs| (*rs).clone())
        .collect::<Vec<_>>();
    if let Some(new_replicaset) = &new_replicaset {
        all_rss.push(new_replicaset.clone())
    }
    let active_old_rss = filter_active_replicasets(&old_replicasets);

    // scale down replica sets
    let scaled_down = scale_down_old_replicasets_for_recreate(&active_old_rss, deployment);
    if let Some(op) = scaled_down {
        return Some(op);
        // TODO: work out how to handle this bit too
        // return sync_rollout_status(all_rss, new_rs, deployment);
    }

    if old_pods_running(&new_replicaset, &old_replicasets, pod_map) {
        let all_rss = all_rss.iter().collect::<Vec<_>>();
        return sync_rollout_status(&all_rss, &new_replicaset, deployment);
    }

    // If we need to create a new RS, create it now.
    let (new_replicaset, old_replicasets) = if let Some(new_replicaset) = new_replicaset {
        (new_replicaset, old_replicasets)
    } else {
        let (new_replicaset, old_replicasets) =
            get_all_replicasets_and_sync_revision(deployment, replicasets, replicasets_in_ns, true);
        let new_replicaset = match new_replicaset {
            Some(ValOrOp::Resource(r)) => r,
            Some(ValOrOp::Op(op)) => return Some(op),
            None => unreachable!(),
        };
        all_rss.push(new_replicaset.clone());
        (new_replicaset, old_replicasets)
    };

    // scale up new replica set
    if let Some(op) = scale_up_new_replicaset_for_recreate(&new_replicaset, deployment) {
        return Some(op);
    }

    if deployment_complete(deployment, &deployment.status) {
        if let Some(op) = cleanup_deployment(&old_replicasets, deployment) {
            return Some(op);
        }
    }

    let all_rss = all_rss.iter().collect::<Vec<_>>();
    sync_rollout_status(&all_rss, &Some(new_replicaset), deployment)
}

fn scale_down_old_replicasets_for_recreate(
    old_replicasets: &[&ReplicaSet],
    deployment: &Deployment,
) -> Option<DeploymentControllerAction> {
    let mut updated_rss = Vec::new();
    for rs in old_replicasets {
        if rs.spec.replicas.unwrap() == 0 {
            continue;
        }

        if let Some(DeploymentControllerAction::UpdateReplicaSet(rs)) =
            scale_replicaset_and_record_event(rs, 0, deployment)
        {
            updated_rss.push(rs);
        }
    }
    if updated_rss.is_empty() {
        None
    } else {
        Some(DeploymentControllerAction::UpdateReplicaSets(updated_rss))
    }
}

// DeploymentComplete considers a deployment to be complete once all of its desired replicas
// are updated and available, and no old pods are running.
pub fn deployment_complete(deployment: &Deployment, new_status: &DeploymentStatus) -> bool {
    new_status.updated_replicas == deployment.spec.replicas
        && new_status.replicas == deployment.spec.replicas
        && new_status.available_replicas == deployment.spec.replicas
        && new_status.observed_generation >= deployment.metadata.generation
}

fn deployment_progressing(deployment: &Deployment, new_status: &DeploymentStatus) -> bool {
    let old_status = &deployment.status;

    let old_status_old_replicas = old_status.replicas - old_status.updated_replicas;
    let new_status_old_replicas = new_status.replicas - new_status.updated_replicas;

    new_status.updated_replicas > old_status.updated_replicas
        || new_status_old_replicas < old_status_old_replicas
        || new_status.ready_replicas > old_status.ready_replicas
        || new_status.available_replicas > old_status.available_replicas
}

fn deployment_timed_out(deployment: &Deployment, new_status: &DeploymentStatus) -> bool {
    if !has_progress_deadline(deployment) {
        return false;
    }

    let Some(cond) = get_deployment_condition(new_status, DeploymentConditionType::Progressing)
    else {
        return false;
    };

    if cond.reason.as_ref().unwrap() == NEW_RSAVAILABLE_REASON {
        return false;
    }
    if cond.reason.as_ref().unwrap() == TIMED_OUT_REASON {
        return true;
    }

    let from = cond.last_update_time.unwrap();
    let now = now();
    let delta = std::time::Duration::from_secs(
        deployment
            .spec
            .progress_deadline_seconds
            .unwrap_or_default() as u64,
    );

    from.0 + delta < now.0
}

fn get_replica_failures(
    all_replicasets: &[&ReplicaSet],
    new_rs: &Option<ReplicaSet>,
) -> Vec<DeploymentCondition> {
    let mut conditions = Vec::new();

    if let Some(new_rs) = new_rs {
        for c in &new_rs.status.conditions {
            if c.r#type != ReplicaSetConditionType::ReplicaFailure {
                continue;
            }
            conditions.push(replicaset_to_deployment_condition(c.clone()))
        }
    }

    // Return failures for the new replica set over failures from old replica sets.
    if !conditions.is_empty() {
        return conditions;
    }

    for rs in all_replicasets {
        for c in &rs.status.conditions {
            if c.r#type != ReplicaSetConditionType::ReplicaFailure {
                continue;
            }
            conditions.push(replicaset_to_deployment_condition(c.clone()))
        }
    }

    conditions
}

fn replicaset_to_deployment_condition(cond: ReplicaSetCondition) -> DeploymentCondition {
    let ty = match cond.r#type {
        ReplicaSetConditionType::ReplicaFailure => DeploymentConditionType::ReplicaFailure,
    };
    DeploymentCondition {
        r#type: ty,
        status: cond.status,
        last_transition_time: cond.last_transition_time,
        last_update_time: cond.last_transition_time,
        message: cond.message,
        reason: cond.reason,
    }
}

// requeueStuckDeployment checks whether the provided deployment needs to be synced for a progress
// check. It returns the time after the deployment will be requeued for the progress check, 0 if it
// will be requeued now, or -1 if it does not need to be requeued.
fn requeue_stuck_deployment(
    deployment: &Deployment,
    new_status: DeploymentStatus,
) -> Option<DeploymentControllerAction> {
    let current_cond =
        get_deployment_condition(&deployment.status, DeploymentConditionType::Progressing);

    // Can't estimate progress if there is no deadline in the spec or progressing condition in the current status.
    if !has_progress_deadline(deployment) || current_cond.is_none() {
        return None;
    }

    // No need to estimate progress if the rollout is complete or already timed out.
    if deployment_complete(deployment, &new_status)
        || current_cond
            .as_ref()
            .unwrap()
            .reason
            .clone()
            .unwrap_or_default()
            == TIMED_OUT_REASON
    {
        return None;
    }

    debug!("Requeueing stuck deployment");

    // If there is no sign of progress at this point then there is a high chance that the
    // deployment is stuck. We should resync this deployment at some point in the future[1]
    // and check whether it has timed out. We definitely need this, otherwise we depend on the
    // controller resync interval. See https://github.com/kubernetes/kubernetes/issues/34458.
    //
    // [1] ProgressingCondition.LastUpdatedTime + progressDeadlineSeconds - time.Now()
    //
    // For example, if a Deployment updated its Progressing condition 3 minutes ago and has a
    // deadline of 10 minutes, it would need to be resynced for a progress check after 7 minutes.
    //
    // lastUpdated: 			00:00:00
    // now: 					00:03:00
    // progressDeadlineSeconds: 600 (10 minutes)
    //
    // lastUpdated + progressDeadlineSeconds - now => 00:00:00 + 00:10:00 - 00:03:00 => 07:00
    // TODO: could delay requeue but just do it for now, the rate limiting can handle that
    // TODO: fix requeueing
    None
    // Some(DeploymentControllerAction::RequeueDeployment(
    //     deployment.clone(),
    // ))
}

fn old_pods_running(
    new_replicaset: &Option<ReplicaSet>,
    old_replicasets: &[&ReplicaSet],
    pod_map: &BTreeMap<String, Vec<Pod>>,
) -> bool {
    let old_pods = get_actual_replica_count_for_replicasets(old_replicasets);
    if old_pods > 0 {
        return true;
    }

    for (rs_uid, pod_list) in pod_map {
        // If the pods belong to the new ReplicaSet, ignore.
        if let Some(new_rs) = new_replicaset {
            if &new_rs.metadata.uid == rs_uid {
                continue;
            }
        }

        for pod in pod_list {
            match pod.status.phase {
                crate::resources::PodPhase::Failed | crate::resources::PodPhase::Succeeded => {
                    // don't count pods in terminal state
                    continue;
                }
                crate::resources::PodPhase::Unknown => {
                    // v1.PodUnknown is a deprecated status.
                    // This logic is kept for backward compatibility.
                    // This used to happen in situation like when the node is temporarily disconnected from the cluster.
                    // If we can't be sure that the pod is not running, we have to count it.
                    return true;
                }
                _ => {
                    // Pod is not in terminal phase.
                    return true;
                }
            }
        }
    }
    false
}

fn scale_up_new_replicaset_for_recreate(
    new_replicaset: &ReplicaSet,
    deployment: &Deployment,
) -> Option<DeploymentControllerAction> {
    scale_replicaset_and_record_event(new_replicaset, deployment.spec.replicas, deployment)
}

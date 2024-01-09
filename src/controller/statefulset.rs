use std::{collections::BTreeMap, time::Duration};

use tracing::{debug, trace};

use super::{
    util::{get_pod_from_template, new_controller_ref},
    Controller,
};
use crate::{
    abstract_model::ControllerAction,
    hasher::FnvHasher,
    resources::{
        ConditionStatus, ControllerRevision, GroupVersionKind, Metadata, OwnerReference,
        PersistentVolumeClaim, PersistentVolumeClaimVolumeSource, Pod, PodConditionType,
        PodManagementPolicyType, PodPhase, StatefulSet,
        StatefulSetPersistentVolumeClaimRetentionPolicyType, StatefulSetSpec, StatefulSetStatus,
        Volume,
    },
    state::StateView,
    utils::now,
};

const STATEFULSET_REVISION_LABEL: &str = "controller-revision-hash";
const STATEFUL_SET_POD_NAME_LABEL: &str = "statefulset.kubernetes.io/pod-name";
const POD_INDEX_LABEL: &str = "apps.kubernetes.io/pod-index";
const CONTROLLER_REVISION_HASH_LABEL: &str = "controller.kubernetes.io/hash";

#[derive(Clone, Debug)]
pub struct StatefulSetController;

#[derive(Debug, Default, Hash, Clone, PartialEq, Eq)]
pub struct StatefulSetControllerState;

#[derive(Debug)]
pub enum StatefulSetControllerAction {
    ControllerJoin(usize),

    UpdateStatefulSetStatus(StatefulSet),

    CreatePod(Pod),
    UpdatePod(Pod),
    DeletePod(Pod),

    CreatePersistentVolumeClaim(PersistentVolumeClaim),
    UpdatePersistentVolumeClaim(PersistentVolumeClaim),

    CreateControllerRevision(ControllerRevision),
    UpdateControllerRevision(ControllerRevision),
    DeleteControllerRevision(ControllerRevision),
}

impl From<StatefulSetControllerAction> for ControllerAction {
    fn from(val: StatefulSetControllerAction) -> Self {
        match val {
            StatefulSetControllerAction::ControllerJoin(id) => ControllerAction::ControllerJoin(id),
            StatefulSetControllerAction::UpdateStatefulSetStatus(sts) => {
                ControllerAction::UpdateStatefulSetStatus(sts)
            }
            StatefulSetControllerAction::CreatePod(p) => ControllerAction::CreatePod(p),
            StatefulSetControllerAction::UpdatePod(p) => ControllerAction::UpdatePod(p),
            StatefulSetControllerAction::DeletePod(p) => ControllerAction::DeletePod(p),
            StatefulSetControllerAction::CreatePersistentVolumeClaim(pvc) => {
                ControllerAction::CreatePersistentVolumeClaim(pvc)
            }
            StatefulSetControllerAction::UpdatePersistentVolumeClaim(pvc) => {
                ControllerAction::UpdatePersistentVolumeClaim(pvc)
            }
            StatefulSetControllerAction::CreateControllerRevision(cr) => {
                ControllerAction::CreateControllerRevision(cr)
            }
            StatefulSetControllerAction::UpdateControllerRevision(cr) => {
                ControllerAction::UpdateControllerRevision(cr)
            }
            StatefulSetControllerAction::DeleteControllerRevision(cr) => {
                ControllerAction::DeleteControllerRevision(cr)
            }
        }
    }
}

type ValOrOp<V> = super::util::ValOrOp<V, StatefulSetControllerAction>;

impl Controller for StatefulSetController {
    type State = StatefulSetControllerState;

    type Action = StatefulSetControllerAction;

    fn step(
        &self,
        id: usize,
        global_state: &StateView,
        _local_state: &mut Self::State,
    ) -> Option<StatefulSetControllerAction> {
        if !global_state.controllers.contains(&id) {
            return Some(StatefulSetControllerAction::ControllerJoin(id));
        } else {
            for statefulset in global_state.statefulsets.iter() {
                let pods = global_state.pods.iter().collect::<Vec<_>>();
                let revisions = global_state.controller_revisions.iter().collect::<Vec<_>>();
                let pvcs = global_state
                    .persistent_volume_claims
                    .iter()
                    .collect::<Vec<_>>();
                if let Some(op) = reconcile(statefulset, &pods, &revisions, &pvcs) {
                    return Some(op);
                }
            }
        }
        None
    }

    fn name(&self) -> String {
        "StatefulSet".to_owned()
    }
}

fn reconcile(
    statefulset: &StatefulSet,
    all_pods: &[&Pod],
    all_revisions: &[&ControllerRevision],
    all_pvcs: &[&PersistentVolumeClaim],
) -> Option<StatefulSetControllerAction> {
    // TODO: claim things

    let pods = all_pods
        .iter()
        .filter(|p| statefulset.spec.selector.matches(&p.metadata.labels))
        .copied()
        .collect::<Vec<_>>();

    let revisions = all_revisions
        .iter()
        .filter(|r| statefulset.spec.selector.matches(&r.metadata.labels))
        .copied()
        .collect::<Vec<_>>();

    let pvcs = all_pvcs;

    sync(statefulset, &pods, &revisions, pvcs)
}

fn sync(
    statefulset: &StatefulSet,
    pods: &[&Pod],
    revisions: &[&ControllerRevision],
    pvcs: &[&PersistentVolumeClaim],
) -> Option<StatefulSetControllerAction> {
    if let Some(op) = update_statefulset(statefulset, pods, revisions, pvcs) {
        return Some(op);
    }
    None
}

fn update_statefulset(
    statefulset: &StatefulSet,
    pods: &[&Pod],
    revisions: &[&ControllerRevision],
    pvcs: &[&PersistentVolumeClaim],
) -> Option<StatefulSetControllerAction> {
    // list all revisions and sort them
    let mut revisions = revisions.to_vec();
    sort_controller_revisions(&mut revisions);

    let rop = perform_update(statefulset, pods, &revisions, pvcs);
    let (current_revision, update_revision, _status) = match rop {
        ValOrOp::Op(op) => return Some(op),
        ValOrOp::Resource(r) => r,
    };

    // maintain the set's revision history limit
    truncate_history(
        statefulset,
        pods,
        &revisions,
        &current_revision,
        &update_revision,
    )
}

fn sort_controller_revisions(revisions: &mut [&ControllerRevision]) {
    revisions.sort_by(|r1, r2| {
        if r1.revision == r2.revision {
            if r1.metadata.creation_timestamp == r2.metadata.creation_timestamp {
                r1.metadata.name.cmp(&r2.metadata.name)
            } else {
                r1.metadata
                    .creation_timestamp
                    .cmp(&r2.metadata.creation_timestamp)
            }
        } else {
            r1.revision.cmp(&r2.revision)
        }
    });
}

fn perform_update(
    sts: &StatefulSet,
    pods: &[&Pod],
    revisions: &[&ControllerRevision],
    pvcs: &[&PersistentVolumeClaim],
) -> ValOrOp<(ControllerRevision, ControllerRevision, StatefulSetStatus)> {
    debug!("perform_update");
    let (current_revision, update_revision, collision_count) =
        match get_statefulset_revisions(sts, revisions) {
            ValOrOp::Resource(r) => r,
            ValOrOp::Op(op) => return ValOrOp::Op(op),
        };

    let current_status = do_update_statefulset(
        sts,
        &current_revision,
        &update_revision,
        collision_count,
        pods,
        pvcs,
    );
    let mut current_status = match current_status {
        ValOrOp::Resource(r) => r,
        ValOrOp::Op(op) => return ValOrOp::Op(op),
    };

    if let Some(op) = update_statefulset_status(sts, &mut current_status) {
        return ValOrOp::Op(op);
    }

    ValOrOp::Resource((current_revision, update_revision, current_status))
}

// updateStatefulSet performs the update function for a StatefulSet. This method creates, updates, and deletes Pods in
// the set in order to conform the system to the target state for the set. The target state always contains
// set.Spec.Replicas Pods with a Ready Condition. If the UpdateStrategy.Type for the set is
// RollingUpdateStatefulSetStrategyType then all Pods in the set must be at set.Status.CurrentRevision.
// If the UpdateStrategy.Type for the set is OnDeleteStatefulSetStrategyType, the target state implies nothing about
// the revisions of Pods in the set. If the UpdateStrategy.Type for the set is PartitionStatefulSetStrategyType, then
// all Pods with ordinal less than UpdateStrategy.Partition.Ordinal must be at Status.CurrentRevision and all other
// Pods must be at Status.UpdateRevision. If the returned error is nil, the returned StatefulSetStatus is valid and the
// update must be recorded. If the error is not nil, the method should be retried until successful.
fn do_update_statefulset(
    sts: &StatefulSet,
    current_revision: &ControllerRevision,
    update_revision: &ControllerRevision,
    collision_count: u32,
    pods: &[&Pod],
    pvcs: &[&PersistentVolumeClaim],
) -> ValOrOp<StatefulSetStatus> {
    debug!("do_update_statefulset");
    let current_sts = apply_revision(sts, current_revision);
    let update_sts = apply_revision(sts, update_revision);

    // set the generation, and revisions in the returned status
    let mut status = StatefulSetStatus {
        observed_generation: sts.metadata.generation,
        current_revision: current_revision.metadata.name.clone(),
        update_revision: update_revision.metadata.name.clone(),
        collision_count,
        ..Default::default()
    };

    update_status(
        &mut status,
        sts.spec.min_ready_seconds.unwrap_or_default(),
        current_revision,
        update_revision,
        &[pods.to_vec()],
    );

    if status != sts.status {
        let mut sts = sts.clone();
        sts.status = status;
        return ValOrOp::Op(StatefulSetControllerAction::UpdateStatefulSetStatus(sts));
    }

    let replica_count = sts.spec.replicas;
    let mut replicas = vec![None; replica_count.unwrap_or_default() as usize];
    let mut condemned = Vec::new();

    // First we partition pods into two lists valid replicas and condemned Pods
    for pod in pods {
        if pod_in_ordinal_range(pod, sts) {
            debug!(name = pod.metadata.name, "Pod in ordinal range");
            // if the ordinal of the pod is within the range of the current number of replicas,
            // insert it at the indirection of its ordinal
            if let Some(ordinal) = get_ordinal(pod) {
                let replica_index = (ordinal - get_start_ordinal(sts)) as usize;
                if replica_index < replicas.len() {
                    replicas[replica_index] = Some((*pod).clone());
                }
            }
        } else if get_ordinal(pod).is_some() {
            debug!(name = pod.metadata.name, "Pod not in ordinal range");
            // if the ordinal is valid, but not within the range add it to the condemned list
            condemned.push(*pod)
        }
        // If the ordinal could not be parsed (ord < 0), ignore the Pod.
    }

    // for any empty indices in the sequence [0,set.Spec.Replicas) create a new Pod at the correct revision
    for ord in get_start_ordinal(sts)..=get_end_ordinal(sts) {
        let replica_index = ord - get_start_ordinal(sts);
        if let Some(replica) = replicas.get_mut(replica_index as usize) {
            if replica.is_none() {
                debug!(ord, "filling in a missing pod");
                *replica = Some(new_versioned_statefulset_pod(
                    &current_sts,
                    &update_sts,
                    &current_revision.metadata.name,
                    &update_revision.metadata.name,
                    ord,
                ))
            }
        }
    }

    // sort the condemned Pods by their ordinals
    condemned.sort_by(|p1, p2| {
        let o1 = get_ordinal(p1);
        let o2 = get_ordinal(p2);
        o1.cmp(&o2).reverse()
    });

    let mut first_unhealthy_pod = None;

    // find the first unhealthy Pod
    for replica in &replicas {
        if !is_healthy(replica.as_ref().unwrap()) && first_unhealthy_pod.is_none() {
            first_unhealthy_pod = replica.clone()
        }
    }

    // or the first unhealthy condemned Pod (condemned are sorted in descending order for ease of use)
    for c in &condemned {
        if !is_healthy(c) && first_unhealthy_pod.is_none() {
            first_unhealthy_pod = Some((*c).clone());
        }
    }

    // If the StatefulSet is being deleted, don't do anything other than updating
    // status.
    if sts.metadata.deletion_timestamp.is_some() {
        return ValOrOp::Resource(status);
    }

    let monotonic = !allows_burst(sts);

    // First, process each living replica. Exit if we run into an error or something blocking in monotonic mode.
    let process_replica_fn = |replica| {
        process_replica(
            sts,
            current_revision,
            update_revision,
            &current_sts,
            &update_sts,
            monotonic,
            replica,
            pvcs,
        )
    };
    debug!("Processing replicas");
    match run_for_all(
        &replicas
            .iter()
            .filter_map(|i| i.as_ref())
            .collect::<Vec<_>>(),
        process_replica_fn,
        monotonic,
    ) {
        ValOrOp::Op(op) => return ValOrOp::Op(op),
        ValOrOp::Resource(should_exit) => {
            if should_exit {
                update_status(
                    &mut status,
                    sts.spec.min_ready_seconds.unwrap_or_default(),
                    current_revision,
                    update_revision,
                    &[
                        replicas.iter().filter_map(|i| i.as_ref()).collect(),
                        condemned,
                    ],
                );
                return ValOrOp::Resource(status);
            }
        }
    }

    // Fix pod claims for condemned pods, if necessary.
    let fix_pod_claim = |replica| {
        let match_policy = claims_match_retention_policy(&update_sts, replica, pvcs);
        if !match_policy {
            if let Some(op) = update_pod_claim_for_retention_policy(&update_sts, replica, pvcs) {
                return ValOrOp::Op(op);
            }
        }
        ValOrOp::Resource(false)
    };
    debug!("Fixing pod claims");
    match run_for_all(&condemned, fix_pod_claim, monotonic) {
        ValOrOp::Op(op) => return ValOrOp::Op(op),
        ValOrOp::Resource(should_exit) => {
            if should_exit {
                update_status(
                    &mut status,
                    sts.spec.min_ready_seconds.unwrap_or_default(),
                    current_revision,
                    update_revision,
                    &[
                        replicas.iter().filter_map(|i| i.as_ref()).collect(),
                        condemned,
                    ],
                );
                return ValOrOp::Resource(status);
            }
        }
    }

    // At this point, in monotonic mode all of the current Replicas are Running, Ready and Available,
    // and we can consider termination.
    // We will wait for all predecessors to be Running and Ready prior to attempting a deletion.
    // We will terminate Pods in a monotonically decreasing order.
    // Note that we do not resurrect Pods in this interval. Also note that scaling will take precedence over
    // updates.
    let process_condemned_fn =
        |replica| process_condemned(sts, first_unhealthy_pod.as_ref(), monotonic, replica);

    debug!("Processing condemned pods");
    match run_for_all(&condemned, process_condemned_fn, monotonic) {
        ValOrOp::Op(op) => return ValOrOp::Op(op),
        ValOrOp::Resource(should_exit) => {
            if should_exit {
                update_status(
                    &mut status,
                    sts.spec.min_ready_seconds.unwrap_or_default(),
                    current_revision,
                    update_revision,
                    &[
                        replicas.iter().filter_map(|i| i.as_ref()).collect(),
                        condemned,
                    ],
                );
                return ValOrOp::Resource(status);
            }
        }
    }

    update_status(
        &mut status,
        sts.spec.min_ready_seconds.unwrap_or_default(),
        current_revision,
        update_revision,
        &[
            replicas.iter().filter_map(|i| i.as_ref()).collect(),
            condemned,
        ],
    );

    // for the OnDelete strategy we short circuit. Pods will be updated when they are manually deleted.
    if sts.spec.update_strategy.r#type == "OnDelete" {
        return ValOrOp::Resource(status);
    }

    // we compute the minimum ordinal of the target sequence for a destructive update based on the strategy.
    let mut update_min = 0;
    if let Some(ru) = &sts.spec.update_strategy.rolling_update {
        update_min = ru.partition;
    }

    debug!(
        update_min,
        replicas = replicas.len(),
        "checking for deleteable pods"
    );
    // we terminate the Pod with the largest ordinal that does not match the update revision.
    for replica in replicas.iter().skip(update_min as usize).rev() {
        debug!(
            replica =? replica.as_ref().map(|r| &r.metadata.name),
            "checking for deleteable pods"
        );
        // delete the Pod if it is not already terminating and does not match the update revision.
        if get_pod_revision(replica.as_ref().unwrap()) != update_revision.metadata.name
            && !is_terminating(replica.as_ref().unwrap())
        {
            return ValOrOp::Op(StatefulSetControllerAction::DeletePod(
                replica.as_ref().unwrap().clone(),
            ));
        }

        // wait for unhealthy Pods on update
        if !is_healthy(replica.as_ref().unwrap()) {
            return ValOrOp::Resource(status);
        }
    }

    ValOrOp::Resource(status)
}

fn get_statefulset_revisions(
    sts: &StatefulSet,
    revisions: &[&ControllerRevision],
) -> ValOrOp<(ControllerRevision, ControllerRevision, u32)> {
    let revision_count = revisions.len();
    let mut revisions = revisions.to_vec();
    sort_controller_revisions(&mut revisions);

    let collision_count = sts.status.collision_count;

    // create a new revision from the current set
    let mut update_revision = new_revision(sts, next_revision(&revisions), collision_count);
    trace!(?update_revision, "built default update revision");

    // find any equivalent revisions
    let equal_revisions = find_equal_revisions(&revisions, &update_revision);
    let equal_count = equal_revisions.len();

    if equal_count > 0
        && equal_revision(
            revisions[revision_count - 1],
            equal_revisions[equal_count - 1],
        )
    {
        // if the equivalent revision is immediately prior the update revision has not changed
        update_revision = revisions[revision_count - 1].clone();
        trace!(?update_revision, "using prior unchanged revision");
    } else if equal_count > 0 {
        // if the equivalent revision is not immediately prior we will roll back by incrementing the
        // Revision of the equivalent revision
        if let Some(op) =
            update_controller_revision(equal_revisions[equal_count - 1], update_revision.revision)
        {
            return ValOrOp::Op(op);
        }
        update_revision = equal_revisions[equal_count - 1].clone();
        trace!(?update_revision, "rolling back");
    } else {
        //if there is no equivalent revision we create a new one
        trace!("creating new revision");
        return ValOrOp::Op(create_controller_revision(
            sts,
            &update_revision,
            collision_count,
        ));
    }

    let mut current_revision = None;

    // attempt to find the revision that corresponds to the current revision
    for rev in revisions {
        if rev.metadata.name == sts.status.current_revision {
            current_revision = Some(rev.clone());
            break;
        }
    }

    // if the current revision is nil we initialize the history by setting it to the update revision
    if current_revision.is_none() {
        current_revision = Some(update_revision.clone());
    }

    ValOrOp::Resource((current_revision.unwrap(), update_revision, collision_count))
}

fn truncate_history(
    sts: &StatefulSet,
    pods: &[&Pod],
    revisions: &[&ControllerRevision],
    current_revision: &ControllerRevision,
    update_revision: &ControllerRevision,
) -> Option<StatefulSetControllerAction> {
    debug!("truncate_history");
    let mut history = Vec::new();
    let mut live = BTreeMap::new();
    live.insert(current_revision.metadata.name.clone(), true);
    live.insert(update_revision.metadata.name.clone(), true);
    for pod in pods {
        live.insert(get_pod_revision(pod), true);
    }

    // collect live revisions and historic revisions
    for rev in revisions {
        if !live.get(&rev.metadata.name).copied().unwrap_or_default() {
            history.push(rev);
        }
    }

    let history_len = history.len();
    let history_limit = sts.spec.revision_history_limit.unwrap_or_default() as usize;
    if history_len <= history_limit {
        return None;
    }

    // delete any non-live history to maintain revision limit
    let history = &history[..(history_len - history_limit)];
    // for rev in history {
    if let Some(rev) = history.first() {
        return Some(StatefulSetControllerAction::DeleteControllerRevision(
            (**rev).clone(),
        ));
    }
    None
}

fn is_healthy(pod: &Pod) -> bool {
    is_running_and_ready(pod) && !is_terminating(pod)
}

fn is_running_and_ready(pod: &Pod) -> bool {
    pod.status.phase == PodPhase::Running && is_pod_ready(pod)
}

fn is_running_and_available(pod: &Pod, min_ready_seconds: u32) -> bool {
    if !is_pod_ready(pod) {
        return false;
    }

    let c = pod
        .status
        .conditions
        .iter()
        .find(|c| c.r#type == PodConditionType::Ready);
    if let Some(c) = c {
        if min_ready_seconds == 0
            || (c.last_transition_time.is_some()
                && c.last_transition_time.unwrap().0
                    + Duration::from_secs(min_ready_seconds as u64)
                    < now().0)
        {
            return true;
        }
    }
    false
}

fn is_pod_ready(pod: &Pod) -> bool {
    pod.status
        .conditions
        .iter()
        .find(|c| c.r#type == PodConditionType::Ready)
        .map(|c| c.status == ConditionStatus::True)
        .unwrap_or_default()
}

fn is_terminating(pod: &Pod) -> bool {
    pod.metadata.deletion_timestamp.is_some()
}

fn is_created(pod: &Pod) -> bool {
    pod.status.phase != PodPhase::Unknown
}

fn is_pending(pod: &Pod) -> bool {
    pod.status.phase == PodPhase::Pending
}

fn is_failed(pod: &Pod) -> bool {
    pod.status.phase == PodPhase::Failed
}

fn pod_claim_is_stale(sts: &StatefulSet, pod: &Pod, claims: &[&PersistentVolumeClaim]) -> bool {
    let policy = &sts.spec.persistent_volume_claim_retention_policy;
    if policy.when_scaled == StatefulSetPersistentVolumeClaimRetentionPolicyType::Retain {
        // PVCs are meant to be reused and so can't be stale.
        return false;
    }
    for (_, claim) in get_persistent_volume_claims(sts, pod) {
        if let Some(pvc) = claims.iter().find(|c| c.metadata.uid == claim.metadata.uid) {
            // A claim is stale if it doesn't match the pod's UID, including if the pod has no UID.
            if has_stale_owner_ref(&pvc.metadata.owner_references, &pod.metadata) {
                return true;
            }
        }
    }
    false
}

fn allows_burst(sts: &StatefulSet) -> bool {
    sts.spec.pod_management_policy == PodManagementPolicyType::Parallel
}

/// Restore the old statefulset based on current statefulset and the old saved state (just the pod template).
fn apply_revision(sts: &StatefulSet, revision: &ControllerRevision) -> StatefulSet {
    let unmarshaled: StatefulSet = serde_json::from_str(&revision.data).unwrap();
    let mut restored_sts = sts.clone();
    restored_sts.spec.template = unmarshaled.spec.template;
    restored_sts
}

fn update_status(
    status: &mut StatefulSetStatus,
    min_ready_seconds: u32,
    current_revision: &ControllerRevision,
    update_revision: &ControllerRevision,
    podlists: &[Vec<&Pod>],
) {
    let num_pods = podlists.iter().map(|l| l.len()).sum::<usize>();
    debug!(num_pods, "Updating status");
    status.replicas = 0;
    status.ready_replicas = 0;
    status.available_replicas = 0;
    status.current_replicas = 0;
    status.updated_replicas = 0;

    for list in podlists {
        let replica_status =
            compute_replica_status(list, min_ready_seconds, current_revision, update_revision);
        status.replicas += replica_status.replicas;
        status.ready_replicas += replica_status.ready_replicas;
        status.available_replicas += replica_status.available_replicas;
        status.current_replicas += replica_status.current_replicas;
        status.updated_replicas += replica_status.updated_replicas;
    }
}

#[derive(Default)]
struct ReplicaStatus {
    replicas: u32,
    ready_replicas: u32,
    available_replicas: u32,
    current_replicas: u32,
    updated_replicas: u32,
}

fn compute_replica_status(
    pods: &[&Pod],
    min_ready_seconds: u32,
    current_revision: &ControllerRevision,
    update_revision: &ControllerRevision,
) -> ReplicaStatus {
    debug!("compute_replica_status");
    let mut status = ReplicaStatus::default();
    for pod in pods {
        if is_created(pod) {
            status.replicas += 1;
        }

        // count the number of running and ready replicas
        if is_running_and_ready(pod) {
            status.ready_replicas += 1;
            if is_running_and_available(pod, min_ready_seconds) {
                status.available_replicas += 1;
            }
        }

        // count the number of current and update replicas
        if is_created(pod) && !is_terminating(pod) {
            if get_pod_revision(pod) == current_revision.metadata.name {
                status.current_replicas += 1
            }
            if get_pod_revision(pod) == update_revision.metadata.name {
                status.updated_replicas += 1
            }
        }
    }
    status
}

fn get_pod_revision(pod: &Pod) -> String {
    pod.metadata
        .labels
        .get(STATEFULSET_REVISION_LABEL)
        .cloned()
        .unwrap_or_default()
}

pub fn get_ordinal(pod: &Pod) -> Option<u32> {
    pod.metadata
        .name
        .split('-')
        .last()
        .and_then(|o| o.parse().ok())
}

fn get_start_ordinal(sts: &StatefulSet) -> u32 {
    if let Some(o) = &sts.spec.ordinals {
        o.start
    } else {
        0
    }
}

fn get_end_ordinal(sts: &StatefulSet) -> u32 {
    (get_start_ordinal(sts) + sts.spec.replicas.unwrap_or_default()).saturating_sub(1)
}

fn pod_in_ordinal_range(pod: &Pod, sts: &StatefulSet) -> bool {
    if let Some(ordinal) = get_ordinal(pod) {
        ordinal >= get_start_ordinal(sts) && ordinal <= get_end_ordinal(sts)
    } else {
        false
    }
}

fn process_replica(
    sts: &StatefulSet,
    _current_revision: &ControllerRevision,
    _update_revision: &ControllerRevision,
    _current_sts: &StatefulSet,
    update_sts: &StatefulSet,
    monotonic: bool,
    replica: &Pod,
    pvcs: &[&PersistentVolumeClaim],
) -> ValOrOp<bool> {
    debug!(
        name = replica.metadata.name,
        phase = ?replica.status.phase,
        "Processing replica"
    );
    // delete and recreate failed pods
    if is_failed(replica) {
        debug!(
            name = replica.metadata.name,
            "Replica has failed, deleting it"
        );
        return ValOrOp::Op(StatefulSetControllerAction::DeletePod(replica.clone()));
    }

    // If we find a Pod that has not been created we create the Pod
    if !is_created(replica) {
        let is_stale = pod_claim_is_stale(sts, replica, pvcs);
        if is_stale {
            debug!(name = replica.metadata.name, "Pod was stale");
            // If a pod has a stale PVC, no more work can be done this round.
            return ValOrOp::Resource(true);
        }
        debug!(
            name = replica.metadata.name,
            "Replica hasn't been created, creating it"
        );
        return ValOrOp::Op(StatefulSetControllerAction::CreatePod(replica.clone()));
    }

    // If the Pod is in pending state then trigger PVC creation to create missing PVCs
    if is_pending(replica) {
        debug!(
            name = replica.metadata.name,
            "Replica is pending, trying to create missing persistent volume claims"
        );
        if let Some(op) = create_missing_persistent_volume_claims(sts, replica, pvcs) {
            return ValOrOp::Op(op);
        }
    }

    // If we find a Pod that is currently terminating, we must wait until graceful deletion
    // completes before we continue to make progress.
    if is_terminating(replica) && monotonic {
        return ValOrOp::Resource(true);
    }

    // If we have a Pod that has been created but is not running and ready we can not make progress.
    // We must ensure that all for each Pod, when we create it, all of its predecessors, with respect to its
    // ordinal, are Running and Ready.
    if !is_running_and_ready(replica) && monotonic {
        return ValOrOp::Resource(true);
    }

    // If we have a Pod that has been created but is not available we can not make progress.
    // We must ensure that all for each Pod, when we create it, all of its predecessors, with respect to its
    // ordinal, are Available.
    if !is_running_and_available(replica, sts.spec.min_ready_seconds.unwrap_or_default())
        && monotonic
    {
        return ValOrOp::Resource(true);
    }

    let retention_match = claims_match_retention_policy(update_sts, replica, pvcs);

    if identity_matches(sts, replica) && storage_matches(sts, replica) && retention_match {
        return ValOrOp::Resource(false);
    }

    let mut replica = replica.clone();
    if let Some(op) = update_stateful_pod(update_sts, &mut replica, pvcs) {
        return ValOrOp::Op(op);
    }

    ValOrOp::Resource(false)
}

fn update_stateful_pod(
    sts: &StatefulSet,
    pod: &mut Pod,
    claims: &[&PersistentVolumeClaim],
) -> Option<StatefulSetControllerAction> {
    let mut consistent = true;
    if !identity_matches(sts, pod) {
        update_identity(sts, pod);
        consistent = false;
    }

    if !storage_matches(sts, pod) {
        update_storage(sts, pod);
        return create_missing_persistent_volume_claims(sts, pod, claims);
    }

    // if the Pod's PVCs are not consistent with the StatefulSet's PVC deletion policy, update the PVC
    // and dirty the pod.
    if !claims_match_retention_policy(sts, pod, claims) {
        if let Some(op) = update_pod_claim_for_retention_policy(sts, pod, claims) {
            return Some(op);
        }
    }

    if consistent {
        None
    } else {
        Some(StatefulSetControllerAction::UpdatePod(pod.clone()))
    }
}

fn run_for_all<'a>(
    pods: &[&'a Pod],
    f: impl Fn(&'a Pod) -> ValOrOp<bool>,
    _monotonic: bool,
) -> ValOrOp<bool> {
    // if monotonic {
    for pod in pods {
        match f(pod) {
            ValOrOp::Resource(should_exit) => {
                if should_exit {
                    return ValOrOp::Resource(true);
                }
            }
            ValOrOp::Op(op) => return ValOrOp::Op(op),
        }
    }
    // } else {
    //     // TODO: could be slowstartbatch instead
    //     for pod in pods {
    //         match f(pod) {
    //             ResourceOrOp::Resource(should_exit) => {
    //                 if should_exit {
    //                     return ResourceOrOp::Resource(true);
    //                 }
    //             }
    //             ResourceOrOp::Op(op) => return ResourceOrOp::Op(op),
    //         }
    //     }
    // }
    ValOrOp::Resource(false)
}

fn process_condemned(
    sts: &StatefulSet,
    first_unhealthy_pod: Option<&Pod>,
    monotonic: bool,
    condemned: &Pod,
) -> ValOrOp<bool> {
    if is_terminating(condemned) {
        // if we are in monotonic mode, block and wait for terminating pods to expire
        if monotonic {
            return ValOrOp::Resource(true);
        }
        return ValOrOp::Resource(false);
    }

    // if we are in monotonic mode and the condemned target is not the first unhealthy Pod block
    if !is_running_and_ready(condemned) && monotonic && Some(condemned) != first_unhealthy_pod {
        return ValOrOp::Resource(true);
    }

    // if we are in monotonic mode and the condemned target is not the first unhealthy Pod, block.
    if !is_running_and_available(condemned, sts.spec.min_ready_seconds.unwrap_or_default())
        && monotonic
        && Some(condemned) != first_unhealthy_pod
    {
        return ValOrOp::Resource(true);
    }

    ValOrOp::Op(StatefulSetControllerAction::DeletePod(condemned.clone()))
}

fn identity_matches(sts: &StatefulSet, pod: &Pod) -> bool {
    let mut name_parts = pod.metadata.name.split('-').collect::<Vec<_>>();
    let ordinal: u32 = name_parts.remove(name_parts.len() - 1).parse().unwrap();
    let parent = name_parts.join("-");

    sts.metadata.name == parent
        && pod.metadata.name == get_pod_name(sts, ordinal)
        && pod.metadata.namespace == sts.metadata.namespace
        && pod.metadata.labels.get(STATEFUL_SET_POD_NAME_LABEL) == Some(&pod.metadata.name)
}

fn get_pod_name(sts: &StatefulSet, ordinal: u32) -> String {
    format!("{}-{}", sts.metadata.name, ordinal)
}

fn storage_matches(sts: &StatefulSet, pod: &Pod) -> bool {
    if let Some(ordinal) = get_ordinal(pod) {
        let volumes = pod
            .spec
            .volumes
            .iter()
            .map(|v| (v.name.clone(), v))
            .collect::<BTreeMap<_, _>>();
        for claim in &sts.spec.volume_claim_templates {
            let volume = volumes.get(&claim.metadata.name);
            if volume.is_none()
                || volume.unwrap().persistent_volume_claim.is_none()
                || volume
                    .unwrap()
                    .persistent_volume_claim
                    .as_ref()
                    .unwrap()
                    .claim_name
                    != get_persistent_volume_claim_name(sts, claim, ordinal)
            {
                return false;
            }
        }
        true
    } else {
        false
    }
}

fn get_persistent_volume_claim_name(
    sts: &StatefulSet,
    claim: &PersistentVolumeClaim,
    ordinal: u32,
) -> String {
    format!("{}-{}-{}", claim.metadata.name, sts.metadata.name, ordinal)
}

fn create_missing_persistent_volume_claims(
    sts: &StatefulSet,
    pod: &Pod,
    claims: &[&PersistentVolumeClaim],
) -> Option<StatefulSetControllerAction> {
    if let Some(op) = create_persistent_volume_claims(sts, pod, claims) {
        let StatefulSetControllerAction::CreatePersistentVolumeClaim(mut claim) = op else {
            unreachable!()
        };
        update_claim_owner_ref_for_set_and_pod(&mut claim, sts, pod);
        Some(StatefulSetControllerAction::CreatePersistentVolumeClaim(
            claim,
        ))
    } else {
        None
    }
}

fn create_persistent_volume_claims(
    sts: &StatefulSet,
    pod: &Pod,
    claims: &[&PersistentVolumeClaim],
) -> Option<StatefulSetControllerAction> {
    debug!(pod = pod.metadata.name, "Creating persistent volume claims");
    for (_, claim) in get_persistent_volume_claims(sts, pod) {
        if !claims
            .iter()
            .any(|c| c.metadata.name == claim.metadata.name)
        {
            debug!(
                pod = pod.metadata.name,
                claim = claim.metadata.name,
                "Creating persistent volume claim"
            );
            return Some(StatefulSetControllerAction::CreatePersistentVolumeClaim(
                claim,
            ));
        }
    }

    None
}

fn get_persistent_volume_claims(
    sts: &StatefulSet,
    pod: &Pod,
) -> BTreeMap<String, PersistentVolumeClaim> {
    let mut claims = BTreeMap::new();
    if let Some(ordinal) = get_ordinal(pod) {
        for template_claim in &sts.spec.volume_claim_templates {
            debug!(
                ordinal,
                template_name = template_claim.metadata.name,
                "Found volume claim template"
            );
            let mut claim = template_claim.clone();
            claim.metadata.name = get_persistent_volume_claim_name(sts, &claim, ordinal);
            claim.metadata.namespace = sts.metadata.namespace.clone();
            for (k, v) in &sts.spec.selector.match_labels {
                claim.metadata.labels.insert(k.clone(), v.clone());
            }
            claims.insert(template_claim.metadata.name.clone(), claim);
        }
    }
    claims
}

fn new_versioned_statefulset_pod(
    current_sts: &StatefulSet,
    update_sts: &StatefulSet,
    current_revision: &str,
    update_revision: &str,
    ordinal: u32,
) -> Pod {
    if current_sts.spec.update_strategy.r#type == "Rolling"
        && (current_sts.spec.update_strategy.rolling_update.is_none()
            && ordinal < (get_start_ordinal(current_sts) + current_sts.status.current_replicas))
        || (current_sts.spec.update_strategy.rolling_update.is_some()
            && ordinal
                < get_start_ordinal(current_sts)
                    + current_sts
                        .spec
                        .update_strategy
                        .rolling_update
                        .as_ref()
                        .unwrap()
                        .partition)
    {
        let mut pod = new_statefulset_pod(current_sts, ordinal);
        set_pod_revision(&mut pod, current_revision.to_owned());
        pod
    } else {
        let mut pod = new_statefulset_pod(update_sts, ordinal);
        set_pod_revision(&mut pod, update_revision.to_owned());
        pod
    }
}

fn new_statefulset_pod(sts: &StatefulSet, ordinal: u32) -> Pod {
    let mut pod = get_pod_from_template(&sts.metadata, &sts.spec.template, &StatefulSet::GVK);
    pod.metadata.name = get_pod_name(sts, ordinal);
    init_identity(sts, &mut pod);
    update_storage(sts, &mut pod);
    pod
}

fn set_pod_revision(pod: &mut Pod, revision: String) {
    pod.metadata
        .labels
        .insert(STATEFULSET_REVISION_LABEL.to_owned(), revision);
}

fn init_identity(sts: &StatefulSet, pod: &mut Pod) {
    update_identity(sts, pod);
    // Set these immutable fields only on initial Pod creation, not updates.
    pod.spec.hostname = pod.metadata.name.clone();
    pod.spec.subdomain = sts.spec.service_name.clone();
}

fn update_identity(sts: &StatefulSet, pod: &mut Pod) {
    if let Some(ordinal) = get_ordinal(pod) {
        pod.metadata.name = get_pod_name(sts, ordinal);
        pod.metadata.namespace = sts.metadata.namespace.clone();
        pod.metadata.labels.insert(
            STATEFUL_SET_POD_NAME_LABEL.to_owned(),
            pod.metadata.name.clone(),
        );
        pod.metadata
            .labels
            .insert(POD_INDEX_LABEL.to_owned(), ordinal.to_string());
    }
}

fn update_storage(sts: &StatefulSet, pod: &mut Pod) {
    let current_volumes = &pod.spec.volumes;
    let claims = get_persistent_volume_claims(sts, pod);
    let mut new_volumes = Vec::new();

    for (name, claim) in &claims {
        new_volumes.push(Volume {
            name: name.clone(),
            persistent_volume_claim: Some(PersistentVolumeClaimVolumeSource {
                claim_name: claim.metadata.name.clone(),
                // TODO: Use source definition to set this value when we have one.
                read_only: false,
            }),
        });
    }
    for cv in current_volumes {
        if !claims.contains_key(&cv.name) {
            new_volumes.push(cv.clone());
        }
    }
    pod.spec.volumes = new_volumes;
}

fn next_revision(revisions: &[&ControllerRevision]) -> u64 {
    let count = if revisions.is_empty() {
        1
    } else {
        revisions.len()
    };
    revisions.get(count - 1).map_or(0, |r| r.revision) + 1
}

fn new_revision(sts: &StatefulSet, revision: u64, collision_count: u32) -> ControllerRevision {
    let patch = get_patch(sts);

    let mut cr = new_controller_revision(
        sts,
        &StatefulSet::GVK,
        &sts.spec.template.metadata.labels,
        String::from_utf8(patch).unwrap(),
        revision,
        collision_count,
    );

    for (k, v) in &sts.metadata.annotations {
        cr.metadata.annotations.insert(k.clone(), v.clone());
    }
    cr
}

/// Return just the patch of the pod template.
fn get_patch(sts: &StatefulSet) -> Vec<u8> {
    let template_spec = &sts.spec.template;
    let patch_sts = StatefulSet {
        spec: StatefulSetSpec {
            template: template_spec.clone(),
            ..Default::default()
        },
        ..Default::default()
    };

    serde_json::to_vec(&patch_sts).unwrap()
}

fn find_equal_revisions<'a>(
    revisions: &[&'a ControllerRevision],
    needle: &ControllerRevision,
) -> Vec<&'a ControllerRevision> {
    revisions
        .iter()
        .filter(|r| equal_revision(r, needle))
        .copied()
        .collect()
}

fn equal_revision(lhs: &ControllerRevision, rhs: &ControllerRevision) -> bool {
    let lhs_hash = lhs.metadata.labels.get(CONTROLLER_REVISION_HASH_LABEL);
    let rhs_hash = rhs.metadata.labels.get(CONTROLLER_REVISION_HASH_LABEL);
    debug!(lhs_hash, rhs_hash, "checking equal revision");
    if lhs_hash != rhs_hash {
        return false;
    }
    lhs.data == rhs.data
}

fn update_controller_revision(
    revision: &ControllerRevision,
    new_revision: u64,
) -> Option<StatefulSetControllerAction> {
    let mut clone = revision.clone();
    if revision.revision == new_revision {
        return None;
    }

    clone.revision = new_revision;
    Some(StatefulSetControllerAction::UpdateControllerRevision(clone))
}

fn create_controller_revision(
    parent: &StatefulSet,
    revision: &ControllerRevision,
    collision_count: u32,
) -> StatefulSetControllerAction {
    let mut revision = revision.clone();
    revision.metadata.namespace = parent.metadata.namespace.clone();

    let hash = hash_controller_revision(&revision, collision_count);
    revision.metadata.name = controller_revision_name(&parent.metadata.name, &hash);

    StatefulSetControllerAction::CreateControllerRevision(revision)
}

fn hash_controller_revision(cr: &ControllerRevision, collision_count: u32) -> String {
    let mut hasher = FnvHasher::new_32a();
    if !cr.data.is_empty() {
        hasher.write(cr.data.as_bytes())
    }

    hasher.write(collision_count.to_string().as_bytes());

    hasher.finish_32().to_string()
}

fn controller_revision_name(prefix: &str, hash: &str) -> String {
    format!("{}-{}", prefix, hash)
}

fn new_controller_revision(
    parent: &StatefulSet,
    controller_kind: &GroupVersionKind,
    template_labels: &BTreeMap<String, String>,
    data: String,
    revision: u64,
    collision_count: u32,
) -> ControllerRevision {
    let mut cr = ControllerRevision {
        metadata: Metadata {
            labels: template_labels.clone(),
            owner_references: vec![new_controller_ref(&parent.metadata, controller_kind)],
            ..Default::default()
        },
        revision,
        data,
    };
    let hash = hash_controller_revision(&cr, collision_count);
    cr.metadata.name = controller_revision_name(&parent.metadata.name, &hash);
    cr.metadata
        .labels
        .insert(CONTROLLER_REVISION_HASH_LABEL.to_owned(), hash);
    cr
}

fn update_statefulset_status(
    sts: &StatefulSet,
    status: &mut StatefulSetStatus,
) -> Option<StatefulSetControllerAction> {
    complete_rolling_update(sts, status);

    if !inconsistent_status(sts, status) {
        return None;
    }

    let mut sts = sts.clone();
    sts.status = status.clone();
    Some(StatefulSetControllerAction::UpdateStatefulSetStatus(sts))
}

fn complete_rolling_update(sts: &StatefulSet, status: &mut StatefulSetStatus) {
    if sts.spec.update_strategy.r#type == "RollingUpdate"
        && status.updated_replicas == status.replicas
        && status.ready_replicas == status.replicas
    {
        status.current_replicas = status.updated_replicas;
        status.current_revision = status.update_revision.clone();
    }
}

fn inconsistent_status(sts: &StatefulSet, status: &StatefulSetStatus) -> bool {
    status.observed_generation > sts.status.observed_generation
        || status.replicas != sts.status.replicas
        || status.current_replicas != sts.status.current_replicas
        || status.ready_replicas != sts.status.ready_replicas
        || status.updated_replicas != sts.status.updated_replicas
        || status.current_revision != sts.status.current_revision
        || status.available_replicas != sts.status.available_replicas
        || status.update_revision != sts.status.update_revision
}

fn claims_match_retention_policy(
    sts: &StatefulSet,
    pod: &Pod,
    claims: &[&PersistentVolumeClaim],
) -> bool {
    if let Some(ordinal) = get_ordinal(pod) {
        for template in &sts.spec.volume_claim_templates {
            let claim_name = get_persistent_volume_claim_name(sts, template, ordinal);
            if let Some(claim) = claims.iter().find(|c| c.metadata.name == claim_name) {
                if !claim_owner_matches_set_and_pod(claim, sts, pod) {
                    return false;
                }
            }
        }
    }
    true
}

fn update_pod_claim_for_retention_policy(
    sts: &StatefulSet,
    pod: &Pod,
    claims: &[&PersistentVolumeClaim],
) -> Option<StatefulSetControllerAction> {
    if let Some(ordinal) = get_ordinal(pod) {
        for template in &sts.spec.volume_claim_templates {
            let claim_name = get_persistent_volume_claim_name(sts, template, ordinal);
            if let Some(claim) = claims.iter().find(|c| c.metadata.name == claim_name) {
                if !claim_owner_matches_set_and_pod(claim, sts, pod) {
                    debug!("Updating pod claim for retention policy");
                    let mut updated_claim = (*claim).clone();
                    update_claim_owner_ref_for_set_and_pod(&mut updated_claim, sts, pod);
                    if &updated_claim != *claim {
                        return Some(StatefulSetControllerAction::UpdatePersistentVolumeClaim(
                            updated_claim,
                        ));
                    }
                }
            }
        }
    }
    None
}

fn claim_owner_matches_set_and_pod(
    claim: &PersistentVolumeClaim,
    sts: &StatefulSet,
    pod: &Pod,
) -> bool {
    let policy = &sts.spec.persistent_volume_claim_retention_policy;

    match (policy.when_scaled, policy.when_deleted) {
        (
            StatefulSetPersistentVolumeClaimRetentionPolicyType::Retain,
            StatefulSetPersistentVolumeClaimRetentionPolicyType::Retain,
        ) => {
            if has_owner_ref(&claim.metadata.owner_references, &sts.metadata.uid)
                || has_owner_ref(&claim.metadata.owner_references, &pod.metadata.uid)
            {
                return false;
            }
        }
        (
            StatefulSetPersistentVolumeClaimRetentionPolicyType::Retain,
            StatefulSetPersistentVolumeClaimRetentionPolicyType::Delete,
        ) => {
            if !has_owner_ref(&claim.metadata.owner_references, &sts.metadata.uid)
                || has_owner_ref(&claim.metadata.owner_references, &pod.metadata.uid)
            {
                return false;
            }
        }
        (
            StatefulSetPersistentVolumeClaimRetentionPolicyType::Delete,
            StatefulSetPersistentVolumeClaimRetentionPolicyType::Retain,
        ) => {
            if has_owner_ref(&claim.metadata.owner_references, &sts.metadata.uid) {
                return false;
            }
            let pod_scaled_down = !pod_in_ordinal_range(pod, sts);
            if pod_scaled_down != has_owner_ref(&claim.metadata.owner_references, &pod.metadata.uid)
            {
                return false;
            }
        }
        (
            StatefulSetPersistentVolumeClaimRetentionPolicyType::Delete,
            StatefulSetPersistentVolumeClaimRetentionPolicyType::Delete,
        ) => {
            let pod_scaled_down = !pod_in_ordinal_range(pod, sts);
            // If a pod is scaled down, there should be no set ref and a pod ref;
            // if the pod is not scaled down it's the other way around.
            if pod_scaled_down == has_owner_ref(&claim.metadata.owner_references, &sts.metadata.uid)
            {
                return false;
            }
            if pod_scaled_down != has_owner_ref(&claim.metadata.owner_references, &pod.metadata.uid)
            {
                return false;
            }
        }
    }
    true
}

fn update_claim_owner_ref_for_set_and_pod(
    claim: &mut PersistentVolumeClaim,
    sts: &StatefulSet,
    pod: &Pod,
) {
    let pod_meta = Pod::GVK;
    let sts_meta = StatefulSet::GVK;

    let policy = &sts.spec.persistent_volume_claim_retention_policy;
    match (policy.when_scaled, policy.when_deleted) {
        (
            StatefulSetPersistentVolumeClaimRetentionPolicyType::Retain,
            StatefulSetPersistentVolumeClaimRetentionPolicyType::Retain,
        ) => {
            remove_owner_ref(&mut claim.metadata, &sts.metadata);
            remove_owner_ref(&mut claim.metadata, &pod.metadata);
        }
        (
            StatefulSetPersistentVolumeClaimRetentionPolicyType::Retain,
            StatefulSetPersistentVolumeClaimRetentionPolicyType::Delete,
        ) => {
            set_owner_ref(&mut claim.metadata, &sts.metadata, &sts_meta);
            remove_owner_ref(&mut claim.metadata, &pod.metadata);
        }
        (
            StatefulSetPersistentVolumeClaimRetentionPolicyType::Delete,
            StatefulSetPersistentVolumeClaimRetentionPolicyType::Retain,
        ) => {
            remove_owner_ref(&mut claim.metadata, &sts.metadata);
            let pod_scaled_down = !pod_in_ordinal_range(pod, sts);
            if pod_scaled_down {
                set_owner_ref(&mut claim.metadata, &pod.metadata, &pod_meta);
            } else {
                remove_owner_ref(&mut claim.metadata, &pod.metadata);
            }
        }
        (
            StatefulSetPersistentVolumeClaimRetentionPolicyType::Delete,
            StatefulSetPersistentVolumeClaimRetentionPolicyType::Delete,
        ) => {
            let pod_scaled_down = !pod_in_ordinal_range(pod, sts);
            if pod_scaled_down {
                remove_owner_ref(&mut claim.metadata, &sts.metadata);
                set_owner_ref(&mut claim.metadata, &pod.metadata, &pod_meta);
            } else {
                set_owner_ref(&mut claim.metadata, &sts.metadata, &sts_meta);
                remove_owner_ref(&mut claim.metadata, &pod.metadata);
            }
        }
    }
}

fn has_owner_ref(owner_refs: &[OwnerReference], owner_uid: &str) -> bool {
    owner_refs.iter().any(|or| or.uid == owner_uid)
}

fn set_owner_ref(target: &mut Metadata, owner: &Metadata, owner_type: &GroupVersionKind) -> bool {
    if has_owner_ref(&target.owner_references, &owner.uid) {
        return false;
    }
    target.owner_references.push(OwnerReference {
        api_version: owner_type.api_version(),
        kind: owner_type.kind.to_owned(),
        name: owner.name.clone(),
        uid: owner.uid.clone(),
        block_owner_deletion: false,
        controller: false,
    });
    true
}

fn remove_owner_ref(target: &mut Metadata, owner: &Metadata) -> bool {
    if !has_owner_ref(&target.owner_references, &owner.uid) {
        return false;
    }

    target.owner_references.retain(|or| or.uid != owner.uid);
    true
}

fn has_stale_owner_ref(target: &[OwnerReference], owner: &Metadata) -> bool {
    target
        .iter()
        .any(|or| or.name == owner.name && or.uid != owner.uid)
}

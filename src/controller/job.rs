use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

use tracing::debug;

use crate::{
    abstract_model::ControllerAction,
    resources::{
        ConditionStatus, Container, ContainerStatus, EnvVar, EnvVarSource, JobCompletionMode,
        JobCondition, JobConditionType, JobPodFailurePolicy, JobPodFailurePolicyRuleAction,
        JobPodFailurePolicyRuleOnExitCodesRequirement,
        JobPodFailurePolicyRuleOnExitCodesRequirementOperator,
        JobPodFailurePolicyRuleOnPodConditionsPattern, JobStatus, ObjectFieldSelector, Pod,
        PodCondition, PodPhase, PodRestartPolicy, PodStatus, PodTemplateSpec, Time,
    },
    resources::{Job, PodConditionType},
    utils::now,
};

use super::{
    util::{
        self, filter_terminating_pods, get_pod_from_template, is_pod_ready, is_pod_terminating,
    },
    Controller,
};

const JOB_COMPLETION_INDEX_ANNOTATION: &str = "batch.kubernetes.io/job-completion-index";
const JOB_TRACKING_FINALIZER: &str = "batch.kubernetes.io/job-tracking";

const JOB_COMPLETION_INDEX_ENV_NAME: &str = "JOB_COMPLETION_INDEX";

const JOB_REASON_POD_FAILURE_POLICY: &str = "PodFailurePolicy";
const JOB_REASON_BACKOFF_LIMIT_EXCEEDED: &str = "BackoffLimitExceeded";
const JOB_REASON_DEADLINE_EXCEEDED: &str = "DeadlineExceeded";
const MAX_POD_CREATE_DELETE_PER_SYNC: usize = 500;

// MaxUncountedPods is the maximum size the slices in
// .status.uncountedTerminatedPods should have to keep their representation
// roughly below 20 KB. Exported for tests
const MAX_UNCOUNTED_PODS: u32 = 500;

#[derive(Clone, Debug)]
pub struct JobController;

#[derive(Debug, Default, Hash, Clone, PartialEq, Eq)]
pub struct JobControllerState;

#[derive(Debug, Hash, Clone, PartialEq, Eq)]
#[must_use]
pub enum JobControllerAction {
    UpdateJobStatus(Job),

    CreatePod(Pod),
    UpdatePod(Pod),
    DeletePod(Pod),
}

#[must_use]
struct OptionalJobControllerAction(Option<JobControllerAction>);
impl From<Option<JobControllerAction>> for OptionalJobControllerAction {
    fn from(value: Option<JobControllerAction>) -> Self {
        Self(value)
    }
}

impl From<JobControllerAction> for ControllerAction {
    fn from(value: JobControllerAction) -> Self {
        match value {
            JobControllerAction::UpdateJobStatus(j) => ControllerAction::UpdateJobStatus(j),
            JobControllerAction::CreatePod(pod) => ControllerAction::CreatePod(pod),
            JobControllerAction::UpdatePod(pod) => ControllerAction::UpdatePod(pod),
            JobControllerAction::DeletePod(pod) => ControllerAction::SoftDeletePod(pod),
        }
    }
}

impl Controller for JobController {
    type State = JobControllerState;

    type Action = JobControllerAction;

    fn step(
        &self,
        global_state: &crate::state::RawState,
        _local_state: &mut Self::State,
    ) -> Option<Self::Action> {
        for job in global_state.jobs.iter() {
            let mut pods = global_state
                .pods
                .iter()
                .filter(|p| job.spec.selector.matches(&p.metadata.labels))
                .collect::<Vec<_>>();
            let mut job = job.clone();
            if let Some(op) = reconcile(&mut job, &mut pods).0 {
                return Some(op);
            }
        }
        None
    }

    fn name(&self) -> String {
        "Job".to_owned()
    }
}

fn reconcile(job: &mut Job, pods: &mut [&Pod]) -> OptionalJobControllerAction {
    let active_pods = util::filter_active_pods(pods);
    let active = active_pods.len();
    let expected_rm_finalizers = Vec::new();
    let (new_succeeded_pods, new_failed_pods) =
        get_new_finished_pods(job, pods, &expected_rm_finalizers);
    let mut succeeded = job.status.succeeded as usize
        + new_succeeded_pods.len()
        + job.status.uncounted_terminated_pods.succeeded.len();
    let failed = job.status.failed as usize
        + non_ignored_failed_pods_count(job, &new_failed_pods)
        + job.status.uncounted_terminated_pods.failed.len();
    let ready = count_ready_pods(&active_pods);

    // Job first start. Set StartTime only if the job is not in the suspended state.
    if job.status.start_time.is_none() && !job.spec.suspend {
        job.status.start_time = Some(now());
    }

    let exceeds_backoff_limit = failed > job.spec.backoff_limit.unwrap_or_default() as usize;

    let mut finished_condition = if let Some(failure_target_condition) =
        find_condition_by_type(&job.status.conditions, JobConditionType::FailureTarget)
    {
        Some(new_condition(
            JobConditionType::Failed,
            ConditionStatus::True,
            failure_target_condition.reason.clone(),
            failure_target_condition.message.clone(),
            now(),
        ))
    } else if let Some(fail_job_message) = get_fail_job_message(job, pods) {
        // Prepare the interim FailureTarget condition to record the failure message before the finalizers (allowing removal of the pods) are removed.
        Some(new_condition(
            JobConditionType::FailureTarget,
            ConditionStatus::True,
            JOB_REASON_POD_FAILURE_POLICY.to_owned(),
            fail_job_message,
            now(),
        ))
    } else if exceeds_backoff_limit || past_backoff_limit_on_failure(job, pods) {
        // check if the number of pod restart exceeds backoff (for restart OnFailure only)
        // OR if the number of failed jobs increased since the last syncJob
        Some(new_condition(
            JobConditionType::Failed,
            ConditionStatus::True,
            JOB_REASON_BACKOFF_LIMIT_EXCEEDED.to_owned(),
            "Job has reached the specified backoff limit".to_owned(),
            now(),
        ))
    } else if past_active_deadline(job) {
        Some(new_condition(
            JobConditionType::Failed,
            ConditionStatus::True,
            JOB_REASON_DEADLINE_EXCEEDED.to_owned(),
            "Job was active longer than specified deadline".to_owned(),
            now(),
        ))
    } else if job.spec.active_deadline_seconds.is_some() && !job.spec.suspend {
        // let sync_duration = job.spec.active_deadline_seconds - (now() - job.status.start_time);
        // TODO: requeue
        todo!()
    } else {
        None
    };

    let (prev_succeeded_indexes, succeeded_indexes) = if job.spec.completion_mode
        == JobCompletionMode::Indexed
    {
        let (prev_succeeded_indexes, succeeded_indexes) = calculate_succeeded_indexes(job, pods);
        succeeded = succeeded_indexes.total() as usize;
        debug!(?succeeded_indexes, "succeeded_indexes");
        (prev_succeeded_indexes, succeeded_indexes)
    } else {
        (OrderedIntervals::default(), OrderedIntervals::default())
    };

    let mut suspend_cond_changed = false;
    // Remove active pods if Job failed.
    if finished_condition.is_some() {
        if let Some(delete_op) = delete_active_pods(&active_pods).0 {
            return Some(delete_op).into();
        }
        // if deleted != active {
        //     // Can't declare the Job as finished yet, as there might be remaining
        //     // pod finalizers or pods that are not in the informer's cache yet.
        //     finished_condition = None;
        // }
        // active -= deleted;
        // ASSUME that active is empty if we got here as we always return a delete operation
        // otherwise.
    } else {
        let mut manage_job_called = false;
        if job.metadata.deletion_timestamp.is_none() {
            if let Some(op) = manage_job(job, pods, &active_pods, succeeded, &succeeded_indexes).0 {
                return Some(op).into();
            }
            manage_job_called = true;
        }
        debug!(succeeded, active, ?job.spec.completions, "Calculating complete");
        let complete = if job.spec.completions.is_none() {
            // This type of job is complete when any pod exits with success.
            // Each pod is capable of
            // determining whether or not the entire Job is done.  Subsequent pods are
            // not expected to fail, but if they do, the failure is ignored.  Once any
            // pod succeeds, the controller waits for remaining pods to finish, and
            // then the job is complete.
            succeeded > 0 && active == 0
        } else {
            // Job specifies a number of completions.  This type of job signals
            // success by having that number of successes.  Since we do not
            // start more pods than there are remaining completions, there should
            // not be any remaining active pods once this count is reached.
            succeeded as u32 >= job.spec.completions.unwrap() && active == 0
        };

        if complete {
            debug!("Job complete");
            finished_condition = Some(new_condition(
                JobConditionType::Complete,
                ConditionStatus::True,
                String::new(),
                String::new(),
                now(),
            ));
        } else if manage_job_called {
            debug!("Manage job called");
            // Update the conditions / emit events only if manageJob was called in
            // this syncJob. Otherwise wait for the right syncJob call to make
            // updates.
            if job.spec.suspend {
                // Job can be in the suspended state only if it is NOT completed.
                if let Some(new_conditions) = ensure_job_condition_status(
                    &job.status.conditions,
                    JobConditionType::Suspended,
                    ConditionStatus::True,
                    "JobSuspended".to_owned(),
                    "Job suspended".to_owned(),
                    now(),
                ) {
                    job.status.conditions = new_conditions;
                    debug!("Suspend condition changed");
                    suspend_cond_changed = true;
                }
            } else {
                // Job not suspended.
                if let Some(new_conditions) = ensure_job_condition_status(
                    &job.status.conditions,
                    JobConditionType::Suspended,
                    ConditionStatus::False,
                    "JobResumed".to_owned(),
                    "Job resumed".to_owned(),
                    now(),
                ) {
                    job.status.conditions = new_conditions;
                    debug!("Suspend condition changed");
                    suspend_cond_changed = true;
                    // Resumed jobs will always reset StartTime to current time. This is
                    // done because the ActiveDeadlineSeconds timer shouldn't go off
                    // whilst the Job is still suspended and resetting StartTime is
                    // consistent with resuming a Job created in the suspended state.
                    // (ActiveDeadlineSeconds is interpreted as the number of seconds a
                    // Job is continuously active.)
                    job.status.start_time = Some(now());
                }
            }
        }
    }

    debug!(
        suspend_cond_changed,
        active, job.status.active, ready, job.status.ready, "calculating needs_status_update"
    );
    let needs_status_update = suspend_cond_changed
        || active as u32 != job.status.active
        || ready as u32 != job.status.ready;
    job.status.active = active as u32;
    job.status.ready = ready as u32;

    track_job_status_and_remove_finalizers(
        needs_status_update,
        job,
        pods,
        &expected_rm_finalizers,
        succeeded_indexes,
        prev_succeeded_indexes,
        finished_condition,
    )
}

// getNewFinishedPods returns the list of newly succeeded and failed pods that are not accounted
// in the job status. The list of failed pods can be affected by the podFailurePolicy.
fn get_new_finished_pods<'a>(
    job: &Job,
    pods: &[&'a Pod],
    expected_rm_finalizers: &[String],
) -> (Vec<&'a Pod>, Vec<&'a Pod>) {
    let succeeded_pods = get_valid_pods_with_filter(
        job,
        pods,
        &job.status.uncounted_terminated_pods.succeeded,
        expected_rm_finalizers,
        |p| p.status.phase == PodPhase::Succeeded,
    );
    let failed_pods = get_valid_pods_with_filter(
        job,
        pods,
        &job.status.uncounted_terminated_pods.failed,
        expected_rm_finalizers,
        |p| is_pod_failed(p, job),
    );
    (succeeded_pods, failed_pods)
}

fn get_valid_pods_with_filter<'a>(
    job: &Job,
    pods: &[&'a Pod],
    uncounted_uids: &[String],
    expected_rm_finalizers: &[String],
    f: impl Fn(&Pod) -> bool,
) -> Vec<&'a Pod> {
    pods.iter()
        .filter(|&&p| {
            // Pods that don't have a completion finalizer are in the uncounted set or
            // have already been accounted for in the Job status.
            if !has_job_tracking_finalizer(p)
                || uncounted_uids.contains(&p.metadata.uid)
                || expected_rm_finalizers.contains(&p.metadata.uid)
            {
                return false;
            }

            if job.spec.completion_mode == JobCompletionMode::Indexed {
                let index = get_completion_index(&p.metadata.annotations);
                if index.map_or(true, |i| i >= job.spec.completions.unwrap_or_default()) {
                    return false;
                }
            }

            f(p)
        })
        .copied()
        .collect()
}

fn has_job_tracking_finalizer(pod: &Pod) -> bool {
    pod.metadata
        .finalizers
        .iter()
        .any(|f| f == JOB_TRACKING_FINALIZER)
}

fn get_completion_index(annotations: &BTreeMap<String, String>) -> Option<u32> {
    annotations
        .get(JOB_COMPLETION_INDEX_ANNOTATION)
        .and_then(|v| v.parse().ok())
}

fn is_pod_failed(pod: &Pod, job: &Job) -> bool {
    if job.spec.pod_failure_policy.is_some() {
        pod.status.phase == PodPhase::Failed
    } else if pod.status.phase == PodPhase::Failed {
        true
    } else if only_replace_failed_pods(job) {
        pod.status.phase == PodPhase::Failed
    } else {
        // Count deleted Pods as failures to account for orphan Pods that
        // never have a chance to reach the Failed phase.
        pod.metadata.deletion_timestamp.is_some() && pod.status.phase != PodPhase::Succeeded
    }
}

fn only_replace_failed_pods(job: &Job) -> bool {
    job.spec.pod_failure_policy.is_some()
}

fn non_ignored_failed_pods_count(job: &Job, failed_pods: &[&Pod]) -> usize {
    let mut result = failed_pods.len();
    if let Some(pfp) = &job.spec.pod_failure_policy {
        for p in failed_pods {
            let (_, count_failed, _) = match_pod_failure_policy(pfp, p);
            if !count_failed {
                result -= 1
            }
        }
    }
    result
}

// matchPodFailurePolicy returns information about matching a given failed pod
// against the pod failure policy rules. The information is represented as an
//   - optional job failure message (present in case the pod matched a 'FailJob' rule),
//   - a boolean indicating if the failure should be counted towards backoffLimit
//     (and backoffLimitPerIndex if specified). It should not be counted
//     if the pod matched an 'Ignore' rule,
//   - a pointer to the matched pod failure policy action.
fn match_pod_failure_policy(
    pfp: &JobPodFailurePolicy,
    pod: &Pod,
) -> (Option<String>, bool, Option<JobPodFailurePolicyRuleAction>) {
    for (index, rule) in pfp.rules.iter().enumerate() {
        if let Some(on_exit_codes) = &rule.on_exit_codes {
            if let Some(container_status) = match_on_exit_codes(&pod.status, on_exit_codes) {
                match rule.action {
                    JobPodFailurePolicyRuleAction::Ignore => {
                        return (None, false, Some(rule.action))
                    }
                    JobPodFailurePolicyRuleAction::FailIndex => {}
                    JobPodFailurePolicyRuleAction::Count => return (None, true, Some(rule.action)),
                    JobPodFailurePolicyRuleAction::FailJob => {
                        let msg = format!("Container {} for pod {}/{} failed with exit code {} matching {:?} rule at index {}", container_status.name, pod.metadata.namespace, pod.metadata.name, container_status.state.terminated.as_ref().unwrap().exit_code, rule.action, index);
                        return (Some(msg), true, Some(rule.action));
                    }
                }
            }
        } else if let Some(on_pod_conditions) = &rule.on_pod_conditions {
            if let Some(pod_condition) = match_on_pod_conditions(&pod.status, on_pod_conditions) {
                match rule.action {
                    JobPodFailurePolicyRuleAction::Ignore => {
                        return (None, false, Some(rule.action))
                    }
                    JobPodFailurePolicyRuleAction::FailIndex => {}
                    JobPodFailurePolicyRuleAction::Count => return (None, true, Some(rule.action)),
                    JobPodFailurePolicyRuleAction::FailJob => {
                        let msg = format!(
                            "Pod {}/{} has condition {:?} matching {:?} rule at index {}",
                            pod.metadata.namespace,
                            pod.metadata.name,
                            pod_condition.r#type,
                            rule.action,
                            index
                        );
                        return (Some(msg), true, Some(rule.action));
                    }
                }
            }
        }
    }
    (None, true, None)
}

fn match_on_exit_codes<'a>(
    pod_status: &'a PodStatus,
    requirement: &JobPodFailurePolicyRuleOnExitCodesRequirement,
) -> Option<&'a ContainerStatus> {
    if let Some(cs) = get_matching_container_from_list(&pod_status.container_statuses, requirement)
    {
        return Some(cs);
    }
    get_matching_container_from_list(&pod_status.init_container_statuses, requirement)
}

fn match_on_pod_conditions<'a>(
    pod_status: &'a PodStatus,
    requirement: &[JobPodFailurePolicyRuleOnPodConditionsPattern],
) -> Option<&'a PodCondition> {
    for pc in &pod_status.conditions {
        for pattern in requirement {
            if pc.r#type == pattern.r#type && pc.status == pattern.status {
                return Some(pc);
            }
        }
    }
    None
}

fn get_matching_container_from_list<'a>(
    css: &'a [ContainerStatus],
    requirement: &JobPodFailurePolicyRuleOnExitCodesRequirement,
) -> Option<&'a ContainerStatus> {
    css.iter().find(|cs| {
        if cs.state.terminated.is_none() {
            // This container is still be terminating. There is no exit code to match.
            return false;
        }
        if requirement
            .container_name
            .as_ref()
            .map_or(true, |cn| cn == &cs.name)
            && cs.state.terminated.as_ref().unwrap().exit_code != 0
            && is_on_exit_codes_operator_matching(
                cs.state.terminated.as_ref().unwrap().exit_code,
                requirement,
            )
        {
            return true;
        }
        false
    })
}

fn is_on_exit_codes_operator_matching(
    exit_code: u32,
    requirement: &JobPodFailurePolicyRuleOnExitCodesRequirement,
) -> bool {
    match requirement.operator {
        JobPodFailurePolicyRuleOnExitCodesRequirementOperator::In => {
            requirement.values.iter().any(|v| *v == exit_code)
        }
        JobPodFailurePolicyRuleOnExitCodesRequirementOperator::NotIn => {
            requirement.values.iter().all(|v| *v != exit_code)
        }
    }
}

fn count_ready_pods(pods: &[&Pod]) -> usize {
    pods.iter().filter(|p| is_pod_ready(p)).count()
}

fn find_condition_by_type(
    conditions: &[JobCondition],
    cond_type: JobConditionType,
) -> Option<&JobCondition> {
    conditions.iter().find(|c| c.r#type == cond_type)
}

fn find_condition_by_type_mut(
    conditions: &mut [JobCondition],
    cond_type: JobConditionType,
) -> Option<&mut JobCondition> {
    conditions.iter_mut().find(|c| c.r#type == cond_type)
}

fn new_condition(
    condition_type: JobConditionType,
    status: ConditionStatus,
    reason: String,
    message: String,
    now: Time,
) -> JobCondition {
    JobCondition {
        status,
        r#type: condition_type,
        last_probe_time: Some(now),
        last_transition_time: Some(now),
        message,
        reason,
    }
}

fn get_fail_job_message(job: &Job, pods: &[&Pod]) -> Option<String> {
    for p in pods {
        if is_pod_failed(p, job) {
            if let Some(pfp) = &job.spec.pod_failure_policy {
                let (job_failure_message, _, _) = match_pod_failure_policy(pfp, p);
                if let Some(m) = job_failure_message {
                    return Some(m);
                }
            }
        }
    }
    None
}

// pastBackoffLimitOnFailure checks if container restartCounts sum exceeds BackoffLimit
// this method applies only to pods with restartPolicy == OnFailure
fn past_backoff_limit_on_failure(job: &Job, pods: &[&Pod]) -> bool {
    if job
        .spec
        .template
        .spec
        .restart_policy
        .map_or(true, |rp| rp != PodRestartPolicy::OnFailure)
    {
        return false;
    }

    let mut result = 0;
    for pod in pods {
        if pod.status.phase == PodPhase::Running || pod.status.phase == PodPhase::Pending {
            for stat in &pod.status.init_container_statuses {
                result += stat.restart_count
            }
            for stat in &pod.status.container_statuses {
                result += stat.restart_count
            }
        }
    }
    if job.spec.backoff_limit.map_or(false, |bl| bl == 0) {
        return result > 0;
    }
    result >= job.spec.backoff_limit.unwrap()
}

// pastActiveDeadline checks if job has ActiveDeadlineSeconds field set and if
// it is exceeded. If the job is currently suspended, the function will always
// return false.
fn past_active_deadline(job: &Job) -> bool {
    if job.spec.active_deadline_seconds.is_none()
        || job.status.start_time.is_none()
        || job.spec.suspend
    {
        return false;
    }
    let duration = job.status.start_time.unwrap().0 - now().0;
    let allowed_duration =
        Duration::from_secs(job.spec.active_deadline_seconds.unwrap_or_default());
    duration >= allowed_duration
}

// calculateSucceededIndexes returns the old and new list of succeeded indexes
// in compressed format (intervals).
// The old list is solely based off .status.completedIndexes, but returns an
// empty list if this Job is not tracked with finalizers. The new list includes
// the indexes that succeeded since the last sync.
fn calculate_succeeded_indexes(job: &Job, pods: &[&Pod]) -> (OrderedIntervals, OrderedIntervals) {
    let prev_intervals = OrderedIntervals::parse_indexes_from_string(
        &job.status.completed_indexes,
        job.spec.completions.unwrap_or_default(),
    );
    let mut new_succeeded = BTreeSet::new();
    for pod in pods {
        if let Some(index) = get_completion_index(&pod.metadata.annotations) {
            // Succeeded Pod with valid index and, if tracking with finalizers,
            // has a finalizer (meaning that it is not counted yet).
            if pod.status.phase == PodPhase::Succeeded
                && index < job.spec.completions.unwrap()
                && has_job_tracking_finalizer(pod)
            {
                new_succeeded.insert(index);
            }
        }
    }

    // List returns the items of the set in order.
    let result = with_ordered_indexes(&prev_intervals, new_succeeded.into_iter().collect());
    (prev_intervals, result)
}

fn with_ordered_indexes(oi: &OrderedIntervals, new_indexes: Vec<u32>) -> OrderedIntervals {
    debug!(original=?oi, new=?new_indexes, "with_ordered_indexes");
    let mut new_index_intervals = OrderedIntervals::default();
    for new_index in new_indexes {
        new_index_intervals.0.push(Interval {
            first: new_index,
            last: new_index,
        });
    }
    oi.merge(&new_index_intervals)
}

// deleteActivePods issues deletion for active Pods, preserving finalizers.
// This is done through DELETE calls that set deletion timestamps.
// The method trackJobStatusAndRemoveFinalizers removes the finalizers, after
// which the objects can actually be deleted.
// Returns number of successfully deletions issued.
fn delete_active_pods(pods: &[&Pod]) -> OptionalJobControllerAction {
    pods.first()
        .map(|p| JobControllerAction::DeletePod((*p).clone()))
        .into()
}

// ensureJobConditionStatus appends or updates an existing job condition of the
// given type with the given status value. Note that this function will not
// append to the conditions list if the new condition's status is false
// (because going from nothing to false is meaningless); it can, however,
// update the status condition to false. The function returns a bool to let the
// caller know if the list was changed (either appended or updated).
fn ensure_job_condition_status(
    conditions: &[JobCondition],
    cond_type: JobConditionType,
    status: ConditionStatus,
    reason: String,
    message: String,
    now: Time,
) -> Option<Vec<JobCondition>> {
    let mut conditions = conditions.to_vec();
    if let Some(c) = find_condition_by_type_mut(&mut conditions, cond_type) {
        if c.status != status || c.reason != reason || c.message != message {
            *c = new_condition(cond_type, status, reason, message, now);
            Some(conditions)
        } else {
            None
        }
    } else {
        // A condition with that type doesn't exist in the list.
        if status != ConditionStatus::False {
            conditions.push(new_condition(cond_type, status, reason, message, now));
            Some(conditions)
        } else {
            None
        }
    }
}

// trackJobStatusAndRemoveFinalizers does:
//  1. Add finished Pods to .status.uncountedTerminatedPods
//  2. Remove the finalizers from the Pods if they completed or were removed
//     or the job was removed.
//  3. Increment job counters for pods that no longer have a finalizer.
//  4. Add Complete condition if satisfied with current counters.
//
// It does this up to a limited number of Pods so that the size of .status
// doesn't grow too much and this sync doesn't starve other Jobs.
fn track_job_status_and_remove_finalizers(
    mut needs_flush: bool,
    job: &mut Job,
    pods: &mut [&Pod],
    expected_rm_finalizers: &[String],
    mut succeeded_indexes: OrderedIntervals,
    prev_succeeded_indexes: OrderedIntervals,
    mut finished_condition: Option<JobCondition>,
) -> OptionalJobControllerAction {
    let is_indexed = job.spec.completion_mode == JobCompletionMode::Indexed;

    let mut pods_to_remove_finalizer = Vec::new();
    let mut new_succeeded_indexes = Vec::new();
    if is_indexed {
        // Sort to introduce completed Indexes in order.
        pods.sort_by_key(|p| get_completion_index(&p.metadata.annotations));
    }
    let mut uids_with_finalizer = Vec::new();
    for p in &*pods {
        if has_job_tracking_finalizer(p) && !expected_rm_finalizers.contains(&p.metadata.uid) {
            uids_with_finalizer.push(p.metadata.uid.as_str())
        }
    }

    if clean_uncounted_pods_without_finalizers(&mut job.status, &uids_with_finalizer) {
        needs_flush = true;
    }

    let mut reached_max_uncounted_pods = false;
    for pod in pods {
        debug!(pod = pod.metadata.name, "Processing pod");
        if !has_job_tracking_finalizer(pod) || expected_rm_finalizers.contains(&pod.metadata.uid) {
            // This pod was processed in a previous sync.
            continue;
        }
        let consider_pod_failed = is_pod_failed(pod, job);
        if !can_remove_finalizer(job, pod, consider_pod_failed, &finished_condition) {
            continue;
        }

        pods_to_remove_finalizer.push(*pod);

        if pod.status.phase == PodPhase::Succeeded
            && !job
                .status
                .uncounted_terminated_pods
                .failed
                .contains(&pod.metadata.uid)
        {
            if is_indexed {
                // The completion index is enough to avoid recounting succeeded pods.
                // No need to track UIDs.
                let ix = get_completion_index(&pod.metadata.annotations);
                if ix.map_or(false, |i| i < job.spec.completions.unwrap_or_default())
                    && !prev_succeeded_indexes.has(ix.unwrap())
                {
                    new_succeeded_indexes.push(ix.unwrap());
                }
            } else if !job
                .status
                .uncounted_terminated_pods
                .succeeded
                .contains(&pod.metadata.uid)
            {
                debug!("needs flush succeeded changed");
                needs_flush = true;
                job.status
                    .uncounted_terminated_pods
                    .succeeded
                    .push(pod.metadata.uid.clone());
            }
        } else if consider_pod_failed || finished_condition.is_some() {
            // When the job is considered finished, every non-terminated pod is considered failed
            let ix = get_completion_index(&pod.metadata.annotations);
            if !job
                .status
                .uncounted_terminated_pods
                .failed
                .contains(&pod.metadata.uid)
                && (!is_indexed
                    || (ix.map_or(false, |i| i < job.spec.completions.unwrap_or_default())))
            {
                if let Some(pfp) = &job.spec.pod_failure_policy {
                    let (_, count_failed, _) = match_pod_failure_policy(pfp, pod);
                    if count_failed {
                        debug!("needs flush count failed");
                        needs_flush = true;
                        job.status
                            .uncounted_terminated_pods
                            .failed
                            .push(pod.metadata.uid.clone());
                    }
                } else {
                    debug!("needs flush failed pods");
                    needs_flush = true;
                    job.status
                        .uncounted_terminated_pods
                        .failed
                        .push(pod.metadata.uid.clone());
                }
            }
        }

        if new_succeeded_indexes.len()
            + job.status.uncounted_terminated_pods.succeeded.len()
            + job.status.uncounted_terminated_pods.failed.len()
            >= MAX_UNCOUNTED_PODS as usize
        {
            // The controller added enough Pods already to .status.uncountedTerminatedPods
            // We stop counting pods and removing finalizers here to:
            // 1. Ensure that the UIDs representation are under 20 KB.
            // 2. Cap the number of finalizer removals so that syncing of big Jobs
            //    doesn't starve smaller ones.
            //
            // The job will be synced again because the Job status and Pod updates
            // will put the Job back to the work queue.
            reached_max_uncounted_pods = true;
            break;
        }
    }

    if is_indexed {
        succeeded_indexes = with_ordered_indexes(&succeeded_indexes, new_succeeded_indexes);
        let succeeded_indexes_str = succeeded_indexes.to_string();
        if succeeded_indexes_str != job.status.completed_indexes {
            debug!("needs flush indexes differ");
            needs_flush = true;
        }
        job.status.succeeded = succeeded_indexes.total();
        job.status.completed_indexes = succeeded_indexes_str;
    }

    if finished_condition
        .as_ref()
        .map_or(false, |fc| fc.r#type == JobConditionType::FailureTarget)
    {
        // Append the interim FailureTarget condition to update the job status with before finalizers are removed.
        job.status
            .conditions
            .push(finished_condition.clone().unwrap());
        debug!("needs flush finished condition");
        needs_flush = true;
        // Prepare the final Failed condition to update the job status with after the finalizers are removed.
        // It is also used in the enactJobFinished function for reporting.
        finished_condition = Some(new_failed_condition_for_failure_target(
            &finished_condition.unwrap(),
            now(),
        ));
    }

    if let Some(op) = flush_uncounted_and_remove_finalizers(
        job,
        &pods_to_remove_finalizer,
        &uids_with_finalizer,
        needs_flush,
    )
    .0
    {
        return Some(op).into();
    }

    let job_finished =
        !reached_max_uncounted_pods && enact_job_finished(&mut job.status, finished_condition);
    if job_finished {
        debug!("needs flush job finished");
        needs_flush = true;
    }

    if needs_flush {
        debug!("Job status needed flush");
        Some(JobControllerAction::UpdateJobStatus(job.clone())).into()
    } else {
        None.into()
    }
}

// manageJob is the core method responsible for managing the number of running
// pods according to what is specified in the job.Spec.
// Respects back-off; does not create new pods if the back-off time has not passed
// Does NOT modify <activePods>.
fn manage_job(
    job: &Job,
    pods: &[&Pod],
    active_pods: &[&Pod],
    succeeded: usize,
    succeeded_indexes: &OrderedIntervals,
) -> OptionalJobControllerAction {
    let active = active_pods.len();
    let parallelism = job.spec.parallelism.unwrap_or_default() as usize;

    if job.spec.suspend {
        debug!("Deleting all active pods in suspended job");
        let pods_to_delete = active_pods_for_removal(job, active_pods, active);
        return delete_job_pods(&pods_to_delete);
    }

    let mut terminating = 0;
    if only_replace_failed_pods(job) {
        // For PodFailurePolicy specified but PodReplacementPolicy disabled
        // we still need to count terminating pods for replica counts
        // But we will not allow updates to status.
        terminating = count_terminating_pods(pods);
    }

    let mut want_active;
    if let Some(completions) = job.spec.completions {
        // Job specifies a specific number of completions.  Therefore, number
        // active should not ever exceed number of remaining completions.
        want_active = (completions as usize).saturating_sub(succeeded);
        if want_active > parallelism {
            want_active = parallelism;
        }
    } else {
        // Job does not specify a number of completions.  Therefore, number active
        // should be equal to parallelism, unless the job has seen at least
        // once success, in which leave whatever is running, running.
        if succeeded > 0 {
            want_active = active
        } else {
            want_active = parallelism
        }
    }

    let rm_at_least = (active + terminating).saturating_sub(want_active);

    let mut pods_to_delete = active_pods_for_removal(job, active_pods, rm_at_least);
    if pods_to_delete.len() > MAX_POD_CREATE_DELETE_PER_SYNC {
        pods_to_delete = pods_to_delete[..MAX_POD_CREATE_DELETE_PER_SYNC].to_vec();
    }

    if !pods_to_delete.is_empty() {
        debug!(
            job = job.metadata.name,
            deleted = pods_to_delete.len(),
            active = active_pods.len(),
            target = want_active,
            "Too many pods running for job"
        );
        return delete_job_pods(&pods_to_delete);
    }

    let mut diff = want_active
        .saturating_sub(terminating)
        .saturating_sub(active);
    if diff > 0 {
        if diff > MAX_POD_CREATE_DELETE_PER_SYNC {
            diff = MAX_POD_CREATE_DELETE_PER_SYNC
        }

        let mut indexes_to_add = Vec::new();
        if job.spec.completion_mode == JobCompletionMode::Indexed {
            indexes_to_add = first_pending_indexes(
                diff,
                job.spec.completions.unwrap(),
                pods,
                active_pods,
                job,
                succeeded_indexes,
            );
            diff = indexes_to_add.len();
        }

        debug!(
            job = job.metadata.name,
            need = want_active,
            creating = diff,
            "Too few pods running"
        );

        let mut pod_template = job.spec.template.clone();
        if job.spec.completion_mode == JobCompletionMode::Indexed {
            add_completion_index_env_variables(&mut pod_template);
        }

        append_job_completion_finalizer_if_not_found(&mut pod_template.metadata.finalizers);
        let mut completion_index = None;
        if !indexes_to_add.is_empty() {
            completion_index = indexes_to_add.first().copied();
            indexes_to_add.remove(0);
        }

        let generate_name = if let Some(completion_index) = completion_index {
            add_completion_index_annotation(&mut pod_template, completion_index);
            pod_template.spec.hostname = format!("{}-{}", job.metadata.name, completion_index);
            pod_generate_name_with_index(job.metadata.name.clone(), completion_index)
        } else {
            String::new()
        };

        return Some(create_pod_with_generate_name(
            job,
            pod_template,
            generate_name,
        ))
        .into();
    }

    None.into()
}

fn active_pods_for_removal<'a>(job: &Job, pods: &[&'a Pod], rm_at_least: usize) -> Vec<&'a Pod> {
    let mut rm = Vec::new();
    let mut left = Vec::new();
    if job.spec.completion_mode == JobCompletionMode::Indexed {
        append_duplicated_index_pods_for_removal(
            &mut rm,
            &mut left,
            pods,
            job.spec.completions.unwrap(),
        );
    } else {
        left = pods.to_vec();
    }

    if rm.len() < rm_at_least {
        // sort left by active
        left.truncate(rm_at_least - rm.len());
        rm.append(&mut left);
    }
    rm
}

// appendDuplicatedIndexPodsForRemoval scans active `pods` for duplicated
// completion indexes. For each index, it selects n-1 pods for removal, where n
// is the number of repetitions. The pods to be removed are appended to `rm`,
// while the remaining pods are appended to `left`.
// All pods that don't have a completion index are appended to `rm`.
// All pods with index not in valid range are appended to `rm`.
fn append_duplicated_index_pods_for_removal<'a>(
    rm: &mut Vec<&'a Pod>,
    left: &mut Vec<&'a Pod>,
    pods: &[&'a Pod],
    completions: u32,
) {
    let mut pods = pods.to_vec();

    pods.sort_by_key(|p| get_completion_index(&p.metadata.annotations));

    let mut last_index = None;
    let mut first_repeat_pos = 0;
    let mut count_looped = 0;

    for i in 0..pods.len() {
        let p = &pods[i];
        let ix = get_completion_index(&p.metadata.annotations);
        if ix.map_or(false, |i| i >= completions) {
            rm.extend(pods.iter().skip(i));
            break;
        }
        if ix != last_index {
            append_pods_with_same_index_for_removal_and_remaining(
                rm,
                left,
                &mut pods[first_repeat_pos..i],
                last_index,
            );
            first_repeat_pos = i;
            last_index = ix;
        }
        count_looped += 1;
    }
    append_pods_with_same_index_for_removal_and_remaining(
        rm,
        left,
        &mut pods[first_repeat_pos..count_looped],
        last_index,
    )
}

fn append_pods_with_same_index_for_removal_and_remaining<'a>(
    rm: &mut Vec<&'a Pod>,
    left: &mut Vec<&'a Pod>,
    pods: &mut [&'a Pod],
    ix: Option<u32>,
) {
    if ix.is_none() {
        rm.extend(pods.iter().copied());
        return;
    }
    if pods.len() == 1 {
        left.push(pods[0]);
        return;
    }

    sort_active_pods(pods);

    rm.extend(&pods[..pods.len() - 1]);
    left.push(pods[pods.len() - 1]);
}

fn sort_active_pods(pods: &mut [&Pod]) {
    pods.sort_by(|p1, p2| {
        // 1. Unassigned < assigned
        // If only one of the pods is unassigned, the unassigned one is smaller
        if p1.spec.node_name != p2.spec.node_name
            && (p1.spec.node_name.as_ref().map_or(0, |n| n.len()) == 0
                || p2.spec.node_name.as_ref().map_or(0, |n| n.len()) == 0)
        {
            return if p1.spec.node_name.as_ref().map_or(0, |n| n.len()) == 0 {
                Ordering::Less
            } else {
                Ordering::Equal
            };
        }

        // 2. PodPending < PodUnknown < PodRunning
        if p1.status.phase as u8 != p2.status.phase as u8 {
            return (p1.status.phase as u8).cmp(&(p2.status.phase as u8));
        }

        // 3. Not ready < ready
        // If only one of the pods is not ready, the not ready one is smaller
        if is_pod_ready(p1) != is_pod_ready(p2) {
            return if is_pod_ready(p1) {
                Ordering::Equal
            } else {
                Ordering::Less
            };
        }

        // TODO: take availability into account when we push minReadySeconds information from deployment into pods,
        //       see https://github.com/kubernetes/kubernetes/issues/22065
        // 4. Been ready for empty time < less time < more time
        // If both pods are ready, the latest ready one is smaller
        if is_pod_ready(p1) && is_pod_ready(p2) {
            let ready_time_1 = pod_ready_time(p1);
            let ready_time_2 = pod_ready_time(p2);
            if ready_time_1 != ready_time_2 {
                return after_or_zero(&ready_time_1, &ready_time_2);
            }
        }

        // 5. Pods with containers with higher restart counts < lower restart counts
        if max_container_restarts(p1) != max_container_restarts(p2) {
            return max_container_restarts(p1)
                .cmp(&max_container_restarts(p2))
                .reverse();
        }

        // 6. Empty creation time pods < newer pods < older pods
        if p1.metadata.creation_timestamp != p2.metadata.creation_timestamp {
            return after_or_zero(
                &p1.metadata.creation_timestamp,
                &p2.metadata.creation_timestamp,
            );
        }

        Ordering::Equal
    });
}

fn after_or_zero(t1: &Option<Time>, t2: &Option<Time>) -> Ordering {
    if t1.is_none() || t2.is_none() {
        if t1.is_none() {
            Ordering::Less
        } else {
            Ordering::Equal
        }
    } else {
        t1.cmp(t2).reverse()
    }
}

fn pod_ready_time(pod: &Pod) -> Option<Time> {
    if is_pod_ready(pod) {
        pod.status.conditions.iter().find_map(|c| {
            if c.r#type == PodConditionType::Ready && c.status == ConditionStatus::True {
                c.last_transition_time
            } else {
                None
            }
        })
    } else {
        None
    }
}

fn max_container_restarts(pod: &Pod) -> u32 {
    pod.status
        .container_statuses
        .iter()
        .map(|cs| cs.restart_count)
        .max()
        .unwrap_or_default()
}

fn delete_job_pods(pods: &[&Pod]) -> OptionalJobControllerAction {
    if let Some(pod) = pods.first() {
        if let Some(op) = remove_tracking_finalizer_patch(pod).0 {
            return Some(op).into();
        }
        debug!(pod = pod.metadata.name, "Deleting pod");
        Some(JobControllerAction::DeletePod((*pod).clone())).into()
    } else {
        None.into()
    }
}

fn remove_tracking_finalizer_patch(pod: &Pod) -> OptionalJobControllerAction {
    debug!(
        pod = pod.metadata.name,
        "Trying to remove tracking finalizer from pod"
    );
    if !has_job_tracking_finalizer(pod) {
        return None.into();
    }

    let mut pod = pod.clone();
    pod.metadata
        .finalizers
        .retain(|f| f != JOB_TRACKING_FINALIZER);

    // TODO: this should be a patch operation
    debug!(pod = pod.metadata.name, "Removing tracking finalizer");
    Some(JobControllerAction::UpdatePod(pod)).into()
}

fn create_pod_with_generate_name(
    job: &Job,
    template: PodTemplateSpec,
    generate_name: String,
) -> JobControllerAction {
    let mut pod = get_pod_from_template(&job.metadata, &template, &Job::GVK);
    if !generate_name.is_empty() {
        pod.metadata.generate_name = generate_name;
    }
    debug!("Creating pod");
    JobControllerAction::CreatePod(pod)
}

fn pod_generate_name_with_index(job_name: String, index: u32) -> String {
    const MAX_GENERATED_NAME_LENGTH: usize = 58;
    let append_index = format!("-{}-", index);
    let mut generate_name_prefix = job_name + &append_index;
    if generate_name_prefix.len() > MAX_GENERATED_NAME_LENGTH {
        generate_name_prefix =
            generate_name_prefix[..MAX_GENERATED_NAME_LENGTH - append_index.len()].to_string()
                + &append_index;
    }
    generate_name_prefix
}

fn add_completion_index_annotation(template: &mut PodTemplateSpec, index: u32) {
    template.metadata.annotations.insert(
        JOB_COMPLETION_INDEX_ANNOTATION.to_owned(),
        index.to_string(),
    );
}

fn append_job_completion_finalizer_if_not_found(finalizers: &mut Vec<String>) {
    if !finalizers.iter().any(|f| f == JOB_TRACKING_FINALIZER) {
        finalizers.push(JOB_TRACKING_FINALIZER.to_owned());
    }
}

fn add_completion_index_env_variables(template: &mut PodTemplateSpec) {
    for c in &mut template.spec.init_containers {
        add_completion_index_env_variable(c)
    }
    for c in &mut template.spec.containers {
        add_completion_index_env_variable(c)
    }
}

fn add_completion_index_env_variable(container: &mut Container) {
    if container
        .env
        .iter()
        .any(|e| e.name == JOB_COMPLETION_INDEX_ENV_NAME)
    {
        return;
    }
    let field_path = format!("metadata.labels['{}']", JOB_COMPLETION_INDEX_ANNOTATION);
    container.env.push(EnvVar {
        name: JOB_COMPLETION_INDEX_ENV_NAME.to_owned(),
        value: None,
        value_from: Some(EnvVarSource {
            field_ref: Some(ObjectFieldSelector {
                field_path,
                api_version: None,
            }),
        }),
    })
}

fn first_pending_indexes(
    count: usize,
    completions: u32,
    pods: &[&Pod],
    active_pods: &[&Pod],
    job: &Job,
    succeeded_indexes: &OrderedIntervals,
) -> Vec<u32> {
    if count == 0 {
        return Vec::new();
    }
    debug!(
        active = active_pods.len(),
        "finding first pod index to create"
    );

    let active = get_indexes(active_pods);

    println!("active {:?}", active);
    let mut non_pending = with_ordered_indexes(succeeded_indexes, active);
    println!("non_pending {:?}", non_pending);

    if only_replace_failed_pods(job) {
        let terminating = get_indexes(&filter_terminating_pods(pods));
        non_pending = with_ordered_indexes(&non_pending, terminating);
    }

    let mut result = Vec::new();
    // The following algorithm is bounded by len(nonPending) and count.
    let mut candidate = 0;
    for s_interval in non_pending.0 {
        while candidate < completions && result.len() < count && candidate < s_interval.first {
            result.push(candidate);
            candidate += 1;
        }
        if candidate < s_interval.last + 1 {
            candidate = s_interval.last + 1;
        }
    }
    while candidate < completions && result.len() < count {
        result.push(candidate);
        candidate += 1;
    }
    result
}

fn get_indexes(pods: &[&Pod]) -> Vec<u32> {
    pods.iter()
        .filter_map(|p| get_completion_index(&p.metadata.annotations))
        .collect()
}

fn count_terminating_pods(pods: &[&Pod]) -> usize {
    pods.iter().filter(|p| is_pod_terminating(p)).count()
}

fn enact_job_finished(
    job_status: &mut JobStatus,
    finished_condition: Option<JobCondition>,
) -> bool {
    if let Some(fc) = finished_condition {
        let uncounted = &job_status.uncounted_terminated_pods;
        if !uncounted.succeeded.is_empty() || !uncounted.failed.is_empty() {
            return false;
        }

        let conditions = ensure_job_condition_status(
            &job_status.conditions,
            fc.r#type,
            fc.status,
            fc.reason,
            fc.message,
            now(),
        );
        job_status.conditions = conditions.unwrap_or_default();
        if fc.r#type == JobConditionType::Complete {
            job_status.completion_time = fc.last_transition_time;
        }
        true
    } else {
        false
    }
}

fn flush_uncounted_and_remove_finalizers(
    job: &mut Job,
    pods_to_remove_finalizer: &[&Pod],
    uids_with_finalizer: &[&str],
    needs_flush: bool,
) -> OptionalJobControllerAction {
    if needs_flush {
        debug!("updating job status as needs flush in flush_uncounted_and_remove_finalizers");
        return Some(JobControllerAction::UpdateJobStatus(job.clone())).into();
    }

    if !pods_to_remove_finalizer.is_empty() {
        debug!("Had some pods to remove finalizer from");
        if let Some(op) = remove_tracking_finalizer_from_pods(pods_to_remove_finalizer).0 {
            return Some(op).into();
        }
    }

    if clean_uncounted_pods_without_finalizers(&mut job.status, uids_with_finalizer) {
        debug!("Cleaned uncounted pods without finalizers");
        return Some(JobControllerAction::UpdateJobStatus(job.clone())).into();
    }

    None.into()
}

// cleanUncountedPodsWithoutFinalizers removes the Pod UIDs from
// .status.uncountedTerminatedPods for which the finalizer was successfully
// removed and increments the corresponding status counters.
// Returns whether there was any status change.
fn clean_uncounted_pods_without_finalizers(
    status: &mut JobStatus,
    uids_with_finalizer: &[&str],
) -> bool {
    let mut updated = false;
    let uncounted_status = &mut status.uncounted_terminated_pods;
    let new_uncounted = filter_in_uncounted_uids(&uncounted_status.succeeded, uids_with_finalizer);
    if new_uncounted.len() != uncounted_status.succeeded.len() {
        updated = true;
        status.succeeded += (uncounted_status.succeeded.len() - new_uncounted.len()) as u32;
        uncounted_status.succeeded = new_uncounted;
    }

    let new_uncounted = filter_in_uncounted_uids(&uncounted_status.failed, uids_with_finalizer);
    if new_uncounted.len() != uncounted_status.failed.len() {
        updated = true;
        status.failed += (uncounted_status.failed.len() - new_uncounted.len()) as u32;
        uncounted_status.failed = new_uncounted;
    }
    debug!(updated, "Cleaned uncounted pods without finalizers");
    updated
}

fn filter_in_uncounted_uids(uncounted: &[String], include: &[&str]) -> Vec<String> {
    uncounted
        .iter()
        .filter(|u| include.contains(&u.as_str()))
        .cloned()
        .collect()
}

// removeTrackingFinalizerFromPods removes tracking finalizers from Pods and
// returns an array of booleans where the i-th value is true if the finalizer
// of the i-th Pod was successfully removed (if the pod was deleted when this
// function was called, it's considered as the finalizer was removed successfully).
fn remove_tracking_finalizer_from_pods(pods: &[&Pod]) -> OptionalJobControllerAction {
    for pod in pods {
        if let Some(op) = remove_tracking_finalizer_patch(pod).0 {
            return Some(op).into();
        }
    }
    None.into()
}

fn new_failed_condition_for_failure_target(condition: &JobCondition, now: Time) -> JobCondition {
    new_condition(
        JobConditionType::Failed,
        ConditionStatus::True,
        condition.reason.clone(),
        condition.message.clone(),
        now,
    )
}

#[derive(Debug, Clone, Copy)]
struct Interval {
    pub first: u32,
    pub last: u32,
}

#[derive(Debug, Default, Clone)]
struct OrderedIntervals(Vec<Interval>);

impl std::fmt::Display for OrderedIntervals {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = String::new();
        for v in &self.0 {
            if !s.is_empty() {
                s.push(',');
            }
            s.push_str(&v.first.to_string());
            if v.last > v.first {
                if v.last == v.first + 1 {
                    s.push(',');
                } else {
                    s.push('-');
                }
                s.push_str(&v.last.to_string());
            }
        }
        f.write_str(&s)
    }
}

impl OrderedIntervals {
    fn total(&self) -> u32 {
        self.0.iter().map(|i| i.last - i.first + 1).sum()
    }

    fn merge(&self, new_intervals: &OrderedIntervals) -> Self {
        let mut last_interval: Option<Interval> = None;
        let mut result = Self::default();
        let mut append_or_merge_with_last_interval = |i: Interval| {
            if last_interval.map_or(true, |li| i.first > li.last + 1) {
                result.0.push(i);
                last_interval = Some(i);
            } else if last_interval.unwrap().last < i.last {
                result.0.last_mut().unwrap().last = i.last;
                last_interval.as_mut().unwrap().last = i.last;
            }
        };

        let mut i = 0;
        let mut j = 0;

        while i < self.0.len() && j < new_intervals.0.len() {
            if self.0[i].first < new_intervals.0[j].first {
                append_or_merge_with_last_interval(self.0[i]);
                i += 1;
            } else {
                append_or_merge_with_last_interval(new_intervals.0[j]);
                j += 1;
            }
        }

        while i < self.0.len() {
            append_or_merge_with_last_interval(self.0[i]);
            i += 1;
        }
        while j < new_intervals.0.len() {
            append_or_merge_with_last_interval(new_intervals.0[j]);
            j += 1;
        }

        result
    }

    fn parse_indexes_from_string(indexes_str: &str, completions: u32) -> Self {
        let mut result = Self(Vec::new());

        if indexes_str.is_empty() {
            return result;
        }

        let mut last_interval: Option<Interval> = None;
        for interval_str in indexes_str.split(',') {
            let mut limits_str = interval_str.split('-');
            let first = limits_str.next().unwrap().parse().unwrap();
            if first >= completions {
                break;
            }

            let mut last;
            if let Some(last_s) = limits_str.next() {
                last = last_s.parse().unwrap();
                if last >= completions {
                    last = completions - 1;
                }
            } else {
                last = first;
            }
            if let Some(li) = &mut last_interval {
                if li.last == first - 1 {
                    li.last = last;
                    continue;
                }
            }
            let i = Interval { first, last };
            result.0.push(i);
            last_interval = Some(i);
        }
        result
    }

    fn has(&self, ix: u32) -> bool {
        self.0
            .binary_search_by(|i| {
                if ix <= i.first {
                    Ordering::Greater
                } else if ix >= i.last {
                    Ordering::Less
                } else {
                    Ordering::Equal
                }
            })
            .is_ok()
    }
}

// canRemoveFinalizer determines if the pod's finalizer can be safely removed.
// The finalizer can be removed when:
//   - the entire Job is terminating; or
//   - the pod's index is succeeded; or
//   - the Pod is considered failed, unless it's removal is delayed for the
//     purpose of transferring the JobIndexFailureCount annotations to the
//     replacement pod. the entire Job is terminating the finalizer can be
//     removed unconditionally.
fn can_remove_finalizer(
    job: &Job,
    pod: &Pod,
    consider_pod_failed: bool,
    finished_condition: &Option<JobCondition>,
) -> bool {
    if job.metadata.deletion_timestamp.is_some()
        || finished_condition.is_some()
        || pod.status.phase == PodPhase::Succeeded
    {
        return true;
    }

    if !consider_pod_failed {
        return false;
    }

    true
}

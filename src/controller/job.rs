use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

use crate::{
    abstract_model::ControllerAction,
    resources::Job,
    resources::{
        ConditionStatus, ContainerStatus, JobCompletionMode, JobCondition, JobConditionType,
        JobPodFailurePolicy, JobPodFailurePolicyRuleAction,
        JobPodFailurePolicyRuleOnExitCodesRequirement,
        JobPodFailurePolicyRuleOnExitCodesRequirementOperator,
        JobPodFailurePolicyRuleOnPodConditionsPattern, Pod, PodCondition, PodPhase,
        PodRestartPolicy, PodStatus, Time, UncountedTerminatedPods,
    },
    utils::now,
};

use super::{
    util::{self, is_pod_ready},
    Controller,
};

const JOB_COMPLETION_INDEX_ANNOTATION: &str = "batch.kubernetes.io/job-completion-index";
const JOB_TRACKING_FINALIZER: &str = "batch.kubernetes.io/job-tracking";
const JOB_NAME_LABEL: &str = "batch.kubernetes.io/job-name";
const CONTROLLER_UID_LABEL: &str = "batch.kubernetes.io/controller-uid";
const JOB_INDEX_FAILURE_COUNT_ANNOTATION: &str = "batch.kubernetes.io/job-index-failure-count";
const JOB_INDEX_IGNORED_FAILURE_COUNT_ANNOTATION: &str =
    "batch.kubernetes.io/job-index-ignored-failure-count";

const JOB_REASON_POD_FAILURE_POLICY: &str = "PodFailurePolicy";
const JOB_REASON_BACKOFF_LIMIT_EXCEEDED: &str = "BackoffLimitExceeded";
const JOB_REASON_DEADLINE_EXCEEDED: &str = "DeadlineExceeded";

#[derive(Clone, Debug)]
pub struct JobController;

#[derive(Debug, Default, Hash, Clone, PartialEq, Eq)]
pub struct JobControllerState;

#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub enum JobControllerAction {
    ControllerJoin(usize),

    UpdateJobStatus(Job),

    DeletePod(Pod),
}

impl From<JobControllerAction> for ControllerAction {
    fn from(value: JobControllerAction) -> Self {
        match value {
            JobControllerAction::ControllerJoin(id) => ControllerAction::ControllerJoin(id),
            JobControllerAction::UpdateJobStatus(j) => ControllerAction::UpdateJobStatus(j),
            JobControllerAction::DeletePod(pod) => ControllerAction::DeletePod(pod),
        }
    }
}

impl Controller for JobController {
    type State = JobControllerState;

    type Action = JobControllerAction;

    fn step(
        &self,
        id: usize,
        global_state: &crate::state::StateView,
        _local_state: &mut Self::State,
    ) -> Option<Self::Action> {
        if !global_state.controllers.contains(&id) {
            return Some(JobControllerAction::ControllerJoin(id));
        } else {
            for job in global_state.jobs.values() {
                let pods = global_state
                    .pods
                    .values()
                    .filter(|p| job.spec.selector.matches(&p.metadata.labels))
                    .collect::<Vec<_>>();
                if let Some(op) = reconcile(job, &pods) {
                    return Some(op);
                }
            }
        }
        None
    }

    fn name(&self) -> String {
        "Job".to_owned()
    }
}

fn reconcile(job: &Job, pods: &[&Pod]) -> Option<JobControllerAction> {
    let active_pods = util::filter_active_pods(pods);
    let active = active_pods.len();
    let uncounted = &job.status.uncounted_terminated_pods;
    let expected_rm_finalizers = Vec::new();
    let (new_succeeded_pods, new_failed_pods) = get_new_finished_pods(
        job,
        pods,
        &job.status.uncounted_terminated_pods,
        &expected_rm_finalizers,
    );
    let mut succeeded = job.status.succeeded.unwrap_or_default() as usize
        + new_succeeded_pods.len()
        + uncounted.succeeded.len();
    let failed = job.status.failed.unwrap_or_default() as usize
        + non_ignored_failed_pods_count(job, &new_failed_pods)
        + uncounted.failed.len();
    let ready = count_ready_pods(&active_pods);

    let mut new_status = job.status.clone();

    // Job first start. Set StartTime only if the job is not in the suspended state.
    if job.status.start_time.is_none() && !job.spec.suspend {
        new_status.start_time = Some(now());
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
        succeeded = succeeded_indexes.len();
        (prev_succeeded_indexes, succeeded_indexes)
    } else {
        (Vec::new(), Vec::new())
    };

    let mut suspend_cond_changed = false;
    // Remove active pods if Job failed.
    if finished_condition.is_some() {
        if let Some(delete_op) = delete_active_pods(job, &active_pods) {
            return Some(delete_op);
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
            manage_job(job);
            manage_job_called = true;
        }
        let mut complete = false;
        if job.spec.completions.is_none() {
            // This type of job is complete when any pod exits with success.
            // Each pod is capable of
            // determining whether or not the entire Job is done.  Subsequent pods are
            // not expected to fail, but if they do, the failure is ignored.  Once any
            // pod succeeds, the controller waits for remaining pods to finish, and
            // then the job is complete.
            complete = succeeded > 0 && active == 0;
        } else {
            // Job specifies a number of completions.  This type of job signals
            // success by having that number of successes.  Since we do not
            // start more pods than there are remaining completions, there should
            // not be any remaining active pods once this count is reached.
            complete = succeeded as u32 >= job.spec.completions.unwrap() && active == 0;
        }

        if complete {
            finished_condition = Some(new_condition(
                JobConditionType::Complete,
                ConditionStatus::True,
                String::new(),
                String::new(),
                now(),
            ));
        } else if manage_job_called {
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
                    new_status.conditions = new_conditions;
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
                    new_status.conditions = new_conditions;
                    suspend_cond_changed = true;
                    // Resumed jobs will always reset StartTime to current time. This is
                    // done because the ActiveDeadlineSeconds timer shouldn't go off
                    // whilst the Job is still suspended and resetting StartTime is
                    // consistent with resuming a Job created in the suspended state.
                    // (ActiveDeadlineSeconds is interpreted as the number of seconds a
                    // Job is continuously active.)
                    new_status.start_time = Some(now());
                }
            }
        }
    }

    let needs_status_update = suspend_cond_changed
        || active as u32 != job.status.active
        || ready as u32 == job.status.ready;
    new_status.active = active as u32;
    new_status.ready = ready as u32;
    track_job_status_and_remove_finalizers(needs_status_update);

    None
}

// getNewFinishedPods returns the list of newly succeeded and failed pods that are not accounted
// in the job status. The list of failed pods can be affected by the podFailurePolicy.
fn get_new_finished_pods<'a>(
    job: &Job,
    pods: &[&'a Pod],
    uncounted: &UncountedTerminatedPods,
    expected_rm_finalizers: &[String],
) -> (Vec<&'a Pod>, Vec<&'a Pod>) {
    let succeeded_pods = get_valid_pods_with_filter(
        job,
        pods,
        &uncounted.succeeded,
        expected_rm_finalizers,
        |p| p.status.phase == PodPhase::Succeeded,
    );
    let failed_pods =
        get_valid_pods_with_filter(job, pods, &uncounted.failed, expected_rm_finalizers, |p| {
            is_pod_failed(p, job)
        });
    (succeeded_pods, failed_pods)
}

fn get_valid_pods_with_filter<'a>(
    job: &Job,
    pods: &[&'a Pod],
    uncounted_uids: &[String],
    expected_rm_finalizers: &[String],
    f: impl Fn(&Pod) -> bool,
) -> Vec<&'a Pod> {
    pods.into_iter()
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
                if index.map_or(true, |i| {
                    i >= job.spec.completions.unwrap_or_default() as usize
                }) {
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

fn get_completion_index(annotations: &BTreeMap<String, String>) -> Option<usize> {
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
            if let Some(container_status) = match_on_exit_codes(&pod.status, &on_exit_codes) {
                match rule.action {
                    JobPodFailurePolicyRuleAction::Ignore => {
                        return (None, false, Some(rule.action))
                    }
                    JobPodFailurePolicyRuleAction::FailIndex => {}
                    JobPodFailurePolicyRuleAction::Count => return (None, true, Some(rule.action)),
                    JobPodFailurePolicyRuleAction::FailJob => {
                        let msg = format!("Container {} for pod {}/{} failed with exit code {} matching {:?} rulel at index {}", container_status.name, pod.metadata.namespace, pod.metadata.name, container_status.state.terminated.as_ref().unwrap().exit_code, rule.action, index);
                        return (Some(msg), true, Some(rule.action));
                    }
                }
            }
        } else if let Some(on_pod_conditions) = &rule.on_pod_conditions {
            if let Some(pod_condition) = match_on_pod_conditions(&pod.status, &on_pod_conditions) {
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
    conditions.into_iter().find(|c| c.r#type == cond_type)
}

fn find_condition_by_type_mut(
    conditions: &mut [JobCondition],
    cond_type: JobConditionType,
) -> Option<&mut JobCondition> {
    conditions.into_iter().find(|c| c.r#type == cond_type)
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
        last_probe_time: Some(now.clone()),
        last_transition_time: Some(now.clone()),
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
fn calculate_succeeded_indexes(job: &Job, pods: &[&Pod]) -> (Vec<(u32, u32)>, Vec<(u32, u32)>) {
    let prev_intervals = parse_indexes_from_string(
        &job.status.completed_indexes,
        job.spec.completions.unwrap_or_default(),
    );
    let mut new_succeeded = BTreeSet::new();
    for pod in pods {
        if let Some(index) = get_completion_index(&pod.metadata.annotations) {
            // Succeeded Pod with valid index and, if tracking with finalizers,
            // has a finalizer (meaning that it is not counted yet).
            if pod.status.phase == PodPhase::Succeeded
                && index < job.spec.completions.unwrap() as usize
                && has_job_tracking_finalizer(pod)
            {
                new_succeeded.insert(index);
            }
        }
    }

    // List returns the items of the set in order.
    let result = with_ordered_indexes(
        prev_intervals.clone(),
        new_succeeded.into_iter().map(|u| u as u32).collect(),
    );
    (prev_intervals, result)
}

fn parse_indexes_from_string(indexes_str: &str, completions: u32) -> Vec<(u32, u32)> {
    let mut result = Vec::new();

    if indexes_str.is_empty() {
        return result;
    }

    let mut last_interval: Option<(u32, u32)> = None;
    for interval_str in indexes_str.split(',') {
        let mut limits_str = interval_str.split("-");
        let mut first = 0;
        let mut last = 0;
        first = limits_str.next().unwrap().parse().unwrap();
        if first >= completions {
            break;
        }

        if let Some(last_s) = limits_str.next() {
            last = last_s.parse().unwrap();
            if last >= completions {
                last = completions - 1;
            }
        } else {
            last = first;
        }
        if let Some(mut li) = &mut last_interval {
            if li.1 == first - 1 {
                li.1 = last;
                continue;
            }
        }
        result.push((first, last));
        last_interval = Some((first, last));
    }
    result
}

fn with_ordered_indexes(oi: Vec<(u32, u32)>, new_indexes: Vec<u32>) -> Vec<(u32, u32)> {
    let mut new_index_intervals = Vec::new();
    for new_index in new_indexes {
        new_index_intervals.push((new_index, new_index));
    }
    merge(oi, new_index_intervals)
}

fn merge(oi: Vec<(u32, u32)>, new_intervals: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
    let mut last_interval: Option<(u32, u32)> = None;
    let mut result = Vec::new();
    let mut append_or_merge_with_last_interval = |i: (u32, u32)| {
        if last_interval.map_or(true, |li| i.0 > li.1 + 1) {
            result.push(i);
            last_interval = Some(i);
        } else if last_interval.unwrap().1 < i.1 {
            last_interval.unwrap().1 = i.1
        }
    };

    let mut i = 0;
    let mut j = 0;

    while i < oi.len() && j < new_intervals.len() {
        if oi[i].0 < new_intervals[j].0 {
            append_or_merge_with_last_interval(oi[i]);
            i += 1;
        } else {
            append_or_merge_with_last_interval(new_intervals[j]);
            j += 1;
        }
    }

    while i < oi.len() {
        append_or_merge_with_last_interval(oi[i]);
        i += 1;
    }
    while j < new_intervals.len() {
        append_or_merge_with_last_interval(new_intervals[j]);
        j += 1;
    }

    result
}

// deleteActivePods issues deletion for active Pods, preserving finalizers.
// This is done through DELETE calls that set deletion timestamps.
// The method trackJobStatusAndRemoveFinalizers removes the finalizers, after
// which the objects can actually be deleted.
// Returns number of successfully deletions issued.
fn delete_active_pods(job: &Job, pods: &[&Pod]) -> Option<JobControllerAction> {
    pods.first()
        .map(|p| JobControllerAction::DeletePod((*p).clone()))
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

fn track_job_status_and_remove_finalizers(needs_update: bool) {
    todo!()
}

fn manage_job(job: &Job) {
    todo!()
}

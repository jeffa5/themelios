use common::run;
use common::test_table;
use common::test_table_panic;
use std::collections::BTreeMap;
use stdext::function_name;
use themelios::model::OrchestrationModelCfg;
use themelios::resources::Container;
use themelios::resources::Job;
use themelios::resources::JobSpec;
use themelios::resources::Metadata;
use themelios::resources::PodSpec;
use themelios::resources::PodTemplateSpec;
use themelios::state::history::ConsistencySetup;
use themelios::state::RawState;
use themelios::utils;

mod common;

fn model(
    jobs: impl IntoIterator<Item = Job>,
    consistency: ConsistencySetup,
    controllers: usize,
) -> OrchestrationModelCfg {
    let initial_state = RawState::default().with_jobs(jobs);
    OrchestrationModelCfg {
        initial_state,
        consistency_level: consistency,
        schedulers: controllers,
        nodes: controllers,
        replicaset_controllers: 0,
        deployment_controllers: 0,
        statefulset_controllers: 0,
        job_controllers: controllers,
        podgc_controllers: controllers,
        properties: Vec::new(),
    }
}

fn new_job(name: &str, _namespace: &str) -> Job {
    let mut d = Job {
        metadata: utils::metadata(name.to_owned()),
        spec: JobSpec {
            ..Default::default()
        },
        ..Default::default()
    };
    let mut test_labels = BTreeMap::new();
    test_labels.insert("name".to_owned(), "test".to_owned());
    d.spec.selector.match_labels = test_labels.clone();
    d.spec.template = PodTemplateSpec {
        metadata: Metadata {
            labels: test_labels.clone(),
            ..Default::default()
        },
        spec: PodSpec {
            containers: vec![Container {
                name: "fake".to_owned(),
                image: "fake".to_owned(),
                ..Default::default()
            }],
            ..Default::default()
        },
    };
    d
}

// TestNonParallelJob
fn test_non_parallel_job(
    consistency: ConsistencySetup,
    controllers: usize,
) -> OrchestrationModelCfg {
    let job = new_job("simple", "");
    model([job], consistency, controllers)
}

test_table! {
    test_non_parallel_job,
    linearizable_1(ConsistencySetup::Linearizable, 1),
    linearizable_2(ConsistencySetup::Linearizable, 2),
    monotonic_session_1(ConsistencySetup::MonotonicSession, 1),
    monotonic_session_2(ConsistencySetup::MonotonicSession, 2),
    resettable_session_1(ConsistencySetup::ResettableSession, 1),
    optimistic_linear_1(ConsistencySetup::OptimisticLinear, 1),
    causal_1(ConsistencySetup::Causal, 1),
}

test_table_panic! {
    test_non_parallel_job,
    resettable_session_2(ConsistencySetup::ResettableSession, 2),
    optimistic_linear_2(ConsistencySetup::OptimisticLinear, 2),
    causal_2(ConsistencySetup::Causal, 2),
}

// TestParallelJob
fn test_parallel_job(consistency: ConsistencySetup, controllers: usize) -> OrchestrationModelCfg {
    let mut job = new_job("simple", "");
    job.spec.parallelism = 5;
    model([job], consistency, controllers)
}

test_table! {
    test_parallel_job,
    linearizable_1(ConsistencySetup::Linearizable, 1),
    linearizable_2(ConsistencySetup::Linearizable, 2),
    monotonic_session_1(ConsistencySetup::MonotonicSession, 1),
    monotonic_session_2(ConsistencySetup::MonotonicSession, 2),
    resettable_session_1(ConsistencySetup::ResettableSession, 1),
    optimistic_linear_1(ConsistencySetup::OptimisticLinear, 1),
    causal_1(ConsistencySetup::Causal, 1),
}

test_table_panic! {
    test_parallel_job,
    resettable_session_2(ConsistencySetup::ResettableSession, 2),
    optimistic_linear_2(ConsistencySetup::OptimisticLinear, 2),
    causal_2(ConsistencySetup::Causal, 2),
}

// TESTS TO DO
// func TestJobPodFailurePolicyWithFailedPodDeletedDuringControllerRestart(t *testing.T) {
// func TestJobPodFailurePolicy(t *testing.T) {
// func TestParallelJobParallelism(t *testing.T) {
// func TestParallelJobWithCompletions(t *testing.T) {
// func TestIndexedJob(t *testing.T) {
// func TestJobPodReplacementPolicy(t *testing.T) {
// func TestElasticIndexedJob(t *testing.T) {
// func TestOrphanPodsFinalizersClearedWithGC(t *testing.T) {
// func TestJobFailedWithInterrupts(t *testing.T) {
// func TestOrphanPodsFinalizersClearedOnRestart(t *testing.T) {
// func TestSuspendJob(t *testing.T) {
// func TestSuspendJobControllerRestart(t *testing.T) {
// func TestNodeSelectorUpdate(t *testing.T) {

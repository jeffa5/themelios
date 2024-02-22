use common::run;
use std::collections::BTreeMap;
use stdext::function_name;
use themelios::model::OrchestrationModelCfg;
use themelios::resources::Container;
use themelios::resources::Job;
use themelios::resources::JobSpec;
use themelios::resources::Metadata;
use themelios::resources::PodSpec;
use themelios::resources::PodTemplateSpec;
use themelios::state::RawState;
use themelios::utils;

mod common;

fn model(jobs: impl IntoIterator<Item = Job>) -> OrchestrationModelCfg {
    let initial_state = RawState::default().with_jobs(jobs);
    OrchestrationModelCfg {
        initial_state,
        job_controllers: 1,
        schedulers: 1,
        nodes: 1,
        ..Default::default()
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

// func TestNonParallelJob(t *testing.T) {
#[test_log::test]
fn test_non_parallel_job() {
    let job = new_job("simple", "");

    let m = model([job]);
    run(m, common::CheckMode::Bfs, function_name!())
}

// func TestParallelJob(t *testing.T) {
#[test_log::test]
fn test_parallel_job() {
    let mut job = new_job("simple", "");
    job.spec.parallelism = 5;

    let m = model([job]);
    run(m, common::CheckMode::Bfs, function_name!())
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

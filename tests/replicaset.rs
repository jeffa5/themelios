use common::run;
use common::test_table;
use std::collections::BTreeMap;
use stdext::function_name;
use themelios::model::OrchestrationModelCfg;
use themelios::resources::Container;
use themelios::resources::Metadata;
use themelios::resources::PodSpec;
use themelios::resources::PodTemplateSpec;
use themelios::resources::ReplicaSet;
use themelios::resources::ReplicaSetSpec;
use themelios::state::history::ConsistencySetup;
use themelios::state::RawState;
use themelios::utils;

mod common;

fn model(
    replicasets: impl IntoIterator<Item = ReplicaSet>,
    consistency: ConsistencySetup,
    controllers: usize,
) -> OrchestrationModelCfg {
    let initial_state = RawState::default().with_replicasets(replicasets);
    OrchestrationModelCfg::new(initial_state, consistency, controllers)
}

fn new_replicaset(name: &str, _namespace: &str, replicas: u32) -> ReplicaSet {
    let mut d = ReplicaSet {
        metadata: utils::metadata(name.to_owned()),
        spec: ReplicaSetSpec {
            replicas: Some(replicas),
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

// TestSpecReplicasChange
fn test_spec_replicas_change(consistency: ConsistencySetup, controllers: usize) {
    let mut replicaset = new_replicaset("test-spec-replicas-change", "", 2);

    replicaset
        .metadata
        .annotations
        .insert("test".to_owned(), "should-copy-to-replica-set".to_owned());

    let m = model([replicaset], consistency, controllers);
    run(m, function_name!())
}

test_table! {
    test_spec_replicas_change,
    linearizable_1(ConsistencySetup::Linearizable, 1),
    linearizable_2(ConsistencySetup::Linearizable, 2),
    monotonic_session_1(ConsistencySetup::MonotonicSession, 1),
    monotonic_session_2(ConsistencySetup::MonotonicSession, 2),
    resettable_session_1(ConsistencySetup::ResettableSession, 1),
    resettable_session_2(ConsistencySetup::ResettableSession, 2)
}

// TestOverlappingRSs
fn test_overlapping_rss(consistency: ConsistencySetup, controllers: usize) {
    let replicaset_1 = new_replicaset("test-overlapping-rss-1", "", 1);
    let replicaset_2 = new_replicaset("test-overlapping-rss-2", "", 2);

    let m = model([replicaset_1, replicaset_2], consistency, controllers);
    run(m, function_name!())
}

test_table! {
    test_overlapping_rss,
    linearizable_1(ConsistencySetup::Linearizable, 1),
    linearizable_2(ConsistencySetup::Linearizable, 2),
    monotonic_session_1(ConsistencySetup::MonotonicSession, 1),
    monotonic_session_2(ConsistencySetup::MonotonicSession, 2),
    resettable_session_1(ConsistencySetup::ResettableSession, 1),
    resettable_session_2(ConsistencySetup::ResettableSession, 2)
}

// TESTS TO DO
// TestAdoption
// TestDeletingAndFailedPods
// TestPodDeletionCost: don't support deletion costs
// TestPodOrphaningAndAdoptionWhenLabelsChange
// TestGeneralPodAdoption
// TestReadyAndAvailableReplicas
// TestRSScaleSubresource: subresources aren't supported
// TestExtraPodsAdoptionAndDeletion
// TestFullyLabeledReplicas
// TestReplicaSetsAppsV1DefaultGCPolicy
//
// TestRSSelectorImmutability: ignored as just tests API server

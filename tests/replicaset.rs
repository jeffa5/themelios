use common::run;
use common::test_table;
use common::test_table_panic;
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
fn test_spec_replicas_change(
    consistency: ConsistencySetup,
    controllers: usize,
) -> OrchestrationModelCfg {
    let mut replicaset = new_replicaset("test-spec-replicas-change", "", 2);

    replicaset
        .metadata
        .annotations
        .insert("test".to_owned(), "should-copy-to-replica-set".to_owned());

    model([replicaset], consistency, controllers)
}

test_table! {
    test_spec_replicas_change,
    linearizable_1(ConsistencySetup::Linearizable, 1),
    linearizable_2(ConsistencySetup::Linearizable, 2),
    monotonic_session_1(ConsistencySetup::MonotonicSession, 1),
    monotonic_session_2(ConsistencySetup::MonotonicSession, 2),
    resettable_session_1(ConsistencySetup::ResettableSession, 1),
    causal_1(ConsistencySetup::Causal, 1),
}

test_table_panic! {
    test_spec_replicas_change,
    resettable_session_2(ConsistencySetup::ResettableSession, 2),
    causal_2(ConsistencySetup::Causal, 2),
}

// TestOverlappingRSs
fn test_overlapping_rss(
    consistency: ConsistencySetup,
    controllers: usize,
) -> OrchestrationModelCfg {
    let replicaset_1 = new_replicaset("test-overlapping-rss-1", "", 1);
    let replicaset_2 = new_replicaset("test-overlapping-rss-2", "", 2);

    model([replicaset_1, replicaset_2], consistency, controllers)
}

test_table! {
    test_overlapping_rss,
    linearizable_1(ConsistencySetup::Linearizable, 1),
    linearizable_2(ConsistencySetup::Linearizable, 2),
    monotonic_session_1(ConsistencySetup::MonotonicSession, 1),
    monotonic_session_2(ConsistencySetup::MonotonicSession, 2),
    resettable_session_1(ConsistencySetup::ResettableSession, 1),
    causal_1(ConsistencySetup::Causal, 1),
}

test_table_panic! {
    test_overlapping_rss,
    resettable_session_2(ConsistencySetup::ResettableSession, 2),
    causal_2(ConsistencySetup::Causal, 2),
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

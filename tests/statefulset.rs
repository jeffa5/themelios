use common::run;
use common::test_table;
use common::test_table_panic;
use std::collections::BTreeMap;
use stdext::function_name;
use themelios::model::OrchestrationModelCfg;
use themelios::resources::Container;
use themelios::resources::Metadata;
use themelios::resources::Pod;
use themelios::resources::PodSpec;
use themelios::resources::PodTemplateSpec;
use themelios::resources::StatefulSet;
use themelios::resources::StatefulSetSpec;
use themelios::state::history::ConsistencySetup;
use themelios::state::RawState;
use themelios::utils;

mod common;

fn model(
    statefulsets: impl IntoIterator<Item = StatefulSet>,
    nodes: usize,
    consistency: ConsistencySetup,
    controllers: usize,
) -> OrchestrationModelCfg {
    let initial_state = RawState::default().with_statefulsets(statefulsets);
    let mut omc = OrchestrationModelCfg::new(initial_state, consistency, controllers);
    omc.nodes = nodes;
    omc
}

fn new_statefulset(name: &str, _namespace: &str, replicas: u32) -> StatefulSet {
    let mut d = StatefulSet {
        metadata: utils::metadata(name.to_owned()),
        spec: StatefulSetSpec {
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

test_table! {
    test_spec_replicas_change,
    linearizable_1(ConsistencySetup::Linearizable, 1),
    linearizable_2(ConsistencySetup::Linearizable, 2),
    monotonic_session_1(ConsistencySetup::MonotonicSession, 1),
    monotonic_session_2(ConsistencySetup::MonotonicSession, 2),
    resettable_session_1(ConsistencySetup::ResettableSession, 1),
    resettable_session_2(ConsistencySetup::ResettableSession, 2),
    optimistic_linear_1(ConsistencySetup::OptimisticLinear, 1),
    optimistic_linear_2(ConsistencySetup::OptimisticLinear, 2),
    causal_1(ConsistencySetup::Causal, 1),
    causal_2(ConsistencySetup::Causal, 2),
}

// TestSpecReplicasChange
fn test_spec_replicas_change(
    consistency: ConsistencySetup,
    controllers: usize,
) -> OrchestrationModelCfg {
    let mut statefulset = new_statefulset("test-spec-replicas-change", "", 2);

    statefulset
        .metadata
        .annotations
        .insert("test".to_owned(), "should-copy-to-replica-set".to_owned());

    model([statefulset], 1, consistency, controllers)
    // TODO: fix up what this test is supposed to be doing
}

test_table! {
    test_statefulset_available,
    linearizable_1(ConsistencySetup::Linearizable, 1),
    linearizable_2(ConsistencySetup::Linearizable, 2),
    monotonic_session_1(ConsistencySetup::MonotonicSession, 1),
    monotonic_session_2(ConsistencySetup::MonotonicSession, 2),
    resettable_session_1(ConsistencySetup::ResettableSession, 1),
    resettable_session_2(ConsistencySetup::ResettableSession, 2),
    optimistic_linear_1(ConsistencySetup::OptimisticLinear, 1),
    optimistic_linear_2(ConsistencySetup::OptimisticLinear, 2),
    causal_1(ConsistencySetup::Causal, 1),
    causal_2(ConsistencySetup::Causal, 2),
}

// TestStatefulSetAvailable
fn test_statefulset_available(
    consistency: ConsistencySetup,
    controllers: usize,
) -> OrchestrationModelCfg {
    let statefulset = new_statefulset("sts", "", 4);
    model([statefulset], 1, consistency, controllers)
    // TODO: fix up what this test is supposed to be doing
}

test_table! {
    test_stale_reads,
    linearizable_1(ConsistencySetup::Linearizable, 1),
    linearizable_2(ConsistencySetup::Linearizable, 2),
    monotonic_session_1(ConsistencySetup::MonotonicSession, 1),
    monotonic_session_2(ConsistencySetup::MonotonicSession, 2),
}

test_table_panic! {
    test_stale_reads,
    resettable_session_1(ConsistencySetup::ResettableSession, 1),
    optimistic_linear_1(ConsistencySetup::OptimisticLinear, 1),
    optimistic_linear_2(ConsistencySetup::OptimisticLinear, 2),
    resettable_session_2(ConsistencySetup::ResettableSession, 2),
    causal_1(ConsistencySetup::Causal, 1),
    causal_2(ConsistencySetup::Causal, 2),
}

// https://github.com/kubernetes/kubernetes/issues/59848
fn test_stale_reads(consistency: ConsistencySetup, controllers: usize) -> OrchestrationModelCfg {
    let statefulset = new_statefulset("stale-reads", "", 1);
    let mut m = model([statefulset], 2, consistency, controllers);
    m.initial_state.set_pods(std::iter::once(Pod {
        metadata: utils::metadata("zspare-pod".to_owned()),
        spec: PodSpec::default(),
        status: Default::default(),
    }));
    m
}

// TESTS TO DO
// TestVolumeTemplateNoopUpdate
// TestDeletingAndFailedPods
// TestStatefulSetStatusWithPodFail
// TestAutodeleteOwnerRefs
// TestStatefulSetStartOrdinal

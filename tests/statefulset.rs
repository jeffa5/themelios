use common::run;
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
) -> OrchestrationModelCfg {
    let initial_state = RawState::default().with_statefulsets(statefulsets);
    OrchestrationModelCfg {
        initial_state,
        statefulset_controllers: 1,
        schedulers: 1,
        nodes,
        ..Default::default()
    }
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

// TestSpecReplicasChange
#[test_log::test]
fn test_spec_replicas_change() {
    let mut statefulset = new_statefulset("test-spec-replicas-change", "", 2);

    statefulset
        .metadata
        .annotations
        .insert("test".to_owned(), "should-copy-to-replica-set".to_owned());

    let m = model([statefulset], 1);
    // TODO: fix up what this test is supposed to be doing
    run(m, common::CheckMode::Bfs, function_name!())
}

// TestStatefulSetAvailable
#[test_log::test]
fn test_statefulset_available() {
    let statefulset = new_statefulset("sts", "", 4);

    let m = model([statefulset], 1);
    // TODO: fix up what this test is supposed to be doing
    run(m, common::CheckMode::Bfs, function_name!())
}

// https://github.com/kubernetes/kubernetes/issues/59848
#[test_log::test]
fn stale_reads() {
    let statefulset = new_statefulset("stale-reads", "", 1);

    let mut m = model([statefulset], 2);
    m.initial_state.set_pods(std::iter::once(Pod {
        metadata: utils::metadata("zspare-pod".to_owned()),
        spec: PodSpec::default(),
        status: Default::default(),
    }));
    m.consistency_level = ConsistencySetup::Session;
    run(m, common::CheckMode::Dfs, function_name!())
}

// TESTS TO DO
// TestVolumeTemplateNoopUpdate
// TestDeletingAndFailedPods
// TestStatefulSetStatusWithPodFail
// TestAutodeleteOwnerRefs
// TestStatefulSetStartOrdinal

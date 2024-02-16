use common::run;
use std::collections::BTreeMap;
use stdext::function_name;
use themelios::controller::client::ClientState;
use themelios::model::OrchestrationModelCfg;
use themelios::resources::Container;
use themelios::resources::Metadata;
use themelios::resources::PodSpec;
use themelios::resources::PodTemplateSpec;
use themelios::resources::ReplicaSet;
use themelios::resources::ReplicaSetSpec;
use themelios::state::RawState;
use themelios::utils;

mod common;

fn model(
    replicasets: impl IntoIterator<Item = ReplicaSet>,
    client_state: ClientState,
) -> OrchestrationModelCfg {
    let initial_state = RawState::default().with_replicasets(replicasets);
    OrchestrationModelCfg {
        initial_state,
        replicaset_controllers: 1,
        schedulers: 1,
        nodes: 1,
        client_state,
        ..Default::default()
    }
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
#[test_log::test]
fn test_spec_replicas_change() {
    let mut replicaset = new_replicaset("test-spec-replicas-change", "", 2);

    replicaset
        .metadata
        .annotations
        .insert("test".to_owned(), "should-copy-to-replica-set".to_owned());

    let m = model(
        [replicaset],
        ClientState::new_unordered()
            .with_scale_ups(1)
            .with_scale_downs(1),
    );
    run(m, common::CheckMode::Bfs, function_name!())
}

// TestOverlappingRSs
#[test_log::test]
fn test_overlapping_rss() {
    let replicaset_1 = new_replicaset("test-overlapping-rss-1", "", 1);
    let replicaset_2 = new_replicaset("test-overlapping-rss-2", "", 2);

    let m = model([replicaset_1, replicaset_2], ClientState::default());
    run(m, common::CheckMode::Bfs, function_name!())
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

use common::run;
use common::LogicalBoolExt;
use model_checked_orchestration::controller::client::ClientState;
use model_checked_orchestration::model::OrchestrationModelCfg;
use model_checked_orchestration::resources::Container;
use model_checked_orchestration::resources::Metadata;
use model_checked_orchestration::resources::PodSpec;
use model_checked_orchestration::resources::PodTemplateSpec;
use model_checked_orchestration::resources::ReplicaSet;
use model_checked_orchestration::resources::ReplicaSetSpec;
use model_checked_orchestration::state::RawState;
use model_checked_orchestration::utils;
use stateright::Expectation;
use std::collections::BTreeMap;
use stdext::function_name;

mod common;

fn model(replicaset: ReplicaSet, client_state: ClientState) -> OrchestrationModelCfg {
    let initial_state = RawState::default().with_replicaset(replicaset);
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

    let mut m = model(
        replicaset,
        ClientState::new_unordered()
            .with_scale_ups(1)
            .with_scale_downs(1),
    );
    m.add_property(
        Expectation::Always,
        "when synced, replicas are created or removed to match",
        |_model, s| {
            let s = s.latest();
            let mut replicasets_iter = s.replicasets.iter();
            replicasets_iter.all(|r| {
                let pod_count = s.pods.for_controller(&r.metadata.uid).count();
                // when the resource has finished processing towards the desired state the
                // status should match the desired number of replicas and the pods should match
                // that too
                s.resource_stable(r).implies(
                    // the status has been updated correctly
                    r.spec.replicas.unwrap() == r.status.replicas
                        // and the pods were created
                        && pod_count as u32 == r.status.replicas,
                )
            })
        },
    );
    run(m, common::CheckMode::Bfs, function_name!())
}

// TESTS TO DO
// TestAdoption
// TestDeletingAndFailedPods
// TestPodDeletionCost
// TestOverlappingRSs
// TestPodOrphaningAndAdoptionWhenLabelsChange
// TestGeneralPodAdoption
// TestReadyAndAvailableReplicas
// TestRSScaleSubresource
// TestExtraPodsAdoptionAndDeletion
// TestFullyLabeledReplicas
// TestReplicaSetsAppsV1DefaultGCPolicy
//
// TestRSSelectorImmutability: ignored as just tests API server

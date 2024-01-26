use common::run;
use model_checked_orchestration::controller::client::ClientState;
use model_checked_orchestration::controller::util::is_pod_active;
use model_checked_orchestration::controller::util::is_pod_ready;
use model_checked_orchestration::model::OrchestrationModelCfg;
use model_checked_orchestration::resources::Container;
use model_checked_orchestration::resources::Metadata;
use model_checked_orchestration::resources::Pod;
use model_checked_orchestration::resources::PodSpec;
use model_checked_orchestration::resources::PodTemplateSpec;
use model_checked_orchestration::resources::StatefulSet;
use model_checked_orchestration::resources::StatefulSetSpec;
use model_checked_orchestration::state::history::ConsistencySetup;
use model_checked_orchestration::state::RawState;
use model_checked_orchestration::utils;
use stateright::Expectation;
use std::collections::BTreeMap;
use stdext::function_name;

mod common;

fn model(
    statefulset: StatefulSet,
    client_actions: ClientState,
    nodes: usize,
) -> OrchestrationModelCfg {
    let initial_state = RawState::default().with_statefulset(statefulset);
    OrchestrationModelCfg {
        initial_state,
        statefulset_controllers: 1,
        schedulers: 1,
        nodes,
        client_state: client_actions,
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

    let mut m = model(
        statefulset,
        // scale from initial 2 up to 3 and down to 0 then up to 2 again
        ClientState::new_unordered()
            .with_scale_ups(3)
            .with_scale_downs(3),
        1,
    );
    m.add_property(
        Expectation::Eventually,
        "new replicaset is created",
        |_model, s| {
            let s = s.latest();
            let mut deployment_iter = s.deployments.iter();
            deployment_iter.all(|d| s.replicasets.for_controller(&d.metadata.uid).count() != 0)
        },
    );
    run(m, common::CheckMode::Bfs, function_name!())
}

// TestStatefulSetAvailable
#[test_log::test]
fn test_statefulset_available() {
    let statefulset = new_statefulset("sts", "", 4);

    let mut m = model(statefulset, ClientState::new_unordered(), 1);
    m.add_property(
        Expectation::Always,
        "statefulset status counters are correct",
        |_model, s| {
            let s = s.latest();
            for sts in s.statefulsets.iter() {
                let pods = s.pods.matching(&sts.spec.selector);
                let mut pod_count = 0;
                let mut ready_replicas = 0;
                let mut active_replicas = 0;
                for pod in pods {
                    pod_count += 1;
                    if is_pod_ready(pod) {
                        ready_replicas += 1;
                    }
                    if is_pod_active(pod) {
                        active_replicas += 1;
                    }
                }

                dbg!(
                    &s.revision,
                    &sts.status,
                    pod_count,
                    ready_replicas,
                    active_replicas
                );
                let satisfied = sts.status.replicas == pod_count
                    && sts.status.ready_replicas == ready_replicas
                    && sts.status.available_replicas == active_replicas;
                if !satisfied {
                    return false;
                }
            }
            true
        },
    );
    run(m, common::CheckMode::Bfs, function_name!())
}

// https://github.com/kubernetes/kubernetes/issues/59848
#[test_log::test]
fn stale_reads() {
    let statefulset = new_statefulset("stale-reads", "", 1);

    let mut m = model(
        statefulset,
        ClientState::new_ordered().with_change_images(1),
        2,
    );
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

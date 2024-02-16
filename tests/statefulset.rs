use common::run;
use stateright::Expectation;
use std::collections::BTreeMap;
use stdext::function_name;
use themelios::controller::client::ClientState;
use themelios::controller::util::is_pod_active;
use themelios::controller::util::is_pod_ready;
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
    statefulset: StatefulSet,
    client_actions: ClientState,
    nodes: usize,
) -> OrchestrationModelCfg {
    let initial_state = RawState::default().with_statefulset(statefulset);
    let mut model = OrchestrationModelCfg {
        initial_state,
        statefulset_controllers: 1,
        schedulers: 1,
        nodes,
        client_state: client_actions,
        ..Default::default()
    };
    model.add_property(
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
    model
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

    let m = model(
        statefulset,
        // scale from initial 2 up to 3 and down to 0 then up to 2 again
        ClientState::new_unordered()
            .with_scale_ups(3)
            .with_scale_downs(3),
        1,
    );
    // TODO: fix up what this test is supposed to be doing
    run(m, common::CheckMode::Bfs, function_name!())
}

// TestStatefulSetAvailable
#[test_log::test]
fn test_statefulset_available() {
    let statefulset = new_statefulset("sts", "", 4);

    let m = model(statefulset, ClientState::new_unordered(), 1);
    // TODO: fix up what this test is supposed to be doing
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

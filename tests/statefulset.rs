use common::run;
use model_checked_orchestration::controller::client::ClientState;
use model_checked_orchestration::model::OrchestrationModelCfg;
use model_checked_orchestration::resources::Container;
use model_checked_orchestration::resources::Metadata;
use model_checked_orchestration::resources::Node;
use model_checked_orchestration::resources::NodeSpec;
use model_checked_orchestration::resources::NodeStatus;
use model_checked_orchestration::resources::PodSpec;
use model_checked_orchestration::resources::PodTemplateSpec;
use model_checked_orchestration::resources::StatefulSet;
use model_checked_orchestration::resources::StatefulSetSpec;
use model_checked_orchestration::state::StateView;
use model_checked_orchestration::utils;
use stateright::Expectation;
use std::collections::BTreeMap;
use stdext::function_name;

mod common;

fn model(statefulset: StatefulSet, client_actions: ClientState) -> OrchestrationModelCfg {
    let initial_state = StateView::default()
        .with_statefulset(statefulset)
        .with_nodes((0..1).map(|i| {
            (
                i,
                Node {
                    metadata: utils::metadata(format!("node-{i}")),
                    spec: NodeSpec {
                        taints: Vec::new(),
                        unschedulable: false,
                    },
                    status: NodeStatus::default(),
                },
            )
        }))
        .with_controllers(1..4);
    OrchestrationModelCfg {
        initial_state,
        statefulset_controllers: 1,
        schedulers: 1,
        nodes: 1,
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
    );
    m.add_property(
        Expectation::Eventually,
        "new replicaset is created",
        |_model, s| {
            let s = s.latest();
            s.deployments
                .iter()
                .all(|d| !s.replicasets.for_controller(&d.metadata.uid).is_empty())
        },
    );
    run(m, function_name!())
}

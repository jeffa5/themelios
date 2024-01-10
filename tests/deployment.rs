use common::annotations_subset;
use common::run;
use model_checked_orchestration::controller::client::ClientState;
use model_checked_orchestration::controller::deployment::deployment_complete;
use model_checked_orchestration::controller::deployment::find_old_replicasets;
use model_checked_orchestration::controller::deployment::DEFAULT_DEPLOYMENT_UNIQUE_LABEL_KEY;
use model_checked_orchestration::controller::deployment::LAST_APPLIED_CONFIG_ANNOTATION;
use model_checked_orchestration::model::OrchestrationModelCfg;
use model_checked_orchestration::resources::Container;
use model_checked_orchestration::resources::Deployment;
use model_checked_orchestration::resources::DeploymentSpec;
use model_checked_orchestration::resources::DeploymentStrategy;
use model_checked_orchestration::resources::IntOrString;
use model_checked_orchestration::resources::Metadata;
use model_checked_orchestration::resources::Node;
use model_checked_orchestration::resources::NodeSpec;
use model_checked_orchestration::resources::NodeStatus;
use model_checked_orchestration::resources::Pod;
use model_checked_orchestration::resources::PodSpec;
use model_checked_orchestration::resources::PodTemplateSpec;
use model_checked_orchestration::resources::ReplicaSet;
use model_checked_orchestration::resources::RollingUpdate;
use model_checked_orchestration::state::StateView;
use model_checked_orchestration::utils;
use stateright::Expectation;
use std::collections::BTreeMap;
use stdext::function_name;

mod common;

fn model(deployment: Deployment, client_state: ClientState) -> OrchestrationModelCfg {
    let initial_state = StateView::default().with_deployment(deployment);
    OrchestrationModelCfg {
        initial_state,
        deployment_controllers: 1,
        replicaset_controllers: 1,
        schedulers: 1,
        nodes: 1,
        client_state,
        ..Default::default()
    }
}

fn new_deployment(name: &str, _namespace: &str, replicas: u32) -> Deployment {
    let mut d = Deployment {
        metadata: utils::metadata(name.to_owned()),
        spec: DeploymentSpec {
            replicas,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut test_labels = BTreeMap::new();
    test_labels.insert("name".to_owned(), "test".to_owned());
    d.spec.selector.match_labels = test_labels.clone();
    d.spec.strategy = Some(DeploymentStrategy {
        r#type: model_checked_orchestration::resources::DeploymentStrategyType::RollingUpdate,
        rolling_update: Some(RollingUpdate::default()),
    });
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
fn test_new_deployment() {
    // initial state: deployment with some annotations, 2 replicas, another controller that marks pods as ready immediately
    // eventually: deployment completes when pods are marked ready
    // eventually: new replicaset is created
    // eventually: new replicaset annotations should be copied from the new_deployment
    // eventually: New RS should contain pod-template-hash in its selector, label, and template label
    // eventually: All pods targeted by the deployment should contain pod-template-hash in their labels
    let mut deployment = new_deployment("test-new-deployment", "", 2);

    deployment
        .metadata
        .annotations
        .insert("test".to_owned(), "should-copy-to-replica-set".to_owned());
    deployment.metadata.annotations.insert(
        LAST_APPLIED_CONFIG_ANNOTATION.to_owned(),
        "should-not-copy-to-replica-set".to_owned(),
    );

    let mut m = model(deployment, ClientState::default());
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
    m.add_property(
        Expectation::Eventually,
        "deployment is complete",
        |_m, s| {
            let s = s.latest();
            s.deployments
                .iter()
                .all(|d| deployment_complete(d, &d.status))
        },
    );
    m.add_property(
        Expectation::Eventually,
        "replicaset has annotations from deployment",
        |_m, s| {
            let s = s.latest();
            s.deployments.iter().all(|d| {
                s.replicasets
                    .for_controller(&d.metadata.uid)
                    .iter()
                    .all(|rs| annotations_subset(d, *rs))
            })
        },
    );
    m.add_property(
        Expectation::Eventually,
        "rs has pod-template-hash in selector, label and template label",
        |_m, s| {
            let s = s.latest();
            s.deployments.iter().all(|d| {
                s.replicasets
                    .for_controller(&d.metadata.uid)
                    .iter()
                    .all(|rs| check_rs_hash_labels(rs))
            })
        },
    );
    m.add_property(
        Expectation::Eventually,
        "all pods for the rs should have the pod-template-hash in their labels",
        |_m, s| {
            let s = s.latest();
            s.deployments
                .iter()
                .all(|d| check_pods_hash_label(&s.pods.for_controller(&d.metadata.uid)))
        },
    );
    run(m, function_name!())
}

#[test_log::test]
fn test_deployment_rolling_update() {
    // initial state: deployment with some annotations, 2 replicas, another controller that marks pods as ready immediately
    // eventually: deployment completes when pods are marked ready
    // eventually: old replicasets have no pods
    let name = "test-rolling-update-deployment";
    let mut deployment = new_deployment(name, "", 2);

    deployment
        .metadata
        .annotations
        .insert("test".to_owned(), "should-copy-to-replica-set".to_owned());
    deployment.metadata.annotations.insert(
        LAST_APPLIED_CONFIG_ANNOTATION.to_owned(),
        "should-not-copy-to-replica-set".to_owned(),
    );
    deployment.spec.min_ready_seconds = 4;
    let quarter = IntOrString::Str("25%".to_owned());
    deployment.spec.strategy = Some(DeploymentStrategy {
        r#type: model_checked_orchestration::resources::DeploymentStrategyType::RollingUpdate,
        rolling_update: Some(RollingUpdate {
            max_surge: Some(quarter.clone()),
            max_unavailable: Some(quarter),
        }),
    });

    let mut m = model(
        deployment,
        ClientState::new_unordered().with_change_images(1),
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
    m.add_property(
        Expectation::Eventually,
        "old rss do not have pods",
        |_model, s| {
            let s = s.latest();
            s.deployments.iter().all(|d| {
                let rss = s.replicasets.to_vec();
                let (_, old) = find_old_replicasets(d, &rss);
                old.iter()
                    .all(|rs| rs.spec.replicas.map_or(false, |r| r == 0))
            })
        },
    );
    run(m, function_name!())
}

#[test_log::test]
fn test_paused_deployment() {
    // initial state: deployment with some annotations, 2 replicas, another controller that marks pods as ready immediately
    // always: no replicasets are created
    let name = "test-paused-deployment";
    let mut deployment = new_deployment(name, "", 1);
    deployment.spec.paused = true;
    deployment
        .spec
        .template
        .spec
        .termination_grace_period_seconds = Some(1);

    let mut m = model(
        deployment,
        ClientState::new_unordered()
            .with_change_images(1)
            .with_scale_ups(1)
            .with_scale_downs(1),
    );
    m.add_property(
        Expectation::Always,
        "no replicaset is created",
        |_model, s| {
            let s = s.latest();
            s.replicasets.is_empty()
        },
    );
    run(m, function_name!())
}

fn check_rs_hash_labels(rs: &ReplicaSet) -> bool {
    let hash = rs.metadata.labels.get(DEFAULT_DEPLOYMENT_UNIQUE_LABEL_KEY);
    let selector_hash = rs
        .spec
        .selector
        .match_labels
        .get(DEFAULT_DEPLOYMENT_UNIQUE_LABEL_KEY);
    let template_label_hash = rs
        .spec
        .template
        .metadata
        .labels
        .get(DEFAULT_DEPLOYMENT_UNIQUE_LABEL_KEY);

    if hash != selector_hash
        || selector_hash != template_label_hash
        || hash.map_or(true, |s| s.is_empty())
    {
        false
    } else {
        !hash.map_or(true, |h| !rs.metadata.name.ends_with(h))
    }
}

fn check_pods_hash_label(pods: &[&Pod]) -> bool {
    let mut first_hash = None;
    for pod in pods {
        let pod_hash = pod.metadata.labels.get(DEFAULT_DEPLOYMENT_UNIQUE_LABEL_KEY);
        if pod_hash.map_or(true, |h| h.is_empty()) {
            return false;
        } else {
            // Save the first valid hash
            if first_hash.is_some() {
                if pod_hash != first_hash {
                    return false;
                }
            } else {
                first_hash = pod_hash;
            }
        }
    }
    true
}

use common::annotations_subset;
use common::run;
use stateright::Expectation;
use std::collections::BTreeMap;
use stdext::function_name;
use themelios::controller::client::ClientState;
use themelios::controller::deployment::deployment_complete;
use themelios::controller::deployment::find_old_replicasets;
use themelios::controller::deployment::DEFAULT_DEPLOYMENT_UNIQUE_LABEL_KEY;
use themelios::controller::deployment::LAST_APPLIED_CONFIG_ANNOTATION;
use themelios::model::OrchestrationModelCfg;
use themelios::resources::Container;
use themelios::resources::Deployment;
use themelios::resources::DeploymentSpec;
use themelios::resources::DeploymentStrategy;
use themelios::resources::IntOrString;
use themelios::resources::Metadata;
use themelios::resources::Pod;
use themelios::resources::PodSpec;
use themelios::resources::PodTemplateSpec;
use themelios::resources::ReplicaSet;
use themelios::resources::RollingUpdate;
use themelios::state::RawState;
use themelios::utils;

mod common;

fn model(deployment: Deployment, client_state: ClientState) -> OrchestrationModelCfg {
    let initial_state = RawState::default().with_deployment(deployment);
    let mut model = OrchestrationModelCfg {
        initial_state,
        deployment_controllers: 1,
        replicaset_controllers: 1,
        schedulers: 1,
        nodes: 1,
        client_state,
        ..Default::default()
    };
    model.add_property(
        Expectation::Eventually,
        "new replicaset is created",
        |_model, s| {
            let s = s.latest();
            let mut deployment_iter = s.deployments.iter();
            deployment_iter.all(|d| s.replicasets.for_controller(&d.metadata.uid).count() != 0)
        },
    );
    model.add_property(
        Expectation::Eventually,
        "deployment is complete",
        |_m, s| {
            let s = s.latest();
            let mut deployment_iter = s.deployments.iter();
            deployment_iter.all(|d| deployment_complete(d, &d.status))
        },
    );
    model.add_property(
        Expectation::Eventually,
        "replicaset has annotations from deployment",
        |_m, s| {
            let s = s.latest();
            let mut deployment_iter = s.deployments.iter();
            deployment_iter.all(|d| {
                s.replicasets
                    .for_controller(&d.metadata.uid)
                    .all(|rs| annotations_subset(d, rs))
            })
        },
    );
    model.add_property(
        Expectation::Eventually,
        "rs has pod-template-hash in selector, label and template label",
        |_m, s| {
            let s = s.latest();
            let mut deployment_iter = s.deployments.iter();
            deployment_iter.all(|d| {
                s.replicasets
                    .for_controller(&d.metadata.uid)
                    .all(check_rs_hash_labels)
            })
        },
    );
    model.add_property(
        Expectation::Eventually,
        "all pods for the rs should have the pod-template-hash in their labels",
        |_m, s| {
            let s = s.latest();
            let mut deployment_iter = s.deployments.iter();
            deployment_iter.all(|d| check_pods_hash_label(s.pods.for_controller(&d.metadata.uid)))
        },
    );
    model.add_property(
        Expectation::Eventually,
        "old rss do not have pods",
        |_model, s| {
            let s = s.latest();
            let mut deployment_iter = s.deployments.iter();
            deployment_iter.all(|d| {
                let rss = s.replicasets.to_vec();
                let (_, old) = find_old_replicasets(d, &rss);
                old.iter()
                    .all(|rs| rs.spec.replicas.map_or(false, |r| r == 0))
            })
        },
    );
    model.add_property(
        Expectation::Always,
        "no replicaset is created when a deployment is paused",
        |_model, s| {
            let s = s.latest();
            s.deployments.iter().filter(|d| d.spec.paused).all(|d| {
                s.replicasets
                    .for_controller(&d.metadata.uid)
                    .all(|rs| rs.metadata.resource_version <= d.metadata.resource_version)
            })
        },
    );
    model
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
        r#type: themelios::resources::DeploymentStrategyType::RollingUpdate,
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

// TestNewDeployment
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

    let m = model(deployment, ClientState::default());
    run(m, common::CheckMode::Bfs, function_name!())
}

// TestDeploymentRollingUpdate
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
        r#type: themelios::resources::DeploymentStrategyType::RollingUpdate,
        rolling_update: Some(RollingUpdate {
            max_surge: Some(quarter.clone()),
            max_unavailable: Some(quarter),
        }),
    });

    let m = model(
        deployment,
        ClientState::new_unordered().with_change_images(1),
    );
    run(m, common::CheckMode::Bfs, function_name!())
}

// TestPausedDeployment
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

    let m = model(
        deployment,
        ClientState::new_unordered()
            .with_change_images(1)
            .with_scale_ups(1)
            .with_scale_downs(1),
    );
    run(m, common::CheckMode::Bfs, function_name!())
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

fn check_pods_hash_label<'a>(pods: impl Iterator<Item = &'a Pod>) -> bool {
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

// TESTS TO DO
// TestDeploymentSelectorImmutability
// TestScalePausedDeployment
// TestDeploymentHashCollision
// TestFailedDeployment
// TestOverlappingDeployments
// TestScaledRolloutDeployment
// TestSpecReplicasChange
// TestDeploymentAvailableCondition
// TestGeneralReplicaSetAdoption
// TestDeploymentScaleSubresource
// TestReplicaSetOrphaningAndAdoptionWhenLabelsChange

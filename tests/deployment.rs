use common::run;
use common::test_table;
use common::test_table_panic;
use std::collections::BTreeMap;
use stdext::function_name;
use themelios::controller::deployment::LAST_APPLIED_CONFIG_ANNOTATION;
use themelios::model::OrchestrationModelCfg;
use themelios::resources::Container;
use themelios::resources::Deployment;
use themelios::resources::DeploymentSpec;
use themelios::resources::DeploymentStrategy;
use themelios::resources::IntOrString;
use themelios::resources::Metadata;
use themelios::resources::PodSpec;
use themelios::resources::PodTemplateSpec;
use themelios::resources::RollingUpdate;
use themelios::state::history::ConsistencySetup;
use themelios::state::RawState;
use themelios::utils;

mod common;

fn model(
    deployments: impl IntoIterator<Item = Deployment>,
    consistency: ConsistencySetup,
    controllers: usize,
) -> OrchestrationModelCfg {
    let initial_state = RawState::default().with_deployments(deployments);
    OrchestrationModelCfg {
        initial_state,
        consistency_level: consistency,
        schedulers: controllers,
        nodes: controllers,
        replicaset_controllers: controllers,
        deployment_controllers: controllers,
        statefulset_controllers: 0,
        job_controllers: 0,
        podgc_controllers: controllers,
        properties: Vec::new(),
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
fn test_new_deployment(consistency: ConsistencySetup, controllers: usize) -> OrchestrationModelCfg {
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

    model([deployment], consistency, controllers)
}

test_table! {
    test_new_deployment,
    linearizable_1(ConsistencySetup::Linearizable, 1),
    linearizable_2(ConsistencySetup::Linearizable, 2),
    monotonic_session_1(ConsistencySetup::MonotonicSession, 1),
    monotonic_session_2(ConsistencySetup::MonotonicSession, 2),
    resettable_session_1(ConsistencySetup::ResettableSession, 1),
    optimistic_linear_1(ConsistencySetup::OptimisticLinear, 1),
    causal_1(ConsistencySetup::Causal, 1),
}

test_table_panic! {
    test_new_deployment,
    resettable_session_2(ConsistencySetup::ResettableSession, 2),
    optimistic_linear_2(ConsistencySetup::OptimisticLinear, 2),
    causal_2(ConsistencySetup::Causal, 2),
}

// TestDeploymentRollingUpdate
fn test_deployment_rolling_update(
    consistency: ConsistencySetup,
    controllers: usize,
) -> OrchestrationModelCfg {
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

    model([deployment], consistency, controllers)
}

test_table! {
    test_deployment_rolling_update,
    linearizable_1(ConsistencySetup::Linearizable, 1),
    linearizable_2(ConsistencySetup::Linearizable, 2),
    monotonic_session_1(ConsistencySetup::MonotonicSession, 1),
    monotonic_session_2(ConsistencySetup::MonotonicSession, 2),
    resettable_session_1(ConsistencySetup::ResettableSession, 1),
    optimistic_linear_1(ConsistencySetup::OptimisticLinear, 1),
    causal_1(ConsistencySetup::Causal, 1),
}

test_table_panic! {
    test_deployment_rolling_update,
    resettable_session_2(ConsistencySetup::ResettableSession, 2),
    optimistic_linear_2(ConsistencySetup::OptimisticLinear, 2),
    causal_2(ConsistencySetup::Causal, 2),
}

// TestPausedDeployment
fn test_paused_deployment(
    consistency: ConsistencySetup,
    controllers: usize,
) -> OrchestrationModelCfg {
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

    model([deployment], consistency, controllers)
}

test_table! {
    test_paused_deployment,
    linearizable_1(ConsistencySetup::Linearizable, 1),
    linearizable_2(ConsistencySetup::Linearizable, 2),
    monotonic_session_1(ConsistencySetup::MonotonicSession, 1),
    monotonic_session_2(ConsistencySetup::MonotonicSession, 2),
    resettable_session_1(ConsistencySetup::ResettableSession, 1),
    optimistic_linear_1(ConsistencySetup::OptimisticLinear, 1),
    causal_1(ConsistencySetup::Causal, 1),
}

test_table_panic! {
    test_paused_deployment,
    resettable_session_2(ConsistencySetup::ResettableSession, 2),
    optimistic_linear_2(ConsistencySetup::OptimisticLinear, 2),
    causal_2(ConsistencySetup::Causal, 2),
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

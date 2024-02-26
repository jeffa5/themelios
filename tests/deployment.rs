use common::run;
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
    OrchestrationModelCfg::new(initial_state, consistency, controllers)
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

macro_rules! test_new_deployment {
    { $name:ident($consistency:expr, $controllers:expr) } => {
        // TestNewDeployment
        #[test_log::test]
        fn $name() {
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

            let m = model([deployment], $consistency, $controllers);
            run(m, common::CheckMode::Bfs, function_name!())
        }
    };
    { $name:ident($consistency:expr, $controllers:expr), $($x:ident($y:expr, $z:expr)),+ } => {
        test_new_deployment! { $name($consistency, $controllers) }
        test_new_deployment! { $($x($y, $z)),+ }
    }
}

test_new_deployment! {
    test_new_deployment_linearizable_1(ConsistencySetup::Linearizable, 1),
    test_new_deployment_linearizable_2(ConsistencySetup::Linearizable, 2),
    test_new_deployment_monotonic_session_1(ConsistencySetup::MonotonicSession, 1),
    test_new_deployment_monotonic_session_2(ConsistencySetup::MonotonicSession, 2),
    test_new_deployment_resettable_session_1(ConsistencySetup::ResettableSession, 1),
    test_new_deployment_resettable_session_2(ConsistencySetup::ResettableSession, 2)
}

macro_rules! test_deployment_rolling_update {
    { $name:ident($consistency:expr, $controllers:expr) } => {
        // TestDeploymentRollingUpdate
        #[test_log::test]
        fn $name() {
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

            let m = model([deployment], $consistency, $controllers);
            run(m, common::CheckMode::Bfs, function_name!())
        }
    };
    { $name:ident($consistency:expr, $controllers:expr), $($x:ident($y:expr, $z:expr)),+ } => {
        test_deployment_rolling_update! { $name($consistency, $controllers) }
        test_deployment_rolling_update! { $($x($y, $z)),+ }
    }
}

test_deployment_rolling_update! {
    test_deployment_rolling_update_linearizable_1(ConsistencySetup::Linearizable, 1),
    test_deployment_rolling_update_linearizable_2(ConsistencySetup::Linearizable, 2),
    test_deployment_rolling_update_monotonic_session_1(ConsistencySetup::MonotonicSession, 1),
    test_deployment_rolling_update_monotonic_session_2(ConsistencySetup::MonotonicSession, 2),
    test_deployment_rolling_update_resettable_session_1(ConsistencySetup::ResettableSession, 1),
    test_deployment_rolling_update_resettable_session_2(ConsistencySetup::ResettableSession, 2)
}

macro_rules! test_paused_deployment {
    { $name:ident($consistency:expr, $controllers:expr) } => {
        // TestPausedDeployment
        #[test_log::test]
        fn $name() {
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

            let m = model([deployment], $consistency, $controllers);
            run(m, common::CheckMode::Bfs, function_name!())
        }
    };
    { $name:ident($consistency:expr, $controllers:expr), $($x:ident($y:expr, $z:expr)),+ } => {
        test_paused_deployment! { $name($consistency, $controllers) }
        test_paused_deployment! { $($x($y, $z)),+ }
    }
}

test_paused_deployment! {
    test_paused_deployment_update_linearizable_1(ConsistencySetup::Linearizable, 1),
    test_paused_deployment_update_linearizable_2(ConsistencySetup::Linearizable, 2),
    test_paused_deployment_update_monotonic_session_1(ConsistencySetup::MonotonicSession, 1),
    test_paused_deployment_update_monotonic_session_2(ConsistencySetup::MonotonicSession, 2),
    test_paused_deployment_update_resettable_session_1(ConsistencySetup::ResettableSession, 1),
    test_paused_deployment_update_resettable_session_2(ConsistencySetup::ResettableSession, 2)
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

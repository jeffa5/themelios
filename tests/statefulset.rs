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
    consistency: ConsistencySetup,
    controllers: usize,
) -> OrchestrationModelCfg {
    let initial_state = RawState::default().with_statefulsets(statefulsets);
    let mut omc = OrchestrationModelCfg::new(initial_state, consistency, controllers);
    omc.nodes = nodes;
    omc
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

macro_rules! test_spec_replicas_change {
    { $name:ident($consistency:expr, $controllers:expr) } => {
        // TestSpecReplicasChange
        #[test_log::test]
        fn $name() {
            let mut statefulset = new_statefulset("test-spec-replicas-change", "", 2);

            statefulset
                .metadata
                .annotations
                .insert("test".to_owned(), "should-copy-to-replica-set".to_owned());

            let m = model([statefulset], 1, $consistency, $controllers);
            // TODO: fix up what this test is supposed to be doing
            run(m, common::CheckMode::Bfs, function_name!())
        }
    };
    { $name:ident($consistency:expr, $controllers:expr), $($x:ident($y:expr, $z:expr)),+ } => {
        test_spec_replicas_change! { $name($consistency, $controllers) }
        test_spec_replicas_change! { $($x($y, $z)),+ }
    }
}

test_spec_replicas_change! {
    test_spec_replicas_change_linearizable_1(ConsistencySetup::Linearizable, 1),
    test_spec_replicas_change_linearizable_2(ConsistencySetup::Linearizable, 2),
    test_spec_replicas_change_monotonic_session_1(ConsistencySetup::MonotonicSession, 1),
    test_spec_replicas_change_monotonic_session_2(ConsistencySetup::MonotonicSession, 2),
    test_spec_replicas_change_resettable_session_1(ConsistencySetup::ResettableSession, 1),
    test_spec_replicas_change_resettable_session_2(ConsistencySetup::ResettableSession, 2)
}

macro_rules! test_statefulset_available {
    { $name:ident($consistency:expr, $controllers:expr) } => {
        // TestStatefulSetAvailable
        #[test_log::test]
        fn $name() {
            let statefulset = new_statefulset("sts", "", 4);

            let m = model([statefulset], 1, $consistency, $controllers);
            // TODO: fix up what this test is supposed to be doing
            run(m, common::CheckMode::Bfs, function_name!())
        }
    };
    { $name:ident($consistency:expr, $controllers:expr), $($x:ident($y:expr, $z:expr)),+ } => {
        test_statefulset_available! { $name($consistency, $controllers) }
        test_statefulset_available! { $($x($y, $z)),+ }
    }
}

test_statefulset_available! {
    test_statefulset_available_linearizable_1(ConsistencySetup::Linearizable, 1),
    test_statefulset_available_linearizable_2(ConsistencySetup::Linearizable, 2),
    test_statefulset_available_monotonic_session_1(ConsistencySetup::MonotonicSession, 1),
    test_statefulset_available_monotonic_session_2(ConsistencySetup::MonotonicSession, 2),
    test_statefulset_available_resettable_session_1(ConsistencySetup::ResettableSession, 1),
    test_statefulset_available_resettable_session_2(ConsistencySetup::ResettableSession, 2)
}

macro_rules! test_stale_reads {
    { $name:ident($consistency:expr, $controllers:expr) } => {
        // https://github.com/kubernetes/kubernetes/issues/59848
        #[test_log::test]
        fn $name() {
            let statefulset = new_statefulset("stale-reads", "", 1);

            let mut m = model([statefulset], 2, $consistency, $controllers);
            m.initial_state.set_pods(std::iter::once(Pod {
                metadata: utils::metadata("zspare-pod".to_owned()),
                spec: PodSpec::default(),
                status: Default::default(),
            }));
            m.consistency_level = ConsistencySetup::ResettableSession;
            run(m, common::CheckMode::Dfs, function_name!())
        }
    };
    { $name:ident($consistency:expr, $controllers:expr), $($x:ident($y:expr, $z:expr)),+ } => {
        test_stale_reads! { $name($consistency, $controllers) }
        test_stale_reads! { $($x($y, $z)),+ }
    }
}

test_stale_reads! {
    test_stale_reads_linearizable_1(ConsistencySetup::Linearizable, 1),
    test_stale_reads_linearizable_2(ConsistencySetup::Linearizable, 2),
    test_stale_reads_monotonic_session_1(ConsistencySetup::MonotonicSession, 1),
    test_stale_reads_monotonic_session_2(ConsistencySetup::MonotonicSession, 2),
    test_stale_reads_resettable_session_1(ConsistencySetup::ResettableSession, 1),
    test_stale_reads_resettable_session_2(ConsistencySetup::ResettableSession, 2)
}

// TESTS TO DO
// TestVolumeTemplateNoopUpdate
// TestDeletingAndFailedPods
// TestStatefulSetStatusWithPodFail
// TestAutodeleteOwnerRefs
// TestStatefulSetStartOrdinal

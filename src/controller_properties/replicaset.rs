use stateright::Expectation;

use crate::{controller::ReplicaSetController, utils::LogicalBoolExt};

use super::{ControllerProperties, Properties};

impl ControllerProperties for ReplicaSetController {
    fn properties() -> Properties {
        let mut properties = Properties::default();
        properties.add(
            Expectation::Always,
            "rs: when synced, replicas are created or removed to match",
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
        properties.add(
            Expectation::Always,
            "rs: when stable, all pods are created",
            |_model, s| {
                let s = s.latest();
                let all_stable = s.resources_stable(&s.replicasets);
                let expected_count = s.replicasets.iter().map(|r| r.status.replicas).sum::<u32>();
                all_stable.implies(expected_count == s.pods.len() as u32)
            },
        );
        properties.add(
            Expectation::Always,
            "rs: when stable, status replicas == spec replicas",
            |_model, s| {
                let s = s.latest();
                let mut replicasets_iter = s.replicasets.iter();
                replicasets_iter.all(|r| {
                    let stable = s.resource_stable(r);
                    let replicas_equal = r.spec.replicas.unwrap() == r.status.replicas;
                    stable.implies(replicas_equal)
                })
            },
        );
        properties
    }
}

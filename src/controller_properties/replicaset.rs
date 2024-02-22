use stateright::Expectation;

use crate::{
    controller::{util::is_pod_active, ReplicaSetController},
    utils::LogicalBoolExt,
};

use super::{ControllerProperties, Properties};

impl ControllerProperties for ReplicaSetController {
    fn properties() -> Properties {
        let mut properties = Properties::default();
        properties.add(
            Expectation::Always,
            "rs: when stable, status.replicas == count(active_pods)",
            |_model, s| {
                let s = s.latest();
                s.replicasets.iter().all(|r| {
                    // Despite the reference docs saying that the replicas field is
                    // quote: Replicas is the most recently oberved number of replicas.
                    // from: https://kubernetes.io/docs/reference/generated/kubernetes-api/v1.24/#replicasetstatus-v1-apps
                    // It is only actually supposed to count the number of active pods, based on the
                    // implementation.
                    let pod_count = s
                        .pods
                        .for_controller(&r.metadata.uid)
                        .filter(|p| is_pod_active(p))
                        .count();
                    // when the resource has finished processing towards the desired state the
                    // status should match the desired number of replicas and the pods should match
                    // that too
                    s.resource_stable(r).implies(
                        // the pods were created
                        pod_count as u32 == r.status.replicas,
                    )
                })
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

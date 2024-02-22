use stateright::Expectation;

use crate::{
    controller::{util::is_pod_ready, StatefulSetController},
    utils::LogicalBoolExt,
};

use super::{ControllerProperties, Properties};

impl ControllerProperties for StatefulSetController {
    fn properties() -> Properties {
        let mut properties = Properties::default();
        properties.add(
            Expectation::Always,
            "sts: statefulset status.replicas is correct",
            |_model, s| {
                let s = s.latest();
                s.statefulsets.iter().all(|sts| {
                    let pod_count = s.pods.matching(&sts.spec.selector).count() as u32;
                    let stable = s.resource_stable(sts);
                    stable.implies(sts.status.replicas == pod_count)
                })
            },
        );
        properties.add(
            Expectation::Always,
            "sts: statefulset status.ready_replicas is correct",
            |_model, s| {
                let s = s.latest();
                s.statefulsets.iter().all(|sts| {
                    let pod_count = s
                        .pods
                        .matching(&sts.spec.selector)
                        .filter(|p| is_pod_ready(p))
                        .count() as u32;
                    let stable = s.resource_stable(sts);
                    stable.implies(sts.status.ready_replicas == pod_count)
                })
            },
        );
        properties.add(
            Expectation::Always,
            "sts: statefulset status.available_replicas is correct",
            |_model, s| {
                let s = s.latest();
                s.statefulsets.iter().all(|sts| {
                    let pod_count = s
                        .pods
                        .matching(&sts.spec.selector)
                        .filter(|p| is_pod_ready(p))
                        .count() as u32;
                    let stable = s.resource_stable(sts);
                    stable.implies(sts.status.available_replicas == pod_count)
                })
            },
        );
        properties.add(
            Expectation::Always,
            "sts: statefulsets always have consecutive pods",
            |_model, state| {
                // point one and two from https://kubernetes.io/docs/concepts/workloads/controllers/statefulset/#deployment-and-scaling-guarantees
                let state = state.latest();
                for sts in state.statefulsets.iter() {
                    let mut ordinals = Vec::new();
                    for pod in state.pods.iter() {
                        if sts.spec.selector.matches(&pod.metadata.labels) {
                            ordinals
                                .push(crate::controller::statefulset::get_ordinal(pod).unwrap());
                        }
                    }
                    ordinals.sort();
                    // the first one should be 0
                    if let Some(first) = ordinals.first() {
                        if *first != 0 {
                            return false;
                        }
                    }
                    // then each other should be one more than this
                    for os in ordinals.windows(2) {
                        if os[0] + 1 != os[1] {
                            // violation of the property
                            // we have found a missing pod but then continued to find an existing one
                            // for this statefulset.
                            return false;
                        }
                    }
                }
                true
            },
        );
        properties
    }
}

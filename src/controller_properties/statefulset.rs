use stateright::Expectation;

use crate::controller::{
    util::{is_pod_active, is_pod_ready},
    StatefulSetController,
};

use super::{ControllerProperties, Properties};

impl ControllerProperties for StatefulSetController {
    fn properties() -> Properties {
        let mut properties = Properties::default();
        properties.add(
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
        properties.add(
            Expectation::Always,
            "statefulsets always have consecutive pods",
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

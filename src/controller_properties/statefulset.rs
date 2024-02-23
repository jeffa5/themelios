use stateright::Expectation;

use crate::{
    controller::{statefulset::get_ordinal, util::is_pod_ready, StatefulSetController},
    state::revision::Revision,
    utils::LogicalBoolExt,
};

use super::{ControllerProperties, Properties};

impl ControllerProperties for StatefulSetController {
    fn properties() -> Properties {
        let mut properties = Properties::default();
        properties.add(
            Expectation::Always,
            "sts: statefulset status.replicas is correct",
            |_model, state| {
                let s = state.latest();
                s.statefulsets.iter()
                    .filter(|r| !r.status.observed_revision.is_empty())
                    .all(|sts| {
                    let observed_revision =
                        Revision::try_from(&sts.status.observed_revision).unwrap();
                    let observed = state.view_at(observed_revision);
                    let pod_count = observed.pods.matching(&sts.spec.selector).count() as u32;
                    let stable = s.resource_stable(sts);
                    stable.implies(sts.status.replicas == pod_count)
                })
            },
        );
        properties.add(
            Expectation::Always,
            "sts: statefulset status.ready_replicas is correct",
            |_model, state| {
                let s = state.latest();
                s.statefulsets.iter()
                    .filter(|r| !r.status.observed_revision.is_empty())
                    .all(|sts| {
                    let observed_revision =
                        Revision::try_from(&sts.status.observed_revision).unwrap();
                    let observed = state.view_at(observed_revision);
                    let pod_count = observed
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
            |_model, state| {
                let s = state.latest();
                s.statefulsets.iter()
                    .filter(|r| !r.status.observed_revision.is_empty())
                    .all(|sts| {
                    let observed_revision =
                        Revision::try_from(&sts.status.observed_revision).unwrap();
                    let observed = state.view_at(observed_revision);
                    let pod_count = observed
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
            "sts: when stable, statefulsets always have consecutive pods",
            |_model, state| {
                // point one and two from https://kubernetes.io/docs/concepts/workloads/controllers/statefulset/#deployment-and-scaling-guarantees
                let s = state.latest();
                s.statefulsets.iter()
                    .filter(|r| !r.status.observed_revision.is_empty())
                    .all(|sts| {
                    let observed_revision =
                        Revision::try_from(&sts.status.observed_revision).unwrap();
                    let observed = state.view_at(observed_revision);
                    let mut ordinals = observed
                        .pods
                        .for_controller(&sts.metadata.uid)
                        .map(|p| get_ordinal(p).unwrap())
                        .collect::<Vec<_>>();
                    ordinals.sort();
                    // the first one should be 0
                    let correct_start = ordinals.first().map_or(true, |o| {
                        *o == sts.spec.ordinals.as_ref().map_or(0, |o| o.start)
                    });
                    // then each other should be one more than this
                    let sequential = ordinals.windows(2).all(|os| os[0] + 1 == os[1]);
                    s.resource_stable(sts).implies(correct_start && sequential)
                })
            },
        );
        properties
    }
}

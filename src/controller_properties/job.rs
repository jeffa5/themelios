use crate::controller::job::JOB_TRACKING_FINALIZER;
use crate::controller::util::is_pod_active;
use crate::controller::util::is_pod_ready;
use crate::resources::PodPhase;
use crate::state::revision::Revision;
use crate::utils::LogicalBoolExt;
use stateright::Expectation;

use crate::controller::JobController;

use super::ControllerProperties;
use super::Properties;

impl ControllerProperties for JobController {
    fn properties() -> Properties {
        let mut properties = Properties::default();
        properties.add(
            Expectation::Always,
            "job: when synced, status.active is correct",
            |_model, state| {
                let s = state.latest();
                s.jobs.iter().all(|r| {
                    let observed_revision =
                        Revision::try_from(&r.status.observed_revision).unwrap();
                    let observed = state.view_at(observed_revision);
                    let active_pods = observed
                        .pods
                        .for_controller(&r.metadata.uid)
                        .filter(|p| is_pod_active(p))
                        .count();
                    // when the resource has finished processing towards the desired state the
                    // status should match the desired number of replicas and the pods should match
                    // that too
                    let stable = s.resource_stable(r);
                    // mimic validateJobPodsStatus
                    let active_correct = active_pods as u32 == r.status.active;
                    stable.implies(active_correct)
                })
            },
        );
        properties.add(
            Expectation::Always,
            "job: when synced, status.ready is correct",
            |_model, state| {
                let s = state.latest();
                s.jobs.iter().all(|r| {
                    let observed_revision =
                        Revision::try_from(&r.status.observed_revision).unwrap();
                    let observed = state.view_at(observed_revision);
                    let ready_pods = observed
                        .pods
                        .for_controller(&r.metadata.uid)
                        .filter(|p| is_pod_ready(p))
                        .count();
                    // when the resource has finished processing towards the desired state the
                    // status should match the desired number of replicas and the pods should match
                    // that too
                    let stable = s.resource_stable(r);
                    // mimic validateJobPodsStatus
                    let ready_correct = ready_pods as u32 == r.status.ready;
                    stable.implies(ready_correct)
                })
            },
        );
        // properties.add(
        //     Expectation::Always,
        //     "job: owned active pods have tracking finalizer",
        //     |_model, s| {
        //         let s = s.latest();
        //         s.jobs.iter().all(|r| {
        //             s.pods
        //                 .for_controller(&r.metadata.uid)
        //                 .filter(|p| is_pod_active(p))
        //                 .all(|p| {
        //                     p.metadata
        //                         .finalizers
        //                         .contains(&JOB_TRACKING_FINALIZER.to_string())
        //                 })
        //         })
        //     },
        // );
        properties.add(
            Expectation::Always,
            "job: observed finished pods have no finalizer",
            |_model, state| {
                let s = state.latest();
                s.jobs.iter().all(|r| {
                    let observed_revision =
                        Revision::try_from(&r.status.observed_revision).unwrap();
                    let observed = state.view_at(observed_revision);
                    let stable = s.resource_stable(r);
                    let old_pods_dont_have_finalizer = observed
                        .pods
                        .for_controller(&r.metadata.uid)
                        .filter(|p| p.metadata.resource_version < r.metadata.resource_version)
                        .filter(|p| {
                            matches!(p.status.phase, PodPhase::Succeeded | PodPhase::Failed)
                        })
                        .all(|p| {
                            !p.metadata
                                .finalizers
                                .contains(&JOB_TRACKING_FINALIZER.to_string())
                        });
                    stable.implies(old_pods_dont_have_finalizer)
                })
            },
        );
        properties
    }
}

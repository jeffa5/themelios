use crate::controller::job::JOB_TRACKING_FINALIZER;
use crate::controller::util::is_pod_active;
use crate::controller::util::is_pod_ready;
use crate::resources::PodPhase;
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
            "when synced, job status matches pods",
            |_model, s| {
                let s = s.latest();
                s.jobs.iter().all(|r| {
                    let active_pods = s
                        .pods
                        .for_controller(&r.metadata.uid)
                        .filter(|p| is_pod_active(p))
                        .count();
                    let ready_pods = s
                        .pods
                        .for_controller(&r.metadata.uid)
                        .filter(|p| is_pod_ready(p))
                        .count();
                    // when the resource has finished processing towards the desired state the
                    // status should match the desired number of replicas and the pods should match
                    // that too
                    let stable = s.resource_current(r);
                    // mimic validateJobPodsStatus
                    let active_correct = active_pods as u32 == r.status.active;
                    let ready_correct = ready_pods as u32 == r.status.ready;
                    stable.implies(active_correct && ready_correct)
                })
            },
        );
        properties.add(
            Expectation::Always,
            "owned active pods have tracking finalizer",
            |_model, s| {
                let s = s.latest();
                s.jobs.iter().all(|r| {
                    s.pods
                        .for_controller(&r.metadata.uid)
                        .filter(|p| is_pod_active(p))
                        .all(|p| {
                            p.metadata
                                .finalizers
                                .contains(&JOB_TRACKING_FINALIZER.to_string())
                        })
                })
            },
        );
        properties.add(
            Expectation::Always,
            "finished pods have no finalizer",
            |_model, s| {
                let s = s.latest();
                s.jobs.iter().all(|r| {
                    s.pods
                        .for_controller(&r.metadata.uid)
                        .filter(|p| {
                            matches!(p.status.phase, PodPhase::Succeeded | PodPhase::Failed)
                        })
                        .all(|p| {
                            !p.metadata
                                .finalizers
                                .contains(&JOB_TRACKING_FINALIZER.to_string())
                        })
                })
            },
        );
        properties
    }
}

use crate::controller::deployment::deployment_complete;
use crate::controller::deployment::find_old_replicasets;
use crate::controller::deployment::skip_copy_annotation;
use crate::controller::deployment::DEFAULT_DEPLOYMENT_UNIQUE_LABEL_KEY;
use crate::controller::util::subset;
use crate::resources::Pod;
use crate::resources::ReplicaSet;
use crate::state::revision::Revision;
use crate::utils::LogicalBoolExt;
use stateright::Expectation;

use crate::controller::DeploymentController;

use super::ControllerProperties;
use super::Properties;

impl ControllerProperties for DeploymentController {
    fn properties() -> Properties {
        let mut properties = Properties::default();
        properties.add(
            Expectation::Sometimes,
            "dep: deployment is complete",
            |_m, s| {
                let s = s.latest();
                s.deployments
                    .iter()
                    .all(|d| deployment_complete(d, &d.status))
            },
        );
        properties.add(
            Expectation::Always,
            "dep: replicaset has annotations from deployment",
            |_m, state| {
                let s = state.latest();
                s.deployments
                    .iter()
                    .filter(|r| r.status.observed_revision != Revision::default())
                    .all(|d| {
                        let observed_revision = &d.status.observed_revision;
                        let observed = state.view_at(observed_revision);
                        let stable = s.resource_stable(d);
                        let mut d_annotations = d.metadata.annotations.clone();
                        d_annotations.retain(|k, _| !skip_copy_annotation(k));
                        let correct_annotations = observed
                            .replicasets
                            .for_controller(&d.metadata.uid)
                            .all(|rs| subset(&d_annotations, &rs.metadata.annotations));
                        stable.implies(correct_annotations)
                    })
            },
        );
        properties.add(
            Expectation::Always,
            "dep: rs has pod-template-hash in selector, label and template label",
            |_m, state| {
                let s = state.latest();
                s.deployments
                    .iter()
                    .filter(|r| r.status.observed_revision != Revision::default())
                    .all(|d| {
                        let observed_revision = &d.status.observed_revision;
                        let observed = state.view_at(observed_revision);
                        let stable = s.resource_stable(d);
                        let correct_hash = observed
                            .replicasets
                            .for_controller(&d.metadata.uid)
                            .all(check_rs_hash_labels);
                        stable.implies(correct_hash)
                    })
            },
        );
        properties.add(
            Expectation::Always,
            "dep: all pods for the rs should have the pod-template-hash in their labels",
            |_m, state| {
                let s = state.latest();
                s.deployments
                    .iter()
                    .filter(|r| r.status.observed_revision != Revision::default())
                    .all(|d| {
                        let observed_revision = &d.status.observed_revision;
                        let observed = state.view_at(observed_revision);
                        let stable = s.resource_stable(d);
                        let correct_hash =
                            check_pods_hash_label(observed.pods.for_controller(&d.metadata.uid));
                        stable.implies(correct_hash)
                    })
            },
        );
        properties.add(
            Expectation::Always,
            "dep: old rss do not have pods",
            |_model, state| {
                let s = state.latest();
                s.deployments
                    .iter()
                    .filter(|d| d.status.observed_revision != Revision::default())
                    // don't include paused deployments as they do not create new replicasets
                    .filter(|d| !d.spec.paused)
                    .all(|d| {
                        let observed_revision = &d.status.observed_revision;
                        let observed = state.view_at(observed_revision);
                        let stable = s.resource_stable(d);

                        let rss = observed.replicasets.to_vec();
                        let (_, old) = find_old_replicasets(d, &rss);
                        let empty_old_rss = old
                            .iter()
                            .all(|rs| rs.spec.replicas.map_or(false, |r| r == 0));
                        stable.implies(empty_old_rss)
                    })
            },
        );
        properties.add(
            Expectation::Always,
            "dep: no replicaset is created when a deployment is paused",
            |_model, _s| {
                // let s = s.latest();
                // s.deployments.iter().filter(|d| d.spec.paused).all(|d| {
                //     s.replicasets
                //         .for_controller(&d.metadata.uid)
                //         .all(|rs| rs.metadata.resource_version <= d.metadata.resource_version)
                // })
                // TODO: fix this to check that the deployment controller itself does not generate
                // any replicaset creations
                true
            },
        );
        properties
    }
}

fn check_rs_hash_labels(rs: &ReplicaSet) -> bool {
    let hash = rs.metadata.labels.get(DEFAULT_DEPLOYMENT_UNIQUE_LABEL_KEY);
    let selector_hash = rs
        .spec
        .selector
        .match_labels
        .get(DEFAULT_DEPLOYMENT_UNIQUE_LABEL_KEY);
    let template_label_hash = rs
        .spec
        .template
        .metadata
        .labels
        .get(DEFAULT_DEPLOYMENT_UNIQUE_LABEL_KEY);

    if hash != selector_hash
        || selector_hash != template_label_hash
        || hash.map_or(true, |s| s.is_empty())
    {
        false
    } else {
        !hash.map_or(true, |h| !rs.metadata.name.ends_with(h))
    }
}

fn check_pods_hash_label<'a>(pods: impl IntoIterator<Item = &'a Pod>) -> bool {
    let mut first_hash = None;
    for pod in pods {
        let pod_hash = pod.metadata.labels.get(DEFAULT_DEPLOYMENT_UNIQUE_LABEL_KEY);
        if pod_hash.map_or(true, |h| h.is_empty()) {
            return false;
        } else {
            // Save the first valid hash
            if first_hash.is_some() {
                if pod_hash != first_hash {
                    return false;
                }
            } else {
                first_hash = pod_hash;
            }
        }
    }
    true
}

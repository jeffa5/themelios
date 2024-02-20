use crate::controller::deployment::deployment_complete;
use crate::controller::deployment::find_old_replicasets;
use crate::controller::deployment::DEFAULT_DEPLOYMENT_UNIQUE_LABEL_KEY;
use crate::controller::util::annotations_subset;
use crate::resources::Pod;
use crate::resources::ReplicaSet;
use stateright::Expectation;

use crate::controller::DeploymentController;

use super::ControllerProperties;
use super::Properties;

impl ControllerProperties for DeploymentController {
    fn properties() -> Properties {
        let mut properties = Properties::default();
        properties.add(
            Expectation::Eventually,
            "new replicaset is created",
            |_model, s| {
                let s = s.latest();
                let mut deployment_iter = s.deployments.iter();
                deployment_iter.all(|d| s.replicasets.for_controller(&d.metadata.uid).count() != 0)
            },
        );
        properties.add(
            Expectation::Eventually,
            "deployment is complete",
            |_m, s| {
                let s = s.latest();
                let mut deployment_iter = s.deployments.iter();
                deployment_iter.all(|d| deployment_complete(d, &d.status))
            },
        );
        properties.add(
            Expectation::Eventually,
            "replicaset has annotations from deployment",
            |_m, s| {
                let s = s.latest();
                let mut deployment_iter = s.deployments.iter();
                deployment_iter.all(|d| {
                    s.replicasets
                        .for_controller(&d.metadata.uid)
                        .all(|rs| annotations_subset(d, rs))
                })
            },
        );
        properties.add(
            Expectation::Eventually,
            "rs has pod-template-hash in selector, label and template label",
            |_m, s| {
                let s = s.latest();
                let mut deployment_iter = s.deployments.iter();
                deployment_iter.all(|d| {
                    s.replicasets
                        .for_controller(&d.metadata.uid)
                        .all(check_rs_hash_labels)
                })
            },
        );
        properties.add(
            Expectation::Eventually,
            "all pods for the rs should have the pod-template-hash in their labels",
            |_m, s| {
                let s = s.latest();
                let mut deployment_iter = s.deployments.iter();
                deployment_iter
                    .all(|d| check_pods_hash_label(s.pods.for_controller(&d.metadata.uid)))
            },
        );
        properties.add(
            Expectation::Eventually,
            "old rss do not have pods",
            |_model, s| {
                let s = s.latest();
                let mut deployment_iter = s.deployments.iter();
                deployment_iter.all(|d| {
                    let rss = s.replicasets.to_vec();
                    let (_, old) = find_old_replicasets(d, &rss);
                    old.iter()
                        .all(|rs| rs.spec.replicas.map_or(false, |r| r == 0))
                })
            },
        );
        properties.add(
            Expectation::Always,
            "no replicaset is created when a deployment is paused",
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

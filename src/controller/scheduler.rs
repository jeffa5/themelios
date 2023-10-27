use tracing::debug;

use crate::abstract_model::Operation;
use crate::controller::Controller;
use crate::state::StateView;

#[derive(Clone, Debug)]
pub struct Scheduler;

#[derive(Debug, Default, Hash, Clone, PartialEq, Eq)]
pub struct SchedulerState;

impl Controller for Scheduler {
    type State = SchedulerState;

    fn step(
        &self,
        id: usize,
        global_state: &StateView,
        _local_state: &mut Self::State,
    ) -> Option<Operation> {
        if !global_state.controllers.contains(&id) {
            return Some(Operation::ControllerJoin(id));
        } else {
            let mut nodes = global_state
                .nodes
                .iter()
                .map(|(k, v)| (k, v.clone(), global_state.pods_for_node(&v.metadata.name)))
                .collect::<Vec<_>>();
            // TODO: sort nodes by load
            nodes.sort_by_key(|(_, _, pods)| pods.len());

            let pods_to_schedule = global_state
                .pods
                .values()
                .filter(|p| p.spec.node_name.is_none());

            for pod in pods_to_schedule {
                debug!(?pod, "Attempting to schedule pod");
                // find a pod that needs scheduling
                let requests = pod
                    .spec
                    .resources
                    .as_ref()
                    .and_then(|r| r.requests.as_ref())
                    .cloned()
                    .unwrap_or_default();
                // try to find a node suitable
                for (_, node, pods) in &nodes {
                    debug!(?node, "Seeing if node fits");
                    let mut remaining_capacity = node.status.capacity.clone();
                    for running_pod in pods {
                        if let Some(resources) = &running_pod.spec.resources {
                            if let Some(requests) = &resources.requests {
                                remaining_capacity -= requests.clone();
                            }
                        }
                    }
                    debug!(?remaining_capacity, ?requests, "Checking if node has space");
                    if remaining_capacity >= requests {
                        debug!(?pod, ?node, "Did have space, scheduling pod");
                        return Some(Operation::SchedulePod(
                            pod.metadata.name.clone(),
                            node.metadata.name.clone(),
                        ));
                    } else {
                        debug!("Node does not have space");
                    }
                }
            }
        }
        None
    }

    fn name(&self) -> String {
        "Scheduler".to_owned()
    }
}

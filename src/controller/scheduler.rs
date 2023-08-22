use crate::abstract_model::Operation;
use crate::controller::Controller;
use crate::state::StateView;

#[derive(Clone, Debug)]
pub struct Scheduler;

impl Controller for Scheduler {
    fn step(&self, id: usize, state: &StateView) -> Option<Operation> {
        if !state.controllers.contains(&id) {
            return Some(Operation::ControllerJoin(id));
        } else {
            let mut nodes = state
                .nodes
                .iter()
                .map(|(k, v)| (k, v.clone()))
                .collect::<Vec<_>>();
            // TODO: sort nodes by load
            nodes.sort_by_key(|(_, node)| node.running.len());

            for pod in state.pods.values() {
                // find a pod that needs scheduling
                if pod.node_name.is_none() {
                    let requests = pod
                        .resources
                        .as_ref()
                        .and_then(|r| r.requests.as_ref())
                        .cloned()
                        .unwrap_or_default();
                    // try to find a node suitable
                    for (n, node) in &nodes {
                        let mut remaining_capacity = node.capacity.clone();
                        for running_pod in &node.running {
                            if let Some(running_pod) = state.pods.get(running_pod) {
                                if let Some(resources) = &running_pod.resources {
                                    if let Some(requests) = &resources.requests {
                                        remaining_capacity -= requests.clone();
                                    }
                                }
                            }
                        }
                        if remaining_capacity >= requests {
                            return Some(Operation::SchedulePod(pod.id.clone(), **n));
                        }
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

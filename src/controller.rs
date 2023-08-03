use crate::model::{Change, State};

pub trait Controller {
    fn step(&self, id: usize, state: &State) -> Vec<Change>;

    fn name(&self) -> String;
}

#[derive(Clone, Debug)]
pub enum ControllerType {
    Node,
    Scheduler,
}

impl Controller for ControllerType {
    fn step(&self, id: usize, state: &State) -> Vec<Change> {
        let mut actions = Vec::new();
        match self {
            ControllerType::Node => {
                if !state.nodes.contains_key(&id) {
                    actions.push(Change::NodeJoin(id));
                }
                if state.nodes.get(&id).map_or(false, |n| n.ready) {
                    for pod in state.pods.values() {
                        if let Some(node) = pod.node_name {
                            if node == id {
                                actions.push(Change::RunPod(pod.id, node));
                            }
                        }
                    }
                }
            }
            ControllerType::Scheduler => {
                if !state.schedulers.contains(&id) {
                    actions.push(Change::SchedulerJoin(id))
                }
                for pod in state.pods.values() {
                    let least_loaded_node = state
                        .nodes
                        .iter()
                        .map(|(n, node)| (n, node.running.len()))
                        .min_by_key(|(_, apps)| *apps);
                    if let Some((node, _)) = least_loaded_node {
                        if pod.node_name.is_none() {
                            actions.push(Change::SchedulePod(pod.id, *node));
                        }
                    }
                }
            }
        }
        actions
    }

    fn name(&self) -> String {
        match self {
            ControllerType::Node => "Node",
            ControllerType::Scheduler => "Scheduler",
        }
        .to_owned()
    }
}

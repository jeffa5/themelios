use crate::model::{Change, State};

pub trait Controller {
    fn step(&self, id: usize, state: &State) -> Vec<Change>;

    fn name(&self) -> String;
}

#[derive(Clone, Debug)]
pub enum ControllerType {
    Node,
    Scheduler,
    Client { initial_pods: u32 },
}

impl Controller for ControllerType {
    fn step(&self, id: usize, state: &State) -> Vec<Change> {
        let mut actions = Vec::new();
        match self {
            ControllerType::Node => {
                if !state.nodes.contains_key(&id) {
                    actions.push(Change::NodeJoin(id));
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
                        .map(|(n, apps)| (n, apps.len()))
                        .min_by_key(|(_, apps)| *apps);
                    if let Some((node, _)) = least_loaded_node {
                        if pod.scheduled.is_none() {
                            actions.push(Change::SchedulePod(pod.id, *node));
                        }
                    }
                }
            }
            ControllerType::Client { initial_pods } => {
                for i in 0..*initial_pods {
                    if !state.pods.contains_key(&i) {
                        actions.push(Change::NewPod(i))
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
            ControllerType::Client { initial_pods: _ } => "Client",
        }
        .to_owned()
    }
}

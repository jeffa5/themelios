use crate::model::{Change, State};

pub trait Controller {
    fn step(&self, id: usize, state: &State) -> Vec<Change>;

    fn name(&self) -> String;
}

#[derive(Clone, Debug)]
pub enum Controllers {
    Node(Node),
    Scheduler(Scheduler),
    ReplicaSet(ReplicaSet),
}

impl Controller for Controllers {
    fn step(&self, id: usize, state: &State) -> Vec<Change> {
        match self {
            Controllers::Node(c) => c.step(id, state),
            Controllers::Scheduler(c) => c.step(id, state),
            Controllers::ReplicaSet(c) => c.step(id, state),
        }
    }

    fn name(&self) -> String {
        match self {
            Controllers::Node(c) => c.name(),
            Controllers::Scheduler(c) => c.name(),
            Controllers::ReplicaSet(c) => c.name(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Node;

impl Controller for Node {
    fn step(&self, id: usize, state: &State) -> Vec<Change> {
        let mut actions = Vec::new();
        if let Some(node) = state.nodes.get(&id) {
            if node.ready {
                for pod in state
                    .pods
                    .values()
                    .filter(|p| p.node_name.map_or(false, |n| n == id))
                {
                    if !node.running.contains(&pod.id) {
                        actions.push(Change::RunPod(pod.id, id));
                    }
                }
            }
        } else {
            actions.push(Change::NodeJoin(id));
        }
        actions
    }

    fn name(&self) -> String {
        "Node".to_owned()
    }
}

#[derive(Clone, Debug)]
pub struct Scheduler;

impl Controller for Scheduler {
    fn step(&self, id: usize, state: &State) -> Vec<Change> {
        let mut actions = Vec::new();
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
        actions
    }

    fn name(&self) -> String {
        "Scheduler".to_owned()
    }
}

#[derive(Clone, Debug)]
pub struct ReplicaSet;

impl Controller for ReplicaSet {
    fn step(&self, id: usize, state: &State) -> Vec<Change> {
        let mut actions = Vec::new();
        if !state.replicaset_controllers.contains(&id) {
            actions.push(Change::ReplicasetJoin(id))
        }
        for replicaset in state.replica_sets.values() {
            for pod in replicaset.pods() {
                if !state.pods.contains_key(&pod) {
                    actions.push(Change::NewPod(pod));
                }
            }
        }
        actions
    }

    fn name(&self) -> String {
        "ReplicaSet".to_owned()
    }
}

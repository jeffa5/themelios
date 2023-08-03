use std::collections::{BTreeMap, BTreeSet};

use stateright::{Model, Property};

use crate::controller::{Controller, ControllerType};

#[derive(Debug)]
pub struct ModelCfg {
    pub controllers: Vec<ControllerType>,
    pub initial_pods: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Change {
    NodeJoin(usize),
    SchedulerJoin(usize),
    NewPod(u32),
    SchedulePod(u32, usize),
    RunPod(u32, usize),
}

#[derive(Default, Clone, Debug, PartialEq, Eq, Hash)]
pub struct State {
    pub nodes: BTreeMap<usize, Node>,
    pub schedulers: BTreeSet<usize>,
    pub pods: BTreeMap<u32, Pod>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Pod {
    pub id: u32,
    pub scheduled: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Node {
    pub running: BTreeSet<u32>,
    pub ready: bool,
}

impl State {
    fn apply_change(&mut self, change: Change) {
        match change {
            Change::NodeJoin(i) => {
                self.nodes.insert(
                    i,
                    Node {
                        running: BTreeSet::new(),
                        ready: true,
                    },
                );
            }
            Change::SchedulerJoin(i) => {
                self.schedulers.insert(i);
            }
            Change::NewPod(i) => {
                self.pods.insert(
                    i,
                    Pod {
                        id: i,
                        scheduled: None,
                    },
                );
            }
            Change::SchedulePod(pod, node) => {
                if let Some(pod) = self.pods.get_mut(&pod) {
                    pod.scheduled = Some(node);
                }
            }
            Change::RunPod(pod, node) => {
                self.nodes.get_mut(&node).unwrap().running.insert(pod);
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Action {
    ControllerStep(usize, String, Vec<Change>),
    NodeCrash(usize),
}

impl Model for ModelCfg {
    type State = State;

    type Action = Action;

    fn init_states(&self) -> Vec<Self::State> {
        let mut state = State::default();
        for i in 0..self.initial_pods {
            state.pods.insert(
                i,
                Pod {
                    id: i,
                    scheduled: None,
                },
            );
        }
        vec![state]
    }

    fn actions(&self, state: &Self::State, actions: &mut Vec<Self::Action>) {
        for (i, controller) in self.controllers.iter().enumerate() {
            let changes = controller.step(i, state);
            actions.push(Action::ControllerStep(i, controller.name(), changes));
        }
        for (node_id, node) in &state.nodes {
            if node.ready {
                actions.push(Action::NodeCrash(*node_id));
            }
        }
    }

    fn next_state(&self, last_state: &Self::State, action: Self::Action) -> Option<Self::State> {
        match action {
            Action::ControllerStep(_, _, changes) => {
                let mut state = last_state.clone();
                for change in changes {
                    state.apply_change(change);
                }
                Some(state)
            }
            Action::NodeCrash(node) => {
                let mut state = last_state.clone();
                state.nodes.remove(&node);
                state
                    .pods
                    .retain(|_, pod| pod.scheduled.map_or(true, |n| n != node));
                Some(state)
            }
        }
    }

    fn properties(&self) -> Vec<stateright::Property<Self>> {
        vec![Property::<Self>::eventually(
            "every pod gets scheduled",
            |_model, state| state.pods.values().all(|pod| pod.scheduled.is_some()),
        )]
    }
}

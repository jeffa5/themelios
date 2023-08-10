use stateright::{Model, Property};

use crate::controller::{Controller, Controllers};
use crate::state::{ReadConsistencyLevel, Revision, State, StateView};

#[derive(Debug)]
pub struct AbstractModelCfg {
    /// The controllers running in this configuration.
    pub controllers: Vec<Controllers>,
    /// The initial state.
    pub initial_state: StateView,
    /// The consistency level of the state.
    pub consistency_level: ReadConsistencyLevel,
}

/// Changes to a state.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Change {
    /// The revision of the state that this change was generated from.
    pub revision: Revision,
    /// The operation to perform on the state.
    pub operation: Operation,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Operation {
    NodeJoin(usize),
    ControllerJoin(usize),
    NewPod(String),
    NewReplicaset(String),
    SchedulePod(String, usize),
    RunPod(String, usize),
    NodeCrash(usize),
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Action {
    ControllerStep(usize, String, Vec<Change>),
    NodeCrash(usize),
}

impl Model for AbstractModelCfg {
    type State = State;

    type Action = Action;

    fn init_states(&self) -> Vec<Self::State> {
        vec![State::new(
            self.initial_state.clone(),
            self.consistency_level.clone(),
        )]
    }

    fn actions(&self, state: &Self::State, actions: &mut Vec<Self::Action>) {
        for (i, controller) in self.controllers.iter().enumerate() {
            for view in state.views(&i) {
                let operations = controller.step(i, &view);
                let changes = operations
                    .into_iter()
                    .map(|o| Change {
                        revision: view.revision,
                        operation: o,
                    })
                    .collect();
                actions.push(Action::ControllerStep(i, controller.name(), changes));
            }
        }
        // at max revision as this isn't a controller event
        for (node_id, node) in &state.view_at(state.max_revision()).nodes {
            if node.ready {
                actions.push(Action::NodeCrash(*node_id));
            }
        }
    }

    fn next_state(&self, last_state: &Self::State, action: Self::Action) -> Option<Self::State> {
        match action {
            Action::ControllerStep(from, _, changes) => {
                let mut state = last_state.clone();
                state.push_changes(changes.into_iter(), from);
                Some(state)
            }
            Action::NodeCrash(node) => {
                let mut state = last_state.clone();
                state.push_change(
                    Change {
                        revision: last_state.max_revision(),
                        operation: Operation::NodeCrash(node),
                    },
                    node,
                );
                Some(state)
            }
        }
    }

    fn properties(&self) -> Vec<stateright::Property<Self>> {
        vec![
            Property::<Self>::eventually("every pod gets scheduled", |_model, state| {
                let state = state.view_at(state.max_revision());
                state.pods.values().all(|pod| pod.node_name.is_some())
            }),
            Property::<Self>::always(
                "statefulsets always have consecutive pods",
                |_model, state| {
                    // point one and two from https://kubernetes.io/docs/concepts/workloads/controllers/statefulset/#deployment-and-scaling-guarantees
                    let state = state.view_at(state.max_revision());
                    for sts in state.statefulsets.values() {
                        let mut found_end = false;
                        for pod in sts.pods() {
                            if state.pods.contains_key(&pod) {
                                if found_end {
                                    // violation of the property
                                    // we have found a missing pod but then continued to find an existing one
                                    // for this statefulset.
                                    return false;
                                }
                            } else {
                                found_end = true
                            }
                        }
                    }
                    true
                },
            ),
        ]
    }
}

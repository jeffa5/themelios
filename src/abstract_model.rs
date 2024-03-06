use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use tracing::debug;

use stateright::{Model, Property};

use crate::arbitrary_client::ArbitraryClient;
use crate::arbitrary_client::ArbitraryClientAction;
use crate::controller::util::get_node_condition;
use crate::controller::{Controller, Controllers};
use crate::resources::{
    ConditionStatus, ControllerRevision, Deployment, Job, NodeConditionType, PersistentVolumeClaim,
    Pod, ReplicaSet, ResourceQuantities, StatefulSet,
};
use crate::state::RawState;
use crate::state::{history::ConsistencySetup, revision::Revision, State};

#[derive(derivative::Derivative)]
#[derivative(Debug)]
pub struct AbstractModelCfg {
    /// The controllers running in this configuration.
    pub controllers: Vec<Controllers>,
    /// The initial state.
    pub initial_state: RawState,
    /// The consistency level of the state.
    pub consistency_level: ConsistencySetup,
    #[derivative(Debug = "ignore")]
    pub properties: Vec<Property<AbstractModel>>,
}

#[derive(derivative::Derivative)]
#[derivative(Debug)]
pub struct AbstractModel {
    pub controllers: Vec<Controllers>,
    pub initial_states: Vec<State>,
    #[derivative(Debug = "ignore")]
    pub properties: Vec<Property<Self>>,
}

impl AbstractModel {
    pub fn new(cfg: AbstractModelCfg) -> Self {
        let mut state = State::new(cfg.initial_state, cfg.consistency_level);
        for c in &cfg.controllers {
            state.add_controller(c.new_state());
        }
        let initial_states = vec![state];
        Self {
            controllers: cfg.controllers,
            initial_states,
            properties: cfg.properties,
        }
    }
}

/// Changes to a state.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Change {
    /// The revision of the state that this change was generated from.
    pub revision: Revision,
    /// The operation to perform on the state.
    pub operation: ControllerAction,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ControllerAction {
    /// Name and resources
    NodeJoin(String, ResourceQuantities),
    DeleteNode(String),

    // Pods
    CreatePod(Pod),
    SoftDeletePod(Pod),
    HardDeletePod(Pod),
    UpdatePod(Pod),

    // Deployments
    UpdateDeployment(Deployment),
    RequeueDeployment(Deployment),
    // Update just the status part of the resource, not triggering more reconciliations (I think)
    UpdateDeploymentStatus(Deployment),

    // ReplicaSets
    CreateReplicaSet(ReplicaSet),
    UpdateReplicaSet(ReplicaSet),
    UpdateReplicaSetStatus(ReplicaSet),
    // a batch update of multiple replicasets that should cause a new reconciliation if it fails to
    // have this
    UpdateReplicaSets(Vec<ReplicaSet>),
    DeleteReplicaSet(ReplicaSet),

    // StatefulSets
    UpdateStatefulSet(StatefulSet),
    UpdateStatefulSetStatus(StatefulSet),

    // ControllerRevisions
    CreateControllerRevision(ControllerRevision),
    UpdateControllerRevision(ControllerRevision),
    DeleteControllerRevision(ControllerRevision),

    // PersistentVolumeClaims
    CreatePersistentVolumeClaim(PersistentVolumeClaim),
    UpdatePersistentVolumeClaim(PersistentVolumeClaim),

    // Jobs
    UpdateJob(Job),
    UpdateJobStatus(Job),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Action {
    ControllerStep(Revision, usize),
    ArbitraryStep(ArbitraryClientAction),

    /// The controller at the given index restarts, losing its state.
    ControllerRestart(usize),
    NodeRestart(usize),
}

impl Model for AbstractModel {
    type State = State;

    type Action = Action;

    fn init_states(&self) -> Vec<Self::State> {
        self.initial_states.clone()
    }

    fn actions(&self, state: &Self::State, actions: &mut Vec<Self::Action>) {
        for (i, controller) in self.controllers.iter().enumerate() {
            let cstate = state.get_controller(i);
            let min_revision = controller.min_revision_accepted(cstate);
            for revision in state.revisions(min_revision) {
                debug!(?revision, "Adding revision choice");
                actions.push(Action::ControllerStep(revision, i));
            }
        }

        // arbitrary client
        let latest_view = state.latest();
        let arbitrary_actions = ArbitraryClient::actions(&latest_view)
            .into_iter()
            .map(Action::ArbitraryStep);
        actions.extend(arbitrary_actions);

        for (i, controller) in self.controllers.iter().enumerate() {
            if matches!(controller, Controllers::Node(_)) {
                // skip nodes for now
                continue;
            }
            if state.get_controller(i) != &controller.new_state() {
                actions.push(Action::ControllerRestart(i));
            }
        }

        // at max revision as this isn't a controller event
        for node in latest_view.nodes.iter() {
            if let Some(cond) =
                get_node_condition(&node.status.conditions, NodeConditionType::Ready)
            {
                if cond.status == ConditionStatus::True {
                    // find the controller index for the corresponding node
                    for (i, controller) in self.controllers.iter().enumerate() {
                        if let Controllers::Node(n) = controller {
                            if n.name == node.metadata.name {
                                // match
                                actions.push(Action::NodeRestart(i));
                            }
                        }
                    }
                }
            }
        }
    }

    fn next_state(&self, last_state: &Self::State, action: Self::Action) -> Option<Self::State> {
        match action {
            Action::ControllerStep(revision, controller_index) => {
                let controller = &self.controllers[controller_index];
                let mut cstate = last_state.get_controller(controller_index).clone();
                let view = &last_state.view_at(&revision);
                let mut state = last_state.clone();
                if let Some(action) = controller.step(view, &mut cstate) {
                    state.push_change(Change {
                        revision,
                        operation: action,
                    });
                }
                state.update_controller(controller_index, cstate);
                Some(state)
            }
            Action::ArbitraryStep(action) => {
                let mut state = last_state.clone();
                let controller_action = ArbitraryClient::controller_action(&state.latest(), action);
                state.push_change(Change {
                    revision: state.max_revision(),
                    operation: controller_action,
                });
                Some(state)
            }
            Action::ControllerRestart(controller_index) => {
                let mut state = last_state.clone();
                let controller_state = self.controllers[controller_index].new_state();
                state.update_controller(controller_index, controller_state);
                Some(state)
            }
            Action::NodeRestart(controller_index) => {
                let mut state = last_state.clone();
                let controller_state = self.controllers[controller_index].new_state();
                state.update_controller(controller_index, controller_state);
                let s = state.latest();
                if let Controllers::Node(n) = &self.controllers[controller_index] {
                    if let Some(node) = s.nodes.get(&n.name) {
                        state.push_change(Change {
                            revision: s.revision.clone(),
                            operation: ControllerAction::DeleteNode(node.metadata.name.clone()),
                        });
                    }
                }
                Some(state)
            }
        }
    }

    fn properties(&self) -> Vec<stateright::Property<Self>> {
        let mut p = self.properties.clone();
        p.append(&mut vec![Property::<Self>::always(
            "all resources have unique names",
            |_model, state| {
                let state = state.latest();
                all_unique(state.nodes.iter().map(|n| &n.metadata.name))
                    && all_unique(state.pods.iter().map(|n| &n.metadata.name))
                    && all_unique(state.replicasets.iter().map(|n| &n.metadata.name))
                    && all_unique(state.deployments.iter().map(|n| &n.metadata.name))
                    && all_unique(state.statefulsets.iter().map(|n| &n.metadata.name))
                    && all_unique(state.controller_revisions.iter().map(|n| &n.metadata.name))
                    && all_unique(
                        state
                            .persistent_volume_claims
                            .iter()
                            .map(|n| &n.metadata.name),
                    )
                    && all_unique(state.jobs.iter().map(|n| &n.metadata.name))
            },
        )]);
        p
    }

    fn format_action(&self, action: &Self::Action) -> String
    where
        Self::Action: std::fmt::Debug,
    {
        match action {
            Action::ControllerStep(_, i) => {
                let name = self.controllers[*i].name();
                format!("{:?}: {}", action, name)
            }
            Action::ArbitraryStep(_) => format!("{:?}", action),
            Action::ControllerRestart(i) => {
                let name = self.controllers[*i].name();
                format!("{:?}: {}", action, name)
            }
            Action::NodeRestart(_) => format!("{:?}", action),
        }
    }

    fn format_step(&self, last_state: &Self::State, action: Self::Action) -> Option<String>
    where
        Self::State: std::fmt::Debug,
    {
        let last = format!("{:#?}", last_state);
        let next = self
            .next_state(last_state, action)
            .map(|next_state| format!("{:#?}", next_state))
            .unwrap_or_default();
        let textdiff = similar::TextDiff::from_lines(&last, &next);
        let diff = similar::udiff::UnifiedDiff::from_text_diff(&textdiff);
        let out = diff.to_string();
        Some(out)
    }
}

fn all_unique<T: Ord>(iter: impl IntoIterator<Item = T>) -> bool {
    let mut set = BTreeSet::new();
    for item in iter {
        if !set.insert(item) {
            return false;
        }
    }
    true
}

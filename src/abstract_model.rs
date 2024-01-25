use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use tracing::debug;

use stateright::{Model, Property};

use crate::controller::client::Client;
use crate::controller::client::ClientAction;
use crate::controller::client::ClientState;
use crate::controller::util::get_node_condition;
use crate::controller::{Controller, ControllerStates, Controllers, NodeControllerState};
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
    /// The clients manipulating the system.
    pub clients: Vec<Client>,
    /// The initial state.
    pub initial_state: RawState,
    /// The consistency level of the state.
    pub consistency_level: ConsistencySetup,
    #[derivative(Debug = "ignore")]
    pub properties: Vec<Property<Self>>,
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

    // Pods
    CreatePod(Pod),
    SoftDeletePod(Pod),
    HardDeletePod(Pod),
    SchedulePod(String, String),
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
    UpdateJobStatus(Job),

    // Environmental
    /// Name of the crashed node.
    NodeCrash(String),
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Action {
    ControllerStep(usize, ControllerStates, Change),
    Client(usize, ClientState, ClientAction),
    /// The node with the given controller index, and name, crashes.
    NodeCrash(usize, String),
}

impl Model for AbstractModelCfg {
    type State = State;

    type Action = Action;

    fn init_states(&self) -> Vec<Self::State> {
        let mut state = State::new(self.initial_state.clone(), self.consistency_level.clone());
        for c in &self.controllers {
            state.add_controller(c.new_state());
        }
        for c in &self.clients {
            state.add_client(c.new_state());
        }
        vec![state]
    }

    fn actions(&self, state: &Self::State, actions: &mut Vec<Self::Action>) {
        for (i, controller) in self.controllers.iter().enumerate() {
            for view in state.views(i) {
                let mut cstate = state.get_controller(i).clone();
                debug!(rev = ?view.revision, "Reconciling state");
                let action = controller.step(&view, &mut cstate);
                debug!(
                    controller = controller.name(),
                    ?action,
                    "Controller step completed"
                );
                let change = action.map(|action| Change {
                    revision: view.revision.clone(),
                    operation: action,
                });
                if let Some(change) = change {
                    actions.push(Action::ControllerStep(i, cstate, change));
                }
            }
        }

        for (i, client) in self.clients.iter().enumerate() {
            for view in state.views(i) {
                let cstate = state.get_client(i);
                let cactions = client.actions(i, &view, cstate);
                debug!(?cactions, "Client step completed");
                let mut changes = cactions
                    .into_iter()
                    .map(|(state, action)| Action::Client(i, state, action));
                actions.extend(&mut changes);
            }
        }

        // at max revision as this isn't a controller event
        for node in state.view_at(state.max_revision()).nodes.iter() {
            if let Some(cond) =
                get_node_condition(&node.status.conditions, NodeConditionType::Ready)
            {
                if cond.status == ConditionStatus::True {
                    let mut controller_index = None;
                    // find the controller index for the corresponding node
                    for (i, controller) in self.controllers.iter().enumerate() {
                        if let Controllers::Node(n) = controller {
                            if n.name == node.metadata.name {
                                // match
                                controller_index = Some(i);
                                break;
                            }
                        }
                    }
                    actions.push(Action::NodeCrash(
                        controller_index.unwrap(),
                        node.metadata.name.clone(),
                    ));
                }
            }
        }
    }

    fn next_state(&self, last_state: &Self::State, action: Self::Action) -> Option<Self::State> {
        match action {
            Action::ControllerStep(from, cstate, change) => {
                let mut state = last_state.clone();
                state.push_changes(std::iter::once(change), from);
                state.update_controller(from, cstate);
                Some(state)
            }
            Action::Client(from, cstate, action) => {
                let mut state = last_state.clone();
                let client = self.clients.get(from).unwrap();
                let sv = state.latest();
                let action = client.apply(&sv, action);
                let change = Change {
                    revision: sv.revision,
                    operation: action,
                };
                state.push_changes(std::iter::once(change), from);
                state.update_client(from, cstate);
                Some(state)
            }
            Action::NodeCrash(controller_index, node_name) => {
                let mut state = last_state.clone();
                state.push_change(
                    Change {
                        revision: last_state.max_revision(),
                        operation: ControllerAction::NodeCrash(node_name),
                    },
                    controller_index,
                );
                state.reset_session(controller_index);
                // reset the node's local state
                state.update_controller(
                    controller_index,
                    ControllerStates::Node(NodeControllerState::default()),
                );
                Some(state)
            }
        }
    }

    fn properties(&self) -> Vec<stateright::Property<Self>> {
        let mut p = self.properties.clone();
        p.append(&mut vec![
            Property::<Self>::always("all resources have unique names", |_model, state| {
                let state = state.view_at(state.max_revision());
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
            }),
            Property::<Self>::eventually("every pod gets scheduled", |_model, state| {
                let state = state.view_at(state.max_revision());
                let mut pods_iter = state.pods.iter();
                pods_iter.all(|pod| pod.spec.node_name.is_some())
            }),
            Property::<Self>::always("pods on nodes are unique", |model, state| {
                let mut node_pods = BTreeSet::new();
                for c in 0..model.controllers.len() {
                    let cstate = state.get_controller(c);
                    if let ControllerStates::Node(n) = cstate {
                        for node in &n.running {
                            if !node_pods.insert(node) {
                                return false;
                            }
                        }
                    }
                }
                true
            }),
            Property::<Self>::always(
                "statefulsets always have consecutive pods",
                |_model, state| {
                    // point one and two from https://kubernetes.io/docs/concepts/workloads/controllers/statefulset/#deployment-and-scaling-guarantees
                    let state = state.view_at(state.max_revision());
                    for sts in state.statefulsets.iter() {
                        let mut ordinals = Vec::new();
                        for pod in state.pods.iter() {
                            if sts.spec.selector.matches(&pod.metadata.labels) {
                                ordinals.push(
                                    crate::controller::statefulset::get_ordinal(pod).unwrap(),
                                );
                            }
                        }
                        ordinals.sort();
                        // the first one should be 0
                        if let Some(first) = ordinals.first() {
                            if *first != 0 {
                                return false;
                            }
                        }
                        // then each other should be one more than this
                        for os in ordinals.windows(2) {
                            if os[0] + 1 != os[1] {
                                // violation of the property
                                // we have found a missing pod but then continued to find an existing one
                                // for this statefulset.
                                return false;
                            }
                        }
                    }
                    true
                },
            ),
            Property::<Self>::always("resources have a resource version", |_model, state| {
                let state = state.latest();
                let res = state
                    .nodes
                    .iter()
                    .map(|r| &r.metadata)
                    .chain(state.pods.iter().map(|r| &r.metadata))
                    .chain(state.replicasets.iter().map(|r| &r.metadata))
                    .chain(state.deployments.iter().map(|r| &r.metadata))
                    .chain(state.statefulsets.iter().map(|r| &r.metadata))
                    .chain(state.controller_revisions.iter().map(|r| &r.metadata))
                    .chain(state.persistent_volume_claims.iter().map(|r| &r.metadata))
                    .chain(state.jobs.iter().map(|r| &r.metadata))
                    .all(|m| !m.resource_version.is_empty());
                res
            }),
        ]);
        p
    }
}

fn all_unique<T: Ord>(iter: impl Iterator<Item = T>) -> bool {
    let mut set = BTreeSet::new();
    for item in iter {
        if !set.insert(item) {
            return false;
        }
    }
    true
}

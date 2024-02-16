use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use tracing::debug;

use stateright::{Model, Property};

use crate::arbitrary_client::ArbitraryClient;
use crate::controller::client::Client;
use crate::controller::client::ClientAction;
use crate::controller::client::ClientState;
use crate::controller::util::get_node_condition;
use crate::controller::{Controller, ControllerStates, Controllers};
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
    UpdateJob(Job),
    UpdateJobStatus(Job),
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Action {
    ControllerStep(usize, ControllerStates, Change),
    ArbitraryStep(Change),
    Client(usize, ClientState, ClientAction),

    /// The controller at the given index restarts, losing its state.
    ControllerRestart(usize),
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
            let cstate = state.get_controller(i);
            let min_revision = controller.min_revision_accepted(cstate);
            for view in state.views(min_revision.clone()) {
                if view.revision <= min_revision {
                    panic!(
                        "Tried to give a controller an old revision! {} vs {}",
                        view.revision, min_revision
                    );
                }
                debug!(rev = ?view.revision, "Reconciling state");
                let mut cstate = cstate.clone();
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

        // arbitrary client
        let latest_view = state.latest();
        let arbitrary_actions = ArbitraryClient.actions(&latest_view).into_iter().map(|a| {
            Action::ArbitraryStep(Change {
                revision: latest_view.revision.clone(),
                operation: a,
            })
        });
        actions.extend(arbitrary_actions);

        for (i, client) in self.clients.iter().enumerate() {
            let view = state.latest();
            let cstate = state.get_client(i);
            let cactions = client.actions(i, &view, cstate);
            debug!(?cactions, "Client step completed");
            let mut changes = cactions
                .into_iter()
                .map(|(state, action)| Action::Client(i, state, action));
            actions.extend(&mut changes);
        }

        for (i, controller) in self.controllers.iter().enumerate() {
            if matches!(controller, Controllers::Node(_)) {
                // skip nodes for now
                continue;
            }
            actions.push(Action::ControllerRestart(i));
        }

        // at max revision as this isn't a controller event
        for node in state.latest().nodes.iter() {
            if let Some(cond) =
                get_node_condition(&node.status.conditions, NodeConditionType::Ready)
            {
                if cond.status == ConditionStatus::True {
                    // find the controller index for the corresponding node
                    for (i, controller) in self.controllers.iter().enumerate() {
                        if let Controllers::Node(n) = controller {
                            if n.name == node.metadata.name {
                                // match
                                actions.push(Action::ControllerRestart(i));
                            }
                        }
                    }
                }
            }
        }
    }

    fn next_state(&self, last_state: &Self::State, action: Self::Action) -> Option<Self::State> {
        match action {
            Action::ControllerStep(from, cstate, change) => {
                let mut state = last_state.clone();
                state.push_changes(std::iter::once(change));
                state.update_controller(from, cstate);
                Some(state)
            }
            Action::ArbitraryStep(change) => {
                let mut state = last_state.clone();
                state.push_changes(std::iter::once(change));
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
                state.push_changes(std::iter::once(change));
                state.update_client(from, cstate);
                Some(state)
            }
            Action::ControllerRestart(controller_index) => {
                let mut state = last_state.clone();
                let controller_state = self.controllers[controller_index].new_state();
                state.update_controller(controller_index, controller_state);
                Some(state)
            }
        }
    }

    fn properties(&self) -> Vec<stateright::Property<Self>> {
        let mut p = self.properties.clone();
        p.append(&mut vec![
            Property::<Self>::always("all resources have unique names", |_model, state| {
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
            }),
            Property::<Self>::eventually("every pod gets scheduled", |_model, state| {
                let state = state.latest();
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
                    let state = state.latest();
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

fn all_unique<T: Ord>(iter: impl IntoIterator<Item = T>) -> bool {
    let mut set = BTreeSet::new();
    for item in iter {
        if !set.insert(item) {
            return false;
        }
    }
    true
}

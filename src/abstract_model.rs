use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use stateright::{Model, Property};

use crate::controller::util::get_node_condition;
use crate::controller::{Controller, ControllerStates, Controllers};
use crate::resources::{
    ConditionStatus, ControllerRevision, Deployment, NodeConditionType, PersistentVolumeClaim, Pod,
    ReplicaSet, ResourceQuantities, StatefulSet,
};
use crate::state::{ConsistencySetup, Revision, State, StateView};

#[derive(Debug)]
pub struct AbstractModelCfg {
    /// The controllers running in this configuration.
    pub controllers: Vec<Controllers>,
    /// The initial state.
    pub initial_state: StateView,
    /// The consistency level of the state.
    pub consistency_level: ConsistencySetup,
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
    NodeJoin(usize, ResourceQuantities),
    ControllerJoin(usize),

    // Pods
    CreatePod(Pod),
    DeletePod(Pod),
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
    UpdateStatefulSetStatus(StatefulSet),

    // ControllerRevisions
    CreateControllerRevision(ControllerRevision),
    UpdateControllerRevision(ControllerRevision),
    DeleteControllerRevision(ControllerRevision),

    // PersistentVolumeClaims
    CreatePersistentVolumeClaim(PersistentVolumeClaim),
    UpdatePersistentVolumeClaim(PersistentVolumeClaim),

    // Environmental
    NodeCrash(usize),
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Action {
    ControllerStep(usize, String, ControllerStates, Vec<Change>),
    NodeCrash(usize),
}

impl Model for AbstractModelCfg {
    type State = State;

    type Action = Action;

    fn init_states(&self) -> Vec<Self::State> {
        let mut state = State::new(self.initial_state.clone(), self.consistency_level.clone());
        for c in &self.controllers {
            state.add_controller(c.new_state());
        }
        vec![state]
    }

    fn actions(&self, state: &Self::State, actions: &mut Vec<Self::Action>) {
        for (i, controller) in self.controllers.iter().enumerate() {
            for view in state.views(i) {
                let mut cstate = state.get_controller(i).clone();
                let operations = controller.step(i, &view, &mut cstate);
                let changes = operations
                    .into_iter()
                    .map(|o| Change {
                        revision: view.revision.clone(),
                        operation: o,
                    })
                    .collect();
                actions.push(Action::ControllerStep(
                    i,
                    controller.name(),
                    cstate,
                    changes,
                ));
            }
        }
        // at max revision as this isn't a controller event
        for (node_id, node) in &state.view_at(state.max_revision()).nodes {
            if let Some(cond) =
                get_node_condition(&node.status.conditions, NodeConditionType::Ready)
            {
                if cond.status == ConditionStatus::True {
                    actions.push(Action::NodeCrash(*node_id));
                }
            }
        }
        // TODO: re-enable node crashes
    }

    fn next_state(&self, last_state: &Self::State, action: Self::Action) -> Option<Self::State> {
        match action {
            Action::ControllerStep(from, _, cstate, changes) => {
                let mut state = last_state.clone();
                state.push_changes(changes.into_iter(), from);
                state.update_controller(from, cstate);
                Some(state)
            }
            Action::NodeCrash(node) => {
                let mut state = last_state.clone();
                state.push_change(
                    Change {
                        revision: last_state.max_revision(),
                        operation: ControllerAction::NodeCrash(node),
                    },
                    node,
                );
                Some(state)
            }
        }
    }

    fn properties(&self) -> Vec<stateright::Property<Self>> {
        vec![
            Property::<Self>::always("all resources have unique names", |_model, state| {
                let state = state.view_at(state.max_revision());
                if !all_unique(state.nodes.values().map(|n| &n.metadata.name)) {
                    return false;
                }
                if !all_unique(state.pods.values().map(|n| &n.metadata.name)) {
                    return false;
                }
                if !all_unique(state.replica_sets.values().map(|n| &n.metadata.name)) {
                    return false;
                }
                if !all_unique(state.deployments.values().map(|n| &n.metadata.name)) {
                    return false;
                }
                if !all_unique(state.statefulsets.values().map(|n| &n.metadata.name)) {
                    return false;
                }
                true
            }),
            Property::<Self>::eventually("every pod gets scheduled", |_model, state| {
                let state = state.view_at(state.max_revision());
                state.pods.values().all(|pod| pod.spec.node_name.is_some())
            }),
            Property::<Self>::always(
                "statefulsets always have consecutive pods",
                |_model, state| {
                    // point one and two from https://kubernetes.io/docs/concepts/workloads/controllers/statefulset/#deployment-and-scaling-guarantees
                    let state = state.view_at(state.max_revision());
                    for sts in state.statefulsets.values() {
                        let mut ordinals = Vec::new();
                        for pod in state.pods.values() {
                            if sts.spec.selector.matches(&pod.metadata.labels) {
                                ordinals.push(
                                    crate::controller::statefulset::get_ordinal(pod).unwrap(),
                                );
                            }
                        }
                        ordinals.sort();
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
        ]
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

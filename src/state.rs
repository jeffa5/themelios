use std::collections::{BTreeMap, BTreeSet};

use crate::abstract_model::{Change, Operation};

/// Consistency level for viewing the state with.
#[derive(Default, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ReadConsistencyLevel {
    /// Always work off the latest state.
    #[default]
    Strong,
    /// Work off a state that is close to the latest, bounded by the `k`.
    BoundedStaleness(usize),
    /// Work off a state that derives from the last one seen.
    Session,
    /// Work off any historical state.
    Eventual,
}

pub trait History {
    fn add_change(&mut self, change: Change) -> Revision;

    fn max_revision(&self) -> Revision;

    fn state_at(&self, revision: Revision) -> &StateView;

    fn valid_revisions(&self, session: Revision) -> Vec<Revision>;

    fn states_for(&self, session: Revision) -> Vec<&StateView> {
        let revisions = self.valid_revisions(session);
        revisions.into_iter().map(|r| self.state_at(r)).collect()
    }
}

#[derive(Default, Clone, PartialEq, Eq, Hash, Debug)]
pub struct StrongHistory {
    state: StateView,
}

impl StrongHistory {
    fn new(initial_state: StateView) -> Self {
        Self {
            state: initial_state,
        }
    }
}

impl History for StrongHistory {
    fn add_change(&mut self, change: Change) -> Revision {
        self.state.apply_change(&change);
        self.max_revision()
    }

    fn max_revision(&self) -> Revision {
        self.state.revision
    }

    fn state_at(&self, revision: Revision) -> &StateView {
        assert_eq!(revision, self.state.revision);
        &self.state
    }

    fn valid_revisions(&self, _session: Revision) -> Vec<Revision> {
        vec![self.state.revision]
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct BoundedHistory {
    k: usize,
    last_k_states: Vec<StateView>,
}

impl BoundedHistory {
    fn new(initial_state: StateView, k: usize) -> Self {
        Self {
            k,
            last_k_states: vec![initial_state],
        }
    }
}

impl History for BoundedHistory {
    fn add_change(&mut self, change: Change) -> Revision {
        let mut state = self.last_k_states.last().unwrap().clone();
        state.apply_change(&change);
        if self.last_k_states.len() > self.k {
            self.last_k_states.remove(0);
        }
        self.last_k_states.push(state);
        self.max_revision()
    }

    fn max_revision(&self) -> Revision {
        self.last_k_states.last().unwrap().revision
    }

    fn state_at(&self, revision: Revision) -> &StateView {
        let index = self
            .last_k_states
            .binary_search_by_key(&revision, |s| s.revision)
            .unwrap();
        &self.last_k_states[index]
    }

    fn valid_revisions(&self, _session: Revision) -> Vec<Revision> {
        self.last_k_states.iter().map(|s| s.revision).collect()
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum ChangeHistory {
    Strong(StrongHistory),
    Bounded(BoundedHistory),
}

impl Default for ChangeHistory {
    fn default() -> Self {
        Self::Strong(StrongHistory::default())
    }
}

impl ChangeHistory {
    fn new(consistency_level: ReadConsistencyLevel, initial_state: StateView) -> Self {
        match consistency_level {
            ReadConsistencyLevel::Strong => Self::Strong(StrongHistory::new(initial_state)),
            ReadConsistencyLevel::BoundedStaleness(k) => {
                Self::Bounded(BoundedHistory::new(initial_state, k))
            }
            ReadConsistencyLevel::Session => todo!(),
            ReadConsistencyLevel::Eventual => todo!(),
        }
    }

    fn add_change(&mut self, change: Change) -> Revision {
        match self {
            ChangeHistory::Strong(s) => s.add_change(change),
            ChangeHistory::Bounded(s) => s.add_change(change),
        }
    }

    fn max_revision(&self) -> Revision {
        match self {
            ChangeHistory::Strong(s) => s.max_revision(),
            ChangeHistory::Bounded(s) => s.max_revision(),
        }
    }

    fn state_at(&self, revision: Revision) -> &StateView {
        match self {
            ChangeHistory::Strong(s) => s.state_at(revision),
            ChangeHistory::Bounded(s) => s.state_at(revision),
        }
    }

    fn states_for(&self, session: Revision) -> Vec<&StateView> {
        match self {
            ChangeHistory::Strong(s) => s.states_for(session),
            ChangeHistory::Bounded(s) => s.states_for(session),
        }
    }
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Revision(usize);

/// The history of the state, enabling generating views for different historical versions.
#[derive(Default, Clone, PartialEq, Eq, Hash)]
pub struct State {
    /// The changes that have been made to the state.
    changes: ChangeHistory,
    sessions: BTreeMap<usize, Revision>,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("sessions", &self.sessions)
            .field("changes", &self.changes)
            .finish()
    }
}

impl State {
    pub fn new(initial_state: StateView, consistency_level: ReadConsistencyLevel) -> Self {
        Self {
            changes: ChangeHistory::new(consistency_level, initial_state),
            sessions: Default::default(),
        }
    }

    /// Record a change for this state from a given controller.
    pub fn push_change(&mut self, change: Change, from: usize) -> Revision {
        let rev = self.changes.add_change(change);
        self.sessions.insert(from, rev);
        rev
    }

    /// Record changes for this state.
    pub fn push_changes(&mut self, changes: impl Iterator<Item = Change>, from: usize) -> Revision {
        for change in changes {
            self.push_change(change, from);
        }
        self.max_revision()
    }

    /// Get the maximum revision for this change.
    pub fn max_revision(&self) -> Revision {
        self.changes.max_revision()
    }

    /// Get a view for a specific revision in the change history.
    pub fn view_at(&self, revision: Revision) -> &StateView {
        self.changes.state_at(revision)
    }

    /// Get all the possible views under the given consistency level.
    pub fn views(&self, from: &usize) -> Vec<&StateView> {
        self.changes
            .states_for(self.sessions.get(from).copied().unwrap_or_default())
    }
}

#[derive(Default, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StateView {
    pub revision: Revision,
    pub nodes: BTreeMap<usize, NodeResource>,
    /// Set of the controllers that have joined the cluster.
    pub controllers: BTreeSet<usize>,
    pub pods: BTreeMap<String, PodResource>,
    pub replica_sets: BTreeMap<String, ReplicaSetResource>,
    pub deployments: BTreeMap<String, DeploymentResource>,
    pub statefulsets: BTreeMap<String, StatefulSetResource>,
}

impl StateView {
    pub fn with_pods(mut self, pods: impl Iterator<Item = PodResource>) -> Self {
        self.set_pods(pods);
        self
    }

    pub fn set_pods(&mut self, pods: impl Iterator<Item = PodResource>) -> &mut Self {
        for pod in pods {
            self.pods.insert(pod.id.clone(), pod);
        }
        self
    }

    pub fn with_replicasets(
        mut self,
        replicasets: impl Iterator<Item = ReplicaSetResource>,
    ) -> Self {
        self.set_replicasets(replicasets);
        self
    }

    pub fn set_replicasets(
        &mut self,
        replicasets: impl Iterator<Item = ReplicaSetResource>,
    ) -> &mut Self {
        for replicaset in replicasets {
            self.replica_sets.insert(replicaset.id.clone(), replicaset);
        }
        self
    }

    pub fn with_deployments(
        mut self,
        deployments: impl Iterator<Item = DeploymentResource>,
    ) -> Self {
        self.set_deployments(deployments);
        self
    }

    pub fn set_deployments(
        &mut self,
        deployments: impl Iterator<Item = DeploymentResource>,
    ) -> &mut Self {
        for deployment in deployments {
            self.deployments.insert(deployment.id.clone(), deployment);
        }
        self
    }

    pub fn with_statefulsets(
        mut self,
        statefulsets: impl Iterator<Item = StatefulSetResource>,
    ) -> Self {
        self.set_statefulsets(statefulsets);
        self
    }

    pub fn set_statefulsets(
        &mut self,
        statefulsets: impl Iterator<Item = StatefulSetResource>,
    ) -> &mut Self {
        for statefulset in statefulsets {
            self.statefulsets
                .insert(statefulset.id.clone(), statefulset);
        }
        self
    }

    pub fn apply_change(&mut self, change: &Change) {
        match &change.operation {
            Operation::NodeJoin(i) => {
                self.nodes.insert(
                    *i,
                    NodeResource {
                        running: BTreeSet::new(),
                        ready: true,
                    },
                );
            }
            Operation::ControllerJoin(i) => {
                self.controllers.insert(*i);
            }
            Operation::NewPod(i) => {
                self.pods.insert(
                    i.clone(),
                    PodResource {
                        id: i.clone(),
                        node_name: None,
                    },
                );
            }
            Operation::NewReplicaset(i) => {
                self.replica_sets.insert(
                    i.clone(),
                    ReplicaSetResource {
                        id: i.clone(),
                        replicas: 2,
                    },
                );
            }
            Operation::SchedulePod(pod, node) => {
                if let Some(pod) = self.pods.get_mut(pod) {
                    pod.node_name = Some(*node);
                }
            }
            Operation::RunPod(pod, node) => {
                self.nodes
                    .get_mut(node)
                    .unwrap()
                    .running
                    .insert(pod.clone());
            }
            Operation::NodeCrash(node) => {
                self.nodes.remove(node);
                self.pods
                    .retain(|_, pod| pod.node_name.map_or(true, |n| n != *node));
            }
        }
        self.revision = Revision(self.revision.0 + 1);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PodResource {
    pub id: String,
    pub node_name: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReplicaSetResource {
    pub id: String,
    pub replicas: u32,
}

impl ReplicaSetResource {
    pub fn pods(&self) -> Vec<String> {
        (0..self.replicas)
            .map(|i| format!("{}-{}", self.id, i))
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeploymentResource {
    pub id: String,
    pub replicas: u32,
}

impl DeploymentResource {
    pub fn replicasets(&self) -> Vec<String> {
        vec![self.id.clone()]
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StatefulSetResource {
    pub id: String,
    pub replicas: u32,
}

impl StatefulSetResource {
    pub fn pods(&self) -> Vec<String> {
        (0..self.replicas)
            .map(|i| format!("{}-{}", self.id, i))
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeResource {
    pub running: BTreeSet<String>,
    pub ready: bool,
}

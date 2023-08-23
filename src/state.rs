use std::collections::{BTreeMap, BTreeSet};

use crate::{
    abstract_model::{Change, Operation},
    resources::{
        DeploymentResource, NodeResource, PodResource, ReplicaSetResource, StatefulSetResource,
    },
};

/// Consistency level for viewing the state with.
#[derive(Default, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ConsistencySetup {
    /// Always work off the latest state.
    /// Linearizable reads.
    /// Linearizable writes.
    #[default]
    Strong,
    /// Work off a state that is close to the latest, bounded by the `k`.
    /// Bounded staleness on reads.
    /// Linearizable writes.
    BoundedStaleness(usize),
    /// Work off a state that derives from the last one seen.
    /// Session consistency on reads.
    /// Linearizable writes.
    Session,
    /// Work off any historical state.
    /// Eventually consistent reads.
    /// Linearizable writes.
    Eventual,
    /// Optimistically apply changes without guarantee that they are committed.
    /// Commits automatically happen every `k` changes.
    /// Optimistic reads.
    /// Optimistic writes.
    OptimisticLinear(usize),
    /// Apply changes to a causal graph.
    Causal,
}

pub trait History {
    fn add_change(&mut self, change: Change, from: usize) -> Revision;

    fn max_revision(&self) -> Revision;

    fn state_at(&self, revision: Revision) -> StateView;

    fn valid_revisions(&self, from: usize) -> Vec<Revision>;

    fn states_for(&self, from: usize) -> Vec<StateView> {
        let revisions = self.valid_revisions(from);
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
    fn add_change(&mut self, change: Change, _from: usize) -> Revision {
        let new_revision = self.max_revision().increment();
        self.state.apply_change(&change, new_revision);
        self.max_revision()
    }

    fn max_revision(&self) -> Revision {
        self.state.revision.clone()
    }

    fn state_at(&self, revision: Revision) -> StateView {
        assert_eq!(revision, self.state.revision);
        self.state.clone()
    }

    fn valid_revisions(&self, _from: usize) -> Vec<Revision> {
        vec![self.state.revision.clone()]
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
    fn add_change(&mut self, change: Change, _from: usize) -> Revision {
        let mut state = self.last_k_states.last().unwrap().clone();
        let new_revision = self.max_revision().increment();
        state.apply_change(&change, new_revision);
        if self.last_k_states.len() > self.k {
            self.last_k_states.remove(0);
        }
        self.last_k_states.push(state);
        self.max_revision()
    }

    fn max_revision(&self) -> Revision {
        self.last_k_states.last().unwrap().revision.clone()
    }

    fn state_at(&self, revision: Revision) -> StateView {
        let index = self
            .last_k_states
            .binary_search_by_key(&&revision, |s| &s.revision)
            .unwrap();
        self.last_k_states[index].clone()
    }

    fn valid_revisions(&self, _from: usize) -> Vec<Revision> {
        self.last_k_states
            .iter()
            .map(|s| s.revision.clone())
            .collect()
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct SessionHistory {
    sessions: BTreeMap<usize, Revision>,
    states: Vec<StateView>,
}

impl SessionHistory {
    fn new(initial_state: StateView) -> Self {
        Self {
            sessions: BTreeMap::new(),
            states: vec![initial_state],
        }
    }
}

impl History for SessionHistory {
    fn add_change(&mut self, change: Change, from: usize) -> Revision {
        let mut state = self.states.last().unwrap().clone();
        let new_revision = self.max_revision().increment();
        state.apply_change(&change, new_revision);
        self.states.push(state);
        let max = self.max_revision();
        self.sessions.insert(from, max.clone());

        let min_revision = self.sessions.values().min().unwrap();
        loop {
            let val = &self.states.first().unwrap().revision;
            if val < min_revision {
                self.states.remove(0);
            } else {
                break;
            }
        }

        max
    }

    fn max_revision(&self) -> Revision {
        self.states.last().unwrap().revision.clone()
    }

    fn state_at(&self, revision: Revision) -> StateView {
        let index = self
            .states
            .binary_search_by_key(&&revision, |s| &s.revision)
            .unwrap();
        self.states[index].clone()
    }

    fn valid_revisions(&self, from: usize) -> Vec<Revision> {
        let min_revision = self.sessions.get(&from).cloned().unwrap_or_default();
        self.states
            .iter()
            .filter(|s| s.revision >= min_revision)
            .map(|s| s.revision.clone())
            .collect()
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct EventualHistory {
    states: Vec<StateView>,
}

impl EventualHistory {
    fn new(initial_state: StateView) -> Self {
        Self {
            states: vec![initial_state],
        }
    }
}

impl History for EventualHistory {
    fn add_change(&mut self, change: Change, _from: usize) -> Revision {
        let mut state = self.states.last().unwrap().clone();
        let new_revision = self.max_revision().increment();
        state.apply_change(&change, new_revision);
        self.states.push(state);
        self.max_revision()
    }

    fn max_revision(&self) -> Revision {
        self.states.last().unwrap().revision.clone()
    }

    fn state_at(&self, revision: Revision) -> StateView {
        self.states[revision.0[0]].clone()
    }

    fn valid_revisions(&self, _from: usize) -> Vec<Revision> {
        self.states.iter().map(|s| s.revision.clone()).collect()
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct OptimisticLinearHistory {
    /// First is the last committed state.
    /// Last is the optimistic one.
    /// In between are states that could be committed.
    states: Vec<StateView>,
    commit_every: usize,
}

impl OptimisticLinearHistory {
    fn new(initial_state: StateView, commit_every: usize) -> Self {
        Self {
            states: vec![initial_state],
            commit_every,
        }
    }
}

impl History for OptimisticLinearHistory {
    fn add_change(&mut self, change: Change, _from: usize) -> Revision {
        // find the state for the revision that the change operated on, we'll treat this as the
        // committed one if they didn't operate on the latest (optimistic)
        let index = self
            .states
            .binary_search_by_key(&&change.revision, |s| &s.revision)
            .unwrap();
        let mut state_to_mutate = self.states[index].clone();
        let new_revision = self.max_revision().increment();
        state_to_mutate.apply_change(&change, new_revision);

        if index + 1 == self.states.len() {
            // this was a mutation on the optimistic state
            if self.states.len() > self.commit_every {
                // we have triggered a commit point, the last state is now the committed one
                self.states.clear();
            } else {
                // we haven't reached a guaranteed commit yet, just extend the current states
            }
            self.states.push(state_to_mutate);
        } else {
            // this was a mutation on a committed state (leader changed)
            // Discard all states before and after this one
            let committed_state = self.states.swap_remove(index);
            self.states.clear();
            self.states.push(committed_state);
            self.states.push(state_to_mutate);
        }

        self.max_revision()
    }

    fn max_revision(&self) -> Revision {
        self.states.last().unwrap().revision.clone()
    }

    fn state_at(&self, revision: Revision) -> StateView {
        let index = self
            .states
            .binary_search_by_key(&&revision, |s| &s.revision)
            .unwrap();
        self.states[index].clone()
    }

    fn valid_revisions(&self, _from: usize) -> Vec<Revision> {
        self.states.iter().map(|s| s.revision.clone()).collect()
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct CausalHistory {
    /// Mapping of states and their dependencies.
    states: Vec<CausalState>,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
struct CausalState {
    state: StateView,
    predecessors: Vec<usize>,
    successors: Vec<usize>,
}

impl CausalHistory {
    fn new(initial_state: StateView) -> Self {
        Self {
            states: vec![CausalState {
                state: initial_state,
                predecessors: Vec::new(),
                successors: Vec::new(),
            }],
        }
    }
}

impl History for CausalHistory {
    fn add_change(&mut self, change: Change, _from: usize) -> Revision {
        // TODO: do a more fine-grained merge of the states from the revisions
        // for now just use the highest revision number (last writer wins)
        let target_revision = Revision(vec![*change.revision.0.iter().max().unwrap()]);
        let index = self
            .states
            .binary_search_by_key(&&target_revision, |s| &s.state.revision)
            .unwrap();
        let mut state_to_mutate = self.states[index].state.clone();
        state_to_mutate.apply_change(&change, self.max_revision().increment());

        // find the dependencies of the change
        let mut predecessors = Vec::new();
        let new_index = self.states.len();
        for revision in change.revision.0 {
            let index = self
                .states
                .binary_search_by_key(&&Revision(vec![revision]), |s| &s.state.revision)
                .unwrap();
            predecessors.push(index);
            self.states[index].successors.push(new_index);
        }

        self.states.push(CausalState {
            state: state_to_mutate,
            predecessors,
            successors: Vec::new(),
        });

        self.max_revision()
    }

    fn max_revision(&self) -> Revision {
        self.states.last().unwrap().state.revision.clone()
    }

    fn state_at(&self, revision: Revision) -> StateView {
        // TODO: do a more fine-grained merge of the states from the revisions
        // for now just use the highest revision number (last writer wins)
        let target_revision = Revision(vec![*revision.0.iter().max().unwrap()]);
        let index = self
            .states
            .binary_search_by_key(&&target_revision, |s| &s.state.revision)
            .unwrap();

        let mut s = self.states[index].state.clone();
        s.revision = revision;
        s
    }

    fn valid_revisions(&self, _from: usize) -> Vec<Revision> {
        // all individual revisions are valid to work from
        let base_revisions = self
            .states
            .iter()
            .map(|s| s.state.revision.clone())
            .collect();
        // we can also find combinations of concurrent edits
        // TODO

        base_revisions
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum StateHistory {
    /// Linearizable reads.
    /// Linearizable writes.
    Strong(StrongHistory),
    /// Bounded staleness on reads.
    /// Linearizable writes.
    Bounded(BoundedHistory),
    /// Session consistency on reads.
    /// Linearizable writes.
    Session(SessionHistory),
    /// Eventually consistent reads.
    /// Linearizable writes.
    Eventual(EventualHistory),
    /// Optimistic reads.
    /// Optimistic writes.
    OptimisticLinear(OptimisticLinearHistory),
    Causal(CausalHistory),
}

impl Default for StateHistory {
    fn default() -> Self {
        Self::Strong(StrongHistory::default())
    }
}

impl StateHistory {
    fn new(consistency_level: ConsistencySetup, initial_state: StateView) -> Self {
        match consistency_level {
            ConsistencySetup::Strong => Self::Strong(StrongHistory::new(initial_state)),
            ConsistencySetup::BoundedStaleness(k) => {
                Self::Bounded(BoundedHistory::new(initial_state, k))
            }
            ConsistencySetup::Session => Self::Session(SessionHistory::new(initial_state)),
            ConsistencySetup::Eventual => Self::Eventual(EventualHistory::new(initial_state)),
            ConsistencySetup::OptimisticLinear(commit_every) => {
                Self::OptimisticLinear(OptimisticLinearHistory::new(initial_state, commit_every))
            }
            ConsistencySetup::Causal => Self::Causal(CausalHistory::new(initial_state)),
        }
    }

    fn add_change(&mut self, change: Change, from: usize) -> Revision {
        match self {
            StateHistory::Strong(s) => s.add_change(change, from),
            StateHistory::Bounded(s) => s.add_change(change, from),
            StateHistory::Session(s) => s.add_change(change, from),
            StateHistory::Eventual(s) => s.add_change(change, from),
            StateHistory::OptimisticLinear(s) => s.add_change(change, from),
            StateHistory::Causal(s) => s.add_change(change, from),
        }
    }

    fn max_revision(&self) -> Revision {
        match self {
            StateHistory::Strong(s) => s.max_revision(),
            StateHistory::Bounded(s) => s.max_revision(),
            StateHistory::Session(s) => s.max_revision(),
            StateHistory::Eventual(s) => s.max_revision(),
            StateHistory::OptimisticLinear(s) => s.max_revision(),
            StateHistory::Causal(s) => s.max_revision(),
        }
    }

    fn state_at(&self, revision: Revision) -> StateView {
        match self {
            StateHistory::Strong(s) => s.state_at(revision),
            StateHistory::Bounded(s) => s.state_at(revision),
            StateHistory::Session(s) => s.state_at(revision),
            StateHistory::Eventual(s) => s.state_at(revision),
            StateHistory::OptimisticLinear(s) => s.state_at(revision),
            StateHistory::Causal(s) => s.state_at(revision),
        }
    }

    fn states_for(&self, from: usize) -> Vec<StateView> {
        match self {
            StateHistory::Strong(s) => s.states_for(from),
            StateHistory::Bounded(s) => s.states_for(from),
            StateHistory::Session(s) => s.states_for(from),
            StateHistory::Eventual(s) => s.states_for(from),
            StateHistory::OptimisticLinear(s) => s.states_for(from),
            StateHistory::Causal(s) => s.states_for(from),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Revision(Vec<usize>);

impl Default for Revision {
    fn default() -> Self {
        Self(vec![0])
    }
}

impl Revision {
    fn increment(mut self) -> Self {
        assert_eq!(self.0.len(), 1);
        self.0[0] += 1;
        self
    }
}

/// The history of the state, enabling generating views for different historical versions.
#[derive(Default, Debug, Clone, PartialEq, Eq, Hash)]
pub struct State {
    /// The changes that have been made to the state.
    states: StateHistory,
}

impl State {
    pub fn new(initial_state: StateView, consistency_level: ConsistencySetup) -> Self {
        Self {
            states: StateHistory::new(consistency_level, initial_state),
        }
    }

    /// Record a change for this state from a given controller.
    pub fn push_change(&mut self, change: Change, from: usize) -> Revision {
        self.states.add_change(change, from)
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
        self.states.max_revision()
    }

    /// Get a view for a specific revision in the change history.
    pub fn view_at(&self, revision: Revision) -> StateView {
        self.states.state_at(revision)
    }

    /// Get all the possible views under the given consistency level.
    pub fn views(&self, from: usize) -> Vec<StateView> {
        self.states.states_for(from)
    }
}

#[derive(derivative::Derivative)]
#[derivative(PartialEq, Hash)]
#[derive(Default, Clone, Debug, Eq, PartialOrd, Ord)]
pub struct StateView {
    // Ignore the revision field as we just care whether the rest of the state is the same.
    #[derivative(PartialEq = "ignore", Hash = "ignore")]
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

    pub fn apply_change(&mut self, change: &Change, new_revision: Revision) {
        match &change.operation {
            Operation::NodeJoin(i, capacity) => {
                self.nodes.insert(
                    *i,
                    NodeResource {
                        running: BTreeSet::new(),
                        ready: true,
                        capacity: capacity.clone(),
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
                        resources: None,
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
        self.revision = new_revision;
    }
}

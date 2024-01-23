use std::sync::Arc;

use crate::abstract_model::Change;

use super::{revision::Revision, RawState, StateView};

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

    fn reset_session(&mut self, from: usize);

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
    state: Arc<StateView>,
}

impl StrongHistory {
    fn new(initial_state: RawState) -> Self {
        Self {
            state: Arc::new(initial_state.into()),
        }
    }
}

impl History for StrongHistory {
    fn add_change(&mut self, change: Change, _from: usize) -> Revision {
        let new_revision = self.max_revision().increment();
        Arc::make_mut(&mut self.state).apply_change(&change, new_revision);
        self.max_revision()
    }

    fn reset_session(&mut self, _from: usize) {
        // nothing to do
    }

    fn max_revision(&self) -> Revision {
        self.state.revision.clone()
    }

    fn state_at(&self, revision: Revision) -> StateView {
        assert_eq!(revision, self.state.revision);
        (*self.state).clone()
    }

    fn valid_revisions(&self, _from: usize) -> Vec<Revision> {
        vec![self.state.revision.clone()]
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct BoundedHistory {
    k: usize,
    last_k_states: Vec<Arc<StateView>>,
}

impl BoundedHistory {
    fn new(initial_state: RawState, k: usize) -> Self {
        Self {
            k,
            last_k_states: vec![Arc::new(initial_state.into())],
        }
    }
}

impl History for BoundedHistory {
    fn add_change(&mut self, change: Change, _from: usize) -> Revision {
        let mut new_state_ref = Arc::clone(self.last_k_states.last().unwrap());
        let new_state = Arc::make_mut(&mut new_state_ref);
        let new_revision = self.max_revision().increment();
        new_state.apply_change(&change, new_revision);
        if self.last_k_states.len() > self.k {
            self.last_k_states.remove(0);
        }
        self.last_k_states.push(new_state_ref);
        self.max_revision()
    }

    fn reset_session(&mut self, _from: usize) {
        // nothing to do
    }

    fn max_revision(&self) -> Revision {
        self.last_k_states.last().unwrap().revision.clone()
    }

    fn state_at(&self, revision: Revision) -> StateView {
        let index = self
            .last_k_states
            .binary_search_by_key(&&revision, |s| &s.revision)
            .unwrap();
        (*self.last_k_states[index]).clone()
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
    sessions: imbl::OrdMap<usize, Revision>,
    states: imbl::Vector<Arc<StateView>>,
}

impl SessionHistory {
    fn new(initial_state: RawState) -> Self {
        Self {
            sessions: imbl::OrdMap::new(),
            states: imbl::vector![Arc::new(initial_state.into())],
        }
    }
}

impl History for SessionHistory {
    fn add_change(&mut self, change: Change, from: usize) -> Revision {
        let mut new_state_ref = self.states.last().unwrap().clone();
        let new_state = Arc::make_mut(&mut new_state_ref);
        let new_revision = self.max_revision().increment();
        new_state.apply_change(&change, new_revision);
        self.states.push_back(new_state_ref);
        let max = self.max_revision();
        self.sessions.insert(from, max.clone());

        // let min_revision = self.sessions.values().min().unwrap();
        // loop {
        //     let val = &self.states.first().unwrap().revision;
        //     if val < min_revision {
        //         self.states.remove(0);
        //     } else {
        //         break;
        //     }
        // }

        max
    }

    fn reset_session(&mut self, from: usize) {
        self.sessions.remove(&from);
    }

    fn max_revision(&self) -> Revision {
        self.states.last().unwrap().revision.clone()
    }

    fn state_at(&self, revision: Revision) -> StateView {
        let index = self
            .states
            .binary_search_by_key(&revision, |s| s.revision.clone())
            .unwrap();
        (*self.states[index]).clone()
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
    states: Vec<Arc<StateView>>,
}

impl EventualHistory {
    fn new(initial_state: RawState) -> Self {
        Self {
            states: vec![Arc::new(initial_state.into())],
        }
    }
}

impl History for EventualHistory {
    fn add_change(&mut self, change: Change, _from: usize) -> Revision {
        let mut new_state_ref = Arc::clone(self.states.last().unwrap());
        let new_state = Arc::make_mut(&mut new_state_ref);
        let new_revision = self.max_revision().increment();
        new_state.apply_change(&change, new_revision);
        self.states.push(new_state_ref);
        self.max_revision()
    }
    fn reset_session(&mut self, _from: usize) {
        // nothing to do
    }

    fn max_revision(&self) -> Revision {
        self.states.last().unwrap().revision.clone()
    }

    fn state_at(&self, revision: Revision) -> StateView {
        (*self.states[revision.components()[0]]).clone()
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
    states: Vec<Arc<StateView>>,
    commit_every: usize,
}

impl OptimisticLinearHistory {
    fn new(initial_state: RawState, commit_every: usize) -> Self {
        Self {
            states: vec![Arc::new(initial_state.into())],
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
        let mut new_state_ref = Arc::clone(&self.states[index]);
        let new_state = Arc::make_mut(&mut new_state_ref);
        let new_revision = self.max_revision().increment();
        new_state.apply_change(&change, new_revision);

        if index + 1 == self.states.len() {
            // this was a mutation on the optimistic state
            if self.states.len() > self.commit_every {
                // we have triggered a commit point, the last state is now the committed one
                self.states.clear();
            } else {
                // we haven't reached a guaranteed commit yet, just extend the current states
            }
            self.states.push(new_state_ref);
        } else {
            // this was a mutation on a committed state (leader changed)
            // Discard all states before and after this one
            let committed_state = self.states.swap_remove(index);
            self.states.clear();
            self.states.push(committed_state);
            self.states.push(new_state_ref);
        }

        self.max_revision()
    }
    fn reset_session(&mut self, _from: usize) {
        // nothing to do
    }

    fn max_revision(&self) -> Revision {
        self.states.last().unwrap().revision.clone()
    }

    fn state_at(&self, revision: Revision) -> StateView {
        let index = self
            .states
            .binary_search_by_key(&&revision, |s| &s.revision)
            .unwrap();
        (*self.states[index]).clone()
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
    state: Arc<StateView>,
    predecessors: Vec<usize>,
    successors: Vec<usize>,
}

impl CausalHistory {
    fn new(initial_state: RawState) -> Self {
        Self {
            states: vec![CausalState {
                state: Arc::new(initial_state.into()),
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
        let target_revision =
            Revision::from(vec![*change.revision.components().iter().max().unwrap()]);
        let index = self
            .states
            .binary_search_by_key(&&target_revision, |s| &s.state.revision)
            .unwrap();
        let mut new_state_ref = Arc::clone(&self.states[index].state);
        let new_state = Arc::make_mut(&mut new_state_ref);
        new_state.apply_change(&change, self.max_revision().increment());

        // find the dependencies of the change
        let mut predecessors = Vec::new();
        let new_index = self.states.len();
        for revision in change.revision.components() {
            let index = self
                .states
                .binary_search_by_key(&&Revision::from(vec![*revision]), |s| &s.state.revision)
                .unwrap();
            predecessors.push(index);
            self.states[index].successors.push(new_index);
        }

        self.states.push(CausalState {
            state: new_state_ref,
            predecessors,
            successors: Vec::new(),
        });

        self.max_revision()
    }
    fn reset_session(&mut self, _from: usize) {
        // nothing to do
    }

    fn max_revision(&self) -> Revision {
        self.states.last().unwrap().state.revision.clone()
    }

    fn state_at(&self, revision: Revision) -> StateView {
        // TODO: do a more fine-grained merge of the states from the revisions
        // for now just use the highest revision number (last writer wins)
        let target_revision = Revision::from(vec![*revision.components().iter().max().unwrap()]);
        let index = self
            .states
            .binary_search_by_key(&&target_revision, |s| &s.state.revision)
            .unwrap();

        let mut s = (*self.states[index].state).clone();
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
    pub fn new(consistency_level: ConsistencySetup, initial_state: RawState) -> Self {
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

    pub fn add_change(&mut self, change: Change, from: usize) -> Revision {
        match self {
            StateHistory::Strong(s) => s.add_change(change, from),
            StateHistory::Bounded(s) => s.add_change(change, from),
            StateHistory::Session(s) => s.add_change(change, from),
            StateHistory::Eventual(s) => s.add_change(change, from),
            StateHistory::OptimisticLinear(s) => s.add_change(change, from),
            StateHistory::Causal(s) => s.add_change(change, from),
        }
    }

    pub fn reset_session(&mut self, from: usize) {
        match self {
            StateHistory::Strong(s) => s.reset_session(from),
            StateHistory::Bounded(s) => s.reset_session(from),
            StateHistory::Session(s) => s.reset_session(from),
            StateHistory::Eventual(s) => s.reset_session(from),
            StateHistory::OptimisticLinear(s) => s.reset_session(from),
            StateHistory::Causal(s) => s.reset_session(from),
        }
    }

    pub fn max_revision(&self) -> Revision {
        match self {
            StateHistory::Strong(s) => s.max_revision(),
            StateHistory::Bounded(s) => s.max_revision(),
            StateHistory::Session(s) => s.max_revision(),
            StateHistory::Eventual(s) => s.max_revision(),
            StateHistory::OptimisticLinear(s) => s.max_revision(),
            StateHistory::Causal(s) => s.max_revision(),
        }
    }

    pub fn state_at(&self, revision: Revision) -> StateView {
        match self {
            StateHistory::Strong(s) => s.state_at(revision),
            StateHistory::Bounded(s) => s.state_at(revision),
            StateHistory::Session(s) => s.state_at(revision),
            StateHistory::Eventual(s) => s.state_at(revision),
            StateHistory::OptimisticLinear(s) => s.state_at(revision),
            StateHistory::Causal(s) => s.state_at(revision),
        }
    }

    pub fn states_for(&self, from: usize) -> Vec<StateView> {
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

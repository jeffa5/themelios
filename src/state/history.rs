use crate::abstract_model::Change;

use self::{
    causal::CausalHistory, eventual::EventualHistory, linearizable::LinearizableHistory,
    monotonic_session::MonotonicSessionHistory, optimistic::OptimisticLinearHistory,
    session::SessionHistory,
};

use super::{revision::Revision, RawState, StateView};

pub mod causal;
pub mod eventual;
pub mod linearizable;
pub mod monotonic_session;
pub mod optimistic;
pub mod session;

/// Consistency level for viewing the state with.
#[derive(Default, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ConsistencySetup {
    /// Always work off the latest state.
    /// Linearizable reads.
    /// Linearizable writes.
    #[default]
    Linearizable,
    /// Work off a state that derives from the last one seen, defaulting to the latest when no
    /// session is present.
    /// Session consistency on reads.
    /// Linearizable writes.
    MonotonicSession,
    /// Work off a state that derives from the last one seen, defaulting to any valid when no session
    /// is present.
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
    fn add_change(&mut self, change: Change) -> Revision;

    fn max_revision(&self) -> Revision;

    fn state_at(&self, revision: Revision) -> StateView;

    fn valid_revisions(&self, min_revision: Revision) -> Vec<Revision>;

    fn states_for(&self, min_revision: Revision) -> Vec<StateView> {
        let revisions = self.valid_revisions(min_revision);
        revisions.into_iter().map(|r| self.state_at(r)).collect()
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum StateHistory {
    /// Linearizable reads.
    /// Linearizable writes.
    Linearizable(LinearizableHistory),
    /// Session consistency on reads.
    /// Linearizable writes.
    MonotonicSession(MonotonicSessionHistory),
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
        Self::Linearizable(LinearizableHistory::default())
    }
}

impl StateHistory {
    pub fn new(consistency_level: ConsistencySetup, initial_state: RawState) -> Self {
        match consistency_level {
            ConsistencySetup::Linearizable => {
                Self::Linearizable(LinearizableHistory::new(initial_state))
            }
            ConsistencySetup::MonotonicSession => {
                Self::MonotonicSession(MonotonicSessionHistory::new(initial_state))
            }
            ConsistencySetup::Session => Self::Session(SessionHistory::new(initial_state)),
            ConsistencySetup::Eventual => Self::Eventual(EventualHistory::new(initial_state)),
            ConsistencySetup::OptimisticLinear(commit_every) => {
                Self::OptimisticLinear(OptimisticLinearHistory::new(initial_state, commit_every))
            }
            ConsistencySetup::Causal => Self::Causal(CausalHistory::new(initial_state)),
        }
    }

    pub fn add_change(&mut self, change: Change) -> Revision {
        match self {
            StateHistory::Linearizable(s) => s.add_change(change),
            StateHistory::MonotonicSession(s) => s.add_change(change),
            StateHistory::Session(s) => s.add_change(change),
            StateHistory::Eventual(s) => s.add_change(change),
            StateHistory::OptimisticLinear(s) => s.add_change(change),
            StateHistory::Causal(s) => s.add_change(change),
        }
    }

    pub fn max_revision(&self) -> Revision {
        match self {
            StateHistory::Linearizable(s) => s.max_revision(),
            StateHistory::MonotonicSession(s) => s.max_revision(),
            StateHistory::Session(s) => s.max_revision(),
            StateHistory::Eventual(s) => s.max_revision(),
            StateHistory::OptimisticLinear(s) => s.max_revision(),
            StateHistory::Causal(s) => s.max_revision(),
        }
    }

    pub fn state_at(&self, revision: Revision) -> StateView {
        match self {
            StateHistory::Linearizable(s) => s.state_at(revision),
            StateHistory::MonotonicSession(s) => s.state_at(revision),
            StateHistory::Session(s) => s.state_at(revision),
            StateHistory::Eventual(s) => s.state_at(revision),
            StateHistory::OptimisticLinear(s) => s.state_at(revision),
            StateHistory::Causal(s) => s.state_at(revision),
        }
    }

    pub fn states_for(&self, min_revision: Revision) -> Vec<StateView> {
        match self {
            StateHistory::Linearizable(s) => s.states_for(min_revision),
            StateHistory::MonotonicSession(s) => s.states_for(min_revision),
            StateHistory::Session(s) => s.states_for(min_revision),
            StateHistory::Eventual(s) => s.states_for(min_revision),
            StateHistory::OptimisticLinear(s) => s.states_for(min_revision),
            StateHistory::Causal(s) => s.states_for(min_revision),
        }
    }
}

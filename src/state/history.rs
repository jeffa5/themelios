use std::{borrow::Cow, fmt::Display};

use crate::abstract_model::Change;

use self::{
    causal::CausalHistory, linearizable::LinearizableHistory,
    monotonic_session::MonotonicSessionHistory, optimistic::OptimisticLinearHistory,
    resettable_session::ResettableSessionHistory,
};

use super::{revision::Revision, RawState, StateView};

pub mod causal;
pub mod linearizable;
pub mod monotonic_session;
pub mod optimistic;
pub mod resettable_session;

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
    ResettableSession,
    /// Optimistically apply changes without guarantee that they are committed.
    /// Optimistic reads.
    /// Optimistic writes.
    OptimisticLinear,
    /// Apply changes to a causal graph.
    Causal,
}

impl Display for ConsistencySetup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                ConsistencySetup::Linearizable => "linearizable",
                ConsistencySetup::MonotonicSession => "monotonic-session",
                ConsistencySetup::ResettableSession => "resettable-session",
                ConsistencySetup::OptimisticLinear => "optimistic-linear",
                ConsistencySetup::Causal => "causal",
            }
        )
    }
}

pub trait History {
    fn add_change(&mut self, change: Change);

    fn max_revision(&self) -> Revision;

    fn state_at(&self, revision: &Revision) -> Cow<'_, StateView>;

    fn valid_revisions(&self, min_revision: Option<&Revision>) -> Vec<Revision>;
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
    ResettableSession(ResettableSessionHistory),
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
            ConsistencySetup::ResettableSession => {
                Self::ResettableSession(ResettableSessionHistory::new(initial_state))
            }
            ConsistencySetup::OptimisticLinear => {
                Self::OptimisticLinear(OptimisticLinearHistory::new(initial_state))
            }
            ConsistencySetup::Causal => Self::Causal(CausalHistory::new(initial_state)),
        }
    }
}

impl History for StateHistory {
    fn add_change(&mut self, change: Change) {
        match self {
            StateHistory::Linearizable(s) => s.add_change(change),
            StateHistory::MonotonicSession(s) => s.add_change(change),
            StateHistory::ResettableSession(s) => s.add_change(change),
            StateHistory::OptimisticLinear(s) => s.add_change(change),
            StateHistory::Causal(s) => s.add_change(change),
        }
    }

    fn max_revision(&self) -> Revision {
        match self {
            StateHistory::Linearizable(s) => s.max_revision(),
            StateHistory::MonotonicSession(s) => s.max_revision(),
            StateHistory::ResettableSession(s) => s.max_revision(),
            StateHistory::OptimisticLinear(s) => s.max_revision(),
            StateHistory::Causal(s) => s.max_revision(),
        }
    }

    fn state_at(&self, revision: &Revision) -> Cow<StateView> {
        match self {
            StateHistory::Linearizable(s) => s.state_at(revision),
            StateHistory::MonotonicSession(s) => s.state_at(revision),
            StateHistory::ResettableSession(s) => s.state_at(revision),
            StateHistory::OptimisticLinear(s) => s.state_at(revision),
            StateHistory::Causal(s) => s.state_at(revision),
        }
    }

    fn valid_revisions(&self, min_revision: Option<&Revision>) -> Vec<Revision> {
        match self {
            StateHistory::Linearizable(s) => s.valid_revisions(min_revision),
            StateHistory::MonotonicSession(s) => s.valid_revisions(min_revision),
            StateHistory::ResettableSession(s) => s.valid_revisions(min_revision),
            StateHistory::OptimisticLinear(s) => s.valid_revisions(min_revision),
            StateHistory::Causal(s) => s.valid_revisions(min_revision),
        }
    }
}

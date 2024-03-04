use std::{borrow::Cow, sync::Arc};

use crate::{
    abstract_model::Change,
    state::{revision::Revision, RawState, StateView},
};

use super::History;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct OptimisticLinearHistory {
    /// First is the last committed state.
    /// Last is the optimistic one.
    /// In between are states that could be committed.
    states: Vec<Arc<StateView>>,
    commit_every: usize,
}

impl OptimisticLinearHistory {
    pub fn new(initial_state: RawState, commit_every: usize) -> Self {
        Self {
            states: vec![Arc::new(initial_state.into())],
            commit_every,
        }
    }
}

impl History for OptimisticLinearHistory {
    fn add_change(&mut self, change: Change) {
        // find the state for the revision that the change operated on, we'll treat this as the
        // committed one if they didn't operate on the latest (optimistic)
        let index = self
            .states
            .binary_search_by_key(&&change.revision, |s| &s.revision)
            .unwrap();
        let mut new_state = (*self.states[index]).clone();
        let new_revision = self.max_revision().increment();
        if new_state.apply_operation(change.operation, new_revision) {
            if index + 1 == self.states.len() {
                // this was a mutation on the optimistic state
                if self.states.len() > self.commit_every {
                    // we have triggered a commit point, the last state is now the committed one
                    self.states.clear();
                } else {
                    // we haven't reached a guaranteed commit yet, just extend the current states
                }
                self.states.push(Arc::new(new_state));
            } else {
                // this was a mutation on a committed state (leader changed)
                // Discard all states before and after this one
                let committed_state = self.states.swap_remove(index);
                self.states.clear();
                self.states.push(committed_state);
                self.states.push(Arc::new(new_state));
            }
        }
    }

    fn max_revision(&self) -> Revision {
        self.states.last().unwrap().revision.clone()
    }

    fn state_at(&self, revision: &Revision) -> Cow<StateView> {
        let index = self
            .states
            .binary_search_by_key(&revision, |s| &s.revision)
            .unwrap();
        Cow::Borrowed(&self.states[index])
    }

    fn valid_revisions(&self, _min_revision: Option<&Revision>) -> Vec<Revision> {
        self.states.iter().map(|s| s.revision.clone()).collect()
    }
}

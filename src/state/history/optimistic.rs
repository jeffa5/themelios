use std::{borrow::Cow, sync::Arc};

use crate::{
    abstract_model::Change,
    state::{revision::Revision, RawState, StateView},
};

use super::{History, StatesVec};

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct OptimisticLinearHistory {
    /// First is the last committed state.
    /// Last is the optimistic one.
    /// In between are states that could be committed.
    states: StatesVec<HistoryState>,
    committed: usize,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct HistoryState {
    state: StateView,
    parent: usize,
}

impl OptimisticLinearHistory {
    pub fn new(initial_state: RawState) -> Self {
        Self {
            states: StatesVec(imbl::vector![Arc::new(HistoryState {
                state: initial_state.into(),
                parent: 0,
            })]),
            committed: 0,
        }
    }
}

impl History for OptimisticLinearHistory {
    fn add_change(&mut self, change: Change) {
        // find the state for the revision that the change operated on, we'll treat this as the
        // committed one if they didn't operate on the latest (optimistic)
        let index = change.revision.components().first().unwrap();
        let mut new_state = self.states[*index].state.clone();
        let new_revision = self.max_revision().increment();
        if new_state.apply_operation(change.operation, new_revision) {
            self.states.push_back(Arc::new(HistoryState {
                state: new_state,
                parent: *index,
            }));
        }
    }

    fn max_revision(&self) -> Revision {
        self.states.last().unwrap().state.revision.clone()
    }

    fn state_at(&self, revision: &Revision) -> Cow<StateView> {
        let index = revision.components().first().unwrap();
        Cow::Borrowed(&self.states[*index].state)
    }

    fn valid_revisions(&self, min_revision: Option<&Revision>) -> Vec<Revision> {
        if let Some(min_revision) = min_revision {
            let index = min_revision.components().first().unwrap();
            let mut revisions = Vec::new();
            let mut sindex = self.states.len() - 1;
            // iteratively build up the revisions from the latest, following the parent pointers
            // until we are past the session revision, or past the last committed one.
            loop {
                if sindex <= *index || sindex < self.committed {
                    break;
                }
                let state = &self.states[sindex];
                sindex = state.parent;
                revisions.push(state.state.revision.clone());
            }
            revisions
        } else {
            // for a new requester who doesn't have a session we give them the latest (a quorum
            // read sort of thing)
            vec![self.max_revision()]
        }
    }
}

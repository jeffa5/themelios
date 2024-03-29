use std::{borrow::Cow, sync::Arc};

use crate::{
    abstract_model::Change,
    state::{revision::Revision, RawState, StateView},
};

use super::{History, StatesVec};

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct MonotonicSessionHistory {
    states: StatesVec,
}

impl MonotonicSessionHistory {
    pub fn new(initial_state: RawState) -> Self {
        Self {
            states: StatesVec(imbl::vector![Arc::new(initial_state.into())]),
        }
    }
}

impl History for MonotonicSessionHistory {
    fn add_change(&mut self, change: Change) {
        let mut new_state = (**self.states.last().unwrap()).clone();
        let new_revision = self.max_revision().increment();
        if new_state.apply_operation(change.operation, new_revision) {
            // operation succeeded, add the new state to the list of states
            self.states.push_back(Arc::new(new_state));
        } else {
            // operation did not succeed, however client state may have changed so just return the
            // max revision still
        }
    }

    fn max_revision(&self) -> Revision {
        self.states.last().unwrap().revision.clone()
    }

    fn state_at(&self, revision: &Revision) -> Cow<StateView> {
        let index = revision.components().first().unwrap();
        Cow::Borrowed(&self.states[*index])
    }

    fn valid_revisions(&self, min_revision: Option<&Revision>) -> Vec<Revision> {
        if let Some(min_revision) = min_revision {
            let index = min_revision.components().first().unwrap();
            self.states
                .iter()
                .skip(*index + 1)
                .map(|s| s.revision.clone())
                .collect()
        } else {
            // for a new requester who doesn't have a session we give them the latest (a quorum
            // read sort of thing)
            vec![self.max_revision()]
        }
    }
}

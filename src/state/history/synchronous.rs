use std::{borrow::Cow, sync::Arc};

use crate::{
    abstract_model::Change,
    state::{revision::Revision, RawState, StateView},
};

use super::{History, StatesVec};

#[derive(Default, Clone, PartialEq, Eq, Hash, Debug)]
pub struct SynchronousHistory {
    states: StatesVec,
}

impl SynchronousHistory {
    pub fn new(initial_state: RawState) -> Self {
        Self {
            states: StatesVec(imbl::vector![Arc::new(initial_state.into())]),
        }
    }
}

impl History for SynchronousHistory {
    fn add_change(&mut self, change: Change) {
        let mut new_state = (**self.states.last().unwrap()).clone();
        let new_revision = self.max_revision().increment();
        if new_state.apply_operation(change.operation, new_revision) {
            self.states.push_back(Arc::new(new_state));
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
        let max = self.max_revision();
        if let Some(min_revision) = min_revision {
            if &max > min_revision {
                // there has been changes the client has not observed
                vec![max]
            } else {
                // they have already observed the latest state
                Vec::new()
            }
        } else {
            // they might not have seen anything yet
            vec![max]
        }
    }
}

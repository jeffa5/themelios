use std::{sync::Arc, borrow::Cow};

use crate::{
    abstract_model::Change,
    state::{revision::Revision, RawState, StateView},
};

use super::History;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ResettableSessionHistory {
    states: imbl::Vector<Arc<StateView>>,
}

impl ResettableSessionHistory {
    pub fn new(initial_state: RawState) -> Self {
        Self {
            states: imbl::vector![Arc::new(initial_state.into())],
        }
    }
}

impl History for ResettableSessionHistory {
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
        if let Some(min_revision) = min_revision {
            let index = min_revision.components().first().unwrap();
            self.states
                .iter()
                .skip(*index)
                .map(|s| s.revision.clone())
                .collect()
        } else {
            self.states.iter().map(|s| s.revision.clone()).collect()
        }
    }
}

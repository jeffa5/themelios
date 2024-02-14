use std::sync::Arc;

use crate::{
    abstract_model::Change,
    state::{revision::Revision, RawState, StateView},
};

use super::History;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct SessionHistory {
    states: imbl::Vector<Arc<StateView>>,
}

impl SessionHistory {
    pub fn new(initial_state: RawState) -> Self {
        Self {
            states: imbl::vector![Arc::new(initial_state.into())],
        }
    }
}

impl History for SessionHistory {
    fn add_change(&mut self, change: Change) -> Revision {
        let mut new_state_ref = self.states.last().unwrap().clone();
        let new_state = Arc::make_mut(&mut new_state_ref);
        let new_revision = self.max_revision().increment();
        new_state.apply_operation(change.operation, new_revision);
        self.states.push_back(new_state_ref);
        let max = self.max_revision();
        max
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

    fn valid_revisions(&self, min_revision: Revision) -> Vec<Revision> {
        self.states
            .iter()
            .filter(|s| s.revision >= min_revision)
            .map(|s| s.revision.clone())
            .collect()
    }
}

use std::sync::Arc;

use crate::{
    abstract_model::Change,
    state::{revision::Revision, RawState, StateView},
};

use super::History;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct EventualHistory {
    states: Vec<Arc<StateView>>,
}

impl EventualHistory {
    pub fn new(initial_state: RawState) -> Self {
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

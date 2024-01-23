use std::sync::Arc;

use crate::{
    abstract_model::Change,
    state::{revision::Revision, RawState, StateView},
};

use super::History;

#[derive(Default, Clone, PartialEq, Eq, Hash, Debug)]
pub struct StrongHistory {
    state: Arc<StateView>,
}

impl StrongHistory {
    pub fn new(initial_state: RawState) -> Self {
        Self {
            state: Arc::new(initial_state.into()),
        }
    }
}

impl History for StrongHistory {
    fn add_change(&mut self, change: Change, _from: usize) -> Revision {
        let new_revision = self.max_revision().increment();
        Arc::make_mut(&mut self.state).apply_operation(change.operation, new_revision);
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

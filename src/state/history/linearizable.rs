use std::sync::Arc;

use crate::{
    abstract_model::Change,
    state::{revision::Revision, RawState, StateView},
};

use super::History;

#[derive(Default, Clone, PartialEq, Eq, Hash, Debug)]
pub struct LinearizableHistory {
    states: imbl::Vector<Arc<StateView>>,
}

impl LinearizableHistory {
    pub fn new(initial_state: RawState) -> Self {
        Self {
            states: imbl::vector![Arc::new(initial_state.into())],
        }
    }
}

impl History for LinearizableHistory {
    fn add_change(&mut self, change: Change) -> Revision {
        let mut new_state_ref = self.states.last().unwrap().clone();
        let new_state = Arc::make_mut(&mut new_state_ref);
        let new_revision = self.max_revision().increment();
        new_state.apply_operation(change.operation, new_revision);
        self.states.push_back(new_state_ref);
        self.max_revision()
    }

    fn max_revision(&self) -> Revision {
        self.states.last().unwrap().revision.clone()
    }

    fn state_at(&self, revision: Revision) -> StateView {
        let index = revision.components().first().unwrap();
        (*self.states[*index]).clone()
    }

    fn valid_revisions(&self, _min_revision: Revision) -> Vec<Revision> {
        vec![self.max_revision()]
    }
}

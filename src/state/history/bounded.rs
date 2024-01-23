use std::sync::Arc;

use crate::{
    abstract_model::Change,
    state::{revision::Revision, RawState, StateView},
};

use super::History;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct BoundedHistory {
    k: usize,
    last_k_states: Vec<Arc<StateView>>,
}

impl BoundedHistory {
    pub fn new(initial_state: RawState, k: usize) -> Self {
        Self {
            k,
            last_k_states: vec![Arc::new(initial_state.into())],
        }
    }
}

impl History for BoundedHistory {
    fn add_change(&mut self, change: Change, _from: usize) -> Revision {
        let mut new_state_ref = Arc::clone(self.last_k_states.last().unwrap());
        let new_state = Arc::make_mut(&mut new_state_ref);
        let new_revision = self.max_revision().increment();
        new_state.apply_change(&change, new_revision);
        if self.last_k_states.len() > self.k {
            self.last_k_states.remove(0);
        }
        self.last_k_states.push(new_state_ref);
        self.max_revision()
    }

    fn reset_session(&mut self, _from: usize) {
        // nothing to do
    }

    fn max_revision(&self) -> Revision {
        self.last_k_states.last().unwrap().revision.clone()
    }

    fn state_at(&self, revision: Revision) -> StateView {
        let index = self
            .last_k_states
            .binary_search_by_key(&&revision, |s| &s.revision)
            .unwrap();
        (*self.last_k_states[index]).clone()
    }

    fn valid_revisions(&self, _from: usize) -> Vec<Revision> {
        self.last_k_states
            .iter()
            .map(|s| s.revision.clone())
            .collect()
    }
}

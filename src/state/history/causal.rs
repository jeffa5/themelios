use std::sync::Arc;

use crate::{
    abstract_model::Change,
    state::{revision::Revision, RawState, StateView},
};

use super::History;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct CausalHistory {
    /// Mapping of states and their dependencies.
    states: Vec<CausalState>,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
struct CausalState {
    state: Arc<StateView>,
    predecessors: Vec<usize>,
    successors: Vec<usize>,
}

impl CausalHistory {
    pub fn new(initial_state: RawState) -> Self {
        Self {
            states: vec![CausalState {
                state: Arc::new(initial_state.into()),
                predecessors: Vec::new(),
                successors: Vec::new(),
            }],
        }
    }
}

impl History for CausalHistory {
    fn add_change(&mut self, change: Change, _from: usize) -> Revision {
        // TODO: do a more fine-grained merge of the states from the revisions
        // for now just use the highest revision number (last writer wins)
        let target_revision =
            Revision::from(vec![*change.revision.components().iter().max().unwrap()]);
        let index = self
            .states
            .binary_search_by_key(&&target_revision, |s| &s.state.revision)
            .unwrap();
        let mut new_state_ref = Arc::clone(&self.states[index].state);
        let new_state = Arc::make_mut(&mut new_state_ref);
        new_state.apply_change(&change, self.max_revision().increment());

        // find the dependencies of the change
        let mut predecessors = Vec::new();
        let new_index = self.states.len();
        for revision in change.revision.components() {
            let index = self
                .states
                .binary_search_by_key(&&Revision::from(vec![*revision]), |s| &s.state.revision)
                .unwrap();
            predecessors.push(index);
            self.states[index].successors.push(new_index);
        }

        self.states.push(CausalState {
            state: new_state_ref,
            predecessors,
            successors: Vec::new(),
        });

        self.max_revision()
    }
    fn reset_session(&mut self, _from: usize) {
        // nothing to do
    }

    fn max_revision(&self) -> Revision {
        self.states.last().unwrap().state.revision.clone()
    }

    fn state_at(&self, revision: Revision) -> StateView {
        // TODO: do a more fine-grained merge of the states from the revisions
        // for now just use the highest revision number (last writer wins)
        let target_revision = Revision::from(vec![*revision.components().iter().max().unwrap()]);
        let index = self
            .states
            .binary_search_by_key(&&target_revision, |s| &s.state.revision)
            .unwrap();

        let mut s = (*self.states[index].state).clone();
        s.revision = revision;
        s
    }

    fn valid_revisions(&self, _from: usize) -> Vec<Revision> {
        // all individual revisions are valid to work from
        let base_revisions = self
            .states
            .iter()
            .map(|s| s.state.revision.clone())
            .collect();
        // we can also find combinations of concurrent edits
        // TODO

        base_revisions
    }
}

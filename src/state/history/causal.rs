use std::{collections::BTreeSet, sync::Arc};

use itertools::Itertools;

use crate::{
    abstract_model::Change,
    state::{revision::Revision, RawState, StateView},
};

use super::History;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct CausalHistory {
    /// Mapping of states and their dependencies.
    states: imbl::Vector<CausalState>,
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
            states: imbl::vector![CausalState {
                state: Arc::new(initial_state.into()),
                predecessors: Vec::new(),
                successors: Vec::new(),
            }],
        }
    }
}

impl History for CausalHistory {
    fn add_change(&mut self, change: Change) -> Revision {
        let mut new_state = self.state_at(change.revision.clone());

        new_state.apply_operation(change.operation, self.max_revision().increment());

        // find the dependencies of the change
        let mut predecessors = Vec::new();
        let new_index = self.states.len();
        for index in self.indices_for_revision(&change.revision) {
            predecessors.push(index);
            self.states[index].successors.push(new_index);
        }

        self.states.push_back(CausalState {
            state: Arc::new(new_state),
            predecessors,
            successors: Vec::new(),
        });

        self.max_revision()
    }

    fn max_revision(&self) -> Revision {
        self.states.last().unwrap().state.revision.clone()
    }

    fn state_at(&self, revision: Revision) -> StateView {
        let state_indices = self.indices_for_revision(&revision);
        let merged_states = self.build_state(&state_indices);
        assert_eq!(revision, merged_states.revision);
        merged_states
    }

    fn valid_revisions(&self, min_revision: Revision) -> Vec<Revision> {
        if min_revision == Revision::default() {
            // for a new requester who doesn't have a session we give them the latest (a quorum
            // read sort of thing)
            vec![self.max_revision()]
        } else {
            let mut seen_indices = BTreeSet::new();
            let mut stack = self.indices_for_revision(&min_revision);
            while let Some(index) = stack.pop() {
                seen_indices.insert(index);
                stack.extend(&self.states[index].predecessors);
            }

            // all individual revisions are valid to work from
            let single_states = self
                .states
                .iter()
                .enumerate()
                .filter(|(i, _)| !seen_indices.contains(i))
                .map(|(_, s)| s.state.revision.clone())
                .collect::<Vec<_>>();
            // we can also find combinations of concurrent edits
            // traverse the graph and build up valid states from the min revision
            let combinations = (1..=2)
                .flat_map(|combinations| {
                    single_states.iter().combinations(combinations).map(|rs| {
                        rs.into_iter()
                            .cloned()
                            .reduce(|acc, r| acc.merge(&r))
                            .unwrap()
                    })
                })
                .collect();
            combinations
        }
    }
}

impl CausalHistory {
    fn indices_for_revision(&self, revision: &Revision) -> Vec<usize> {
        revision
            .components()
            .iter()
            .map(|r| {
                let rev = Revision::from(vec![*r]);
                self.states
                    .binary_search_by_key(&rev, |s| s.state.revision.clone())
                    .unwrap()
            })
            .collect::<Vec<_>>()
    }

    fn build_state(&self, indices: &[usize]) -> StateView {
        indices
            .iter()
            .map(|i| &self.states[*i].state)
            .fold(StateView::default(), |acc, s| acc.merge(s))
    }
}

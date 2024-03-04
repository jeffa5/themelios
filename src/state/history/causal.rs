use std::{collections::BTreeSet, sync::Arc, borrow::Cow};

use bit_set::BitSet;

use crate::{
    abstract_model::Change,
    state::{revision::Revision, RawState, StateView},
};

use super::History;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct CausalHistory {
    /// Mapping of states and their dependencies.
    states: imbl::Vector<Arc<CausalState>>,
    heads: BTreeSet<usize>,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
struct CausalState {
    state: StateView,
    predecessors: Vec<usize>,
    successors: Vec<usize>,
    concurrent: BitSet<usize>,
}

impl CausalHistory {
    pub fn new(initial_state: RawState) -> Self {
        let mut heads = BTreeSet::new();
        heads.insert(0);
        Self {
            states: imbl::vector![Arc::new(CausalState {
                state: initial_state.into(),
                predecessors: Vec::new(),
                successors: Vec::new(),
                concurrent: BitSet::default(),
            })],
            heads,
        }
    }
}

impl History for CausalHistory {
    fn add_change(&mut self, change: Change) {
        let mut new_state = self.state_at(&change.revision).into_owned();

        let max_rev = self
            .states
            .last()
            .unwrap()
            .state
            .revision
            .clone()
            .increment();
        if new_state.apply_operation(change.operation, max_rev) {
            // find the dependencies of the change
            let predecessors = change.revision.components().to_owned();
            let new_index = self.states.len();

            let concurrent = self
                .concurrent_many(&predecessors)
                .collect::<BitSet<usize>>();
            for c in &concurrent {
                Arc::make_mut(&mut self.states[c])
                    .concurrent
                    .insert(new_index);
            }

            for &p in &predecessors {
                Arc::make_mut(&mut self.states[p])
                    .successors
                    .push(new_index);
                self.heads.remove(&p);
            }

            self.heads.insert(new_index);

            self.states.push_back(Arc::new(CausalState {
                state: new_state,
                predecessors,
                successors: Vec::new(),
                concurrent,
            }));
        }
    }

    fn max_revision(&self) -> Revision {
        let indices = self.heads.iter().copied().collect::<Vec<_>>();
        Revision::from(indices)
    }

    fn state_at(&self, revision: &Revision) -> Cow<StateView> {
        let state_indices = revision.components();
        let merged_states = self.build_state(state_indices);
        assert_eq!(revision, &merged_states.revision);
        Cow::Owned(merged_states)
    }

    fn valid_revisions(&self, min_revision: Option<&Revision>) -> Vec<Revision> {
        if let Some(min_revision) = min_revision {
            // A client can observe any state that has not been observed given their minimum
            // revision.
            //
            // This is every state not in the transitive closure from the min_revision.
            //
            // Additionally, we can have arbitrary 'merges' between these states and those that
            // they are concurrent with.

            let mut seen_indices = BitSet::<usize>::default();
            let mut stack = min_revision.components().to_owned();
            while let Some(index) = stack.pop() {
                if seen_indices.insert(index) {
                    stack.extend(&self.states[index].predecessors);
                }
            }

            // all individual revisions are valid to work from
            let single_states = (0..self.states.len()).filter(|i| !seen_indices.contains(*i));

            // we can also find combinations of concurrent edits
            // traverse the graph and build up valid states from the min revision
            single_states
                .flat_map(|i| self.concurrent_combinations(i))
                .map(Revision::from)
                .collect::<Vec<_>>()
        } else {
            // for a new requester who doesn't have a session we give them a head state, e.g. they
            // have connected to a single node of the datastore and can find that nodes latest
            // state, or any combination of those with concurrent merges.
            self.heads
                .iter()
                .flat_map(|i| self.concurrent_combinations(*i))
                .map(Revision::from)
                .collect::<Vec<_>>()
        }
    }
}

impl CausalHistory {
    fn build_state(&self, indices: &[usize]) -> StateView {
        let default_stateview = StateView {
            revision: Revision::from(vec![]),
            ..Default::default()
        };
        indices
            .iter()
            .map(|i| &self.states[*i].state)
            .fold(default_stateview, |mut acc, s| {
                acc.merge(s);
                acc
            })
    }

    /// Find all concurrent indices for the given index.
    fn concurrent_inner(&self, index: usize, seen: &mut BitSet<usize>) {
        let mut stack = vec![index];
        let mut seen_pred = BitSet::default();
        while let Some(index) = stack.pop() {
            if seen_pred.insert(index) {
                stack.extend(self.states[index].predecessors.iter().copied());
            }
        }
        let mut stack = vec![index];
        let mut seen_succ = BitSet::default();
        while let Some(index) = stack.pop() {
            if seen_succ.insert(index) {
                stack.extend(self.states[index].successors.iter().copied());
            }
        }
        seen.union_with(&seen_pred);
        seen.union_with(&seen_succ);
    }

    /// Find all indices that are concurrent with all indices given.
    ///
    /// Thus, all returned indices can be used on their own with the given indices to indicate a
    /// new merged state.
    fn concurrent_many(&self, indices: &[usize]) -> impl Iterator<Item = usize> {
        let mut seen = BitSet::default();
        for &index in indices {
            self.concurrent_inner(index, &mut seen);
        }
        (0..self.states.len()).filter(move |i| !seen.contains(*i))
    }

    fn concurrent_combinations(&self, index: usize) -> Vec<Vec<usize>> {
        let mut combinations = Vec::new();
        self.concurrent_combinations_inner(vec![index], &mut combinations);
        combinations
    }

    fn concurrent_combinations_inner(
        &self,
        indices: Vec<usize>,
        combinations: &mut Vec<Vec<usize>>,
    ) {
        combinations.push(indices.clone());
        let concurrent = intersections(indices.iter().map(|&i| &self.states[i].concurrent));
        for conc in concurrent.iter().filter(|c| c > indices.last().unwrap()) {
            let mut indices = indices.clone();
            indices.push(conc);
            indices.sort();
            indices.dedup();
            self.concurrent_combinations_inner(indices, combinations);
        }
    }
}

fn intersections<'a>(sets: impl IntoIterator<Item = &'a BitSet<usize>>) -> BitSet<usize> {
    let mut iter = sets.into_iter();
    match iter.next() {
        None => BitSet::default(),
        Some(first) => iter.fold(first.clone(), |mut acc, set| {
            acc.intersect_with(set);
            acc
        }),
    }
}

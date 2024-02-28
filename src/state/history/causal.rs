use std::{collections::BTreeSet, sync::Arc};

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
    concurrent: BTreeSet<usize>,
}

impl CausalHistory {
    pub fn new(initial_state: RawState) -> Self {
        Self {
            states: imbl::vector![CausalState {
                state: Arc::new(initial_state.into()),
                predecessors: Vec::new(),
                successors: Vec::new(),
                concurrent: BTreeSet::new(),
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
        }

        let concurrent = self.concurrent_many(&predecessors).collect::<BTreeSet<_>>();
        for &c in &concurrent {
            self.states[c].concurrent.insert(new_index);
        }

        for &p in &predecessors {
            self.states[p].successors.push(new_index);
        }

        self.states.push_back(CausalState {
            state: Arc::new(new_state),
            predecessors,
            successors: Vec::new(),
            concurrent,
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
            // A client can observe any state that has not been observed given their minimum
            // revision.
            //
            // This is every state not in the transitive closure from the min_revision.
            //
            // Additionally, we can have arbitrary 'merges' between these states and those that
            // they are concurrent with.

            let mut seen_indices = BTreeSet::new();
            let mut stack = self.indices_for_revision(&min_revision);
            while let Some(index) = stack.pop() {
                seen_indices.insert(index);
                stack.extend(&self.states[index].predecessors);
            }

            // all individual revisions are valid to work from
            let single_states = (0..self.states.len())
                .filter(|i| !seen_indices.contains(i))
                .collect::<Vec<_>>();

            // we can also find combinations of concurrent edits
            // traverse the graph and build up valid states from the min revision
            single_states
                .iter()
                .flat_map(|i| self.concurrent_combinations(*i))
                .map(Revision::from)
                .collect::<Vec<_>>()
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
            .map(|i| (*self.states[*i].state).clone())
            .reduce(|acc, s| acc.merge(&s))
            .unwrap()
    }

    /// Find all concurrent indices for the given index.
    fn concurrent_inner(&self, index: usize, seen: &mut BTreeSet<usize>) {
        let mut stack = vec![index];
        let mut seen_pred = BTreeSet::new();
        while let Some(index) = stack.pop() {
            if seen_pred.insert(index) {
                stack.extend(self.states[index].predecessors.iter().copied());
            }
        }
        let mut stack = vec![index];
        let mut seen_succ = BTreeSet::new();
        while let Some(index) = stack.pop() {
            if seen_succ.insert(index) {
                stack.extend(self.states[index].successors.iter().copied());
            }
        }
        seen.append(&mut seen_pred);
        seen.append(&mut seen_succ);
    }

    /// Find all indices that are concurrent with all indices given.
    ///
    /// Thus, all returned indices can be used on their own with the given indices to indicate a
    /// new merged state.
    fn concurrent_many(&self, indices: &[usize]) -> impl Iterator<Item = usize> {
        let mut seen = BTreeSet::new();
        for &index in indices {
            self.concurrent_inner(index, &mut seen);
        }
        (0..self.states.len()).filter(move |i| !seen.contains(i))
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
        for &conc in concurrent.iter().filter(|&c| c > indices.last().unwrap()) {
            let mut indices = indices.clone();
            indices.push(conc);
            indices.sort();
            indices.dedup();
            self.concurrent_combinations_inner(indices, combinations);
        }
    }
}

fn intersections<'a>(sets: impl IntoIterator<Item = &'a BTreeSet<usize>>) -> BTreeSet<usize> {
    let mut iter = sets.into_iter();
    match iter.next() {
        None => BTreeSet::new(),
        Some(first) => iter.fold(first.clone(), |mut acc, set| {
            acc.retain(|item| set.contains(item));
            acc
        }),
    }
}

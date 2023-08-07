use std::collections::{BTreeMap, BTreeSet};

use crate::abstract_model::Change;

/// Consistency level for viewing the state with.
pub enum ConsistencyLevel {
    Strong,
}

/// The history of the state, enabling generating views for different historical versions.
#[derive(Default, Clone, Debug, PartialEq, Eq, Hash)]
pub struct State {
    /// The initial state, to enable starting from interesting places.
    initial: StateView,
    /// The changes that have been made to the state.
    changes: Vec<Change>,
}

impl State {
    pub fn with_initial(mut self, initial: StateView) -> Self {
        self.set_initial(initial);
        self
    }

    pub fn set_initial(&mut self, initial: StateView) -> &mut Self {
        self.initial = initial;
        self
    }

    /// Record a change for this state.
    pub fn push_change(&mut self, change: Change) -> usize {
        self.changes.push(change);
        self.changes.len()
    }

    /// Record changes for this state.
    pub fn push_changes(&mut self, changes: impl Iterator<Item = Change>) -> usize {
        for change in changes {
            self.push_change(change);
        }
        self.changes.len()
    }

    /// Get the maximum revision for this change.
    pub fn max_revision(&self) -> usize {
        self.changes.len()
    }

    /// Get a view for a specific revision in the change history.
    pub fn view_at(&self, revision: usize) -> StateView {
        let mut view = self.initial.clone();
        for change in &self.changes[..revision - 1] {
            view.apply_change(change);
        }
        view
    }

    /// Get all the possible views under the given consistency level.
    pub fn views_for(&self, consistency_level: ConsistencyLevel) -> Vec<StateView> {
        match consistency_level {
            ConsistencyLevel::Strong => {
                let rev = self.changes.len();
                vec![self.view_at(rev)]
            }
        }
    }
}

#[derive(Default, Clone, Debug, PartialEq, Eq, Hash)]
pub struct StateView {
    pub nodes: BTreeMap<usize, NodeResource>,
    pub schedulers: BTreeSet<usize>,
    pub replicaset_controllers: BTreeSet<usize>,
    pub pods: BTreeMap<u32, PodResource>,
    pub replica_sets: BTreeMap<u32, ReplicaSetResource>,
}

impl StateView {
    pub fn with_pods(mut self, pods: impl Iterator<Item = PodResource>) -> Self {
        self.set_pods(pods);
        self
    }

    pub fn set_pods(&mut self, pods: impl Iterator<Item = PodResource>) -> &mut Self {
        for pod in pods {
            self.pods.insert(pod.id, pod);
        }
        self
    }

    pub fn with_replicasets(
        mut self,
        replicasets: impl Iterator<Item = ReplicaSetResource>,
    ) -> Self {
        self.set_replicasets(replicasets);
        self
    }

    pub fn set_replicasets(
        &mut self,
        replicasets: impl Iterator<Item = ReplicaSetResource>,
    ) -> &mut Self {
        for replicaset in replicasets {
            self.replica_sets.insert(replicaset.id, replicaset);
        }
        self
    }

    pub fn apply_change(&mut self, change: &Change) {
        match change {
            Change::NodeJoin(i) => {
                self.nodes.insert(
                    *i,
                    NodeResource {
                        running: BTreeSet::new(),
                        ready: true,
                    },
                );
            }
            Change::SchedulerJoin(i) => {
                self.schedulers.insert(*i);
            }
            Change::ReplicasetJoin(i) => {
                self.replicaset_controllers.insert(*i);
            }
            Change::NewPod(i) => {
                self.pods.insert(
                    *i,
                    PodResource {
                        id: *i,
                        node_name: None,
                    },
                );
            }
            Change::SchedulePod(pod, node) => {
                if let Some(pod) = self.pods.get_mut(pod) {
                    pod.node_name = Some(*node);
                }
            }
            Change::RunPod(pod, node) => {
                self.nodes.get_mut(node).unwrap().running.insert(*pod);
            }
            Change::NodeCrash(node) => {
                self.nodes.remove(node);
                self.pods
                    .retain(|_, pod| pod.node_name.map_or(true, |n| n != *node));
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PodResource {
    pub id: u32,
    pub node_name: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReplicaSetResource {
    pub id: u32,
    pub replicas: u32,
}

impl ReplicaSetResource {
    pub fn pods(&self) -> Vec<u32> {
        (0..self.replicas).map(|i| (self.id * 1000) + i).collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeResource {
    pub running: BTreeSet<u32>,
    pub ready: bool,
}

use std::collections::{BTreeMap, BTreeSet};

use crate::abstract_model::{Change, Operation};

/// Consistency level for viewing the state with.
#[derive(Default, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ConsistencyLevel {
    #[default]
    Strong,
    BoundedStaleness(usize),
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Revision(usize);

/// The history of the state, enabling generating views for different historical versions.
#[derive(Default, Clone, Eq)]
pub struct State {
    /// Consistency level for this state.
    consistency_level: ConsistencyLevel,
    /// The initial state, to enable starting from interesting places.
    initial: StateView,
    /// The changes that have been made to the state.
    changes: Vec<Change>,
}

impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        let self_views = self.views();
        let other_views = other.views();
        self_views == other_views
    }
}

impl std::hash::Hash for State {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let views = self.views();
        views.hash(state);
    }
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let views = self.views();
        f.debug_struct("State")
            .field("consistency_level", &self.consistency_level)
            .field("initial", &self.initial)
            .field("changes", &self.changes)
            .field("views", &views)
            .finish()
    }
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

    pub fn with_consistency_level(mut self, consistency_level: ConsistencyLevel) -> Self {
        self.set_consistency_level(consistency_level);
        self
    }

    pub fn set_consistency_level(&mut self, consistency_level: ConsistencyLevel) -> &mut Self {
        self.consistency_level = consistency_level;
        self
    }

    /// Record a change for this state.
    pub fn push_change(&mut self, change: Change) -> Revision {
        self.changes.push(change);
        self.max_revision()
    }

    /// Record changes for this state.
    pub fn push_changes(&mut self, changes: impl Iterator<Item = Change>) -> Revision {
        for change in changes {
            self.push_change(change);
        }
        self.max_revision()
    }

    /// Get the maximum revision for this change.
    pub fn max_revision(&self) -> Revision {
        Revision(self.changes.len())
    }

    /// Get a view for a specific revision in the change history.
    pub fn view_at(&self, revision: Revision) -> StateView {
        let mut view = self.initial.clone();
        for change in &self.changes[..revision.0] {
            view.apply_change(change);
        }
        view
    }

    /// Get all the possible views under the given consistency level.
    pub fn views(&self) -> Vec<StateView> {
        match self.consistency_level {
            ConsistencyLevel::Strong => {
                let rev = self.max_revision();
                vec![self.view_at(rev)]
            }
            ConsistencyLevel::BoundedStaleness(k) => {
                let max_rev = self.max_revision();
                (max_rev.0.saturating_sub(k)..=max_rev.0)
                    .map(|r| self.view_at(Revision(r)))
                    .collect()
            }
        }
    }
}

#[derive(Default, Clone, Debug, PartialEq, Eq, Hash)]
pub struct StateView {
    pub revision: Revision,
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
        match &change.operation {
            Operation::NodeJoin(i) => {
                self.nodes.insert(
                    *i,
                    NodeResource {
                        running: BTreeSet::new(),
                        ready: true,
                    },
                );
            }
            Operation::SchedulerJoin(i) => {
                self.schedulers.insert(*i);
            }
            Operation::ReplicasetJoin(i) => {
                self.replicaset_controllers.insert(*i);
            }
            Operation::NewPod(i) => {
                self.pods.insert(
                    *i,
                    PodResource {
                        id: *i,
                        node_name: None,
                    },
                );
            }
            Operation::SchedulePod(pod, node) => {
                if let Some(pod) = self.pods.get_mut(pod) {
                    pod.node_name = Some(*node);
                }
            }
            Operation::RunPod(pod, node) => {
                self.nodes.get_mut(node).unwrap().running.insert(*pod);
            }
            Operation::NodeCrash(node) => {
                self.nodes.remove(node);
                self.pods
                    .retain(|_, pod| pod.node_name.map_or(true, |n| n != *node));
            }
        }
        self.revision = Revision(self.revision.0 + 1);
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

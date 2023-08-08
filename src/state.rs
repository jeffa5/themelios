use std::collections::{BTreeMap, BTreeSet};

use crate::abstract_model::{Change, Operation};

/// Consistency level for viewing the state with.
#[derive(Default, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ReadConsistencyLevel {
    /// Always work off the latest state.
    #[default]
    Strong,
    /// Work off a state that is close to the latest, bounded by the `k`.
    BoundedStaleness(usize),
    /// Work off a state that derives from the last one seen.
    Session,
    /// Work off any historical state.
    Eventual,
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Revision(usize);

#[derive(Default, Debug, Clone, PartialEq, Eq, Hash)]
// TODO: rework this history to be based on a DAG of changes or a linear history, depending on
// config.
pub struct ChangeHistory(Vec<Change>);

impl ChangeHistory {
    pub fn changes_to(&self, revision: Revision) -> Vec<&Change> {
        self.0[..revision.0].iter().collect()
    }

    pub fn max_revision(&self) -> Revision {
        Revision(self.0.len())
    }

    pub fn add(&mut self, change: Change) -> Revision {
        self.0.push(change);
        self.max_revision()
    }

    pub fn valid_revisions(
        &self,
        consistency_level: ReadConsistencyLevel,
        session: Revision,
    ) -> Vec<Revision> {
        match consistency_level {
            ReadConsistencyLevel::Strong => vec![Revision(self.0.len())],
            ReadConsistencyLevel::BoundedStaleness(k) => {
                let max = self.max_revision().0;
                (max.saturating_sub(k)..=max).map(Revision).collect()
            }
            ReadConsistencyLevel::Session => {
                let max = self.max_revision().0;
                (session.0..=max).map(Revision).collect()
            }
            ReadConsistencyLevel::Eventual => {
                let max = self.max_revision().0;
                (0..=max).map(Revision).collect()
            }
        }
    }
}

/// The history of the state, enabling generating views for different historical versions.
#[derive(Default, Clone, Eq)]
pub struct State {
    /// Consistency level for this state.
    consistency_level: ReadConsistencyLevel,
    /// The initial state, to enable starting from interesting places.
    initial: StateView,
    /// The changes that have been made to the state.
    changes: ChangeHistory,
    sessions: BTreeMap<usize, Revision>,
}

impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        let self_views = self.all_views();
        let other_views = other.all_views();
        self_views == other_views
    }
}

impl std::hash::Hash for State {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let views = self.all_views();
        views.hash(state);
    }
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let views = self.all_views();
        f.debug_struct("State")
            .field("consistency_level", &self.consistency_level)
            .field("sessions", &self.sessions)
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

    pub fn with_consistency_level(mut self, consistency_level: ReadConsistencyLevel) -> Self {
        self.set_consistency_level(consistency_level);
        self
    }

    pub fn set_consistency_level(&mut self, consistency_level: ReadConsistencyLevel) -> &mut Self {
        self.consistency_level = consistency_level;
        self
    }

    /// Record a change for this state from a given controller.
    pub fn push_change(&mut self, change: Change, from: usize) -> Revision {
        let rev = self.changes.add(change);
        self.sessions.insert(from, rev);
        rev
    }

    /// Record changes for this state.
    pub fn push_changes(&mut self, changes: impl Iterator<Item = Change>, from: usize) -> Revision {
        for change in changes {
            self.push_change(change, from);
        }
        self.max_revision()
    }

    /// Get the maximum revision for this change.
    pub fn max_revision(&self) -> Revision {
        self.changes.max_revision()
    }

    /// Get a view for a specific revision in the change history.
    pub fn view_at(&self, revision: Revision) -> StateView {
        let mut view = self.initial.clone();
        for change in &self.changes.changes_to(revision) {
            view.apply_change(change);
        }
        view
    }

    /// Get all the possible views under the given consistency level.
    pub fn views(&self, from: &usize) -> Vec<StateView> {
        let revisions = self.changes.valid_revisions(
            self.consistency_level.clone(),
            self.sessions.get(from).copied().unwrap_or_default(),
        );
        revisions.into_iter().map(|r| self.view_at(r)).collect()
    }

    fn all_views(&self) -> BTreeSet<StateView> {
        self.sessions.keys().flat_map(|s| self.views(s)).collect()
    }
}

#[derive(Default, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StateView {
    pub revision: Revision,
    pub nodes: BTreeMap<usize, NodeResource>,
    pub schedulers: BTreeSet<usize>,
    pub replicaset_controllers: BTreeSet<usize>,
    pub deployment_controllers: BTreeSet<usize>,
    pub statefulset_controllers: BTreeSet<usize>,
    pub pods: BTreeMap<String, PodResource>,
    pub replica_sets: BTreeMap<String, ReplicaSetResource>,
    pub deployments: BTreeMap<String, DeploymentResource>,
    pub statefulsets: BTreeMap<String, StatefulSetResource>,
}

impl StateView {
    pub fn with_pods(mut self, pods: impl Iterator<Item = PodResource>) -> Self {
        self.set_pods(pods);
        self
    }

    pub fn set_pods(&mut self, pods: impl Iterator<Item = PodResource>) -> &mut Self {
        for pod in pods {
            self.pods.insert(pod.id.clone(), pod);
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
            self.replica_sets.insert(replicaset.id.clone(), replicaset);
        }
        self
    }

    pub fn with_deployments(
        mut self,
        deployments: impl Iterator<Item = DeploymentResource>,
    ) -> Self {
        self.set_deployments(deployments);
        self
    }

    pub fn set_deployments(
        &mut self,
        deployments: impl Iterator<Item = DeploymentResource>,
    ) -> &mut Self {
        for deployment in deployments {
            self.deployments.insert(deployment.id.clone(), deployment);
        }
        self
    }

    pub fn with_statefulsets(
        mut self,
        statefulsets: impl Iterator<Item = StatefulSetResource>,
    ) -> Self {
        self.set_statefulsets(statefulsets);
        self
    }

    pub fn set_statefulsets(
        &mut self,
        statefulsets: impl Iterator<Item = StatefulSetResource>,
    ) -> &mut Self {
        for statefulset in statefulsets {
            self.statefulsets
                .insert(statefulset.id.clone(), statefulset);
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
            Operation::DeploymentJoin(i) => {
                self.deployment_controllers.insert(*i);
            }
            Operation::StatefulSetJoin(i) => {
                self.statefulset_controllers.insert(*i);
            }
            Operation::NewPod(i) => {
                self.pods.insert(
                    i.clone(),
                    PodResource {
                        id: i.clone(),
                        node_name: None,
                    },
                );
            }
            Operation::NewReplicaset(i) => {
                self.replica_sets.insert(
                    i.clone(),
                    ReplicaSetResource {
                        id: i.clone(),
                        replicas: 2,
                    },
                );
            }
            Operation::SchedulePod(pod, node) => {
                if let Some(pod) = self.pods.get_mut(pod) {
                    pod.node_name = Some(*node);
                }
            }
            Operation::RunPod(pod, node) => {
                self.nodes
                    .get_mut(node)
                    .unwrap()
                    .running
                    .insert(pod.clone());
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
    pub id: String,
    pub node_name: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReplicaSetResource {
    pub id: String,
    pub replicas: u32,
}

impl ReplicaSetResource {
    pub fn pods(&self) -> Vec<String> {
        (0..self.replicas)
            .map(|i| format!("{}-{}", self.id, i))
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeploymentResource {
    pub id: String,
    pub replicas: u32,
}

impl DeploymentResource {
    pub fn replicasets(&self) -> Vec<String> {
        vec![self.id.clone()]
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StatefulSetResource {
    pub id: String,
    pub replicas: u32,
}

impl StatefulSetResource {
    pub fn replicasets(&self) -> Vec<String> {
        vec![self.id.clone()]
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeResource {
    pub running: BTreeSet<String>,
    pub ready: bool,
}

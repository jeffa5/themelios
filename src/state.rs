use std::sync::Arc;

use crate::controller::client::ClientState;
use crate::controller::ControllerStates;
use crate::resources::{
    ConditionStatus, ControllerRevision, Job, LabelSelector, Meta, NodeCondition,
    NodeConditionType, PersistentVolumeClaim,
};
use crate::utils::{self, now};
use crate::{
    abstract_model::{Change, ControllerAction},
    resources::{Deployment, Node, Pod, ReplicaSet, StatefulSet},
};

/// Consistency level for viewing the state with.
#[derive(Default, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ConsistencySetup {
    /// Always work off the latest state.
    /// Linearizable reads.
    /// Linearizable writes.
    #[default]
    Strong,
    /// Work off a state that is close to the latest, bounded by the `k`.
    /// Bounded staleness on reads.
    /// Linearizable writes.
    BoundedStaleness(usize),
    /// Work off a state that derives from the last one seen.
    /// Session consistency on reads.
    /// Linearizable writes.
    Session,
    /// Work off any historical state.
    /// Eventually consistent reads.
    /// Linearizable writes.
    Eventual,
    /// Optimistically apply changes without guarantee that they are committed.
    /// Commits automatically happen every `k` changes.
    /// Optimistic reads.
    /// Optimistic writes.
    OptimisticLinear(usize),
    /// Apply changes to a causal graph.
    Causal,
}

pub trait History {
    fn add_change(&mut self, change: Change, from: usize) -> Revision;

    fn reset_session(&mut self, from: usize);

    fn max_revision(&self) -> Revision;

    fn state_at(&self, revision: Revision) -> StateView;

    fn valid_revisions(&self, from: usize) -> Vec<Revision>;

    fn states_for(&self, from: usize) -> Vec<StateView> {
        let revisions = self.valid_revisions(from);
        revisions.into_iter().map(|r| self.state_at(r)).collect()
    }
}

#[derive(Default, Clone, PartialEq, Eq, Hash, Debug)]
pub struct StrongHistory {
    state: Arc<StateView>,
}

impl StrongHistory {
    fn new(initial_state: StateView) -> Self {
        Self {
            state: Arc::new(initial_state),
        }
    }
}

impl History for StrongHistory {
    fn add_change(&mut self, change: Change, _from: usize) -> Revision {
        let new_revision = self.max_revision().increment();
        Arc::make_mut(&mut self.state).apply_change(&change, new_revision);
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

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct BoundedHistory {
    k: usize,
    last_k_states: Vec<Arc<StateView>>,
}

impl BoundedHistory {
    fn new(initial_state: StateView, k: usize) -> Self {
        Self {
            k,
            last_k_states: vec![Arc::new(initial_state)],
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

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct SessionHistory {
    sessions: imbl::OrdMap<usize, Revision>,
    states: imbl::Vector<Arc<StateView>>,
}

impl SessionHistory {
    fn new(initial_state: StateView) -> Self {
        Self {
            sessions: imbl::OrdMap::new(),
            states: imbl::vector![Arc::new(initial_state)],
        }
    }
}

impl History for SessionHistory {
    fn add_change(&mut self, change: Change, from: usize) -> Revision {
        let mut new_state_ref = self.states.last().unwrap().clone();
        let new_state = Arc::make_mut(&mut new_state_ref);
        let new_revision = self.max_revision().increment();
        new_state.apply_change(&change, new_revision);
        self.states.push_back(new_state_ref);
        let max = self.max_revision();
        self.sessions.insert(from, max.clone());

        // let min_revision = self.sessions.values().min().unwrap();
        // loop {
        //     let val = &self.states.first().unwrap().revision;
        //     if val < min_revision {
        //         self.states.remove(0);
        //     } else {
        //         break;
        //     }
        // }

        max
    }

    fn reset_session(&mut self, from: usize) {
        self.sessions.remove(&from);
    }

    fn max_revision(&self) -> Revision {
        self.states.last().unwrap().revision.clone()
    }

    fn state_at(&self, revision: Revision) -> StateView {
        let index = self
            .states
            .binary_search_by_key(&revision, |s| s.revision.clone())
            .unwrap();
        (*self.states[index]).clone()
    }

    fn valid_revisions(&self, from: usize) -> Vec<Revision> {
        let min_revision = self.sessions.get(&from).cloned().unwrap_or_default();
        self.states
            .iter()
            .filter(|s| s.revision >= min_revision)
            .map(|s| s.revision.clone())
            .collect()
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct EventualHistory {
    states: Vec<Arc<StateView>>,
}

impl EventualHistory {
    fn new(initial_state: StateView) -> Self {
        Self {
            states: vec![Arc::new(initial_state)],
        }
    }
}

impl History for EventualHistory {
    fn add_change(&mut self, change: Change, _from: usize) -> Revision {
        let mut new_state_ref = Arc::clone(self.states.last().unwrap());
        let new_state = Arc::make_mut(&mut new_state_ref);
        let new_revision = self.max_revision().increment();
        new_state.apply_change(&change, new_revision);
        self.states.push(new_state_ref);
        self.max_revision()
    }
    fn reset_session(&mut self, _from: usize) {
        // nothing to do
    }

    fn max_revision(&self) -> Revision {
        self.states.last().unwrap().revision.clone()
    }

    fn state_at(&self, revision: Revision) -> StateView {
        (*self.states[revision.0[0]]).clone()
    }

    fn valid_revisions(&self, _from: usize) -> Vec<Revision> {
        self.states.iter().map(|s| s.revision.clone()).collect()
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct OptimisticLinearHistory {
    /// First is the last committed state.
    /// Last is the optimistic one.
    /// In between are states that could be committed.
    states: Vec<Arc<StateView>>,
    commit_every: usize,
}

impl OptimisticLinearHistory {
    fn new(initial_state: StateView, commit_every: usize) -> Self {
        Self {
            states: vec![Arc::new(initial_state)],
            commit_every,
        }
    }
}

impl History for OptimisticLinearHistory {
    fn add_change(&mut self, change: Change, _from: usize) -> Revision {
        // find the state for the revision that the change operated on, we'll treat this as the
        // committed one if they didn't operate on the latest (optimistic)
        let index = self
            .states
            .binary_search_by_key(&&change.revision, |s| &s.revision)
            .unwrap();
        let mut new_state_ref = Arc::clone(&self.states[index]);
        let new_state = Arc::make_mut(&mut new_state_ref);
        let new_revision = self.max_revision().increment();
        new_state.apply_change(&change, new_revision);

        if index + 1 == self.states.len() {
            // this was a mutation on the optimistic state
            if self.states.len() > self.commit_every {
                // we have triggered a commit point, the last state is now the committed one
                self.states.clear();
            } else {
                // we haven't reached a guaranteed commit yet, just extend the current states
            }
            self.states.push(new_state_ref);
        } else {
            // this was a mutation on a committed state (leader changed)
            // Discard all states before and after this one
            let committed_state = self.states.swap_remove(index);
            self.states.clear();
            self.states.push(committed_state);
            self.states.push(new_state_ref);
        }

        self.max_revision()
    }
    fn reset_session(&mut self, _from: usize) {
        // nothing to do
    }

    fn max_revision(&self) -> Revision {
        self.states.last().unwrap().revision.clone()
    }

    fn state_at(&self, revision: Revision) -> StateView {
        let index = self
            .states
            .binary_search_by_key(&&revision, |s| &s.revision)
            .unwrap();
        (*self.states[index]).clone()
    }

    fn valid_revisions(&self, _from: usize) -> Vec<Revision> {
        self.states.iter().map(|s| s.revision.clone()).collect()
    }
}

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
    fn new(initial_state: StateView) -> Self {
        Self {
            states: vec![CausalState {
                state: Arc::new(initial_state),
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
        let target_revision = Revision(vec![*change.revision.0.iter().max().unwrap()]);
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
        for revision in change.revision.0 {
            let index = self
                .states
                .binary_search_by_key(&&Revision(vec![revision]), |s| &s.state.revision)
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
        let target_revision = Revision(vec![*revision.0.iter().max().unwrap()]);
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

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum StateHistory {
    /// Linearizable reads.
    /// Linearizable writes.
    Strong(StrongHistory),
    /// Bounded staleness on reads.
    /// Linearizable writes.
    Bounded(BoundedHistory),
    /// Session consistency on reads.
    /// Linearizable writes.
    Session(SessionHistory),
    /// Eventually consistent reads.
    /// Linearizable writes.
    Eventual(EventualHistory),
    /// Optimistic reads.
    /// Optimistic writes.
    OptimisticLinear(OptimisticLinearHistory),
    Causal(CausalHistory),
}

impl Default for StateHistory {
    fn default() -> Self {
        Self::Strong(StrongHistory::default())
    }
}

impl StateHistory {
    fn new(consistency_level: ConsistencySetup, initial_state: StateView) -> Self {
        match consistency_level {
            ConsistencySetup::Strong => Self::Strong(StrongHistory::new(initial_state)),
            ConsistencySetup::BoundedStaleness(k) => {
                Self::Bounded(BoundedHistory::new(initial_state, k))
            }
            ConsistencySetup::Session => Self::Session(SessionHistory::new(initial_state)),
            ConsistencySetup::Eventual => Self::Eventual(EventualHistory::new(initial_state)),
            ConsistencySetup::OptimisticLinear(commit_every) => {
                Self::OptimisticLinear(OptimisticLinearHistory::new(initial_state, commit_every))
            }
            ConsistencySetup::Causal => Self::Causal(CausalHistory::new(initial_state)),
        }
    }

    fn add_change(&mut self, change: Change, from: usize) -> Revision {
        match self {
            StateHistory::Strong(s) => s.add_change(change, from),
            StateHistory::Bounded(s) => s.add_change(change, from),
            StateHistory::Session(s) => s.add_change(change, from),
            StateHistory::Eventual(s) => s.add_change(change, from),
            StateHistory::OptimisticLinear(s) => s.add_change(change, from),
            StateHistory::Causal(s) => s.add_change(change, from),
        }
    }

    fn reset_session(&mut self, from: usize) {
        match self {
            StateHistory::Strong(s) => s.reset_session(from),
            StateHistory::Bounded(s) => s.reset_session(from),
            StateHistory::Session(s) => s.reset_session(from),
            StateHistory::Eventual(s) => s.reset_session(from),
            StateHistory::OptimisticLinear(s) => s.reset_session(from),
            StateHistory::Causal(s) => s.reset_session(from),
        }
    }

    fn max_revision(&self) -> Revision {
        match self {
            StateHistory::Strong(s) => s.max_revision(),
            StateHistory::Bounded(s) => s.max_revision(),
            StateHistory::Session(s) => s.max_revision(),
            StateHistory::Eventual(s) => s.max_revision(),
            StateHistory::OptimisticLinear(s) => s.max_revision(),
            StateHistory::Causal(s) => s.max_revision(),
        }
    }

    fn state_at(&self, revision: Revision) -> StateView {
        match self {
            StateHistory::Strong(s) => s.state_at(revision),
            StateHistory::Bounded(s) => s.state_at(revision),
            StateHistory::Session(s) => s.state_at(revision),
            StateHistory::Eventual(s) => s.state_at(revision),
            StateHistory::OptimisticLinear(s) => s.state_at(revision),
            StateHistory::Causal(s) => s.state_at(revision),
        }
    }

    fn states_for(&self, from: usize) -> Vec<StateView> {
        match self {
            StateHistory::Strong(s) => s.states_for(from),
            StateHistory::Bounded(s) => s.states_for(from),
            StateHistory::Session(s) => s.states_for(from),
            StateHistory::Eventual(s) => s.states_for(from),
            StateHistory::OptimisticLinear(s) => s.states_for(from),
            StateHistory::Causal(s) => s.states_for(from),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Revision(Vec<usize>);

impl Default for Revision {
    fn default() -> Self {
        Self(vec![0])
    }
}

impl std::fmt::Display for Revision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = self
            .0
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<String>>()
            .join("-");
        f.write_str(&s)
    }
}

impl Revision {
    fn increment(mut self) -> Self {
        assert_eq!(self.0.len(), 1);
        self.0[0] += 1;
        self
    }
}

/// The history of the state, enabling generating views for different historical versions.
#[derive(Default, Debug, Clone, PartialEq, Eq, Hash)]
pub struct State {
    /// The changes that have been made to the state.
    states: StateHistory,

    controller_states: Vec<ControllerStates>,

    client_states: Vec<ClientState>,
}

impl State {
    pub fn new(initial_state: StateView, consistency_level: ConsistencySetup) -> Self {
        Self {
            states: StateHistory::new(consistency_level, initial_state),
            controller_states: Vec::new(),
            client_states: Vec::new(),
        }
    }

    /// Reset the session for the given connection.
    pub fn reset_session(&mut self, from: usize) {
        self.states.reset_session(from)
    }

    /// Record a change for this state from a given controller.
    pub fn push_change(&mut self, change: Change, from: usize) -> Revision {
        self.states.add_change(change, from)
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
        self.states.max_revision()
    }

    /// Get a view for a specific revision in the change history.
    pub fn view_at(&self, revision: Revision) -> StateView {
        self.states.state_at(revision)
    }

    /// Get all the possible views under the given consistency level.
    pub fn views(&self, from: usize) -> Vec<StateView> {
        self.states.states_for(from)
    }

    pub fn add_controller(&mut self, controller_state: ControllerStates) {
        self.controller_states.push(controller_state);
    }

    pub fn add_client(&mut self, client: ClientState) {
        self.client_states.push(client)
    }

    pub fn update_client(&mut self, client: usize, state: ClientState) {
        self.client_states[client] = state;
    }

    pub fn update_controller(&mut self, controller: usize, controller_state: ControllerStates) {
        self.controller_states[controller] = controller_state;
    }

    pub fn get_controller(&self, controller: usize) -> &ControllerStates {
        &self.controller_states[controller]
    }

    pub fn get_client(&self, client: usize) -> &ClientState {
        &self.client_states[client]
    }

    pub fn latest(&self) -> StateView {
        self.states.state_at(self.max_revision())
    }
}

#[derive(derivative::Derivative)]
#[derivative(PartialEq, Hash)]
#[derive(Default, Clone, Debug, Eq, PartialOrd, Ord)]
pub struct StateView {
    // Ignore the revision field as we just care whether the rest of the state is the same.
    #[derivative(PartialEq = "ignore", Hash = "ignore")]
    pub revision: Revision,
    pub nodes: Resources<Node>,
    pub pods: Resources<Pod>,
    pub replicasets: Resources<ReplicaSet>,
    pub deployments: Resources<Deployment>,
    pub statefulsets: Resources<StatefulSet>,
    pub controller_revisions: Resources<ControllerRevision>,
    pub persistent_volume_claims: Resources<PersistentVolumeClaim>,
    pub jobs: Resources<Job>,
}

impl StateView {
    pub fn with_pods(mut self, pods: impl Iterator<Item = Pod>) -> Self {
        self.set_pods(pods);
        self
    }

    pub fn set_pods(&mut self, pods: impl Iterator<Item = Pod>) -> &mut Self {
        for pod in pods {
            self.pods.insert(pod);
        }
        self
    }

    pub fn with_replicasets(mut self, replicasets: impl Iterator<Item = ReplicaSet>) -> Self {
        self.set_replicasets(replicasets);
        self
    }

    pub fn set_replicasets(&mut self, replicasets: impl Iterator<Item = ReplicaSet>) -> &mut Self {
        for replicaset in replicasets {
            self.replicasets.insert(replicaset);
        }
        self
    }

    pub fn with_deployments(mut self, deployments: impl Iterator<Item = Deployment>) -> Self {
        self.set_deployments(deployments);
        self
    }

    pub fn set_deployments(&mut self, deployments: impl Iterator<Item = Deployment>) -> &mut Self {
        for deployment in deployments {
            self.deployments.insert(deployment);
        }
        self
    }

    pub fn with_deployment(mut self, deployment: Deployment) -> Self {
        self.set_deployment(deployment);
        self
    }

    pub fn set_deployment(&mut self, deployment: Deployment) -> &mut Self {
        self.deployments.insert(deployment);
        self
    }

    pub fn with_statefulsets(mut self, statefulsets: impl Iterator<Item = StatefulSet>) -> Self {
        self.set_statefulsets(statefulsets);
        self
    }

    pub fn set_statefulsets(
        &mut self,
        statefulsets: impl Iterator<Item = StatefulSet>,
    ) -> &mut Self {
        for statefulset in statefulsets {
            self.statefulsets.insert(statefulset);
        }
        self
    }

    pub fn with_statefulset(mut self, statefulset: StatefulSet) -> Self {
        self.set_statefulset(statefulset);
        self
    }

    pub fn set_statefulset(&mut self, statefulset: StatefulSet) -> &mut Self {
        self.statefulsets.insert(statefulset);
        self
    }

    pub fn with_nodes(mut self, nodes: impl Iterator<Item = Node>) -> Self {
        self.set_nodes(nodes);
        self
    }

    pub fn set_nodes(&mut self, nodes: impl Iterator<Item = Node>) -> &mut Self {
        for node in nodes {
            self.nodes.insert(node);
        }
        self
    }

    pub fn apply_change(&mut self, change: &Change, new_revision: Revision) {
        match &change.operation {
            ControllerAction::NodeJoin(name, capacity) => {
                self.nodes.insert(Node {
                    metadata: utils::metadata(name.clone()),
                    spec: crate::resources::NodeSpec {
                        taints: Vec::new(),
                        unschedulable: false,
                    },
                    status: crate::resources::NodeStatus {
                        capacity: capacity.clone(),
                        allocatable: Some(capacity.clone()),
                        conditions: vec![NodeCondition {
                            r#type: NodeConditionType::Ready,
                            status: ConditionStatus::True,
                            ..Default::default()
                        }],
                    },
                });
            }
            ControllerAction::CreatePod(pod) => {
                let mut pod = pod.clone();
                self.fill_name(&mut pod);
                self.pods.insert(pod);
            }
            ControllerAction::UpdatePod(pod) => {
                self.pods.insert(pod.clone());
            }
            ControllerAction::SoftDeletePod(pod) => {
                let mut pod = pod.clone();
                // marked for deletion
                pod.metadata.deletion_timestamp = Some(now());
                self.pods.insert(pod);
            }
            ControllerAction::HardDeletePod(pod) => {
                self.pods.remove(&pod.metadata.name);
            }
            ControllerAction::SchedulePod(pod, node) => {
                if let Some(pod) = self.pods.get_mut(pod) {
                    pod.spec.node_name = Some(node.clone());
                }
            }
            ControllerAction::NodeCrash(node_name) => {
                if let Some(node) = self.nodes.remove(node_name) {
                    self.pods.retain(|pod| {
                        pod.spec
                            .node_name
                            .as_ref()
                            .map_or(true, |n| n != &node.metadata.name)
                    });
                }
            }
            ControllerAction::UpdateDeployment(dep) => {
                self.deployments.insert(dep.clone());
            }
            ControllerAction::RequeueDeployment(_dep) => {
                // skip
            }
            ControllerAction::UpdateDeploymentStatus(dep) => {
                self.deployments.insert(dep.clone());
            }
            ControllerAction::CreateReplicaSet(rs) => {
                let mut rs = rs.clone();
                self.fill_name(&mut rs);
                self.replicasets.insert(rs);
            }
            ControllerAction::UpdateReplicaSet(rs) => {
                self.replicasets.insert(rs.clone());
            }
            ControllerAction::UpdateReplicaSetStatus(rs) => {
                self.replicasets.insert(rs.clone());
            }
            ControllerAction::UpdateReplicaSets(rss) => {
                for rs in rss {
                    self.replicasets.insert(rs.clone());
                }
            }
            ControllerAction::UpdateStatefulSet(sts) => {
                self.statefulsets.insert(sts.clone());
            }
            ControllerAction::UpdateStatefulSetStatus(sts) => {
                self.statefulsets.insert(sts.clone());
            }
            ControllerAction::CreateControllerRevision(cr) => {
                let mut cr = cr.clone();
                self.fill_name(&mut cr);
                self.controller_revisions.insert(cr);
            }
            ControllerAction::UpdateControllerRevision(cr) => {
                self.controller_revisions.insert(cr.clone());
            }
            ControllerAction::DeleteControllerRevision(cr) => {
                self.controller_revisions.remove(&cr.metadata.name);
            }
            ControllerAction::DeleteReplicaSet(rs) => {
                self.replicasets.remove(&rs.metadata.name);
            }
            ControllerAction::CreatePersistentVolumeClaim(pvc) => {
                let mut pvc = pvc.clone();
                self.fill_name(&mut pvc);
                self.persistent_volume_claims.insert(pvc);
            }
            ControllerAction::UpdatePersistentVolumeClaim(pvc) => {
                self.persistent_volume_claims.insert(pvc.clone());
            }
            ControllerAction::UpdateJobStatus(job) => {
                self.jobs.insert(job.clone());
            }
        }
        self.revision = new_revision;
    }

    pub fn pods_for_node(&self, node: &str) -> Vec<&Pod> {
        self.pods
            .iter()
            .filter(|p| p.spec.node_name.as_ref().map_or(false, |n| n == node))
            .collect()
    }

    fn fill_name<T: Meta>(&self, res: &mut T) {
        if res.metadata().name.is_empty() && !res.metadata().generate_name.is_empty() {
            let rev = &self.revision;
            res.metadata_mut().name = format!("{}{}", res.metadata().generate_name, rev);
        }
    }
}

/// A data structure that ensures the resources are unique by name, and kept in sorted order for
/// efficient lookup and deterministic ordering.
#[derive(derivative::Derivative)]
#[derivative(PartialEq, Hash)]
#[derive(Clone, Debug, Eq, PartialOrd, Ord)]
pub struct Resources<T>(imbl::Vector<Arc<T>>);

impl<T> Default for Resources<T> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<T: Meta + Clone> Resources<T> {
    pub fn insert(&mut self, res: T) {
        if let Some(existing) = self.get_mut(&res.metadata().name) {
            *existing = res;
        } else {
            let pos = self.get_insertion_pos(&res.metadata().name);
            self.0.insert(pos, Arc::new(res));
        }
    }

    fn get_insertion_pos(&self, k: &str) -> usize {
        match self
            .0
            .binary_search_by_key(&k.to_owned(), |t| t.metadata().name.clone())
        {
            Ok(p) => p,
            Err(p) => p,
        }
    }

    fn get_pos(&self, k: &str) -> Option<usize> {
        self.0
            .binary_search_by_key(&k.to_owned(), |t| t.metadata().name.clone())
            .ok()
    }

    pub fn has(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    pub fn get(&self, name: &str) -> Option<&T> {
        self.get_pos(name)
            .and_then(|p| self.0.get(p).map(|r| r.as_ref()))
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut T> {
        self.get_pos(name)
            .and_then(|p| self.0.get_mut(p).map(|r| Arc::make_mut(r)))
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.0.iter().map(|r| r.as_ref())
    }

    pub fn remove(&mut self, name: &str) -> Option<T> {
        self.get_pos(name).map(|p| (*self.0.remove(p)).clone())
    }

    pub fn retain(&mut self, f: impl Fn(&T) -> bool) {
        self.0.retain(|r| f(r))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn for_controller<'a>(&'a self, uid: &'a str) -> impl Iterator<Item = &T> + 'a {
        self.0
            .iter()
            .filter(move |t| t.metadata().owner_references.iter().any(|or| or.uid == uid))
            .map(|r| r.as_ref())
    }

    pub fn matching(&self, selector: LabelSelector) -> impl Iterator<Item = &T> {
        self.0
            .iter()
            .filter(move |t| selector.matches(&t.metadata().labels))
            .map(|r| r.as_ref())
    }

    pub fn to_vec(&self) -> Vec<&T> {
        self.iter().collect()
    }
}

impl<T: Meta + Clone> From<Vec<T>> for Resources<T> {
    fn from(value: Vec<T>) -> Self {
        let mut rv = Resources::default();
        for v in value {
            rv.insert(v);
        }
        rv
    }
}

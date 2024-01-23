use std::ops::{Deref, DerefMut};

use crate::controller::client::ClientState;
use crate::controller::ControllerStates;
use crate::resources::{
    ConditionStatus, ControllerRevision, Job, Meta, NodeCondition, NodeConditionType,
    PersistentVolumeClaim,
};
use crate::utils::{self, now};
use crate::{
    abstract_model::{Change, ControllerAction},
    resources::{Deployment, Node, Pod, ReplicaSet, StatefulSet},
};

use self::history::{ConsistencySetup, StateHistory};
use self::resources::Resources;
use self::revision::Revision;

pub mod history;
pub mod resources;
pub mod revision;

/// The history of the state, enabling generating views for different historical versions.
#[derive(Default, Debug, Clone, PartialEq, Eq, Hash)]
pub struct State {
    /// The changes that have been made to the state.
    states: StateHistory,

    controller_states: Vec<ControllerStates>,

    client_states: Vec<ClientState>,
}

impl State {
    pub fn new(initial_state: RawState, consistency_level: ConsistencySetup) -> Self {
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
    pub state: RawState,
}

impl From<RawState> for StateView {
    fn from(value: RawState) -> Self {
        StateView {
            state: value,
            ..Default::default()
        }
    }
}

#[derive(Default, Clone, Debug, Eq, PartialOrd, Ord, PartialEq, Hash)]
pub struct RawState {
    pub nodes: Resources<Node>,
    pub pods: Resources<Pod>,
    pub replicasets: Resources<ReplicaSet>,
    pub deployments: Resources<Deployment>,
    pub statefulsets: Resources<StatefulSet>,
    pub controller_revisions: Resources<ControllerRevision>,
    pub persistent_volume_claims: Resources<PersistentVolumeClaim>,
    pub jobs: Resources<Job>,
}

impl RawState {
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

    pub fn pods_for_node(&self, node: &str) -> Vec<&Pod> {
        self.pods
            .iter()
            .filter(|p| p.spec.node_name.as_ref().map_or(false, |n| n == node))
            .collect()
    }
}

impl Deref for StateView {
    type Target = RawState;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl DerefMut for StateView {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}

impl StateView {
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
                pod.metadata.uid = self.revision.to_string();
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
                rs.metadata.uid = self.revision.to_string();
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
                cr.metadata.uid = self.revision.to_string();
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
                pvc.metadata.uid = self.revision.to_string();
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

    fn fill_name<T: Meta>(&self, res: &mut T) {
        if res.metadata().name.is_empty() && !res.metadata().generate_name.is_empty() {
            let rev = &self.revision;
            res.metadata_mut().name = format!("{}{}", res.metadata().generate_name, rev);
        }
    }
}

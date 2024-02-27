use std::ops::{Deref, DerefMut};

use crate::controller::ControllerStates;
use crate::resources::{
    ConditionStatus, ControllerRevision, Job, Meta, NodeCondition, NodeConditionType,
    ObservedGeneration, PersistentVolumeClaim,
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
}

impl State {
    pub fn new(initial_state: RawState, consistency_level: ConsistencySetup) -> Self {
        Self {
            states: StateHistory::new(consistency_level, initial_state),
            controller_states: Vec::new(),
        }
    }

    /// Record a change for this state from a given controller.
    pub fn push_change(&mut self, change: Change) -> Revision {
        self.states.add_change(change)
    }

    /// Record changes for this state.
    pub fn push_changes(&mut self, changes: impl IntoIterator<Item = Change>) -> Revision {
        for change in changes {
            self.push_change(change);
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
    pub fn views(&self, min_revision: Revision) -> Vec<StateView> {
        self.states.states_for(min_revision)
    }

    pub fn add_controller(&mut self, controller_state: ControllerStates) {
        self.controller_states.push(controller_state);
    }

    pub fn update_controller(&mut self, controller: usize, controller_state: ControllerStates) {
        self.controller_states[controller] = controller_state;
    }

    pub fn get_controller(&self, controller: usize) -> &ControllerStates {
        &self.controller_states[controller]
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
    pub fn with_pods(mut self, pods: impl IntoIterator<Item = Pod>) -> Self {
        self.set_pods(pods);
        self
    }

    pub fn set_pods(&mut self, pods: impl IntoIterator<Item = Pod>) -> &mut Self {
        for pod in pods {
            let revision = pod
                .metadata
                .resource_version
                .as_str()
                .try_into()
                .unwrap_or_default();
            self.pods.insert(pod, revision).unwrap();
        }
        self
    }

    pub fn with_replicasets(mut self, replicasets: impl IntoIterator<Item = ReplicaSet>) -> Self {
        self.set_replicasets(replicasets);
        self
    }

    pub fn set_replicasets(
        &mut self,
        replicasets: impl IntoIterator<Item = ReplicaSet>,
    ) -> &mut Self {
        for replicaset in replicasets {
            let revision = replicaset
                .metadata
                .resource_version
                .as_str()
                .try_into()
                .unwrap_or_default();
            self.replicasets.insert(replicaset, revision).unwrap();
        }
        self
    }

    pub fn with_deployments(mut self, deployments: impl IntoIterator<Item = Deployment>) -> Self {
        self.set_deployments(deployments);
        self
    }

    pub fn set_deployments(
        &mut self,
        deployments: impl IntoIterator<Item = Deployment>,
    ) -> &mut Self {
        for deployment in deployments {
            let revision = deployment
                .metadata
                .resource_version
                .as_str()
                .try_into()
                .unwrap_or_default();
            self.deployments.insert(deployment, revision).unwrap();
        }
        self
    }

    pub fn with_statefulsets(
        mut self,
        statefulsets: impl IntoIterator<Item = StatefulSet>,
    ) -> Self {
        self.set_statefulsets(statefulsets);
        self
    }

    pub fn set_statefulsets(
        &mut self,
        statefulsets: impl IntoIterator<Item = StatefulSet>,
    ) -> &mut Self {
        for statefulset in statefulsets {
            let revision = statefulset
                .metadata
                .resource_version
                .as_str()
                .try_into()
                .unwrap_or_default();
            self.statefulsets.insert(statefulset, revision).unwrap();
        }
        self
    }

    pub fn with_jobs(mut self, jobs: impl IntoIterator<Item = Job>) -> Self {
        self.set_jobs(jobs);
        self
    }

    pub fn set_jobs(&mut self, jobs: impl IntoIterator<Item = Job>) -> &mut Self {
        for job in jobs {
            let revision = job
                .metadata
                .resource_version
                .as_str()
                .try_into()
                .unwrap_or_default();
            self.jobs.insert(job, revision).unwrap();
        }
        self
    }

    pub fn with_nodes(mut self, nodes: impl IntoIterator<Item = Node>) -> Self {
        self.set_nodes(nodes);
        self
    }

    pub fn set_nodes(&mut self, nodes: impl IntoIterator<Item = Node>) -> &mut Self {
        for node in nodes {
            let revision = node
                .metadata
                .resource_version
                .as_str()
                .try_into()
                .unwrap_or_default();
            self.nodes.insert(node, revision).unwrap();
        }
        self
    }

    pub fn pods_for_node(&self, node: &str) -> Vec<&Pod> {
        self.pods
            .iter()
            .filter(|p| p.spec.node_name.as_ref().map_or(false, |n| n == node))
            .collect()
    }

    pub fn merge(&self, other: &Self) -> Self {
        Self {
            nodes: self.nodes.merge(&other.nodes),
            pods: self.pods.merge(&other.pods),
            replicasets: self.replicasets.merge(&other.replicasets),
            deployments: self.deployments.merge(&other.deployments),
            statefulsets: self.statefulsets.merge(&other.statefulsets),
            controller_revisions: self.controller_revisions.merge(&other.controller_revisions),
            persistent_volume_claims: self
                .persistent_volume_claims
                .merge(&other.persistent_volume_claims),
            jobs: self.jobs.merge(&other.jobs),
        }
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
    pub fn apply_operation(&mut self, operation: ControllerAction, new_revision: Revision) {
        let mut s = self.clone();
        match s.apply_operation_inner(operation, new_revision.clone()) {
            Ok(()) => {
                s.revision = new_revision;
                *self = s;
            }
            Err(()) => {
                // don't update our self, basically abort the transaction so no changes
            }
        }
    }

    fn apply_operation_inner(
        &mut self,
        operation: ControllerAction,
        new_revision: Revision,
    ) -> Result<(), ()> {
        match operation {
            ControllerAction::NodeJoin(name, capacity) => {
                self.nodes.insert(
                    Node {
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
                    },
                    new_revision,
                )?;
            }
            ControllerAction::CreatePod(mut pod) => {
                pod.metadata.uid = self.revision.to_string();
                self.fill_name(&mut pod);
                self.pods.insert(pod, new_revision)?;
            }
            ControllerAction::UpdatePod(pod) => {
                self.pods.insert(pod, new_revision)?;
            }
            ControllerAction::SoftDeletePod(mut pod) => {
                // marked for deletion
                pod.metadata.deletion_timestamp = Some(now());
                self.pods.insert(pod, new_revision)?;
            }
            ControllerAction::HardDeletePod(pod) => {
                self.pods.remove(&pod.metadata.name);
            }
            ControllerAction::UpdateDeployment(dep) => {
                self.deployments.insert(dep, new_revision)?;
            }
            ControllerAction::RequeueDeployment(_dep) => {
                // skip
            }
            ControllerAction::UpdateDeploymentStatus(dep) => {
                self.deployments.insert(dep, new_revision)?;
            }
            ControllerAction::CreateReplicaSet(mut rs) => {
                rs.metadata.uid = self.revision.to_string();
                self.fill_name(&mut rs);
                self.replicasets.insert(rs, new_revision)?;
            }
            ControllerAction::UpdateReplicaSet(rs) => {
                self.replicasets.insert(rs, new_revision)?;
            }
            ControllerAction::UpdateReplicaSetStatus(rs) => {
                self.replicasets.insert(rs, new_revision)?;
            }
            ControllerAction::UpdateReplicaSets(rss) => {
                for rs in rss {
                    self.replicasets.insert(rs, new_revision.clone())?;
                }
            }
            ControllerAction::UpdateStatefulSet(sts) => {
                self.statefulsets.insert(sts, new_revision)?;
            }
            ControllerAction::UpdateStatefulSetStatus(sts) => {
                self.statefulsets.insert(sts, new_revision)?;
            }
            ControllerAction::CreateControllerRevision(mut cr) => {
                cr.metadata.uid = self.revision.to_string();
                self.fill_name(&mut cr);
                self.controller_revisions.insert(cr, new_revision)?;
            }
            ControllerAction::UpdateControllerRevision(cr) => {
                self.controller_revisions.insert(cr, new_revision)?;
            }
            ControllerAction::DeleteControllerRevision(cr) => {
                self.controller_revisions.remove(&cr.metadata.name);
            }
            ControllerAction::DeleteReplicaSet(rs) => {
                self.replicasets.remove(&rs.metadata.name);
            }
            ControllerAction::CreatePersistentVolumeClaim(mut pvc) => {
                pvc.metadata.uid = self.revision.to_string();
                self.fill_name(&mut pvc);
                self.persistent_volume_claims.insert(pvc, new_revision)?;
            }
            ControllerAction::UpdatePersistentVolumeClaim(pvc) => {
                self.persistent_volume_claims.insert(pvc, new_revision)?;
            }
            ControllerAction::UpdateJobStatus(job) => {
                self.jobs.insert(job, new_revision)?;
            }
            ControllerAction::UpdateJob(job) => {
                self.jobs.insert(job, new_revision)?;
            }
        }
        Ok(())
    }

    fn fill_name<T: Meta>(&self, res: &mut T) {
        if res.metadata().name.is_empty() && !res.metadata().generate_name.is_empty() {
            let rev = &self.revision;
            res.metadata_mut().name = format!("{}{}", res.metadata().generate_name, rev);
        }
    }

    pub fn resource_stable<T: Meta + ObservedGeneration>(&self, resource: &T) -> bool {
        // the controller has finished processing its updates
        resource.observed_generation() >= resource.metadata().generation
                    // and no other things have happened in the cluster since the update (e.g. a
                    // node dying which happens to remove pods)
                    && self.revision == resource.metadata().resource_version.as_str().try_into().unwrap()
    }

    pub fn resource_current<T: Meta>(&self, resource: &T) -> bool {
        // no other things have happened in the cluster since the update (e.g. a
        // node dying which happens to remove pods)
        self.revision
            == resource
                .metadata()
                .resource_version
                .as_str()
                .try_into()
                .unwrap()
    }

    pub fn resources_current<'a, T: Meta + 'a>(
        &self,
        resources: impl IntoIterator<Item = &'a T>,
    ) -> bool {
        resources.into_iter().all(|r| self.resource_current(r))
    }

    pub fn merge(&self, other: &Self) -> Self {
        Self {
            revision: self.revision.merge(&other.revision),
            state: self.state.merge(&other.state),
        }
    }
}

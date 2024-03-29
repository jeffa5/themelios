use std::borrow::Cow;
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

use self::history::{ConsistencySetup, History, StateHistory};
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
    pub fn push_change(&mut self, change: Change) {
        self.states.add_change(change)
    }

    /// Get the maximum revision for this change.
    pub fn max_revision(&self) -> Revision {
        self.states.max_revision()
    }

    /// Get a view for a specific revision in the change history.
    pub fn view_at(&self, revision: &Revision) -> Cow<StateView> {
        self.states.state_at(revision)
    }

    /// Get all the possible revisions under the given consistency level.
    pub fn revisions(&self, min_revision: Option<&Revision>) -> Vec<Revision> {
        self.states.valid_revisions(min_revision)
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

    pub fn latest(&self) -> Cow<StateView> {
        self.states.state_at(&self.max_revision())
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
            let revision = pod.metadata.resource_version.clone();
            self.pods.create(pod, revision).unwrap();
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
            let revision = replicaset.metadata.resource_version.clone();
            self.replicasets.create(replicaset, revision).unwrap();
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
            let revision = deployment.metadata.resource_version.clone();
            self.deployments.create(deployment, revision).unwrap();
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
            let revision = statefulset.metadata.resource_version.clone();
            self.statefulsets.create(statefulset, revision).unwrap();
        }
        self
    }

    pub fn with_jobs(mut self, jobs: impl IntoIterator<Item = Job>) -> Self {
        self.set_jobs(jobs);
        self
    }

    pub fn set_jobs(&mut self, jobs: impl IntoIterator<Item = Job>) -> &mut Self {
        for job in jobs {
            let revision = job.metadata.resource_version.clone();
            self.jobs.create(job, revision).unwrap();
        }
        self
    }

    pub fn with_nodes(mut self, nodes: impl IntoIterator<Item = Node>) -> Self {
        self.set_nodes(nodes);
        self
    }

    pub fn set_nodes(&mut self, nodes: impl IntoIterator<Item = Node>) -> &mut Self {
        for node in nodes {
            let revision = node.metadata.resource_version.clone();
            self.nodes.create(node, revision).unwrap();
        }
        self
    }

    pub fn pods_for_node(&self, node: &str) -> Vec<&Pod> {
        self.pods
            .iter()
            .filter(|p| p.spec.node_name.as_ref().map_or(false, |n| n == node))
            .collect()
    }

    pub fn merge(&mut self, other: &Self) {
        self.nodes.merge(&other.nodes);
        self.pods.merge(&other.pods);
        self.replicasets.merge(&other.replicasets);
        self.deployments.merge(&other.deployments);
        self.statefulsets.merge(&other.statefulsets);
        self.controller_revisions.merge(&other.controller_revisions);
        self.persistent_volume_claims
            .merge(&other.persistent_volume_claims);
        self.jobs.merge(&other.jobs);
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
    /// Apply the operation to the state, using the new revision.
    ///
    /// On success it applies the new revision and returns true.
    /// On failure it does nothing and returns false.
    #[must_use]
    pub fn apply_operation(&mut self, operation: ControllerAction, new_revision: Revision) -> bool {
        let mut s = self.clone();
        match s.apply_operation_inner(operation, new_revision.clone()) {
            Ok(()) => {
                s.revision = new_revision;
                *self = s;
                true
            }
            Err(()) => {
                // don't update our self, basically abort the transaction so no changes
                false
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
                self.nodes
                    .create(
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
                    )
                    .map_err(|_| ())?;
            }
            ControllerAction::DeleteNode(name) => {
                self.nodes.remove(&name);
            }
            ControllerAction::CreatePod(mut pod) => {
                pod.metadata.uid = self.revision.to_string();
                self.fill_name(&mut pod);
                self.pods.create(pod, new_revision).map_err(|_| ())?;
            }
            ControllerAction::UpdatePod(pod) => {
                self.pods.update(pod, new_revision).map_err(|_| ())?;
            }
            ControllerAction::SoftDeletePod(mut pod) => {
                // marked for deletion
                pod.metadata.deletion_timestamp = Some(now());
                self.pods.update(pod, new_revision).map_err(|_| ())?;
            }
            ControllerAction::HardDeletePod(pod) => {
                self.pods.remove(&pod);
            }
            ControllerAction::UpdateDeployment(dep) => {
                self.deployments.update(dep, new_revision).map_err(|_| ())?;
            }
            ControllerAction::RequeueDeployment(_dep) => {
                // skip
            }
            ControllerAction::UpdateDeploymentStatus(dep) => {
                self.deployments.update(dep, new_revision).map_err(|_| ())?;
            }
            ControllerAction::CreateReplicaSet(mut rs) => {
                rs.metadata.uid = self.revision.to_string();
                self.fill_name(&mut rs);
                self.replicasets.create(rs, new_revision).map_err(|_| ())?;
            }
            ControllerAction::UpdateReplicaSet(rs) => {
                self.replicasets.update(rs, new_revision).map_err(|_| ())?;
            }
            ControllerAction::UpdateReplicaSetStatus(rs) => {
                self.replicasets.update(rs, new_revision).map_err(|_| ())?;
            }
            ControllerAction::UpdateReplicaSets(rss) => {
                for rs in rss {
                    self.replicasets
                        .update(rs, new_revision.clone())
                        .map_err(|_| ())?;
                }
            }
            ControllerAction::UpdateStatefulSet(sts) => {
                self.statefulsets
                    .update(sts, new_revision)
                    .map_err(|_| ())?;
            }
            ControllerAction::UpdateStatefulSetStatus(sts) => {
                self.statefulsets
                    .update(sts, new_revision)
                    .map_err(|_| ())?;
            }
            ControllerAction::CreateControllerRevision(mut cr) => {
                cr.metadata.uid = self.revision.to_string();
                self.fill_name(&mut cr);
                self.controller_revisions
                    .create(cr, new_revision)
                    .map_err(|_| ())?;
            }
            ControllerAction::UpdateControllerRevision(cr) => {
                self.controller_revisions
                    .update(cr, new_revision)
                    .map_err(|_| ())?;
            }
            ControllerAction::DeleteControllerRevision(cr) => {
                self.controller_revisions.remove(&cr);
            }
            ControllerAction::DeleteReplicaSet(rs) => {
                self.replicasets.remove(&rs);
            }
            ControllerAction::CreatePersistentVolumeClaim(mut pvc) => {
                pvc.metadata.uid = self.revision.to_string();
                self.fill_name(&mut pvc);
                self.persistent_volume_claims
                    .create(pvc, new_revision)
                    .map_err(|_| ())?;
            }
            ControllerAction::UpdatePersistentVolumeClaim(pvc) => {
                self.persistent_volume_claims
                    .update(pvc, new_revision)
                    .map_err(|_| ())?;
            }
            ControllerAction::UpdateJobStatus(job) => {
                self.jobs.update(job, new_revision).map_err(|_| ())?;
            }
            ControllerAction::UpdateJob(job) => {
                self.jobs.update(job, new_revision).map_err(|_| ())?;
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
                    && self.revision == resource.metadata().resource_version
    }

    pub fn resource_current<T: Meta>(&self, resource: &T) -> bool {
        // no other things have happened in the cluster since the update (e.g. a
        // node dying which happens to remove pods)
        self.revision == resource.metadata().resource_version
    }

    pub fn resources_current<'a, T: Meta + 'a>(
        &self,
        resources: impl IntoIterator<Item = &'a T>,
    ) -> bool {
        resources.into_iter().all(|r| self.resource_current(r))
    }

    pub fn merge(&mut self, other: &Self) {
        self.revision.merge(&other.revision);
        self.state.merge(&other.state);
    }
}

use crate::{
    abstract_model::ControllerAction,
    resources::Pod,
    state::{revision::Revision, StateView},
};

use super::{util::is_pod_terminating, Controller};

#[derive(Clone, Debug)]
pub struct PodGCController;

#[derive(Debug, Default, Hash, Clone, PartialEq, Eq)]
pub struct PodGCControllerState {
    revision: Option<Revision>,
}

#[derive(Debug)]
pub enum PodGCAction {
    SoftDeletePod(Pod),
    HardDeletePod(Pod),
}

impl From<PodGCAction> for ControllerAction {
    fn from(value: PodGCAction) -> Self {
        match value {
            PodGCAction::SoftDeletePod(pod) => ControllerAction::SoftDeletePod(pod),
            PodGCAction::HardDeletePod(pod) => ControllerAction::HardDeletePod(pod),
        }
    }
}

impl Controller for PodGCController {
    type Action = PodGCAction;
    type State = PodGCControllerState;

    // https://kubernetes.io/docs/concepts/workloads/pods/pod-lifecycle/#pod-garbage-collection
    fn step(
        &self,
        global_state: &StateView,
        local_state: &mut Self::State,
    ) -> Option<Self::Action> {
        local_state.revision = Some(global_state.revision.clone());
        for pod in global_state.pods.iter() {
            // PodGC cleans up any Pods which satisfy any of the following conditions:
            // - are orphan Pods - bound to a node which no longer exists,
            if let Some(node_name) = &pod.spec.node_name {
                if !global_state.nodes.has(node_name) {
                    if pod.metadata.deletion_timestamp.is_none() {
                        return Some(PodGCAction::SoftDeletePod(pod.clone()));
                    } else {
                        return Some(PodGCAction::HardDeletePod(pod.clone()));
                    }
                }
            }
            // - are unscheduled terminating Pods,
            if pod.spec.node_name.is_none() && is_pod_terminating(pod) {
                return Some(PodGCAction::HardDeletePod(pod.clone()));
            }
            // - are terminating Pods, bound to a non-ready node tainted with node.kubernetes.io/out-of-service, when the NodeOutOfServiceVolumeDetach feature gate is enabled.
        }
        None
    }

    fn arbitrary_steps(&self, _local_state: &Self::State) -> Vec<Self::State> {
        Vec::new()
    }

    fn name(&self) -> String {
        "PodGC".to_owned()
    }

    fn min_revision_accepted<'a>(&self, state: &'a Self::State) -> Option<&'a Revision> {
        state.revision.as_ref()
    }
}

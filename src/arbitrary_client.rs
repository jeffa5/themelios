use crate::{abstract_model::ControllerAction, resources::PodPhase, state::StateView};

pub struct ArbitraryClient;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ArbitraryClientAction {
    ScaleDeployment(String, i32),
    ScaleStatefulSet(String, i32),
    ScaleReplicaSet(String, i32),

    ChangeImageDeployment(String, String),
    ChangeImageStatefulSet(String, String),
    ChangeImageReplicaSet(String, String),

    TogglePauseDeployment(String),

    ToggleSuspendJob(String),

    MarkSucceededPod(String),
    MarkFailedPod(String),
}

impl ArbitraryClient {
    pub fn actions(view: &StateView) -> Vec<ArbitraryClientAction> {
        let mut actions = Vec::new();
        // scale resources up
        macro_rules! scale_up {
            ($kind:ident, $update:expr) => {
                for res in view.$kind.iter() {
                    actions.push($update(res.metadata.name.clone(), 1));
                }
            };
        }
        scale_up!(deployments, ArbitraryClientAction::ScaleDeployment);

        macro_rules! scale_up_option {
            ($kind:ident, $update:expr) => {
                for res in view.$kind.iter() {
                    actions.push($update(res.metadata.name.clone(), 1));
                }
            };
        }
        scale_up_option!(statefulsets, ArbitraryClientAction::ScaleStatefulSet);
        scale_up_option!(replicasets, ArbitraryClientAction::ScaleReplicaSet);

        // scale resources down
        macro_rules! scale_down {
            ($kind:ident, $update:expr) => {
                for res in view.$kind.iter() {
                    if res.spec.replicas > 0 {
                        actions.push($update(res.metadata.name.clone(), -1));
                    }
                }
            };
        }
        scale_down!(deployments, ArbitraryClientAction::ScaleDeployment);

        macro_rules! scale_down_option {
            ($kind:ident, $update:expr) => {
                for res in view.$kind.iter() {
                    if res.spec.replicas.unwrap() > 0 {
                        actions.push($update(res.metadata.name.clone(), -1));
                    }
                }
            };
        }
        scale_down_option!(statefulsets, ArbitraryClientAction::ScaleStatefulSet);
        scale_down_option!(replicasets, ArbitraryClientAction::ScaleReplicaSet);

        // change image in templates
        macro_rules! change_image {
            ($kind:ident, $update:expr) => {
                for res in view.$kind.iter() {
                    if res.spec.template.spec.containers.is_empty() {
                        continue;
                    }
                    let image = &res.spec.template.spec.containers[0].image;
                    let new_image = format!("{}1", image);
                    actions.push($update(res.metadata.name.clone(), new_image));
                }
            };
        }
        change_image!(deployments, ArbitraryClientAction::ChangeImageDeployment);
        change_image!(statefulsets, ArbitraryClientAction::ChangeImageStatefulSet);
        change_image!(replicasets, ArbitraryClientAction::ChangeImageReplicaSet);

        // toggle deployments paused status
        macro_rules! toggle_pause {
            ($kind:ident, $update:expr) => {
                for res in view.$kind.iter() {
                    actions.push($update(res.metadata.name.clone()));
                }
            };
        }
        toggle_pause!(deployments, ArbitraryClientAction::TogglePauseDeployment);

        // toggle job suspension
        macro_rules! toggle_suspension {
            ($kind:ident, $update:expr) => {
                for res in view.$kind.iter() {
                    actions.push($update(res.metadata.name.clone()));
                }
            };
        }
        toggle_suspension!(jobs, ArbitraryClientAction::ToggleSuspendJob);

        // mark pods as succeeded or finished
        for pod in view.pods.iter() {
            if !matches!(
                pod.status.phase,
                PodPhase::Unknown | PodPhase::Succeeded | PodPhase::Failed
            ) {
                actions.push(ArbitraryClientAction::MarkSucceededPod(
                    pod.metadata.name.clone(),
                ));
                actions.push(ArbitraryClientAction::MarkFailedPod(
                    pod.metadata.name.clone(),
                ));
            }
        }

        actions
    }

    pub fn controller_action(state: &StateView, action: ArbitraryClientAction) -> ControllerAction {
        match action {
            ArbitraryClientAction::ScaleDeployment(name, by) => {
                let mut res = state.deployments.get(&name).unwrap().clone();
                res.spec.replicas = (res.spec.replicas as i32 + by) as u32;
                ControllerAction::UpdateDeployment(res)
            }
            ArbitraryClientAction::ScaleStatefulSet(name, by) => {
                let mut res = state.statefulsets.get(&name).unwrap().clone();
                res.spec.replicas = Some((res.spec.replicas.unwrap_or(1) as i32 + by) as u32);
                ControllerAction::UpdateStatefulSet(res)
            }
            ArbitraryClientAction::ScaleReplicaSet(name, by) => {
                let mut res = state.replicasets.get(&name).unwrap().clone();
                res.spec.replicas = Some((res.spec.replicas.unwrap_or(1) as i32 + by) as u32);
                ControllerAction::UpdateReplicaSet(res)
            }
            ArbitraryClientAction::ChangeImageDeployment(name, image) => {
                let mut res = state.deployments.get(&name).unwrap().clone();
                res.spec.template.spec.containers[0].image = image;
                ControllerAction::UpdateDeployment(res)
            }
            ArbitraryClientAction::ChangeImageStatefulSet(name, image) => {
                let mut res = state.statefulsets.get(&name).unwrap().clone();
                res.spec.template.spec.containers[0].image = image;
                ControllerAction::UpdateStatefulSet(res)
            }
            ArbitraryClientAction::ChangeImageReplicaSet(name, image) => {
                let mut res = state.replicasets.get(&name).unwrap().clone();
                res.spec.template.spec.containers[0].image = image;
                ControllerAction::UpdateReplicaSet(res)
            }
            ArbitraryClientAction::TogglePauseDeployment(name) => {
                let mut res = state.deployments.get(&name).unwrap().clone();
                res.spec.paused = !res.spec.paused;
                ControllerAction::UpdateDeployment(res)
            }
            ArbitraryClientAction::ToggleSuspendJob(name) => {
                let mut res = state.jobs.get(&name).unwrap().clone();
                res.spec.suspend = !res.spec.suspend;
                ControllerAction::UpdateJob(res)
            }
            ArbitraryClientAction::MarkSucceededPod(name) => {
                let mut res = state.pods.get(&name).unwrap().clone();
                res.status.phase = PodPhase::Succeeded;
                res.status.conditions.clear();
                ControllerAction::UpdatePod(res)
            }
            ArbitraryClientAction::MarkFailedPod(name) => {
                let mut res = state.pods.get(&name).unwrap().clone();
                res.status.phase = PodPhase::Failed;
                res.status.conditions.clear();
                ControllerAction::UpdatePod(res)
            }
        }
    }
}

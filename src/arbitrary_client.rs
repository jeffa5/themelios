use crate::{
    abstract_model::ControllerAction,
    resources::{Deployment, Job, PodPhase, ReplicaSet, StatefulSet},
    state::StateView,
};

pub struct ArbitraryClient;

fn update_deployment(res: Deployment) -> ControllerAction {
    ControllerAction::UpdateDeployment(res)
}
fn update_job(res: Job) -> ControllerAction {
    ControllerAction::UpdateJob(res)
}
fn update_statefulset(res: StatefulSet) -> ControllerAction {
    ControllerAction::UpdateStatefulSet(res)
}
fn update_replicaset(res: ReplicaSet) -> ControllerAction {
    ControllerAction::UpdateReplicaSet(res)
}

impl ArbitraryClient {
    pub fn actions(&self, view: &StateView) -> Vec<ControllerAction> {
        let mut actions = Vec::new();
        // scale resources up
        macro_rules! scale_up {
            ($kind:ident, $update:expr) => {
                for res in view.$kind.iter() {
                    let mut res = res.clone();
                    res.spec.replicas += 1;
                    actions.push($update(res));
                }
            };
        }
        scale_up!(deployments, update_deployment);

        macro_rules! scale_up_option {
            ($kind:ident, $update:expr) => {
                for res in view.$kind.iter() {
                    let mut res = res.clone();
                    res.spec.replicas = Some(res.spec.replicas.unwrap_or(1) + 1);
                    actions.push($update(res));
                }
            };
        }
        scale_up_option!(statefulsets, update_statefulset);
        scale_up_option!(replicasets, update_replicaset);

        // scale resources down
        macro_rules! scale_down {
            ($kind:ident, $update:expr) => {
                for res in view.$kind.iter() {
                    let mut res = res.clone();
                    if res.spec.replicas > 0 {
                        res.spec.replicas -= 1;
                        actions.push($update(res));
                    }
                }
            };
        }
        scale_down!(deployments, update_deployment);

        macro_rules! scale_down_option {
            ($kind:ident, $update:expr) => {
                for res in view.$kind.iter() {
                    let mut res = res.clone();
                    if res.spec.replicas.unwrap() > 0 {
                        res.spec.replicas = res.spec.replicas.map(|r| r - 1);
                        actions.push($update(res));
                    }
                }
            };
        }
        scale_down_option!(statefulsets, update_statefulset);
        scale_down_option!(replicasets, update_replicaset);

        // change image in templates
        macro_rules! change_image {
            ($kind:ident, $update:expr) => {
                for res in view.$kind.iter() {
                    let mut res = res.clone();
                    if res.spec.template.spec.containers.is_empty() {
                        continue
                    }
                    let image = &res.spec.template.spec.containers[0].image;
                    let new_image = format!("{}1", image);
                    res.spec.template.spec.containers[0].image = new_image;
                    actions.push($update(res));
                }
            };
        }
        change_image!(deployments, update_deployment);
        change_image!(statefulsets, update_statefulset);
        change_image!(replicasets, update_replicaset);

        // toggle deployments paused status
        macro_rules! toggle_pause {
            ($kind:ident, $update:expr) => {
                for res in view.$kind.iter() {
                    let mut res = res.clone();
                    res.spec.paused = !res.spec.paused;
                    actions.push($update(res));
                }
            };
        }
        toggle_pause!(deployments, update_deployment);

        // toggle job suspension
        macro_rules! toggle_suspension {
            ($kind:ident, $update:expr) => {
                for res in view.$kind.iter() {
                    let mut res = res.clone();
                    res.spec.suspend = !res.spec.suspend;
                    actions.push($update(res));
                }
            };
        }
        toggle_suspension!(jobs, update_job);

        // mark pods as succeeded or finished
        for pod in view.pods.iter() {
            let mut pod = pod.clone();
            if !matches!(pod.status.phase, PodPhase::Succeeded | PodPhase::Failed) {
                pod.status.phase = PodPhase::Succeeded;
                actions.push(ControllerAction::UpdatePod(pod.clone()));
                pod.status.phase = PodPhase::Failed;
                actions.push(ControllerAction::UpdatePod(pod));
            }
        }

        actions
    }
}

use crate::resources::Deployment;
use crate::state::StateView;

// Just a deployment client for now
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Client {
    pub change_image: u8,
    pub scale_up: u8,
    pub scale_down: u8,
}

#[derive(Debug)]
pub enum ClientAction {
    UpdateDeployment(Deployment),
}

impl Client {
    pub fn actions(&self, _i: usize, view: &StateView, state: &mut Self) -> Vec<ClientAction> {
        let mut actions = Vec::new();

        if state.change_image > 0 {
            state.change_image -= 1;
            for deployment in view.deployments.iter() {
                let mut d = deployment.clone();
                d.spec.template.spec.containers[0].image = format!("image2");
                actions.push(ClientAction::UpdateDeployment(d));
            }
        }

        if state.scale_up > 0 {
            state.scale_up -= 1;
            for deployment in view.deployments.iter() {
                let mut d = deployment.clone();
                d.spec.replicas += 1;
                actions.push(ClientAction::UpdateDeployment(d));
            }
        }

        if state.scale_down > 0 {
            state.scale_down -= 1;
            for deployment in view.deployments.iter() {
                let mut d = deployment.clone();
                d.spec.replicas += 1;
                actions.push(ClientAction::UpdateDeployment(d));
            }
        }

        actions
    }
}

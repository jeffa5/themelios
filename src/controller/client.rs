use crate::resources::Deployment;
use crate::state::StateView;

// Just a deployment client for now
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Client {
    pub deployment_name: String,
    pub change_image: u8,
    pub scale_up: u8,
    pub scale_down: u8,
}

impl Client {
    pub fn new_state(&self) -> ClientState {
        ClientState {
            change_image: self.change_image,
            scale_up: self.scale_up,
            scale_down: self.scale_down,
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ClientState {
    pub change_image: u8,
    pub scale_up: u8,
    pub scale_down: u8,
}

#[derive(Debug)]
pub enum ClientAction {
    UpdateDeployment(Deployment),
}

impl Client {
    pub fn actions(
        &self,
        _i: usize,
        view: &StateView,
        state: &ClientState,
    ) -> Vec<(ClientState, ClientAction)> {
        let mut actions = Vec::new();
        if let Some(deployment) = view.deployments.get(&self.deployment_name) {
            if state.change_image > 0 {
                let mut state = state.clone();
                state.change_image -= 1;
                let mut d = deployment.clone();
                d.spec.template.spec.containers[0].image = "image2".to_string();
                actions.push((state, ClientAction::UpdateDeployment(d)));
            }

            if state.scale_up > 0 {
                let mut state = state.clone();
                state.scale_up -= 1;
                let mut d = deployment.clone();
                d.spec.replicas += 1;
                actions.push((state.clone(), ClientAction::UpdateDeployment(d)));
            }

            if state.scale_down > 0 {
                let mut state = state.clone();
                state.scale_down -= 1;
                let mut d = deployment.clone();
                d.spec.replicas += 1;
                actions.push((state.clone(), ClientAction::UpdateDeployment(d)));
            }
        }

        actions
    }
}

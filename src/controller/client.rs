use crate::{abstract_model::ControllerAction, state::StateView};

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

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ClientAction {
    ScaleUp,
    ScaleDown,
    ChangeImage(String),
}

impl Client {
    pub fn actions(
        &self,
        _i: usize,
        view: &StateView,
        state: &ClientState,
    ) -> Vec<(ClientState, ClientAction)> {
        let mut actions = Vec::new();
        if view.deployments.has(&self.deployment_name) {
            if state.change_image > 0 {
                let mut state = state.clone();
                state.change_image -= 1;
                actions.push((state, ClientAction::ChangeImage("image2".to_owned())));
            }

            if state.scale_up > 0 {
                let mut state = state.clone();
                state.scale_up -= 1;
                actions.push((state.clone(), ClientAction::ScaleUp));
            }

            if state.scale_down > 0 {
                let mut state = state.clone();
                state.scale_down -= 1;
                actions.push((state.clone(), ClientAction::ScaleDown));
            }
        }

        actions
    }

    pub fn apply(&self, view: &StateView, action: ClientAction) -> ControllerAction {
        let mut deployment = view.deployments.get(&self.deployment_name).unwrap().clone();
        match action {
            ClientAction::ScaleUp => {
                deployment.spec.replicas += 1;
                ControllerAction::UpdateDeployment(deployment)
            }
            ClientAction::ScaleDown => {
                deployment.spec.replicas = deployment.spec.replicas.saturating_sub(1);
                ControllerAction::UpdateDeployment(deployment)
            }
            ClientAction::ChangeImage(new_image) => {
                deployment.spec.template.spec.containers[0].image = new_image;
                ControllerAction::UpdateDeployment(deployment)
            }
        }
    }
}
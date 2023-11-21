use crate::{abstract_model::ControllerAction, state::StateView};

// Just a deployment client for now
#[derive(Debug, Default, Clone, Hash, PartialEq, Eq)]
pub struct Client {
    pub deployment_name: String,
    pub change_image: u8,
    pub scale_up: u8,
    pub scale_down: u8,
    pub actions: Vec<ClientAction>,
}

impl Client {
    pub fn new_state(&self) -> ClientState {
        ClientState {
            change_image: self.change_image,
            scale_up: self.scale_up,
            scale_down: self.scale_down,
            actions: self.actions.clone(),
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ClientState {
    pub change_image: u8,
    pub scale_up: u8,
    pub scale_down: u8,
    pub actions: Vec<ClientAction>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ClientAction {
    ScaleUp,
    ScaleDown,
    ChangeImage,
}

impl Client {
    pub fn actions(
        &self,
        _i: usize,
        view: &StateView,
        state: &ClientState,
    ) -> Vec<(ClientState, ClientAction)> {
        let mut actions = Vec::new();
        if self.actions.is_empty() {
            if view.deployments.has(&self.deployment_name) {
                if state.change_image > 0 {
                    let mut state = state.clone();
                    state.change_image -= 1;
                    actions.push((state, ClientAction::ChangeImage));
                }

                if state.scale_up > 0 {
                    let mut state = state.clone();
                    state.scale_up -= 1;
                    actions.push((state, ClientAction::ScaleUp));
                }

                if state.scale_down > 0 {
                    let mut state = state.clone();
                    state.scale_down -= 1;
                    actions.push((state, ClientAction::ScaleDown));
                }
            }
        } else {
            for i in 0..state.actions.len() {
                let mut state = state.clone();
                let action = state.actions.remove(i);
                actions.push((state, action));
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
            ClientAction::ChangeImage => {
                let image = &deployment.spec.template.spec.containers[0].image;
                let pos = image.rfind(|c: char| c.is_numeric());
                let new_image = if let Some(pos) = pos {
                    let n: u32 = image[pos..].parse().unwrap();
                    format!("{}{}", image, n)
                } else {
                    format!("{}1", image)
                };
                deployment.spec.template.spec.containers[0].image = new_image;
                ControllerAction::UpdateDeployment(deployment)
            }
        }
    }
}

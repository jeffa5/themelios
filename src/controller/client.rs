use crate::{abstract_model::ControllerAction, state::StateView};

// Just a deployment client for now
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Client {
    pub deployment_name: String,
    pub initial_state: ClientState,
}

impl Client {
    pub fn new_state(&self) -> ClientState {
        self.initial_state.clone()
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum ClientState {
    Auto(ClientStateAuto),
    Manual(ClientStateManual),
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ClientStateAuto {
    pub change_image: u8,
    pub scale_up: u8,
    pub scale_down: u8,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ClientStateManual {
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
        match state {
            ClientState::Auto(auto) => {
                if view.deployments.has(&self.deployment_name) {
                    if auto.change_image > 0 {
                        let mut auto = auto.clone();
                        auto.change_image -= 1;
                        actions.push((ClientState::Auto(auto), ClientAction::ChangeImage));
                    }

                    if auto.scale_up > 0 {
                        let mut auto = auto.clone();
                        auto.scale_up -= 1;
                        actions.push((ClientState::Auto(auto), ClientAction::ScaleUp));
                    }

                    if auto.scale_down > 0 {
                        let mut auto = auto.clone();
                        auto.scale_down -= 1;
                        actions.push((ClientState::Auto(auto), ClientAction::ScaleDown));
                    }
                }
            }
            ClientState::Manual(manual) => {
                for i in 0..manual.actions.len() {
                    let mut manual = manual.clone();
                    let action = manual.actions.remove(i);
                    actions.push((ClientState::Manual(manual), action));
                }
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

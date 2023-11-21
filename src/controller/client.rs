use crate::{abstract_model::ControllerAction, state::StateView};

// Just a deployment client for now
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Client {
    pub name: String,
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
    pub toggle_pause: u8,
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
    TogglePause,
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
                if view.deployments.has(&self.name) || view.statefulsets.has(&self.name) {
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

                    if auto.toggle_pause > 0 {
                        let mut auto = auto.clone();
                        auto.toggle_pause -= 1;
                        actions.push((ClientState::Auto(auto), ClientAction::TogglePause));
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
        let deployment = view.deployments.get(&self.name).cloned();
        let statefulset = view.statefulsets.get(&self.name).cloned();

        match action {
            ClientAction::ScaleUp => {
                if let Some(mut d) = deployment {
                    d.spec.replicas += 1;
                    ControllerAction::UpdateDeployment(d)
                } else if let Some(mut s) = statefulset {
                    s.spec.replicas = Some(s.spec.replicas.unwrap_or(1) + 1);
                    ControllerAction::UpdateStatefulSet(s)
                } else {
                    unreachable!()
                }
            }
            ClientAction::ScaleDown => {
                if let Some(mut d) = deployment {
                    d.spec.replicas = d.spec.replicas.saturating_sub(1);
                    ControllerAction::UpdateDeployment(d)
                } else if let Some(mut s) = statefulset {
                    s.spec.replicas = s.spec.replicas.map(|r| r.saturating_sub(1));
                    ControllerAction::UpdateStatefulSet(s)
                } else {
                    unreachable!()
                }
            }
            ClientAction::TogglePause => {
                if let Some(mut d) = deployment {
                    d.spec.paused = !d.spec.paused;
                    ControllerAction::UpdateDeployment(d)
                } else {
                    unreachable!()
                }
            }
            ClientAction::ChangeImage => {
                let image = if let Some(d) = &deployment {
                    d.spec.template.spec.containers[0].image.clone()
                } else if let Some(s) = &statefulset {
                    s.spec.template.spec.containers[0].image.clone()
                } else {
                    unreachable!()
                };
                let pos = image.rfind(|c: char| c.is_numeric());
                let new_image = if let Some(pos) = pos {
                    let n: u32 = image[pos..].parse().unwrap();
                    format!("{}{}", image, n)
                } else {
                    format!("{}1", image)
                };
                if let Some(mut d) = deployment {
                    d.spec.template.spec.containers[0].image = new_image;
                    ControllerAction::UpdateDeployment(d)
                } else if let Some(mut s) = statefulset {
                    s.spec.template.spec.containers[0].image = new_image;
                    ControllerAction::UpdateStatefulSet(s)
                } else {
                    unreachable!()
                }
            }
        }
    }
}

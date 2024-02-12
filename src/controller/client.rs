use std::collections::BTreeSet;

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

#[derive(Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum ClientAction {
    ScaleUp,
    ScaleDown,
    ChangeImage,
    TogglePause,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ClientState {
    /// Process the client actions in the order from first to last
    Ordered(Vec<ClientAction>),
    /// Process the client actions in any order.
    Unordered(Vec<ClientAction>),
}

impl Default for ClientState {
    fn default() -> Self {
        ClientState::Ordered(Vec::new())
    }
}

impl ClientState {
    pub fn new_ordered() -> Self {
        Self::Ordered(Vec::new())
    }

    pub fn new_unordered() -> Self {
        Self::Unordered(Vec::new())
    }

    pub fn len(&self) -> usize {
        match self {
            ClientState::Ordered(a) => a,
            ClientState::Unordered(a) => a,
        }
        .len()
    }

    pub fn is_empty(&self) -> bool {
        match self {
            ClientState::Ordered(a) => a,
            ClientState::Unordered(a) => a,
        }
        .is_empty()
    }

    fn push_action(&mut self, action: ClientAction) {
        match self {
            ClientState::Ordered(a) => a,
            ClientState::Unordered(a) => a,
        }
        .push(action)
    }

    pub fn set_change_images(&mut self, n: usize) -> &mut Self {
        for _ in 0..n {
            self.push_action(ClientAction::ChangeImage)
        }
        self
    }

    pub fn set_scale_ups(&mut self, n: usize) -> &mut Self {
        for _ in 0..n {
            self.push_action(ClientAction::ScaleUp)
        }
        self
    }

    pub fn set_scale_downs(&mut self, n: usize) -> &mut Self {
        for _ in 0..n {
            self.push_action(ClientAction::ScaleDown)
        }
        self
    }

    pub fn set_toggle_pauses(&mut self, n: usize) -> &mut Self {
        for _ in 0..n {
            self.push_action(ClientAction::TogglePause)
        }
        self
    }

    pub fn with_change_images(mut self, n: usize) -> Self {
        self.set_change_images(n);
        self
    }

    pub fn with_scale_ups(mut self, n: usize) -> Self {
        self.set_scale_ups(n);
        self
    }

    pub fn with_scale_downs(mut self, n: usize) -> Self {
        self.set_scale_downs(n);
        self
    }

    pub fn with_toggle_pauses(mut self, n: usize) -> Self {
        self.set_toggle_pauses(n);
        self
    }
}

impl Client {
    pub fn actions(
        &self,
        _i: usize,
        _view: &StateView,
        state: &ClientState,
    ) -> Vec<(ClientState, ClientAction)> {
        let mut possible_actions = Vec::new();
        match state {
            ClientState::Ordered(actions) => {
                // just pop the first one and continue from there
                let mut actions = actions.clone();
                if !actions.is_empty() {
                    let action = actions.remove(0);
                    possible_actions.push((ClientState::Ordered(actions), action));
                }
            }
            ClientState::Unordered(actions) => {
                // propose one action of each kind
                let unique = actions.iter().collect::<BTreeSet<_>>();
                for action in unique {
                    let pos = actions.iter().position(|a| a == action).unwrap();
                    let mut actions = actions.clone();
                    let action = actions.remove(pos);
                    possible_actions.push((ClientState::Unordered(actions), action));
                }
            }
        }
        possible_actions
    }

    pub fn apply(&self, view: &StateView, action: ClientAction) -> ControllerAction {
        let deployment = view.deployments.get(&self.name).cloned();
        let statefulset = view.statefulsets.get(&self.name).cloned();
        let replicaset = view.replicasets.get(&self.name).cloned();

        match action {
            ClientAction::ScaleUp => {
                if let Some(mut d) = deployment {
                    d.spec.replicas += 1;
                    ControllerAction::UpdateDeployment(d)
                } else if let Some(mut s) = statefulset {
                    s.spec.replicas = Some(s.spec.replicas.unwrap_or(1) + 1);
                    ControllerAction::UpdateStatefulSet(s)
                } else if let Some(mut r) = replicaset {
                    r.spec.replicas = Some(r.spec.replicas.unwrap_or(1) + 1);
                    ControllerAction::UpdateReplicaSet(r)
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
                } else if let Some(mut r) = replicaset {
                    r.spec.replicas = r.spec.replicas.map(|r| r.saturating_sub(1));
                    ControllerAction::UpdateReplicaSet(r)
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
                } else if let Some(r) = &replicaset {
                    r.spec.template.spec.containers[0].image.clone()
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
                } else if let Some(mut r) = replicaset {
                    r.spec.template.spec.containers[0].image = new_image;
                    ControllerAction::UpdateReplicaSet(r)
                } else {
                    unreachable!()
                }
            }
        }
    }
}

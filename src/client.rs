use stateright::actor::{Actor, Id, Out};

use crate::{
    app::App,
    root::{RootMsg, RootTimer},
};

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Client {
    /// Id of the datastore node.
    pub datastore: Id,

    pub initial_apps: u32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct ClientState {}

impl Actor for Client {
    type Msg = RootMsg;

    type State = ClientState;

    type Timer = RootTimer;

    fn on_start(&self, _id: Id, o: &mut Out<Self>) -> Self::State {
        for i in 0..self.initial_apps {
            o.send(self.datastore, RootMsg::CreateAppRequest(App { id: i }));
        }
        ClientState::default()
    }

    fn on_msg(
        &self,
        _id: Id,
        _state: &mut std::borrow::Cow<Self::State>,
        _src: Id,
        msg: Self::Msg,
        _o: &mut Out<Self>,
    ) {
        match msg {
            RootMsg::NodeJoin => todo!(),
            RootMsg::NodesRequest => todo!(),
            RootMsg::NodesResponse(_) => todo!(),
            RootMsg::UnscheduledAppsRequest => todo!(),
            RootMsg::UnscheduledAppsResponse(_) => todo!(),
            RootMsg::ScheduleAppRequest(_, _) => todo!(),
            RootMsg::ScheduleAppResponse(_) => {}
            RootMsg::GetAppsForNodeRequest(_) => todo!(),
            RootMsg::GetAppsForNodeResponse(_) => todo!(),
            RootMsg::CreateAppRequest(_) => todo!(),
            RootMsg::CreateAppResponse(_) => todo!(),
        }
    }
}

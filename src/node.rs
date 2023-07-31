use crate::{app::App, root::RootTimer};
use stateright::actor::{Actor, Id, Out};
use std::borrow::Cow;

use crate::root::RootMsg;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Node {
    /// The id of the datastore node.
    pub datastore: Id,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct NodeState {
    running_apps: Vec<App>,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum NodeTimer {}

impl Actor for Node {
    type Msg = RootMsg;

    type State = NodeState;

    type Timer = RootTimer;

    fn on_start(&self, _id: Id, o: &mut Out<Self>) -> Self::State {
        o.send(self.datastore, RootMsg::NodeJoin);
        NodeState::default()
    }

    fn on_msg(
        &self,
        _id: Id,
        state: &mut Cow<Self::State>,
        _src: Id,
        msg: Self::Msg,
        _o: &mut Out<Self>,
    ) {
        match msg {
            RootMsg::NodeJoin => todo!(),
            RootMsg::SchedulerJoin => todo!(),
            RootMsg::ScheduledAppEvent(app) => {
                state.to_mut().running_apps.push(app);
            }
            RootMsg::ScheduleAppRequest(_, _) => todo!(),
            RootMsg::ScheduleAppResponse(_) => todo!(),
            RootMsg::CreateAppRequest(_) => todo!(),
            RootMsg::CreateAppResponse(_) => todo!(),
            RootMsg::NodeJoinedEvent(_) => todo!(),
            RootMsg::NewAppEvent(_) => todo!(),
        }
    }

    fn name(&self) -> String {
        "Node".to_owned()
    }
}

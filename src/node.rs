use crate::{app::App, datastore::DatastoreMsg, root::RootTimer};
use stateright::actor::{model_timeout, Actor, Id, Out};
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
pub enum NodeMsg {}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum NodeTimer {
    GetNewApps,
}

impl Actor for Node {
    type Msg = RootMsg;

    type State = NodeState;

    type Timer = RootTimer;

    fn on_start(&self, _id: Id, o: &mut Out<Self>) -> Self::State {
        o.send(self.datastore, RootMsg::Datastore(DatastoreMsg::NodeJoin));
        o.set_timer(RootTimer::Node(NodeTimer::GetNewApps), model_timeout());
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
            RootMsg::Scheduler(_) => todo!(),
            RootMsg::Node(_) => todo!(),
            RootMsg::Datastore(d) => match d {
                DatastoreMsg::NodeJoin => todo!(),
                DatastoreMsg::GetAppsForNodeRequest(_) => todo!(),
                DatastoreMsg::GetAppsForNodeResponse(apps) => {
                    state.to_mut().running_apps = apps;
                }
                DatastoreMsg::NodesRequest => todo!(),
                DatastoreMsg::NodesResponse(_) => todo!(),
                DatastoreMsg::UnscheduledAppsRequest => todo!(),
                DatastoreMsg::UnscheduledAppsResponse(_) => todo!(),
                DatastoreMsg::ScheduleAppRequest(_, _) => todo!(),
                DatastoreMsg::ScheduleAppResponse(_) => todo!(),
            },
        }
    }

    fn on_timeout(
        &self,
        id: Id,
        _state: &mut Cow<Self::State>,
        timer: &Self::Timer,
        o: &mut Out<Self>,
    ) {
        match timer {
            RootTimer::Node(NodeTimer::GetNewApps) => {
                o.send(
                    self.datastore,
                    RootMsg::Datastore(DatastoreMsg::GetAppsForNodeRequest(id)),
                );
                o.set_timer(RootTimer::Node(NodeTimer::GetNewApps), model_timeout());
            }
        }
    }
}

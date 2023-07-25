use stateright::actor::{Actor, Id, Out};

use crate::root::{RootMsg, RootTimer};

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Scheduler {
    /// Id of the datastore node.
    pub datastore: Id,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct SchedulerState {
    /// The current view of the nodes.
    nodes: Vec<Id>,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum SchedulerMsg {}

impl Actor for Scheduler {
    type Msg = RootMsg;

    type State = SchedulerState;

    type Timer = RootTimer;

    fn on_start(&self, _id: Id, o: &mut Out<Self>) -> Self::State {
        o.send(self.datastore, RootMsg::NodesRequest);
        o.send(self.datastore, RootMsg::UnscheduledAppsRequest);
        SchedulerState::default()
    }

    fn on_msg(
        &self,
        _id: Id,
        state: &mut std::borrow::Cow<Self::State>,
        src: Id,
        msg: Self::Msg,
        o: &mut Out<Self>,
    ) {
        match msg {
            RootMsg::NodeJoin => todo!(),
            RootMsg::NodesRequest => todo!(),
            RootMsg::NodesResponse(nodes) => {
                state.to_mut().nodes = nodes;
            }
            RootMsg::UnscheduledAppsRequest => todo!(),
            RootMsg::UnscheduledAppsResponse(apps) => {
                for app in apps {
                    if let Some(node) = state.nodes.first() {
                        // TODO: use an actual scheduling strategy
                        o.send(src, RootMsg::ScheduleAppRequest(app, *node));
                    }
                }
            }
            RootMsg::ScheduleAppRequest(_, _) => todo!(),
            RootMsg::ScheduleAppResponse(_) => {}
            RootMsg::GetAppsForNodeRequest(_) => todo!(),
            RootMsg::GetAppsForNodeResponse(_) => todo!(),
        }
    }
}

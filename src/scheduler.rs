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
pub enum SchedulerTimer {}

impl Actor for Scheduler {
    type Msg = RootMsg;

    type State = SchedulerState;

    type Timer = RootTimer;

    fn on_start(&self, _id: Id, o: &mut Out<Self>) -> Self::State {
        o.send(self.datastore, RootMsg::SchedulerJoin);
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
            RootMsg::SchedulerJoin => todo!(),
            RootMsg::NodeJoinedEvent(node) => {
                state.to_mut().nodes.push(node);
            }
            RootMsg::NewAppEvent(app) => {
                if let Some(node) = state.nodes.first() {
                    // TODO: use an actual scheduling strategy
                    o.send(src, RootMsg::ScheduleAppRequest(app, *node));
                }
            }
            RootMsg::ScheduledAppEvent(_) => todo!(),
            RootMsg::ScheduleAppRequest(_, _) => todo!(),
            RootMsg::ScheduleAppResponse(_) => {}
            RootMsg::CreateAppRequest(_) => todo!(),
            RootMsg::CreateAppResponse(_) => todo!(),
        }
    }

    fn name(&self) -> String {
        "Scheduler".to_owned()
    }
}

use stateright::actor::{model_timeout, Actor, Id, Out};

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
pub enum SchedulerTimer {
    GetNodes,
    GetApps,
}

impl Actor for Scheduler {
    type Msg = RootMsg;

    type State = SchedulerState;

    type Timer = RootTimer;

    fn on_start(&self, _id: Id, o: &mut Out<Self>) -> Self::State {
        o.set_timer(
            RootTimer::Scheduler(SchedulerTimer::GetNodes),
            model_timeout(),
        );
        o.set_timer(
            RootTimer::Scheduler(SchedulerTimer::GetApps),
            model_timeout(),
        );
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
            RootMsg::CreateAppRequest(_) => todo!(),
            RootMsg::CreateAppResponse(_) => todo!(),
        }
    }

    fn on_timeout(
        &self,
        _id: Id,
        _state: &mut std::borrow::Cow<Self::State>,
        timer: &Self::Timer,
        o: &mut Out<Self>,
    ) {
        match timer {
            RootTimer::Scheduler(s) => match s {
                SchedulerTimer::GetNodes => {
                    o.send(self.datastore, RootMsg::NodesRequest);
                    o.set_timer(timer.clone(), model_timeout());
                }
                SchedulerTimer::GetApps => {
                    o.send(self.datastore, RootMsg::UnscheduledAppsRequest);
                    o.set_timer(timer.clone(), model_timeout());
                }
            },
            RootTimer::Node(_) => todo!(),
        }
    }
}

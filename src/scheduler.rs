use stateright::actor::{Actor, Id, Out};

use crate::{datastore::DatastoreMsg, root::RootMsg};

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Scheduler {
    pub datastore: Id,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct SchedulerState {
    nodes: Vec<Id>,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum SchedulerMsg {
    Empty,
}

impl Actor for Scheduler {
    type Msg = RootMsg;

    type State = SchedulerState;

    type Timer = ();

    fn on_start(&self, _id: Id, o: &mut Out<Self>) -> Self::State {
        o.send(self.datastore, RootMsg::Datastore(DatastoreMsg::NodesRequest));
        o.send(
            self.datastore,
            RootMsg::Datastore(DatastoreMsg::UnscheduledAppsRequest),
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
            RootMsg::Scheduler(s) => match s {
                SchedulerMsg::Empty => todo!(),
            },
            RootMsg::Node(_) => todo!(),
            RootMsg::Datastore(d) => match d {
                DatastoreMsg::NodeJoin => todo!(),
                DatastoreMsg::NodesRequest => todo!(),
                DatastoreMsg::NodesResponse(nodes) => {
                    state.to_mut().nodes = nodes;
                }
                DatastoreMsg::UnscheduledAppsRequest => todo!(),
                DatastoreMsg::UnscheduledAppsResponse(apps) => {
                    for app in apps {
                        if let Some(node) = state.nodes.first() {
                            // TODO: use an actual scheduling strategy
                            o.send(
                                src,
                                RootMsg::Datastore(DatastoreMsg::ScheduleAppRequest(app, *node)),
                            );
                        }
                    }
                }
                DatastoreMsg::ScheduleAppRequest(_, _) => todo!(),
                DatastoreMsg::ScheduleAppResponse(_) => {},
            },
        }
    }
}

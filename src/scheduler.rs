use stateright::actor::{Actor, Id, Out};

use crate::{datastore::DatastoreMsg, root::RootMsg};

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Scheduler {}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct SchedulerState {}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum SchedulerMsg {
    Empty,
}

impl Actor for Scheduler {
    type Msg = RootMsg;

    type State = SchedulerState;

    type Timer = ();

    fn on_start(&self, _id: Id, o: &mut Out<Self>) -> Self::State {
        println!("send nodes");
        o.send(Id::from(0), RootMsg::Datastore(DatastoreMsg::NodesRequest));
        SchedulerState {}
    }

    fn on_msg(
        &self,
        id: Id,
        state: &mut std::borrow::Cow<Self::State>,
        src: Id,
        msg: Self::Msg,
        o: &mut Out<Self>,
    ) {
        dbg!(&msg);
        match msg {
            RootMsg::Scheduler(s) => match s {
                SchedulerMsg::Empty => todo!(),
            },
            RootMsg::Node(_) => todo!(),
            RootMsg::Datastore(d) => match d {
                DatastoreMsg::NodeJoin => todo!(),
                DatastoreMsg::NodesRequest => todo!(),
                DatastoreMsg::NodesResponse(nodes) => println!("got nodes {:?}", nodes),
                DatastoreMsg::UnscheduledAppsRequest => todo!(),
                DatastoreMsg::UnscheduledAppsResponse(_) => todo!(),
                DatastoreMsg::ScheduleAppRequest(_, _) => todo!(),
                DatastoreMsg::ScheduleAppResponse(_) => todo!(),
            },
        }
    }
}

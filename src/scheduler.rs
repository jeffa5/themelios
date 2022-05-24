use stateright::actor::{Actor, Id, Out};

use crate::register::MyRegisterMsg;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Scheduler {}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct SchedulerState {}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum SchedulerMsg {
    Empty,
}

impl Actor for Scheduler {
    type Msg = MyRegisterMsg;

    type State = SchedulerState;

    fn on_start(&self, id: Id, o: &mut Out<Self>) -> Self::State {
        todo!()
    }
}

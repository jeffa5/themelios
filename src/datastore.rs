use stateright::actor::{Actor, Id, Out};

use crate::root::RootMsg;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Datastore {}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct DatastoreState {}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum DatastoreMsg {
    Empty,
}

impl Actor for Datastore {
    type Msg = RootMsg;

    type State = DatastoreState;

    fn on_start(&self, id: Id, o: &mut Out<Self>) -> Self::State {
        todo!()
    }
}

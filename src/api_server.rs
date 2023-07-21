use stateright::actor::{Actor, Id, Out};

use crate::root::RootMsg;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct APIServer {}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct APIServerState {}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum APIServerMsg {
    Empty,
}

impl Actor for APIServer {
    type Msg = RootMsg;

    type State = APIServerState;

    type Timer = ();

    fn on_start(&self, id: Id, o: &mut Out<Self>) -> Self::State {
        todo!()
    }
}

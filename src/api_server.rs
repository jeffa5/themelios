use stateright::actor::{Actor, Id, Out};

use crate::register::MyRegisterMsg;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct APIServer {}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct APIServerState {}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum APIServerMsg {
    Empty,
}

impl Actor for APIServer {
    type Msg = MyRegisterMsg;

    type State = APIServerState;

    fn on_start(&self, id: Id, o: &mut Out<Self>) -> Self::State {
        todo!()
    }
}

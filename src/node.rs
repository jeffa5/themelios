use stateright::actor::{Actor, Id, Out};

use crate::root::RootMsg;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Node {}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct NodeState {}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum NodeMsg {
    Empty,
}

impl Actor for Node {
    type Msg = RootMsg;

    type State = NodeState;

    fn on_start(&self, id: Id, o: &mut Out<Self>) -> Self::State {
        todo!()
    }
}

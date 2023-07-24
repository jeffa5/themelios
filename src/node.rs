use stateright::actor::{Actor, Id, Out};

use crate::root::RootMsg;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Node {
    /// The id of the datastore node.
    pub datastore: Id,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct NodeState {}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum NodeMsg {
}

impl Actor for Node {
    type Msg = RootMsg;

    type State = NodeState;

    type Timer = ();

    fn on_start(&self, _id: Id, o: &mut Out<Self>) -> Self::State {
        o.send(
            self.datastore,
            RootMsg::Datastore(crate::datastore::DatastoreMsg::NodeJoin),
        );
        NodeState {}
    }
}

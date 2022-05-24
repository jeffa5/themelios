use stateright::actor::Actor;
use stateright::actor::Id;
use stateright::actor::Out;
use std::borrow::Cow;
use std::fmt::Debug;
use std::hash::Hash;

use crate::datastore;
use crate::node;
use crate::scheduler;

use crate::api_server;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MyRegisterActor {
    Scheduler(scheduler::Scheduler),
    Node(node::Node),
    Datastore(datastore::Datastore),
    APIServer(api_server::APIServer),
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum MyRegisterActorState {
    Scheduler(<scheduler::Scheduler as Actor>::State),
    Node(<node::Node as Actor>::State),
    Datastore(<datastore::Datastore as Actor>::State),
    APIServer(<api_server::APIServer as Actor>::State),
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum MyRegisterMsg {
    /// A message specific to the register system's internal protocol.
    Scheduler(scheduler::SchedulerMsg),
    Node(node::NodeMsg),

    Datastore(datastore::DatastoreMsg),

    /// Messages originating or destined for clients.
    APIServer(api_server::APIServerMsg),
}

impl Actor for MyRegisterActor {
    type Msg = MyRegisterMsg;

    type State = MyRegisterActorState;

    fn on_start(&self, id: Id, o: &mut Out<Self>) -> Self::State {
        match self {
            MyRegisterActor::Scheduler(client_actor) => {
                let mut client_out = Out::new();
                let state =
                    MyRegisterActorState::Scheduler(client_actor.on_start(id, &mut client_out));
                o.append(&mut client_out);
                state
            }
            MyRegisterActor::Node(client_actor) => {
                let mut client_out = Out::new();
                let state = MyRegisterActorState::Node(client_actor.on_start(id, &mut client_out));
                o.append(&mut client_out);
                state
            }
            MyRegisterActor::Datastore(client_actor) => {
                let mut client_out = Out::new();
                let state =
                    MyRegisterActorState::Datastore(client_actor.on_start(id, &mut client_out));
                o.append(&mut client_out);
                state
            }
            MyRegisterActor::APIServer(server_actor) => {
                let mut server_out = Out::new();
                let state =
                    MyRegisterActorState::APIServer(server_actor.on_start(id, &mut server_out));
                o.append(&mut server_out);
                state
            }
        }
    }

    fn on_msg(
        &self,
        id: Id,
        state: &mut Cow<Self::State>,
        src: Id,
        msg: Self::Msg,
        o: &mut Out<Self>,
    ) {
        use MyRegisterActor as A;
        use MyRegisterActorState as S;

        match (self, &**state) {
            (A::Scheduler(client_actor), S::Scheduler(client_state)) => {
                let mut client_state = Cow::Borrowed(client_state);
                let mut client_out = Out::new();
                client_actor.on_msg(id, &mut client_state, src, msg, &mut client_out);
                if let Cow::Owned(client_state) = client_state {
                    *state = Cow::Owned(MyRegisterActorState::Scheduler(client_state))
                }
                o.append(&mut client_out);
            }
            (A::APIServer(server_actor), S::APIServer(server_state)) => {
                let mut server_state = Cow::Borrowed(server_state);
                let mut server_out = Out::new();
                server_actor.on_msg(id, &mut server_state, src, msg, &mut server_out);
                if let Cow::Owned(server_state) = server_state {
                    *state = Cow::Owned(MyRegisterActorState::APIServer(server_state))
                }
                o.append(&mut server_out);
            }
            (A::APIServer(_), S::Scheduler(_)) => {}
            (A::Scheduler(_), S::APIServer(_)) => {}
            _ => todo!(),
        }
    }

    fn on_timeout(&self, id: Id, state: &mut Cow<Self::State>, o: &mut Out<Self>) {
        use MyRegisterActor as A;
        use MyRegisterActorState as S;
        match (self, &**state) {
            (A::Scheduler(_), S::Scheduler(_)) => {}
            (A::Scheduler(_), S::APIServer(_)) => {}
            (A::APIServer(server_actor), S::APIServer(server_state)) => {
                let mut server_state = Cow::Borrowed(server_state);
                let mut server_out = Out::new();
                server_actor.on_timeout(id, &mut server_state, &mut server_out);
                if let Cow::Owned(server_state) = server_state {
                    *state = Cow::Owned(MyRegisterActorState::APIServer(server_state))
                }
                o.append(&mut server_out);
            }
            (A::APIServer(_), S::Scheduler(_)) => {}
            _ => todo!(),
        }
    }
}

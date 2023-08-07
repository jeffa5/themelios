use stateright::actor::{Actor, Out};

use crate::state::State;
use crate::{
    abstract_model::Change, controller::Node, controller::ReplicaSet, controller::Scheduler,
};

pub use self::controller::ControllerActor;
pub use self::datastore::Datastore;

mod controller;
mod datastore;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Message {
    StateUpdate(State),
    Changes(Vec<Change>),
}

pub enum Actors {
    Datastore(Datastore),
    Node(ControllerActor<Node>),
    Scheduler(ControllerActor<Scheduler>),
    ReplicaSet(ControllerActor<ReplicaSet>),
}

impl Actor for Actors {
    type Msg = Message;

    type Timer = ();

    type State = State;

    fn on_start(
        &self,
        id: stateright::actor::Id,
        o: &mut stateright::actor::Out<Self>,
    ) -> Self::State {
        match self {
            Actors::Datastore(a) => {
                let mut client_out = Out::new();
                let state = a.on_start(id, &mut client_out);
                o.append(&mut client_out);
                state
            }
            Actors::Node(a) => {
                let mut client_out = Out::new();
                let state = a.on_start(id, &mut client_out);
                o.append(&mut client_out);
                state
            }
            Actors::Scheduler(a) => {
                let mut client_out = Out::new();
                let state = a.on_start(id, &mut client_out);
                o.append(&mut client_out);
                state
            }
            Actors::ReplicaSet(a) => {
                let mut client_out = Out::new();
                let state = a.on_start(id, &mut client_out);
                o.append(&mut client_out);
                state
            }
        }
    }

    fn on_msg(
        &self,
        id: stateright::actor::Id,
        state: &mut std::borrow::Cow<Self::State>,
        src: stateright::actor::Id,
        msg: Self::Msg,
        o: &mut stateright::actor::Out<Self>,
    ) {
        match self {
            Actors::Datastore(a) => {
                let mut client_out = Out::new();
                a.on_msg(id, state, src, msg, &mut client_out);
                o.append(&mut client_out);
            }
            Actors::Node(a) => {
                let mut client_out = Out::new();
                a.on_msg(id, state, src, msg, &mut client_out);
                o.append(&mut client_out);
            }
            Actors::Scheduler(a) => {
                let mut client_out = Out::new();
                a.on_msg(id, state, src, msg, &mut client_out);
                o.append(&mut client_out);
            }
            Actors::ReplicaSet(a) => {
                let mut client_out = Out::new();
                a.on_msg(id, state, src, msg, &mut client_out);
                o.append(&mut client_out);
            }
        }
    }

    fn on_timeout(
        &self,
        id: stateright::actor::Id,
        state: &mut std::borrow::Cow<Self::State>,
        timer: &Self::Timer,
        o: &mut stateright::actor::Out<Self>,
    ) {
        match self {
            Actors::Datastore(a) => {
                let mut client_out = Out::new();
                a.on_timeout(id, state, timer, &mut client_out);
                o.append(&mut client_out);
            }
            Actors::Node(a) => {
                let mut client_out = Out::new();
                a.on_timeout(id, state, timer, &mut client_out);
                o.append(&mut client_out);
            }
            Actors::Scheduler(a) => {
                let mut client_out = Out::new();
                a.on_timeout(id, state, timer, &mut client_out);
                o.append(&mut client_out);
            }
            Actors::ReplicaSet(a) => {
                let mut client_out = Out::new();
                a.on_timeout(id, state, timer, &mut client_out);
                o.append(&mut client_out);
            }
        }
    }

    fn name(&self) -> String {
        match self {
            Actors::Datastore(a) => a.name(),
            Actors::Node(a) => a.name(),
            Actors::Scheduler(a) => a.name(),
            Actors::ReplicaSet(a) => a.name(),
        }
    }
}

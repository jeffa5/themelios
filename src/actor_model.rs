use std::borrow::Cow;

use stateright::actor::{Actor, Out};

use crate::state::{State, StateView};
use crate::{
    abstract_model::Change, controller::Node, controller::ReplicaSet, controller::Scheduler,
};

pub use self::controller::ControllerActor;
pub use self::datastore::Datastore;

mod controller;
mod datastore;

pub struct ActorModelCfg {
    pub initial_pods: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Message {
    StateUpdate(StateView),
    Changes(Vec<Change>),
}

pub enum Actors {
    Datastore(Datastore),
    Node(ControllerActor<Node>),
    Scheduler(ControllerActor<Scheduler>),
    ReplicaSet(ControllerActor<ReplicaSet>),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ActorState {
    Datastore(State),
    /// Controllers have no state for now, they work purely on the state given to them.
    Controller,
}

impl Actor for Actors {
    type Msg = Message;

    type Timer = ();

    type State = ActorState;

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
                ActorState::Datastore(state)
            }
            Actors::Node(a) => {
                let mut client_out = Out::new();
                a.on_start(id, &mut client_out);
                o.append(&mut client_out);
                ActorState::Controller
            }
            Actors::Scheduler(a) => {
                let mut client_out = Out::new();
                a.on_start(id, &mut client_out);
                o.append(&mut client_out);
                ActorState::Controller
            }
            Actors::ReplicaSet(a) => {
                let mut client_out = Out::new();
                a.on_start(id, &mut client_out);
                o.append(&mut client_out);
                ActorState::Controller
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
        match (self, &**state) {
            (Actors::Datastore(a), ActorState::Datastore(s)) => {
                let mut client_out = Out::new();
                let mut s = Cow::Borrowed(s);
                a.on_msg(id, &mut s, src, msg, &mut client_out);
                if let Cow::Owned(s) = s {
                    *state = Cow::Owned(ActorState::Datastore(s));
                }
                o.append(&mut client_out);
            }
            (Actors::Node(a), ActorState::Controller) => {
                let mut client_out = Out::new();
                a.on_msg(id, &mut Cow::Owned(()), src, msg, &mut client_out);
                o.append(&mut client_out);
            }
            (Actors::Scheduler(a), ActorState::Controller) => {
                let mut client_out = Out::new();
                a.on_msg(id, &mut Cow::Owned(()), src, msg, &mut client_out);
                o.append(&mut client_out);
            }
            (Actors::ReplicaSet(a), ActorState::Controller) => {
                let mut client_out = Out::new();
                a.on_msg(id, &mut Cow::Owned(()), src, msg, &mut client_out);
                o.append(&mut client_out);
            }
            _ => unreachable!(),
        }
    }

    fn on_timeout(
        &self,
        id: stateright::actor::Id,
        state: &mut std::borrow::Cow<Self::State>,
        timer: &Self::Timer,
        o: &mut stateright::actor::Out<Self>,
    ) {
        match (self, &**state) {
            (Actors::Datastore(a), ActorState::Datastore(s)) => {
                let mut client_out = Out::new();
                let mut s = Cow::Borrowed(s);
                a.on_timeout(id, &mut s, timer, &mut client_out);
                if let Cow::Owned(s) = s {
                    *state = Cow::Owned(ActorState::Datastore(s));
                }
                o.append(&mut client_out);
            }
            (Actors::Node(a), ActorState::Controller) => {
                let mut client_out = Out::new();
                a.on_timeout(id, &mut Cow::Owned(()), timer, &mut client_out);
                o.append(&mut client_out);
            }
            (Actors::Scheduler(a), ActorState::Controller) => {
                let mut client_out = Out::new();
                a.on_timeout(id, &mut Cow::Owned(()), timer, &mut client_out);
                o.append(&mut client_out);
            }
            (Actors::ReplicaSet(a), ActorState::Controller) => {
                let mut client_out = Out::new();
                a.on_timeout(id, &mut Cow::Owned(()), timer, &mut client_out);
                o.append(&mut client_out);
            }
            _ => unreachable!(),
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

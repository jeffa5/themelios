use stateright::actor::{Actor, Id, Out};

use crate::{
    controller::{Controller, ReplicaSet},
    model::{self, Change, Pod, State},
    node::Node,
    scheduler::Scheduler,
};

pub struct ControllerActor<C> {
    controller: C,
}

impl<C> ControllerActor<C> {
    pub fn new(controller: C) -> Self {
        Self { controller }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Message {
    StateUpdate(State),
    Changes(Vec<Change>),
}

impl<C> Actor for ControllerActor<C>
where
    C: Controller,
{
    type Msg = Message;

    type Timer = ();

    type State = State;

    fn on_start(
        &self,
        id: stateright::actor::Id,
        o: &mut stateright::actor::Out<Self>,
    ) -> Self::State {
        let state = State::default();
        let changes = self.controller.step(id.into(), &state);
        o.send(Id::from(0), Message::Changes(changes));
        state
    }

    fn on_msg(
        &self,
        id: stateright::actor::Id,
        state: &mut std::borrow::Cow<Self::State>,
        src: stateright::actor::Id,
        msg: Self::Msg,
        o: &mut stateright::actor::Out<Self>,
    ) {
        match msg {
            Message::StateUpdate(s) => {
                let changes = self.controller.step(id.into(), &s);
                *state.to_mut() = s;
                o.send(src, Message::Changes(changes));
            }
            Message::Changes(_) => todo!(),
        }
    }

    fn on_timeout(
        &self,
        _id: stateright::actor::Id,
        _state: &mut std::borrow::Cow<Self::State>,
        _timer: &Self::Timer,
        _o: &mut stateright::actor::Out<Self>,
    ) {
    }

    fn name(&self) -> String {
        self.controller.name()
    }
}

pub struct Datastore {
    pub initial_pods: u32,
    pub initial_replicasets: u32,
    pub pods_per_replicaset: u32,
}

impl Actor for Datastore {
    type Msg = Message;

    type Timer = ();

    type State = State;

    fn on_start(
        &self,
        _id: stateright::actor::Id,
        _o: &mut stateright::actor::Out<Self>,
    ) -> Self::State {
        let mut state = State::default();
        for i in 0..self.initial_pods {
            state.pods.insert(
                i,
                Pod {
                    id: i,
                    node_name: None,
                },
            );
        }
        for i in 1..=self.initial_replicasets {
            state.replica_sets.insert(
                i,
                model::ReplicaSet {
                    id: i,
                    replicas: self.pods_per_replicaset,
                },
            );
        }
        state
    }

    fn on_msg(
        &self,
        _id: stateright::actor::Id,
        state: &mut std::borrow::Cow<Self::State>,
        _src: stateright::actor::Id,
        msg: Self::Msg,
        o: &mut stateright::actor::Out<Self>,
    ) {
        match msg {
            Message::StateUpdate(_) => todo!(),
            Message::Changes(changes) => {
                if !changes.is_empty() {
                    let state = state.to_mut();
                    for change in changes {
                        state.apply_change(change);
                    }
                    let node_ids = state.nodes.keys().copied();
                    let scheduler_ids = state.schedulers.iter().copied();
                    let replicaset_ids = state.replicaset_controllers.iter().copied();

                    let all_ids = node_ids.chain(scheduler_ids).chain(replicaset_ids);
                    for id in all_ids {
                        o.send(Id::from(id), Message::StateUpdate(state.clone()));
                    }
                }
            }
        }
    }

    fn name(&self) -> String {
        "Datastore".to_owned()
    }
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
        mut state: &mut std::borrow::Cow<Self::State>,
        src: stateright::actor::Id,
        msg: Self::Msg,
        o: &mut stateright::actor::Out<Self>,
    ) {
        match self {
            Actors::Datastore(a) => {
                let mut client_out = Out::new();
                a.on_msg(id, &mut state, src, msg, &mut client_out);
                o.append(&mut client_out);
            }
            Actors::Node(a) => {
                let mut client_out = Out::new();
                a.on_msg(id, &mut state, src, msg, &mut client_out);
                o.append(&mut client_out);
            }
            Actors::Scheduler(a) => {
                let mut client_out = Out::new();
                a.on_msg(id, &mut state, src, msg, &mut client_out);
                o.append(&mut client_out);
            }
            Actors::ReplicaSet(a) => {
                let mut client_out = Out::new();
                a.on_msg(id, &mut state, src, msg, &mut client_out);
                o.append(&mut client_out);
            }
        }
    }

    fn on_timeout(
        &self,
        id: stateright::actor::Id,
        mut state: &mut std::borrow::Cow<Self::State>,
        timer: &Self::Timer,
        o: &mut stateright::actor::Out<Self>,
    ) {
        match self {
            Actors::Datastore(a) => {
                let mut client_out = Out::new();
                a.on_timeout(id, &mut state, timer, &mut client_out);
                o.append(&mut client_out);
            }
            Actors::Node(a) => {
                let mut client_out = Out::new();
                a.on_timeout(id, &mut state, timer, &mut client_out);
                o.append(&mut client_out);
            }
            Actors::Scheduler(a) => {
                let mut client_out = Out::new();
                a.on_timeout(id, &mut state, timer, &mut client_out);
                o.append(&mut client_out);
            }
            Actors::ReplicaSet(a) => {
                let mut client_out = Out::new();
                a.on_timeout(id, &mut state, timer, &mut client_out);
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

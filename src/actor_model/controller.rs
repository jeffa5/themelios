use stateright::actor::{Actor, Id};

use crate::{controller::Controller, state::State};

use super::Message;

pub struct ControllerActor<C> {
    controller: C,
}

impl<C> ControllerActor<C> {
    pub fn new(controller: C) -> Self {
        Self { controller }
    }
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

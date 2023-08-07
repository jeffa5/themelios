use stateright::actor::{Actor, Id};

use crate::controller::Controller;

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

    type State = ();

    fn on_start(
        &self,
        id: stateright::actor::Id,
        o: &mut stateright::actor::Out<Self>,
    ) -> Self::State {
        let change = self.controller.register(id.into());
        o.send(Id::from(0), Message::Changes(vec![change]));
    }

    fn on_msg(
        &self,
        id: stateright::actor::Id,
        _state: &mut std::borrow::Cow<Self::State>,
        src: stateright::actor::Id,
        msg: Self::Msg,
        o: &mut stateright::actor::Out<Self>,
    ) {
        match msg {
            Message::StateUpdate(s) => {
                let changes = self.controller.step(id.into(), &s);
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

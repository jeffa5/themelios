use stateright::actor::{Actor, Id};

use crate::{abstract_model::Change, controller::Controller, state::StateView};

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

    type State = C::State;

    fn on_start(
        &self,
        id: stateright::actor::Id,
        o: &mut stateright::actor::Out<Self>,
    ) -> Self::State {
        let view = StateView::default();
        let mut s = C::State::default();
        let operations = self.controller.step(id.into(), &view, &mut s);
        let changes = operations
            .into_iter()
            .map(|o| Change {
                revision: view.revision.clone(),
                operation: o.into(),
            })
            .collect();
        o.send(Id::from(0), Message::Changes(changes));
        s
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
                // TODO: should we use this to_mut?
                let operations = self.controller.step(id.into(), &s, state.to_mut());
                let changes = operations
                    .into_iter()
                    .map(|o| Change {
                        revision: s.revision.clone(),
                        operation: o.into(),
                    })
                    .collect();
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

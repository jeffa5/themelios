use stateright::actor::{Actor, Id};

use crate::state::{ReadConsistencyLevel, State, StateView};

use super::Message;

pub struct Datastore {
    pub initial_state: StateView,
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
        State::new(self.initial_state.clone(), ReadConsistencyLevel::Strong)
    }

    fn on_msg(
        &self,
        _id: stateright::actor::Id,
        state: &mut std::borrow::Cow<Self::State>,
        src: stateright::actor::Id,
        msg: Self::Msg,
        o: &mut stateright::actor::Out<Self>,
    ) {
        match msg {
            Message::StateUpdate(_) => todo!(),
            Message::Changes(changes) => {
                if !changes.is_empty() {
                    let state = state.to_mut();
                    let rev = state.push_changes(changes.into_iter(), src.into());
                    let view = state.view_at(rev);
                    let node_ids = view.nodes.keys().copied();
                    let controller_ids = view.controllers.iter().copied();

                    let all_ids = node_ids.chain(controller_ids).collect::<Vec<_>>();
                    for id in &all_ids {
                        for view in state.views(*id) {
                            o.send(Id::from(*id), Message::StateUpdate(view.clone()));
                        }
                    }
                }
            }
        }
    }

    fn name(&self) -> String {
        "Datastore".to_owned()
    }
}

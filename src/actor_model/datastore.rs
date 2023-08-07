use stateright::actor::{Actor, Id};

use crate::state::State;

use super::Message;

pub struct Datastore {
    pub initial_state: State,
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
        self.initial_state.clone()
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

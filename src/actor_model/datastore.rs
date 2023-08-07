use stateright::actor::{Actor, Id};

use crate::state::{ConsistencyLevel, State, StateView};

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
        State::default().with_initial(self.initial_state.clone())
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
                    let rev = state.push_changes(changes.into_iter());
                    let view = state.view_at(rev);
                    let node_ids = view.nodes.keys().copied();
                    let scheduler_ids = view.schedulers.iter().copied();
                    let replicaset_ids = view.replicaset_controllers.iter().copied();

                    let all_ids = node_ids
                        .chain(scheduler_ids)
                        .chain(replicaset_ids)
                        .collect::<Vec<_>>();
                    for view in state.views_for(ConsistencyLevel::Strong) {
                        for id in &all_ids {
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

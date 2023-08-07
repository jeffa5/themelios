use stateright::actor::{Actor, Id};

use crate::state::{PodResource, ReplicaSetResource, State};

use super::Message;

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
                PodResource {
                    id: i,
                    node_name: None,
                },
            );
        }
        for i in 1..=self.initial_replicasets {
            state.replica_sets.insert(
                i,
                ReplicaSetResource {
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

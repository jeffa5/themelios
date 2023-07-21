use std::sync::Arc;

use crate::node;
use stateright::actor::ActorModel;
use stateright::actor::ActorModelState;
use stateright::actor::Network;

use crate::datastore;
use crate::root::Root;

use crate::api_server;
use crate::root::RootMsg;
use crate::root::RootState;
use crate::scheduler;

pub struct ModelCfg {
    pub schedulers: usize,
    pub nodes: usize,
    pub datastores: usize,
    pub api_servers: usize,
}

impl ModelCfg {
    pub fn into_actor_model(self) -> ActorModel<Root, (), ()> {
        let mut model = ActorModel::new((), ());
        for _i in 0..self.api_servers {
            model = model.actor(Root::APIServer(api_server::APIServer {}))
        }

        for _ in 0..self.schedulers {
            model = model.actor(Root::Scheduler(scheduler::Scheduler {}))
        }

        for _ in 0..self.nodes {
            model = model.actor(Root::Node(node::Node {}))
        }

        for _ in 0..self.datastores {
            model = model.actor(Root::Datastore(datastore::Datastore {}))
        }

        model
            .property(
                stateright::Expectation::Eventually,
                "all actors have the same value for all keys",
                |_, state| all_same_state(&state.actor_states),
            )
            .property(
                stateright::Expectation::Always,
                "in sync when syncing is done and no in-flight requests",
                |_, state| syncing_done_and_in_sync(state),
            )
            .init_network(Network::new_ordered(vec![]))
    }
}

fn all_same_state(actors: &[Arc<RootState>]) -> bool {
    actors.windows(2).all(|w| match (&*w[0], &*w[1]) {
        (RootState::Scheduler(_), RootState::Scheduler(_)) => true,
        (RootState::Scheduler(_), RootState::APIServer(_)) => true,
        (RootState::APIServer(_), RootState::Scheduler(_)) => true,
        (RootState::APIServer(a), RootState::APIServer(b)) => a == b,
        _ => todo!(),
    })
}

fn syncing_done_and_in_sync(state: &ActorModelState<Root>) -> bool {
    // first check that the network has no sync messages in-flight.
    for envelope in state.network.iter_deliverable() {
        match envelope.msg {
            RootMsg::Scheduler(scheduler::SchedulerMsg::Empty) => {
                return true;
            }
            RootMsg::Node(_) => {}
            RootMsg::Datastore(_) => {}
            RootMsg::APIServer(_) => {}
        }
    }

    // next, check that all actors are in the same states (using sub-property checker)
    all_same_state(&state.actor_states)
}

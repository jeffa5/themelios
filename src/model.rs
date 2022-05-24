use std::sync::Arc;

use stateright::actor::ActorModel;
use stateright::actor::ActorModelState;
use stateright::actor::Network;

use crate::register::MyRegisterActor;

use crate::api_server;
use crate::register::MyRegisterActorState;
use crate::register::MyRegisterMsg;
use crate::scheduler;

pub struct ModelCfg {
    pub schedulers: usize,
    pub api_servers: usize,
}

impl ModelCfg {
    pub fn into_actor_model(self) -> ActorModel<MyRegisterActor, (), ()> {
        let mut model = ActorModel::new((), ());
        for i in 0..self.api_servers {
            model = model.actor(MyRegisterActor::APIServer(api_server::APIServer {}))
        }

        for _ in 0..self.schedulers {
            model = model.actor(MyRegisterActor::Scheduler(scheduler::Scheduler {}))
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

fn all_same_state(actors: &[Arc<MyRegisterActorState>]) -> bool {
    actors.windows(2).all(|w| match (&*w[0], &*w[1]) {
        (MyRegisterActorState::Scheduler(_), MyRegisterActorState::Scheduler(_)) => true,
        (MyRegisterActorState::Scheduler(_), MyRegisterActorState::APIServer(_)) => true,
        (MyRegisterActorState::APIServer(_), MyRegisterActorState::Scheduler(_)) => true,
        (MyRegisterActorState::APIServer(a), MyRegisterActorState::APIServer(b)) => a == b,
    })
}

fn syncing_done_and_in_sync(state: &ActorModelState<MyRegisterActor>) -> bool {
    // first check that the network has no sync messages in-flight.
    for envelope in state.network.iter_deliverable() {
        match envelope.msg {
            MyRegisterMsg::Scheduler(scheduler::SchedulerMsg::Empty) => {
                return true;
            }
            MyRegisterMsg::APIServer(_) => {}
        }
    }

    // next, check that all actors are in the same states (using sub-property checker)
    all_same_state(&state.actor_states)
}

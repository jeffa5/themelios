use clap::Parser;
use report::Reporter;
use stateright::actor::Actor;
use stateright::actor::ActorModel;
use stateright::actor::ActorModelState;
use stateright::actor::Network;
use stateright::actor::Out;
use stateright::Checker;
use stateright::CheckerBuilder;
use stateright::{actor::Id, Model};
use std::borrow::Cow;
use std::fmt::Debug;
use std::hash::Hash;
use std::sync::Arc;

mod report;

#[derive(Clone, Debug, Eq, PartialEq)]
enum MyRegisterActor {
    Scheduler(Scheduler),
    APIServer(APIServer),
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Scheduler {}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct SchedulerState {}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum SchedulerMsg {
    Empty,
}

impl Actor for Scheduler {
    type Msg = MyRegisterMsg;

    type State = SchedulerState;

    fn on_start(&self, id: Id, o: &mut Out<Self>) -> Self::State {
        todo!()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct APIServer {}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct APIServerState {}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum APIServerMsg {
    Empty,
}

impl Actor for APIServer {
    type Msg = MyRegisterMsg;

    type State = APIServerState;

    fn on_start(&self, id: Id, o: &mut Out<Self>) -> Self::State {
        todo!()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
enum MyRegisterActorState {
    Scheduler(<Scheduler as Actor>::State),
    APIServer(<APIServer as Actor>::State),
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum MyRegisterMsg {
    /// A message specific to the register system's internal protocol.
    Scheduler(SchedulerMsg),

    /// Messages originating or destined for clients.
    APIServer(APIServerMsg),
}

impl Actor for MyRegisterActor {
    type Msg = MyRegisterMsg;

    type State = MyRegisterActorState;

    fn on_start(&self, id: Id, o: &mut Out<Self>) -> Self::State {
        match self {
            MyRegisterActor::Scheduler(client_actor) => {
                let mut client_out = Out::new();
                let state =
                    MyRegisterActorState::Scheduler(client_actor.on_start(id, &mut client_out));
                o.append(&mut client_out);
                state
            }
            MyRegisterActor::APIServer(server_actor) => {
                let mut server_out = Out::new();
                let state =
                    MyRegisterActorState::APIServer(server_actor.on_start(id, &mut server_out));
                o.append(&mut server_out);
                state
            }
        }
    }

    fn on_msg(
        &self,
        id: Id,
        state: &mut Cow<Self::State>,
        src: Id,
        msg: Self::Msg,
        o: &mut Out<Self>,
    ) {
        use MyRegisterActor as A;
        use MyRegisterActorState as S;

        match (self, &**state) {
            (A::Scheduler(client_actor), S::Scheduler(client_state)) => {
                let mut client_state = Cow::Borrowed(client_state);
                let mut client_out = Out::new();
                client_actor.on_msg(id, &mut client_state, src, msg, &mut client_out);
                if let Cow::Owned(client_state) = client_state {
                    *state = Cow::Owned(MyRegisterActorState::Scheduler(client_state))
                }
                o.append(&mut client_out);
            }
            (A::APIServer(server_actor), S::APIServer(server_state)) => {
                let mut server_state = Cow::Borrowed(server_state);
                let mut server_out = Out::new();
                server_actor.on_msg(id, &mut server_state, src, msg, &mut server_out);
                if let Cow::Owned(server_state) = server_state {
                    *state = Cow::Owned(MyRegisterActorState::APIServer(server_state))
                }
                o.append(&mut server_out);
            }
            (A::APIServer(_), S::Scheduler(_)) => {}
            (A::Scheduler(_), S::APIServer(_)) => {}
        }
    }

    fn on_timeout(&self, id: Id, state: &mut Cow<Self::State>, o: &mut Out<Self>) {
        use MyRegisterActor as A;
        use MyRegisterActorState as S;
        match (self, &**state) {
            (A::Scheduler(_), S::Scheduler(_)) => {}
            (A::Scheduler(_), S::APIServer(_)) => {}
            (A::APIServer(server_actor), S::APIServer(server_state)) => {
                let mut server_state = Cow::Borrowed(server_state);
                let mut server_out = Out::new();
                server_actor.on_timeout(id, &mut server_state, &mut server_out);
                if let Cow::Owned(server_state) = server_state {
                    *state = Cow::Owned(MyRegisterActorState::APIServer(server_state))
                }
                o.append(&mut server_out);
            }
            (A::APIServer(_), S::Scheduler(_)) => {}
        }
    }
}

struct ModelCfg {
    schedulers: usize,
    api_servers: usize,
}

impl ModelCfg {
    fn into_actor_model(self) -> ActorModel<MyRegisterActor, (), ()> {
        let mut model = ActorModel::new((), ());
        for i in 0..self.api_servers {
            model = model.actor(MyRegisterActor::APIServer(APIServer {}))
        }

        for _ in 0..self.schedulers {
            model = model.actor(MyRegisterActor::Scheduler(Scheduler {}))
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
            MyRegisterMsg::Scheduler(SchedulerMsg::Empty) => {
                return true;
            }
            MyRegisterMsg::APIServer(_) => {}
        }
    }

    // next, check that all actors are in the same states (using sub-property checker)
    all_same_state(&state.actor_states)
}

#[derive(Parser, Debug)]
struct Opts {
    #[clap(subcommand)]
    command: SubCmd,

    #[clap(long, short, global = true, default_value = "2")]
    schedulers: usize,

    #[clap(long, short, global = true, default_value = "2")]
    api_servers: usize,

    #[clap(long, default_value = "8080")]
    port: u16,
}

#[derive(clap::Subcommand, Debug)]
enum SubCmd {
    Serve,
    CheckDfs,
    CheckBfs,
}

fn main() {
    let opts = Opts::parse();

    let model = ModelCfg {
        schedulers: opts.schedulers,
        api_servers: opts.api_servers,
    }
    .into_actor_model()
    .checker()
    .threads(num_cpus::get());
    run(opts, model)
}

fn run(opts: Opts, model: CheckerBuilder<ActorModel<MyRegisterActor>>) {
    println!("Running with config {:?}", opts);
    match opts.command {
        SubCmd::Serve => {
            println!("Serving web ui on http://127.0.0.1:{}", opts.port);
            model.serve(("127.0.0.1", opts.port));
        }
        SubCmd::CheckDfs => {
            model
                .spawn_dfs()
                .report(&mut Reporter::default())
                .join()
                .assert_properties();
        }
        SubCmd::CheckBfs => {
            model
                .spawn_bfs()
                .report(&mut Reporter::default())
                .join()
                .assert_properties();
        }
    }
}

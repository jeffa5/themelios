use stateright::actor::Actor;
use stateright::actor::Id;
use stateright::actor::Out;
use std::borrow::Cow;
use std::fmt::Debug;
use std::hash::Hash;

use crate::app::App;
use crate::client;
use crate::datastore;
use crate::node;
use crate::scheduler;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Root {
    Scheduler(scheduler::Scheduler),
    Client(client::Client),
    Node(node::Node),
    Datastore(datastore::Datastore),
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum RootState {
    Scheduler(<scheduler::Scheduler as Actor>::State),
    Client(<client::Client as Actor>::State),
    Node(<node::Node as Actor>::State),
    Datastore(<datastore::Datastore as Actor>::State),
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum RootMsg {
    /// Join a new node into this cluster.
    NodeJoin,

    /// Get the apps a node should run.
    GetAppsForNodeRequest(Id),
    /// The apps that the node has been assigned.
    GetAppsForNodeResponse(Vec<App>),

    /// Get the current nodes.
    NodesRequest,
    NodesResponse(Vec<Id>),

    /// Get the apps to be scheduled
    UnscheduledAppsRequest,
    UnscheduledAppsResponse(Vec<App>),

    /// Schedule an app to a node.
    ScheduleAppRequest(App, Id),
    /// Return whether the app was successfully scheduled.
    ScheduleAppResponse(bool),

    /// Create an app.
    CreateAppRequest(App),
    /// Whether the app was successfully added.
    CreateAppResponse(bool),
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum RootTimer {
    Node(node::NodeTimer),
    Scheduler(scheduler::SchedulerTimer),
}

impl Actor for Root {
    type Msg = RootMsg;

    type State = RootState;

    type Timer = RootTimer;

    fn on_start(&self, id: Id, o: &mut Out<Self>) -> Self::State {
        match self {
            Root::Scheduler(client_actor) => {
                let mut client_out = Out::new();
                let state = RootState::Scheduler(client_actor.on_start(id, &mut client_out));
                o.append(&mut client_out);
                state
            }
            Root::Client(client_actor) => {
                let mut client_out = Out::new();
                let state = RootState::Client(client_actor.on_start(id, &mut client_out));
                o.append(&mut client_out);
                state
            }
            Root::Node(client_actor) => {
                let mut client_out = Out::new();
                let state = RootState::Node(client_actor.on_start(id, &mut client_out));
                o.append(&mut client_out);
                state
            }
            Root::Datastore(client_actor) => {
                let mut client_out = Out::new();
                let state = RootState::Datastore(client_actor.on_start(id, &mut client_out));
                o.append(&mut client_out);
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
        use Root as A;
        use RootState as S;

        match (self, &**state) {
            (A::Scheduler(client_actor), S::Scheduler(client_state)) => {
                let mut client_state = Cow::Borrowed(client_state);
                let mut client_out = Out::new();
                client_actor.on_msg(id, &mut client_state, src, msg, &mut client_out);
                if let Cow::Owned(client_state) = client_state {
                    *state = Cow::Owned(RootState::Scheduler(client_state))
                }
                o.append(&mut client_out);
            }
            (A::Client(client_actor), S::Client(client_state)) => {
                let mut client_state = Cow::Borrowed(client_state);
                let mut client_out = Out::new();
                client_actor.on_msg(id, &mut client_state, src, msg, &mut client_out);
                if let Cow::Owned(client_state) = client_state {
                    *state = Cow::Owned(RootState::Client(client_state))
                }
                o.append(&mut client_out);
            }
            (A::Node(client_actor), S::Node(client_state)) => {
                let mut client_state = Cow::Borrowed(client_state);
                let mut client_out = Out::new();
                client_actor.on_msg(id, &mut client_state, src, msg, &mut client_out);
                if let Cow::Owned(client_state) = client_state {
                    *state = Cow::Owned(RootState::Node(client_state))
                }
                o.append(&mut client_out);
            }
            (A::Datastore(client_actor), S::Datastore(client_state)) => {
                let mut client_state = Cow::Borrowed(client_state);
                let mut client_out = Out::new();
                client_actor.on_msg(id, &mut client_state, src, msg, &mut client_out);
                if let Cow::Owned(client_state) = client_state {
                    *state = Cow::Owned(RootState::Datastore(client_state))
                }
                o.append(&mut client_out);
            }
            _ => {}
        }
    }

    fn on_timeout(
        &self,
        id: Id,
        state: &mut Cow<Self::State>,
        timer: &Self::Timer,
        o: &mut Out<Self>,
    ) {
        use Root as A;
        use RootState as S;
        match (self, &**state) {
            (A::Scheduler(client_actor), S::Scheduler(client_state)) => {
                let mut client_state = Cow::Borrowed(client_state);
                let mut client_out = Out::new();
                client_actor.on_timeout(id, &mut client_state, timer, &mut client_out);
                if let Cow::Owned(client_state) = client_state {
                    *state = Cow::Owned(RootState::Scheduler(client_state))
                }
                o.append(&mut client_out);
            }
            (A::Client(client_actor), S::Client(client_state)) => {
                let mut client_state = Cow::Borrowed(client_state);
                let mut client_out = Out::new();
                client_actor.on_timeout(id, &mut client_state, timer, &mut client_out);
                if let Cow::Owned(client_state) = client_state {
                    *state = Cow::Owned(RootState::Client(client_state))
                }
                o.append(&mut client_out);
            }
            (A::Node(client_actor), S::Node(client_state)) => {
                let mut client_state = Cow::Borrowed(client_state);
                let mut client_out = Out::new();
                client_actor.on_timeout(id, &mut client_state, timer, &mut client_out);
                if let Cow::Owned(client_state) = client_state {
                    *state = Cow::Owned(RootState::Node(client_state))
                }
                o.append(&mut client_out);
            }
            (A::Datastore(client_actor), S::Datastore(client_state)) => {
                let mut client_state = Cow::Borrowed(client_state);
                let mut client_out = Out::new();
                client_actor.on_timeout(id, &mut client_state, timer, &mut client_out);
                if let Cow::Owned(client_state) = client_state {
                    *state = Cow::Owned(RootState::Datastore(client_state))
                }
                o.append(&mut client_out);
            }
            _ => {}
        }
    }

    fn name(&self) -> String {
        match self {
            Root::Scheduler(a) => a.name(),
            Root::Client(a) => a.name(),
            Root::Node(a) => a.name(),
            Root::Datastore(a) => a.name(),
        }
    }
}

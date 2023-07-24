use crate::app::{App, AppId};
use std::collections::{BTreeMap, BTreeSet};

use stateright::actor::{Actor, Id, Out};

use crate::root::RootMsg;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Datastore {
    pub initial_apps: u32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct DatastoreState {
    /// Ids of worker nodes in this cluster, given by their id.
    nodes: BTreeSet<Id>,
    /// Identifiers of applications to be scheduled in this cluster.
    unscheduled_apps: BTreeMap<AppId, App>,
    /// Scheduled applications in this cluster tagged with the node they are running on.
    scheduled_apps: BTreeMap<AppId, (App, Id)>,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum DatastoreMsg {
    NodeJoin,

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
}

impl Actor for Datastore {
    type Msg = RootMsg;

    type State = DatastoreState;

    type Timer = ();

    fn on_start(&self, _id: Id, _o: &mut Out<Self>) -> Self::State {
        DatastoreState {
            unscheduled_apps: (0..self.initial_apps)
                .map(|i| (i, App::default()))
                .collect(),
            ..Default::default()
        }
    }

    fn on_msg(
        &self,
        _id: Id,
        state: &mut std::borrow::Cow<Self::State>,
        src: Id,
        msg: Self::Msg,
        o: &mut Out<Self>,
    ) {
        match msg {
            RootMsg::Scheduler(_) => todo!(),
            RootMsg::Node(_) => todo!(),
            RootMsg::Datastore(d) => match d {
                DatastoreMsg::NodeJoin => {
                    state.to_mut().nodes.insert(src);
                    // ignore if already registered
                }
                DatastoreMsg::NodesRequest => {
                    o.send(
                        src,
                        RootMsg::Datastore(DatastoreMsg::NodesResponse(
                            state.nodes.iter().cloned().collect(),
                        )),
                    );
                }
                DatastoreMsg::NodesResponse(_) => todo!(),
                DatastoreMsg::UnscheduledAppsRequest => {
                    o.send(
                        src,
                        RootMsg::Datastore(DatastoreMsg::UnscheduledAppsResponse(
                            state.unscheduled_apps.values().cloned().collect(),
                        )),
                    );
                }
                DatastoreMsg::UnscheduledAppsResponse(_) => todo!(),
                DatastoreMsg::ScheduleAppRequest(app, node) => {
                    let state = state.to_mut();
                    state.unscheduled_apps.remove(&app.id);
                    state.scheduled_apps.insert(app.id, (app, node));
                }
                DatastoreMsg::ScheduleAppResponse(_) => todo!(),
            },
        }
    }
}

use crate::{
    app::{App, AppId},
    root::RootTimer,
};
use std::collections::{BTreeMap, BTreeSet};

use stateright::actor::{Actor, Id, Out};

use crate::root::RootMsg;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Datastore {}

#[derive(Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct DatastoreState {
    /// Ids of worker nodes in this cluster, given by their id.
    pub nodes: BTreeSet<Id>,
    /// Ids of schedulers for this cluster.
    pub schedulers: BTreeSet<Id>,
    /// Identifiers of applications to be scheduled in this cluster.
    pub unscheduled_apps: BTreeMap<AppId, App>,
    /// Scheduled applications in this cluster tagged with the node they are running on.
    pub scheduled_apps: Vec<(App, Id)>,
}

impl Actor for Datastore {
    type Msg = RootMsg;

    type State = DatastoreState;

    type Timer = RootTimer;

    fn on_start(&self, _id: Id, _o: &mut Out<Self>) -> Self::State {
        DatastoreState::default()
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
            RootMsg::NodeJoin => {
                if !state.nodes.contains(&src) {
                    state.to_mut().nodes.insert(src);
                    o.broadcast(state.schedulers.iter(), &RootMsg::NodeJoinedEvent(src));
                    // tell the node any apps that it is supposed to be running
                    for (app, node) in &state.scheduled_apps {
                        if node == &src {
                            o.send(src, RootMsg::ScheduledAppEvent(app.clone(), *node));
                        }
                    }
                }
                // ignore if already registered
            }
            RootMsg::SchedulerJoin => {
                if !state.schedulers.contains(&src) {
                    state.to_mut().schedulers.insert(src);
                    // tell the scheduler the set of nodes in the cluster
                    for node in &state.nodes {
                        o.send(src, RootMsg::NodeJoinedEvent(*node));
                    }
                    // tell the scheduler the current set of apps to schedule
                    for app in state.unscheduled_apps.values() {
                        o.send(src, RootMsg::NewAppEvent(app.clone()));
                    }
                    // TODO: a smarter scheduler will probably want the existing state of
                    // scheduling
                }
                // ignore if already registered
            }
            RootMsg::ScheduledAppEvent(_, _) => todo!(),
            RootMsg::ScheduleAppRequest(app, node) => {
                let state = state.to_mut();
                state.unscheduled_apps.remove(&app.id);
                if let Some(_pos) = state.scheduled_apps.iter().find(|(a, _n)| a.id == app.id) {
                    // TODO: should probably be an error or something...
                    o.send(src, RootMsg::ScheduleAppResponse(false));
                } else {
                    state.scheduled_apps.push((app.clone(), node));
                    o.send(node, RootMsg::ScheduledAppEvent(app.clone(), node));
                    o.send(src, RootMsg::ScheduleAppResponse(true));
                    o.broadcast(
                        state.schedulers.iter(),
                        &RootMsg::ScheduledAppEvent(app, node),
                    );
                }
            }
            RootMsg::ScheduleAppResponse(_) => todo!(),
            RootMsg::CreateAppRequest(app) => {
                let exists = state.unscheduled_apps.contains_key(&app.id);
                if !exists {
                    state.to_mut().unscheduled_apps.insert(app.id, app.clone());
                    o.broadcast(state.schedulers.iter(), &RootMsg::NewAppEvent(app));
                }
                o.send(src, RootMsg::CreateAppResponse(!exists));
            }
            RootMsg::CreateAppResponse(_) => todo!(),
            RootMsg::NodeJoinedEvent(_) => todo!(),
            RootMsg::NewAppEvent(_) => todo!(),
        }
    }

    fn name(&self) -> String {
        "Datastore".to_owned()
    }
}

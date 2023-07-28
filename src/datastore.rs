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
    nodes: BTreeSet<Id>,
    /// Identifiers of applications to be scheduled in this cluster.
    pub unscheduled_apps: BTreeMap<AppId, App>,
    /// Scheduled applications in this cluster tagged with the node they are running on.
    scheduled_apps: Vec<(App, Id)>,
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
                state.to_mut().nodes.insert(src);
                // ignore if already registered
            }
            RootMsg::GetAppsForNodeRequest(node) => {
                let apps = state
                    .scheduled_apps
                    .iter()
                    .filter_map(|(a, n)| if n == &node { Some(a.clone()) } else { None })
                    .collect();
                o.send(src, RootMsg::GetAppsForNodeResponse(apps));
            }
            RootMsg::GetAppsForNodeResponse(_) => todo!(),
            RootMsg::NodesRequest => {
                o.send(
                    src,
                    RootMsg::NodesResponse(state.nodes.iter().cloned().collect()),
                );
            }
            RootMsg::NodesResponse(_) => todo!(),
            RootMsg::UnscheduledAppsRequest => {
                o.send(
                    src,
                    RootMsg::UnscheduledAppsResponse(
                        state.unscheduled_apps.values().cloned().collect(),
                    ),
                );
            }
            RootMsg::UnscheduledAppsResponse(_) => todo!(),
            RootMsg::ScheduleAppRequest(app, node) => {
                let state = state.to_mut();
                state.unscheduled_apps.remove(&app.id);
                if let Some(_pos) = state.scheduled_apps.iter().find(|(a, _n)| a.id == app.id) {
                    // TODO: should probably be an error or something...
                } else {
                    state.scheduled_apps.push((app, node));
                }
            }
            RootMsg::ScheduleAppResponse(_) => todo!(),
            RootMsg::CreateAppRequest(app) => {
                let exists = state.unscheduled_apps.contains_key(&app.id);
                if !exists {
                    state.to_mut().unscheduled_apps.insert(app.id, app);
                }
                o.send(src, RootMsg::CreateAppResponse(!exists))
            }
            RootMsg::CreateAppResponse(_) => todo!(),
        }
    }
}

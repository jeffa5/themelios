use std::{borrow::Cow, collections::BTreeMap};

use stateright::actor::{Actor, Id, Out};

use crate::{
    app::{App, AppId},
    root::{RootMsg, RootTimer},
};

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Scheduler {
    /// Id of the datastore node.
    pub datastore: Id,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct SchedulerState {
    /// The current view of the nodes.
    /// Value is the apps that node is running.
    nodes: BTreeMap<Id, Vec<AppId>>,

    /// Apps that need scheduling
    apps_to_schedule: Vec<App>,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum SchedulerTimer {}

impl Actor for Scheduler {
    type Msg = RootMsg;

    type State = SchedulerState;

    type Timer = RootTimer;

    fn on_start(&self, _id: Id, o: &mut Out<Self>) -> Self::State {
        o.send(self.datastore, RootMsg::SchedulerJoin);
        SchedulerState::default()
    }

    fn on_msg(
        &self,
        _id: Id,
        state: &mut std::borrow::Cow<Self::State>,
        _src: Id,
        msg: Self::Msg,
        o: &mut Out<Self>,
    ) {
        match msg {
            RootMsg::NodeJoin => todo!(),
            RootMsg::SchedulerJoin => todo!(),
            RootMsg::NodeJoinedEvent(node) => {
                state.to_mut().nodes.insert(node, Vec::new());
                self.schedule(state, o);
            }
            RootMsg::NewAppEvent(app) => {
                state.to_mut().apps_to_schedule.push(app);
                self.schedule(state, o);
            }
            RootMsg::ScheduledAppEvent(app, node) => {
                state.to_mut().nodes.entry(node).or_default().push(app.id);
            }
            RootMsg::ScheduleAppRequest(_, _) => todo!(),
            RootMsg::ScheduleAppResponse(result) => {
                if !result {
                    // TODO: should refresh its view of the node allocations
                }
            }
            RootMsg::CreateAppRequest(_) => todo!(),
            RootMsg::CreateAppResponse(_) => todo!(),
        }
    }

    fn name(&self) -> String {
        "Scheduler".to_owned()
    }
}

impl Scheduler {
    fn schedule(&self, state: &mut Cow<SchedulerState>, o: &mut Out<Self>) {
        let mut_state = state.to_mut();
        mut_state.apps_to_schedule.retain(|app| {
            let least_loaded_node = mut_state
                .nodes
                .iter()
                .map(|(n, apps)| (n, apps.len()))
                .min_by_key(|(_, apps)| *apps);
            if let Some((node, _)) = least_loaded_node {
                // TODO: use an actual scheduling strategy
                o.send(
                    self.datastore,
                    RootMsg::ScheduleAppRequest(app.clone(), *node),
                );
                // TODO: handle scheduling multiple apps in one go
                // need to update the view as we go and have a way of rolling back
                false
            } else {
                true
            }
        })
    }
}

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use futures::TryStreamExt;
use kube::{
    runtime::{watcher, watcher::Event},
    Api, Client,
};
use tokio::{sync::Mutex, task::JoinHandle};
use tracing::info;

use crate::{
    controller::{Controller, DeploymentController},
    state::StateView,
};

type AppState = Arc<Mutex<StateView>>;

pub async fn run() -> (Arc<AtomicBool>, Vec<JoinHandle<()>>) {
    let client = Client::try_default().await.unwrap();
    let state = Arc::new(Mutex::new(StateView::default()));
    let shutdown = Arc::new(AtomicBool::new(false));
    let deployment_watcher = watcher::watcher(
        Api::<k8s_openapi::api::apps::v1::Deployment>::all(client.clone()),
        watcher::Config::default(),
    );
    let mut handles = Vec::new();
    let state2 = Arc::clone(&state);
    tokio::spawn(async move {
        deployment_watcher
            .try_for_each(|dep| async {
                match dep {
                    Event::Applied(dep) => {
                        println!("deployment applied {}", dep.metadata.name.as_ref().unwrap());
                        let local_dep =
                            serde_json::from_value(serde_json::to_value(dep).unwrap()).unwrap();
                        let mut state = state2.lock().await;
                        let revision = state.revision.clone().increment();
                        state.revision = revision.clone();
                        state.deployments.insert(local_dep, revision).unwrap();
                    }
                    Event::Deleted(dep) => {
                        println!("deployment deleted {}", dep.metadata.name.as_ref().unwrap());
                        let mut state = state2.lock().await;
                        let revision = state.revision.clone().increment();
                        state.revision = revision.clone();
                        state
                            .deployments
                            .remove(dep.metadata.name.as_ref().unwrap());
                    }
                    Event::Restarted(_deps) => {
                        todo!()
                    }
                }
                Ok(())
            })
            .await
            .unwrap();
    });

    macro_rules! run_controller {
        ($cont:ident) => {
            let state2 = Arc::clone(&state);
            let sd = Arc::clone(&shutdown);
            handles.push(tokio::spawn(async move {
                controller_loop(state2, $cont, sd).await;
            }));
        };
    }
    run_controller!(DeploymentController);
    (shutdown, handles)
}

async fn controller_loop<C: Controller>(state: AppState, controller: C, shutdown: Arc<AtomicBool>) {
    info!(name = controller.name(), "Starting controller");
    let mut cstate = C::State::default();
    let mut last_revision = state.lock().await.revision.clone();
    let rate_limit = Duration::from_millis(500);
    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }

        tokio::time::sleep(rate_limit).await;

        let mut s = state.lock().await;

        if s.revision == last_revision {
            continue;
        }

        info!(name = controller.name(), "Checking for steps");
        if let Some(operation) = controller.step(&s.state, &mut cstate) {
            info!(name = controller.name(), "Got operation to perform");
            let revision = s.revision.clone();
            s.apply_operation(operation.into(), revision.increment());
        }
        last_revision = s.revision.clone();
        info!(name = controller.name(), "Finished processing step");
    }
    info!(name = controller.name(), "Stopping controller");
}

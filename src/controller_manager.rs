use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use futures::TryStreamExt;
use kube::{
    api::PostParams,
    runtime::{watcher, watcher::Event},
    Api, Client,
};
use tokio::{sync::Mutex, task::JoinHandle};
use tracing::info;

use crate::{
    abstract_model::ControllerAction,
    controller::{job::JobController, Controller, DeploymentController, ReplicaSetController},
    state::revision::Revision,
    state::StateView,
};

type AppState = Arc<Mutex<StateView>>;

pub async fn run() -> (Arc<AtomicBool>, Vec<JoinHandle<()>>) {
    let client = Client::try_default().await.unwrap();
    let state = Arc::new(Mutex::new(StateView::default()));
    let shutdown = Arc::new(AtomicBool::new(false));
    let mut handles = Vec::new();

    macro_rules! watch_resource {
        ($kind:ty, $field:ident) => {
            let watcher = watcher::watcher(
                Api::<$kind>::all(client.clone()),
                watcher::Config::default(),
            );
            let state2 = Arc::clone(&state);
            tokio::spawn(async move {
                watcher
                    .try_for_each(|dep| async {
                        match dep {
                            Event::Applied(dep) => {
                                println!(
                                    "resource applied {}",
                                    dep.metadata.name.as_ref().unwrap()
                                );
                                let mut state = state2.lock().await;
                                let revision = Revision::try_from(
                                    dep.metadata.resource_version.as_ref().unwrap().as_str(),
                                )
                                .unwrap();
                                state.revision =
                                    std::cmp::max(state.revision.clone(), revision.clone());
                                let local_dep =
                                    serde_json::from_value(serde_json::to_value(dep).unwrap())
                                        .unwrap();
                                state.$field.insert(local_dep, revision).unwrap();
                            }
                            Event::Deleted(dep) => {
                                println!(
                                    "resource deleted {}",
                                    dep.metadata.name.as_ref().unwrap()
                                );
                                let mut state = state2.lock().await;
                                let revision = Revision::try_from(
                                    dep.metadata.resource_version.as_ref().unwrap().as_str(),
                                )
                                .unwrap();
                                state.revision = std::cmp::max(state.revision.clone(), revision);
                                state.$field.remove(dep.metadata.name.as_ref().unwrap());
                            }
                            Event::Restarted(deps) => {
                                println!("resource watch restarted {:?}", deps);
                                let mut state = state2.lock().await;
                                for dep in deps {
                                    let revision = Revision::try_from(
                                        dep.metadata.resource_version.as_ref().unwrap().as_str(),
                                    )
                                    .unwrap();
                                    state.revision =
                                        std::cmp::max(state.revision.clone(), revision.clone());
                                    let local_dep =
                                        serde_json::from_value(serde_json::to_value(dep).unwrap())
                                            .unwrap();
                                    state.$field.insert(local_dep, revision.clone()).unwrap();
                                }
                            }
                        }
                        Ok(())
                    })
                    .await
                    .unwrap();
            });
        };
    }
    watch_resource!(k8s_openapi::api::apps::v1::Deployment, deployments);
    watch_resource!(k8s_openapi::api::apps::v1::ReplicaSet, replicasets);
    watch_resource!(k8s_openapi::api::core::v1::Pod, pods);
    watch_resource!(k8s_openapi::api::batch::v1::Job, jobs);
    // watch_resource!(k8s_openapi::api::apps::v1::StatefulSet, statefulsets);
    // watch_resource!(
    //     k8s_openapi::api::apps::v1::ControllerRevision,
    //     controller_revisions
    // );
    watch_resource!(
        k8s_openapi::api::core::v1::PersistentVolumeClaim,
        persistent_volume_claims
    );
    watch_resource!(k8s_openapi::api::core::v1::Node, nodes);

    macro_rules! run_controller {
        ($cont:ident) => {
            let state2 = Arc::clone(&state);
            let sd = Arc::clone(&shutdown);
            let client2 = client.clone();
            handles.push(tokio::spawn(async move {
                controller_loop(state2, $cont, sd, client2).await;
            }));
        };
    }
    run_controller!(DeploymentController);
    // run_controller!(StatefulSetController);
    run_controller!(JobController);
    run_controller!(ReplicaSetController);

    (shutdown, handles)
}

async fn controller_loop<C: Controller>(
    state: AppState,
    controller: C,
    shutdown: Arc<AtomicBool>,
    client: Client,
) {
    info!(name = controller.name(), "Starting controller");
    let mut cstate = C::State::default();
    let mut last_revision = state.lock().await.revision.clone();
    let rate_limit = Duration::from_millis(500);
    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }

        tokio::time::sleep(rate_limit).await;

        let s = state.lock().await;

        if s.revision == last_revision {
            continue;
        }

        info!(name = controller.name(), "Checking for steps");
        if let Some(operation) = controller.step(&s, &mut cstate) {
            info!(name = controller.name(), "Got operation to perform");
            // let revision = s.revision.clone();
            // s.apply_operation(operation.into(), revision.increment());
            handle_action(operation.into(), client.clone()).await;
        }
        last_revision = s.revision.clone();
        info!(name = controller.name(), "Finished processing step");
    }
    info!(name = controller.name(), "Stopping controller");
}

async fn handle_action(action: ControllerAction, client: Client) {
    match action {
        ControllerAction::NodeJoin(_, _) => todo!(),
        ControllerAction::CreatePod(mut pod) => {
            if pod.metadata.namespace.is_empty() {
                pod.metadata.namespace = "default".to_owned();
            }
            let api =
                Api::<k8s_openapi::api::core::v1::Pod>::namespaced(client, &pod.metadata.namespace);
            let remote_pod: k8s_openapi::api::core::v1::Pod =
                serde_json::from_value(serde_json::to_value(pod).unwrap()).unwrap();
            api.create(&PostParams::default(), &remote_pod)
                .await
                .unwrap();
        }
        ControllerAction::SoftDeletePod(_) => todo!(),
        ControllerAction::HardDeletePod(_) => todo!(),
        ControllerAction::SchedulePod(_, _) => todo!(),
        ControllerAction::UpdatePod(_) => todo!(),
        ControllerAction::UpdateDeployment(mut dep) => {
            if dep.metadata.namespace.is_empty() {
                dep.metadata.namespace = "default".to_owned();
            }
            let api = Api::<k8s_openapi::api::apps::v1::Deployment>::namespaced(
                client,
                &dep.metadata.namespace,
            );
            let remote_dep: k8s_openapi::api::apps::v1::Deployment =
                serde_json::from_value(serde_json::to_value(dep).unwrap()).unwrap();
            api.replace(
                &remote_dep.metadata.name.clone().unwrap(),
                &PostParams::default(),
                &remote_dep,
            )
            .await
            .unwrap();
        }
        ControllerAction::RequeueDeployment(_) => todo!(),
        ControllerAction::UpdateDeploymentStatus(mut dep) => {
            if dep.metadata.namespace.is_empty() {
                dep.metadata.namespace = "default".to_owned();
            }
            let api = Api::<k8s_openapi::api::apps::v1::Deployment>::namespaced(
                client,
                &dep.metadata.namespace,
            );
            api.replace_status(
                &dep.metadata.name.clone(),
                &PostParams::default(),
                serde_json::to_vec(&dep).unwrap(),
            )
            .await
            .unwrap();
        }
        ControllerAction::CreateReplicaSet(mut rs) => {
            if rs.metadata.namespace.is_empty() {
                rs.metadata.namespace = "default".to_owned();
            }
            let api = Api::<k8s_openapi::api::apps::v1::ReplicaSet>::namespaced(
                client,
                &rs.metadata.namespace,
            );
            let remote_rs: k8s_openapi::api::apps::v1::ReplicaSet =
                serde_json::from_value(serde_json::to_value(rs).unwrap()).unwrap();
            api.create(&PostParams::default(), &remote_rs)
                .await
                .unwrap();
        }
        ControllerAction::UpdateReplicaSet(mut rs) => {
            if rs.metadata.namespace.is_empty() {
                rs.metadata.namespace = "default".to_owned();
            }
            let api = Api::<k8s_openapi::api::apps::v1::ReplicaSet>::namespaced(
                client,
                &rs.metadata.namespace,
            );
            let remote_rs: k8s_openapi::api::apps::v1::ReplicaSet =
                serde_json::from_value(serde_json::to_value(rs).unwrap()).unwrap();
            api.replace(
                &remote_rs.metadata.name.clone().unwrap(),
                &PostParams::default(),
                &remote_rs,
            )
            .await
            .unwrap();
        }
        ControllerAction::UpdateReplicaSetStatus(mut rs) => {
            if rs.metadata.namespace.is_empty() {
                rs.metadata.namespace = "default".to_owned();
            }
            let api = Api::<k8s_openapi::api::apps::v1::ReplicaSet>::namespaced(
                client,
                &rs.metadata.namespace,
            );
            api.replace_status(
                &rs.metadata.name.clone(),
                &PostParams::default(),
                serde_json::to_vec(&rs).unwrap(),
            )
            .await
            .unwrap();
        }
        ControllerAction::UpdateReplicaSets(_) => todo!(),
        ControllerAction::DeleteReplicaSet(_) => todo!(),
        ControllerAction::UpdateStatefulSet(_) => todo!(),
        ControllerAction::UpdateStatefulSetStatus(_) => todo!(),
        ControllerAction::CreateControllerRevision(_) => todo!(),
        ControllerAction::UpdateControllerRevision(_) => todo!(),
        ControllerAction::DeleteControllerRevision(_) => todo!(),
        ControllerAction::CreatePersistentVolumeClaim(_) => todo!(),
        ControllerAction::UpdatePersistentVolumeClaim(_) => todo!(),
        ControllerAction::UpdateJob(_) => todo!(),
        ControllerAction::UpdateJobStatus(_) => todo!(),
    }
}

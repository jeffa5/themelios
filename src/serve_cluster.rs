use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use crate::api::APIObject;
use crate::api::SerializableResource;
use crate::controller::job::JobController;
use crate::controller::podgc::PodGCController;
use crate::controller::Controller;
use crate::controller::DeploymentController;
use crate::controller::NodeController;
use crate::controller::ReplicaSetController;
use crate::controller::SchedulerController;
use crate::controller::StatefulSetController;
use crate::resources::Deployment;
use crate::resources::Node;
use crate::resources::Pod;
use crate::resources::ReplicaSet;
use crate::resources::Scale;
use crate::state::StateView;
use axum::extract::Path;
use axum::extract::State;
use axum::routing::delete;
use axum::routing::patch;
use axum::routing::put;
use axum::{
    http::{Method, StatusCode, Uri},
    routing::get,
    routing::post,
    Json, Router,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::APIGroup;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::APIGroupList;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::GroupVersionForDiscovery;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::Status;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{APIResourceList, ListMeta};
use k8s_openapi::List;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tower_http::trace::TraceLayer;
use tracing::{debug, info, warn};

type AppState = Arc<Mutex<StateView>>;

pub async fn run(address: String) -> (Arc<AtomicBool>, Vec<JoinHandle<()>>) {
    let trace_layer = TraceLayer::new_for_http();
    let state = Arc::new(Mutex::new(StateView::default()));
    let shutdown = Arc::new(AtomicBool::new(false));
    let mut handles = Vec::new();

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
    run_controller!(StatefulSetController);
    run_controller!(JobController);
    run_controller!(ReplicaSetController);
    run_controller!(SchedulerController);
    run_controller!(PodGCController);

    let state2 = Arc::clone(&state);
    let sd = Arc::clone(&shutdown);
    handles.push(tokio::spawn(async move {
        controller_loop(
            state2,
            NodeController {
                name: "node1".to_owned(),
            },
            sd,
        )
        .await;
    }));

    let app = app(state).layer(trace_layer);
    let listener = tokio::net::TcpListener::bind(address).await.unwrap();
    let sd = Arc::clone(&shutdown);
    handles.push(tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                loop {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    if sd.load(Ordering::Relaxed) {
                        break;
                    }
                }
                info!("Stopping serving api");
            })
            .await
            .unwrap()
    }));
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

        debug!(name = controller.name(), "Checking for steps");
        if let Some(operation) = controller.step(&s, &mut cstate) {
            info!(name = controller.name(), "Got operation to perform");
            let revision = s.revision.clone();
            if !s.apply_operation(operation.into(), revision.increment()) {
                warn!(name = controller.name(), "Failed to apply operation");
            }
        }
        last_revision = s.revision.clone();
        debug!(name = controller.name(), "Finished processing step");
    }
    info!(name = controller.name(), "Stopping controller");
}

fn app(state: AppState) -> Router {
    Router::new()
        .route("/apis", get(api_groups))
        .nest("/apis", apis())
        .nest("/api", apis())
        .fallback(fallback)
        .with_state(state)
}

fn apis() -> Router<AppState> {
    Router::new()
        .route("/v1", get(list_core_v1))
        .nest("/v1", core_v1())
        .route("/apps/v1", get(list_apps_v1))
        .nest("/apps/v1", apps_v1())
}

fn core_v1() -> Router<AppState> {
    Router::new()
        // .route("/namespaces", get(list_namespaces))
        .nest("/namespaces", namespaces_core_v1())
}

fn namespaces_core_v1() -> Router<AppState> {
    Router::new().nest("/default", resources_core_v1())
}

fn resources_core_v1() -> Router<AppState> {
    Router::new()
        .nest("/pods", pods_router())
        .nest("/nodes", nodes_router())
}

fn pods_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_pods))
        .route("/:name", get(get_pod))
        .route("/:name", delete(delete_pod))
}
fn nodes_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_nodes))
        .route("/:name", get(get_node))
}

fn apps_v1() -> Router<AppState> {
    Router::new()
        // .route("/namespaces", get(list_namespaces))
        .nest("/namespaces", namespaces_apps_v1())
}

fn namespaces_apps_v1() -> Router<AppState> {
    Router::new().nest("/default", resources_apps_v1())
}

fn resources_apps_v1() -> Router<AppState> {
    Router::new()
        .nest("/deployments", deployments_router())
        .nest("/replicasets", replicasets_router())
}

fn deployments_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_deployments))
        .route("/:name", get(get_deployment))
        .route("/", post(create_deployment))
        .route("/:name", put(update_deployment))
        .route("/:name/scale", patch(scale_deployment))
        .route("/:name", delete(delete_deployment))
}

#[tracing::instrument(skip_all)]
async fn list_deployments(
    State(state): State<AppState>,
) -> (StatusCode, Json<List<SerializableResource<Deployment>>>) {
    info!("Got list request for deployments");
    let state = state.lock().await;
    let deployments = List {
        items: state
            .deployments
            .iter()
            .map(|d| SerializableResource::new(d.clone()))
            .collect(),
        metadata: ListMeta {
            continue_: None,
            remaining_item_count: None,
            resource_version: Some(state.revision.to_string()),
            self_link: None,
        },
    };
    (StatusCode::OK, Json(deployments))
}

#[tracing::instrument(skip_all)]
async fn get_deployment(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> (StatusCode, Json<SerializableResource<Deployment>>) {
    info!("Got get request for deployment");
    let state = state.lock().await;
    if let Some(deployment) = state.deployments.get(&name) {
        (
            StatusCode::OK,
            Json(SerializableResource::new(deployment.clone())),
        )
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(SerializableResource::new(Deployment::default())),
        )
    }
}

#[tracing::instrument(skip_all)]
async fn create_deployment(
    State(state): State<AppState>,
    Json(deployment): Json<Deployment>,
) -> (StatusCode, Json<SerializableResource<Deployment>>) {
    info!("Got create request for deployment");
    let mut s = state.lock().await;
    s.revision = s.revision.clone().increment();
    let revision = s.revision.clone();
    let deployment_name = deployment.metadata.name.clone();
    s.deployments.create(deployment, revision).unwrap();
    let deployment = s.deployments.get(&deployment_name).unwrap().clone();
    (StatusCode::OK, Json(SerializableResource::new(deployment)))
}

#[tracing::instrument(skip_all)]
async fn update_deployment(
    State(state): State<AppState>,
    Json(deployment): Json<Deployment>,
) -> (StatusCode, Json<SerializableResource<Deployment>>) {
    info!("Got create request for deployment");
    let mut s = state.lock().await;
    s.revision = s.revision.clone().increment();
    let revision = s.revision.clone();
    let deployment_name = deployment.metadata.name.clone();
    s.deployments.update(deployment, revision).unwrap();
    let deployment = s.deployments.get(&deployment_name).unwrap().clone();
    (StatusCode::OK, Json(SerializableResource::new(deployment)))
}

#[tracing::instrument(skip_all)]
async fn scale_deployment(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(scale): Json<Scale>,
) -> (StatusCode, Json<SerializableResource<Deployment>>) {
    info!("Got scale request for deployment");
    let mut s = state.lock().await;
    s.revision = s.revision.clone().increment();
    let revision = s.revision.clone();
    let mut deployment = s.deployments.get(&name).unwrap().clone();
    deployment.spec.replicas = scale.spec.replicas;
    s.deployments.update(deployment, revision).unwrap();
    let deployment = s.deployments.get(&name).unwrap().clone();
    (StatusCode::OK, Json(SerializableResource::new(deployment)))
}

#[tracing::instrument(skip_all)]
async fn delete_deployment(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> (StatusCode, Json<Status>) {
    info!("Got create request for deployment");
    let mut s = state.lock().await;
    s.revision = s.revision.clone().increment();
    s.deployments.remove(&name);
    (
        StatusCode::OK,
        Json(Status {
            code: None,
            details: None,
            message: None,
            metadata: ListMeta::default(),
            reason: None,
            status: Some("Success".to_owned()),
        }),
    )
}

fn replicasets_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_replicasets))
        .route("/:name", get(get_replicaset))
        .route("/", post(create_replicaset))
        .route("/:name", put(update_replicaset))
        .route("/:name", delete(delete_replicaset))
}

#[tracing::instrument(skip_all)]
async fn list_replicasets(
    State(state): State<AppState>,
) -> (StatusCode, Json<List<SerializableResource<ReplicaSet>>>) {
    info!("Got list request for replicasets");
    let state = state.lock().await;
    let replicasets = List {
        items: state
            .replicasets
            .iter()
            .map(|d| SerializableResource::new(d.clone()))
            .collect(),
        metadata: ListMeta {
            continue_: None,
            remaining_item_count: None,
            resource_version: Some(state.revision.to_string()),
            self_link: None,
        },
    };
    (StatusCode::OK, Json(replicasets))
}

#[tracing::instrument(skip_all)]
async fn get_replicaset(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> (StatusCode, Json<SerializableResource<ReplicaSet>>) {
    info!("Got get request for replicaset");
    let state = state.lock().await;
    if let Some(replicaset) = state.replicasets.get(&name) {
        (
            StatusCode::OK,
            Json(SerializableResource::new(replicaset.clone())),
        )
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(SerializableResource::new(ReplicaSet::default())),
        )
    }
}

#[tracing::instrument(skip_all)]
async fn create_replicaset(
    State(state): State<AppState>,
    Json(replicaset): Json<ReplicaSet>,
) -> (StatusCode, Json<ReplicaSet>) {
    info!("Got create request for replicaset");
    let mut s = state.lock().await;
    s.revision = s.revision.clone().increment();
    let revision = s.revision.clone();
    let replicaset_name = replicaset.metadata.name.clone();
    s.replicasets.create(replicaset, revision).unwrap();
    let replicaset = s.replicasets.get(&replicaset_name).unwrap().clone();
    (StatusCode::OK, Json(replicaset))
}

#[tracing::instrument(skip_all)]
async fn update_replicaset(
    State(state): State<AppState>,
    Json(replicaset): Json<ReplicaSet>,
) -> (StatusCode, Json<ReplicaSet>) {
    info!("Got create request for replicaset");
    let mut s = state.lock().await;
    s.revision = s.revision.clone().increment();
    let revision = s.revision.clone();
    let replicaset_name = replicaset.metadata.name.clone();
    s.replicasets.update(replicaset, revision).unwrap();
    let replicaset = s.replicasets.get(&replicaset_name).unwrap().clone();
    (StatusCode::OK, Json(replicaset))
}

#[tracing::instrument(skip_all)]
async fn delete_replicaset(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> (StatusCode, Json<Status>) {
    info!("Got create request for replicaset");
    let mut s = state.lock().await;
    s.revision = s.revision.clone().increment();
    s.replicasets.remove(&name);
    (
        StatusCode::OK,
        Json(Status {
            code: None,
            details: None,
            message: None,
            metadata: ListMeta::default(),
            reason: None,
            status: Some("Success".to_owned()),
        }),
    )
}

#[tracing::instrument(skip_all)]
async fn api_groups() -> (StatusCode, Json<APIGroupList>) {
    info!("Got request for api groups");
    let apiversions = APIGroupList {
        groups: vec![
            APIGroup {
                name: "".to_owned(),
                preferred_version: None,
                server_address_by_client_cidrs: None,
                versions: vec![GroupVersionForDiscovery {
                    group_version: "v1".to_owned(),
                    version: "v1".to_owned(),
                }],
            },
            APIGroup {
                name: "apps".to_owned(),
                preferred_version: None,
                server_address_by_client_cidrs: None,
                versions: vec![GroupVersionForDiscovery {
                    group_version: "apps/v1".to_owned(),
                    version: "v1".to_owned(),
                }],
            },
        ],
    };
    (StatusCode::OK, Json(apiversions))
}

#[tracing::instrument(skip_all)]
async fn list_core_v1() -> (StatusCode, Json<APIResourceList>) {
    info!("Got request for api v1 versions");
    let apiversions = APIResourceList {
        group_version: "v1".to_owned(),
        resources: vec![Pod::api_resource(), Node::api_resource()],
    };
    (StatusCode::OK, Json(apiversions))
}

#[tracing::instrument(skip_all)]
async fn list_apps_v1() -> (StatusCode, Json<APIResourceList>) {
    info!("Got request for api apps/v1 versions");
    let apiversions = APIResourceList {
        group_version: "apps/v1".to_owned(),
        resources: vec![
            Deployment::api_resource(),
            Scale::api_resource::<Deployment>(),
            ReplicaSet::api_resource(),
        ],
    };
    (StatusCode::OK, Json(apiversions))
}

#[tracing::instrument(skip_all)]
async fn list_pods(
    State(state): State<AppState>,
) -> (StatusCode, Json<List<SerializableResource<Pod>>>) {
    info!("Got list request for pods");
    let state = state.lock().await;
    let pods = List {
        items: state
            .pods
            .iter()
            .map(|p| SerializableResource::new(p.clone()))
            .collect(),
        metadata: ListMeta {
            continue_: None,
            remaining_item_count: None,
            resource_version: None,
            self_link: None,
        },
    };
    (StatusCode::OK, Json(pods))
}

#[tracing::instrument(skip_all)]
async fn get_pod(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> (StatusCode, Json<SerializableResource<Pod>>) {
    info!("Got get request for pods");
    let state = state.lock().await;
    if let Some(pod) = state.pods.get(&name) {
        (StatusCode::OK, Json(SerializableResource::new(pod.clone())))
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(SerializableResource::new(Pod::default())),
        )
    }
}

#[tracing::instrument(skip_all)]
async fn delete_pod(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> (StatusCode, Json<Status>) {
    info!("Got delete request for pods");
    let mut state = state.lock().await;
    state.revision = state.revision.clone().increment();
    state.pods.remove(&name);
    (
        StatusCode::OK,
        Json(Status {
            code: None,
            details: None,
            message: None,
            metadata: ListMeta::default(),
            reason: None,
            status: Some("Success".to_owned()),
        }),
    )
}

#[tracing::instrument(skip_all)]
async fn list_nodes(
    State(state): State<AppState>,
) -> (StatusCode, Json<List<SerializableResource<Node>>>) {
    info!("Got list request for nodes");
    let state = state.lock().await;
    let nodes = List {
        items: state
            .nodes
            .iter()
            .map(|p| SerializableResource::new(p.clone()))
            .collect(),
        metadata: ListMeta {
            continue_: None,
            remaining_item_count: None,
            resource_version: None,
            self_link: None,
        },
    };
    (StatusCode::OK, Json(nodes))
}

#[tracing::instrument(skip_all)]
async fn get_node(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> (StatusCode, Json<SerializableResource<Node>>) {
    info!("Got get request for nodes");
    let state = state.lock().await;
    if let Some(node) = state.nodes.get(&name) {
        (
            StatusCode::OK,
            Json(SerializableResource::new(node.clone())),
        )
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(SerializableResource::new(Node::default())),
        )
    }
}

#[tracing::instrument(skip_all)]
async fn fallback(method: Method, uri: Uri) -> StatusCode {
    warn!(%method, %uri, "No matching handler for request");
    StatusCode::NOT_FOUND
}

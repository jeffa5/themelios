use std::sync::Arc;

use crate::api::APIObject;
use crate::api::SerializableResource;
use crate::resources::Deployment;
use crate::resources::Pod;
use crate::state::StateView;
use axum::extract::Path;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::delete;
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
use tracing::{info, warn};

type AppState = Arc<Mutex<StateView>>;

pub fn app() -> Router {
    let state = Arc::new(Mutex::new(StateView::default()));
    Router::new()
        .route("/apis", get(api_groups))
        .nest("/apis", apis())
        .fallback(fallback)
        .with_state(state)
}

pub fn apis() -> Router<AppState> {
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
    Router::new().route("/pods", get(list_pods))
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
        .route("/deployments", get(list_deployments))
        .route("/deployments/:name", get(get_deployment))
        .route("/deployments", post(create_deployment))
        .route("/deployments/:name", put(update_deployment))
        .route("/deployments/:name", delete(delete_deployment))
}

#[tracing::instrument(skip_all)]
async fn list_deployments(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> (StatusCode, Json<List<SerializableResource<Deployment>>>) {
    info!("Got list request for deployments");
    dbg!(headers);
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
    println!("{}", serde_json::to_string(&deployments).unwrap());
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
) -> (StatusCode, Json<Deployment>) {
    info!("Got create request for deployment");
    let mut s = state.lock().await;
    s.revision = s.revision.clone().increment();
    let revision = s.revision.clone();
    let deployment_name = deployment.metadata.name.clone();
    s.deployments.insert(deployment, revision).unwrap();
    let deployment = s.deployments.get(&deployment_name).unwrap().clone();
    (StatusCode::OK, Json(deployment))
}

#[tracing::instrument(skip_all)]
async fn update_deployment(
    State(state): State<AppState>,
    Json(deployment): Json<Deployment>,
) -> (StatusCode, Json<Deployment>) {
    info!("Got create request for deployment");
    let mut s = state.lock().await;
    s.revision = s.revision.clone().increment();
    let revision = s.revision.clone();
    let deployment_name = deployment.metadata.name.clone();
    s.deployments.insert(deployment, revision).unwrap();
    let deployment = s.deployments.get(&deployment_name).unwrap().clone();
    (StatusCode::OK, Json(deployment))
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
        resources: vec![Pod::api_resource()],
    };
    (StatusCode::OK, Json(apiversions))
}

#[tracing::instrument(skip_all)]
async fn list_apps_v1() -> (StatusCode, Json<APIResourceList>) {
    info!("Got request for api apps/v1 versions");
    let apiversions = APIResourceList {
        group_version: "apps/v1".to_owned(),
        resources: vec![Deployment::api_resource()],
    };
    (StatusCode::OK, Json(apiversions))
}

#[tracing::instrument(skip_all)]
async fn list_pods() -> (StatusCode, Json<List<Pod>>) {
    info!("Got list request for pods");
    let pods = List {
        items: vec![],
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
async fn fallback(method: Method, uri: Uri) -> StatusCode {
    warn!(%method, %uri, "No matching handler for request");
    StatusCode::NOT_FOUND
}

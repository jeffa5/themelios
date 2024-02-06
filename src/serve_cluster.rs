use std::sync::Arc;

use crate::api::APIObject;
use crate::api::SerializableResource;
use crate::resources::Deployment;
use crate::resources::Pod;
use crate::state::StateView;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::{
    http::{Method, StatusCode, Uri},
    routing::get,
    routing::post,
    Json, Router,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{
    APIResourceList, APIVersions, ListMeta, ServerAddressByClientCIDR,
};
use k8s_openapi::List;
use tokio::sync::Mutex;
use tracing::{info, warn};

type AppState = Arc<Mutex<StateView>>;

pub fn app() -> Router {
    let state = Arc::new(Mutex::new(StateView::default()));
    Router::new()
        .route("/api", get(api))
        .route("/apis", get(api))
        .nest("/api", apis())
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
        .route("/deployments", post(create_deployment))
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
    println!("{}", serde_json::to_string_pretty(&deployments).unwrap());
    (StatusCode::OK, Json(deployments))
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
    s.deployments.insert(deployment.clone(), revision).unwrap();
    (StatusCode::OK, Json(deployment))
}

#[tracing::instrument(skip_all)]
async fn api() -> (StatusCode, Json<APIVersions>) {
    info!("Got request for api versions");
    let apiversions = APIVersions {
        server_address_by_client_cidrs: vec![ServerAddressByClientCIDR {
            client_cidr: "0.0.0.0".to_owned(),
            server_address: "127.0.0.1:8000".to_owned(),
        }],
        versions: vec!["v1".to_owned(), "apps/v1".to_owned()],
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

use crate::resources::Pod;
use axum::{
    http::{Method, StatusCode, Uri},
    routing::get,
    Json, Router,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{
    APIResource, APIResourceList, APIVersions, ListMeta, ServerAddressByClientCIDR,
};
use k8s_openapi::List;
use tracing::{info, warn};

pub fn app() -> Router {
    Router::new()
        .route("/api", get(api))
        .route("/apis", get(api))
        .nest("/api", apis())
        .nest("/apis", apis())
        .fallback(fallback)
}

pub fn apis() -> Router {
    Router::new()
        .route("/v1", get(list_v1))
        .nest("/v1", apis_v1())
}

fn apis_v1() -> Router {
    Router::new()
        // .route("/namespaces", get(list_namespaces))
        .nest("/namespaces", namespaces_v1())
}

fn namespaces_v1() -> Router {
    Router::new().nest("/default", resources_v1())
}

fn resources_v1() -> Router {
    Router::new().route("/pods", get(list_pods))
}

#[tracing::instrument(skip_all)]
async fn api() -> (StatusCode, Json<APIVersions>) {
    info!("Got request for api versions");
    let apiversions = APIVersions {
        server_address_by_client_cidrs: vec![ServerAddressByClientCIDR {
            client_cidr: "0.0.0.0".to_owned(),
            server_address: "127.0.0.1:8000".to_owned(),
        }],
        versions: vec!["v1".to_owned()],
    };
    (StatusCode::OK, Json(apiversions))
}

#[tracing::instrument(skip_all)]
async fn list_v1() -> (StatusCode, Json<APIResourceList>) {
    info!("Got request for api v1 versions");
    let apiversions = APIResourceList {
        group_version: "v1".to_owned(),
        resources: vec![APIResource {
            categories: None,
            group: None,
            kind: "Pod".to_owned(),
            name: "pods".to_owned(),
            namespaced: true,
            short_names: None,
            singular_name: "pod".to_owned(),
            storage_version_hash: None,
            verbs: vec![
                "get".to_owned(),
                "list".to_owned(),
                "create".to_owned(),
                "update".to_owned(),
                "delete".to_owned(),
            ],
            version: None,
        }],
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

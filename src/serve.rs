use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use maplit::btreemap;
use maplit::btreeset;
use serde_json::json;

use crate::abstract_model::Operation;
use crate::controller::{Controller, Scheduler, SchedulerState};
use crate::resources::{NodeResource, PodResource};
use crate::state::{Revision, StateView};

pub fn app() -> Router {
    Router::new().route("/scheduler", post(scheduler))
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct SchedulerRequest {
    pod: PodResource,
    nodes: Vec<NodeResource>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct SchedulerResponse {
    node_name: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
enum ErrorResponse {
    InvalidOperationReturned(Operation),
    NoOperation,
}

impl std::fmt::Display for ErrorResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::InvalidOperationReturned(op) => {
                    format!("Invalid operation returned from controller: {op:?}")
                }
                Self::NoOperation => {
                    "No operation returned from controller".to_owned()
                }
            }
        )
    }
}

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> axum::response::Response {
        let status = StatusCode::BAD_REQUEST;
        let body = Json(json!({
            "error": self.to_string(),
        }));
        (status, body).into_response()
    }
}

async fn scheduler(
    Json(payload): Json<SchedulerRequest>,
) -> Result<Json<SchedulerResponse>, ErrorResponse> {
    let s = Scheduler;
    let pod = payload.pod;
    let controller_id = 0;
    let state_view = StateView {
        revision: Revision::default(),
        nodes: payload.nodes.iter().cloned().enumerate().collect(),
        pods: btreemap!(pod.metadata.name.clone() => pod),
        controllers: btreeset![controller_id],
        ..Default::default()
    };
    let mut local_state = SchedulerState;
    match s.step(controller_id, &state_view, &mut local_state) {
        Some(Operation::SchedulePod(_, node)) => Ok(Json(SchedulerResponse { node_name: node })),
        Some(op) => Err(ErrorResponse::InvalidOperationReturned(op)),
        None => Err(ErrorResponse::NoOperation),
    }
}

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use maplit::btreemap;
use maplit::btreeset;
use serde_json::json;
use tracing::debug;
use tracing::warn;

use crate::abstract_model::Operation;
use crate::controller::{
    Controller, Deployment, DeploymentState, ReplicaSet, ReplicaSetState, Scheduler, SchedulerState,
};
use crate::resources::{DeploymentResource, NodeResource, PodResource, ReplicaSetResource};
use crate::state::{Revision, StateView};

pub fn app() -> Router {
    Router::new()
        .route("/scheduler", post(scheduler))
        .route("/deployment", post(deployment))
        .route("/replicaset", post(replicaset))
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
struct DeploymentRequest {
    deployment: DeploymentResource,
    replicasets: Vec<ReplicaSetResource>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
enum DeploymentResponse {
    UpdateDeployment {
        deployment: DeploymentResource,
    },
    RequeueDeployment {
        deployment: DeploymentResource,
    },
    UpdateDeploymentStatus {
        deployment: DeploymentResource,
    },
    CreateReplicaSet {
        replicaset: ReplicaSetResource,
    },
    UpdateReplicaSet {
        replicaset: ReplicaSetResource,
    },
    UpdateReplicaSets {
        replicasets: Vec<ReplicaSetResource>,
    },
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct ReplicasetRequest {
    replicaset: ReplicaSetResource,
    replicasets: Vec<ReplicaSetResource>,
    pods: Vec<PodResource>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
enum ReplicasetResponse {
    UpdatePod { pod: PodResource },
    CreatePod { pod: PodResource },
    DeletePod { pod: PodResource },
    UpdateReplicaSetStatus { replicaset: ReplicaSetResource },
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
        match &self {
            Self::InvalidOperationReturned(_op) => {
                let status = StatusCode::BAD_REQUEST;
                let body = Json(json!({
                    "error": self.to_string(),
                }));
                (status, body).into_response()
            }
            Self::NoOperation => (StatusCode::NO_CONTENT).into_response(),
        }
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

async fn deployment(
    Json(payload): Json<DeploymentRequest>,
) -> Result<Json<DeploymentResponse>, ErrorResponse> {
    let s = Deployment;
    debug!("Got deployment controller request");
    println!("{}", serde_yaml::to_string(&payload).unwrap());
    let controller_id = 0;
    let state_view = StateView {
        revision: Revision::default(),
        deployments: btreemap!(payload.deployment.metadata.name.clone() => payload.deployment),
        replica_sets: payload
            .replicasets
            .into_iter()
            .map(|rs| (rs.metadata.name.clone(), rs))
            .collect(),
        controllers: btreeset![controller_id],
        ..Default::default()
    };
    let mut local_state = DeploymentState;
    let operation = s.step(controller_id, &state_view, &mut local_state);
    debug!(?operation, "Got operation");
    match operation {
        Some(Operation::UpdateDeployment(dep)) => Ok(Json(DeploymentResponse::UpdateDeployment {
            deployment: dep,
        })),
        Some(Operation::RequeueDeployment(dep)) => {
            Ok(Json(DeploymentResponse::RequeueDeployment {
                deployment: dep,
            }))
        }
        Some(Operation::UpdateDeploymentStatus(dep)) => {
            Ok(Json(DeploymentResponse::UpdateDeploymentStatus {
                deployment: dep,
            }))
        }
        Some(Operation::CreateReplicaSet(rs)) => Ok(Json(DeploymentResponse::CreateReplicaSet {
            replicaset: rs,
        })),
        Some(Operation::UpdateReplicaSet(rs)) => Ok(Json(DeploymentResponse::UpdateReplicaSet {
            replicaset: rs,
        })),
        Some(Operation::UpdateReplicaSets(rss)) => {
            Ok(Json(DeploymentResponse::UpdateReplicaSets {
                replicasets: rss,
            }))
        }
        Some(op) => Err(ErrorResponse::InvalidOperationReturned(op)),
        None => Err(ErrorResponse::NoOperation),
    }
}

async fn replicaset(
    Json(payload): Json<ReplicasetRequest>,
) -> Result<Json<ReplicasetResponse>, ErrorResponse> {
    let s = ReplicaSet;
    debug!("Got replicaset controller request");
    println!("{}", serde_yaml::to_string(&payload).unwrap());
    let controller_id = 0;
    let mut replicasets = payload.replicasets;
    if !replicasets
        .iter()
        .any(|rs| rs.metadata.uid == payload.replicaset.metadata.uid)
    {
        replicasets.push(payload.replicaset);
    }
    let state_view = StateView {
        revision: Revision::default(),
        replica_sets: replicasets
            .into_iter()
            .map(|rs| (rs.metadata.name.clone(), rs))
            .collect(),
        pods: payload
            .pods
            .into_iter()
            .map(|p| (p.metadata.name.clone(), p))
            .collect(),
        controllers: btreeset![controller_id],
        ..Default::default()
    };
    let mut local_state = ReplicaSetState;
    let operation = s.step(controller_id, &state_view, &mut local_state);
    debug!(?operation, "Got operation");
    match operation {
        Some(Operation::UpdatePod(pod)) => Ok(Json(ReplicasetResponse::UpdatePod { pod })),
        Some(Operation::UpdateReplicaSetStatus(rs)) => {
            Ok(Json(ReplicasetResponse::UpdateReplicaSetStatus {
                replicaset: rs,
            }))
        }
        Some(Operation::CreatePod(pod)) => Ok(Json(ReplicasetResponse::CreatePod { pod })),
        Some(Operation::DeletePod(pod)) => Ok(Json(ReplicasetResponse::DeletePod { pod })),
        Some(op) => {
            warn!(?op, "Got invalid operation");
            Err(ErrorResponse::InvalidOperationReturned(op))
        }
        None => Err(ErrorResponse::NoOperation),
    }
}

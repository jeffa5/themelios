use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use maplit::btreeset;
use serde_json::json;
use tracing::debug;

use crate::abstract_model::ControllerAction;
use crate::controller::deployment::DeploymentControllerAction;
use crate::controller::job::{JobController, JobControllerAction, JobControllerState};
use crate::controller::replicaset::ReplicaSetControllerAction;
use crate::controller::scheduler::SchedulerControllerAction;
use crate::controller::statefulset::StatefulSetControllerAction;
use crate::controller::{
    Controller, DeploymentController, DeploymentControllerState, ReplicaSetController,
    ReplicaSetControllerState, SchedulerController, SchedulerControllerState,
    StatefulSetController, StatefulSetControllerState,
};
use crate::resources::{
    ControllerRevision, Deployment, Job, Node, PersistentVolumeClaim, Pod, ReplicaSet, StatefulSet,
};
use crate::state::{Revision, StateView};

pub fn app() -> Router {
    Router::new()
        .route("/scheduler", post(scheduler))
        .route("/deployment", post(deployment))
        .route("/replicaset", post(replicaset))
        .route("/statefulset", post(statefulset))
        .route("/job", post(job))
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SchedulerRequest {
    pod: Pod,
    bound_pods: Vec<Pod>,
    nodes: Vec<Node>,
    persistent_volume_claims: Vec<PersistentVolumeClaim>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
enum SchedulerResponse {
    SchedulePod {
        #[serde(rename = "nodeName")]
        node_name: String,
    },
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct DeploymentRequest {
    deployment: Deployment,
    replicasets: Vec<ReplicaSet>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
enum DeploymentResponse {
    UpdateDeployment { deployment: Deployment },
    RequeueDeployment { deployment: Deployment },
    UpdateDeploymentStatus { deployment: Deployment },
    CreateReplicaSet { replicaset: ReplicaSet },
    UpdateReplicaSet { replicaset: ReplicaSet },
    DeleteReplicaSet { replicaset: ReplicaSet },
    UpdateReplicaSets { replicasets: Vec<ReplicaSet> },
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct ReplicasetRequest {
    replicaset: ReplicaSet,
    replicasets: Vec<ReplicaSet>,
    pods: Vec<Pod>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
enum ReplicasetResponse {
    UpdatePod { pod: Pod },
    CreatePod { pod: Pod },
    DeletePod { pod: Pod },
    UpdateReplicaSetStatus { replicaset: ReplicaSet },
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatefulSetRequest {
    statefulset: StatefulSet,
    pods: Vec<Pod>,
    controller_revisions: Vec<ControllerRevision>,
    persistent_volume_claims: Vec<PersistentVolumeClaim>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
enum StatefulSetResponse {
    UpdatePod {
        pod: Pod,
    },
    CreatePod {
        pod: Pod,
    },
    DeletePod {
        pod: Pod,
    },
    UpdateStatefulSetStatus {
        statefulset: StatefulSet,
    },
    CreateControllerRevision {
        #[serde(rename = "controllerRevision")]
        controller_revision: ControllerRevision,
    },
    UpdateControllerRevision {
        #[serde(rename = "controllerRevision")]
        controller_revision: ControllerRevision,
    },
    DeleteControllerRevision {
        #[serde(rename = "controllerRevision")]
        controller_revision: ControllerRevision,
    },
    CreatePersistentVolumeClaim {
        #[serde(rename = "persistentVolumeClaim")]
        persistent_volume_claim: PersistentVolumeClaim,
    },
    UpdatePersistentVolumeClaim {
        #[serde(rename = "persistentVolumeClaim")]
        persistent_volume_claim: PersistentVolumeClaim,
    },
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobRequest {
    job: Job,
    pods: Vec<Pod>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
enum JobResponse {
    UpdateJobStatus { job: Job },
    CreatePod { pod: Pod },
    UpdatePod { pod: Pod },
    DeletePod { pod: Pod },
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
enum ErrorResponse {
    InvalidOperationReturned(ControllerAction),
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

#[tracing::instrument(skip_all)]
async fn scheduler(
    Json(payload): Json<SchedulerRequest>,
) -> Result<Json<SchedulerResponse>, ErrorResponse> {
    let s = SchedulerController;
    debug!("Got scheduler request");
    println!("{}", serde_yaml::to_string(&payload).unwrap());
    let controller_id = 0;
    let mut pods = payload.bound_pods;
    pods.push(payload.pod);
    let state_view = StateView {
        revision: Revision::default(),
        nodes: payload.nodes.into_iter().enumerate().collect(),
        pods: pods.into(),
        persistent_volume_claims: payload.persistent_volume_claims.into(),
        controllers: btreeset![controller_id],
        ..Default::default()
    };
    let mut local_state = SchedulerControllerState;
    let operation = s.step(controller_id, &state_view, &mut local_state);
    debug!(?operation, "Got operation");
    match operation {
        Some(SchedulerControllerAction::SchedulePod(_, node)) => {
            Ok(Json(SchedulerResponse::SchedulePod { node_name: node }))
        }
        Some(SchedulerControllerAction::ControllerJoin(_)) => {
            panic!("got controller join whilst serving")
        }
        None => Err(ErrorResponse::NoOperation),
    }
}

#[tracing::instrument(skip_all)]
async fn deployment(
    Json(payload): Json<DeploymentRequest>,
) -> Result<Json<DeploymentResponse>, ErrorResponse> {
    let s = DeploymentController;
    debug!("Got deployment controller request");
    println!("{}", serde_yaml::to_string(&payload).unwrap());
    let controller_id = 0;
    let state_view = StateView {
        revision: Revision::default(),
        deployments: vec![payload.deployment].into(),
        replicasets: payload.replicasets.into(),
        controllers: btreeset![controller_id],
        ..Default::default()
    };
    let mut local_state = DeploymentControllerState;
    let operation = s.step(controller_id, &state_view, &mut local_state);
    debug!(?operation, "Got operation");
    match operation {
        Some(DeploymentControllerAction::UpdateDeployment(dep)) => {
            Ok(Json(DeploymentResponse::UpdateDeployment {
                deployment: dep,
            }))
        }
        Some(DeploymentControllerAction::RequeueDeployment(dep)) => {
            Ok(Json(DeploymentResponse::RequeueDeployment {
                deployment: dep,
            }))
        }
        Some(DeploymentControllerAction::UpdateDeploymentStatus(dep)) => {
            Ok(Json(DeploymentResponse::UpdateDeploymentStatus {
                deployment: dep,
            }))
        }
        Some(DeploymentControllerAction::CreateReplicaSet(rs)) => {
            Ok(Json(DeploymentResponse::CreateReplicaSet {
                replicaset: rs,
            }))
        }
        Some(DeploymentControllerAction::UpdateReplicaSet(rs)) => {
            Ok(Json(DeploymentResponse::UpdateReplicaSet {
                replicaset: rs,
            }))
        }
        Some(DeploymentControllerAction::DeleteReplicaSet(rs)) => {
            Ok(Json(DeploymentResponse::DeleteReplicaSet {
                replicaset: rs,
            }))
        }
        Some(DeploymentControllerAction::UpdateReplicaSets(rss)) => {
            Ok(Json(DeploymentResponse::UpdateReplicaSets {
                replicasets: rss,
            }))
        }
        Some(DeploymentControllerAction::ControllerJoin(_)) => {
            panic!("got controller join whilst serving")
        }
        None => Err(ErrorResponse::NoOperation),
    }
}

#[tracing::instrument(skip_all)]
async fn replicaset(
    Json(payload): Json<ReplicasetRequest>,
) -> Result<Json<ReplicasetResponse>, ErrorResponse> {
    let s = ReplicaSetController;
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
        replicasets: replicasets.into(),
        pods: payload.pods.into(),
        controllers: btreeset![controller_id],
        ..Default::default()
    };
    let mut local_state = ReplicaSetControllerState;
    let operation = s.step(controller_id, &state_view, &mut local_state);
    debug!(?operation, "Got operation");
    match operation {
        Some(ReplicaSetControllerAction::UpdatePod(pod)) => {
            Ok(Json(ReplicasetResponse::UpdatePod { pod }))
        }
        Some(ReplicaSetControllerAction::UpdateReplicaSetStatus(rs)) => {
            Ok(Json(ReplicasetResponse::UpdateReplicaSetStatus {
                replicaset: rs,
            }))
        }
        Some(ReplicaSetControllerAction::CreatePod(pod)) => {
            Ok(Json(ReplicasetResponse::CreatePod { pod }))
        }
        Some(ReplicaSetControllerAction::DeletePod(pod)) => {
            Ok(Json(ReplicasetResponse::DeletePod { pod }))
        }
        Some(ReplicaSetControllerAction::ControllerJoin(_)) => {
            panic!("got controller join whilst serving")
        }
        None => Err(ErrorResponse::NoOperation),
    }
}

#[tracing::instrument(skip_all)]
async fn statefulset(
    Json(payload): Json<StatefulSetRequest>,
) -> Result<Json<StatefulSetResponse>, ErrorResponse> {
    let s = StatefulSetController;
    debug!("Got statefulset controller request");
    println!("{}", serde_yaml::to_string(&payload).unwrap());
    let controller_id = 0;
    let state_view = StateView {
        revision: Revision::default(),
        statefulsets: vec![payload.statefulset].into(),
        controller_revisions: payload.controller_revisions.into(),
        pods: payload.pods.into(),
        persistent_volume_claims: payload.persistent_volume_claims.into(),
        controllers: btreeset![controller_id],
        ..Default::default()
    };
    let mut local_state = StatefulSetControllerState;
    let operation = s.step(controller_id, &state_view, &mut local_state);
    debug!(?operation, "Got operation");
    match operation {
        Some(StatefulSetControllerAction::UpdateStatefulSetStatus(sts)) => {
            Ok(Json(StatefulSetResponse::UpdateStatefulSetStatus {
                statefulset: sts,
            }))
        }
        Some(StatefulSetControllerAction::UpdatePod(pod)) => {
            Ok(Json(StatefulSetResponse::UpdatePod { pod }))
        }
        Some(StatefulSetControllerAction::CreatePod(pod)) => {
            Ok(Json(StatefulSetResponse::CreatePod { pod }))
        }
        Some(StatefulSetControllerAction::DeletePod(pod)) => {
            Ok(Json(StatefulSetResponse::DeletePod { pod }))
        }
        Some(StatefulSetControllerAction::CreateControllerRevision(cr)) => {
            Ok(Json(StatefulSetResponse::CreateControllerRevision {
                controller_revision: cr,
            }))
        }
        Some(StatefulSetControllerAction::UpdateControllerRevision(cr)) => {
            Ok(Json(StatefulSetResponse::UpdateControllerRevision {
                controller_revision: cr,
            }))
        }
        Some(StatefulSetControllerAction::DeleteControllerRevision(cr)) => {
            Ok(Json(StatefulSetResponse::DeleteControllerRevision {
                controller_revision: cr,
            }))
        }
        Some(StatefulSetControllerAction::CreatePersistentVolumeClaim(pvc)) => {
            Ok(Json(StatefulSetResponse::CreatePersistentVolumeClaim {
                persistent_volume_claim: pvc,
            }))
        }
        Some(StatefulSetControllerAction::UpdatePersistentVolumeClaim(pvc)) => {
            Ok(Json(StatefulSetResponse::UpdatePersistentVolumeClaim {
                persistent_volume_claim: pvc,
            }))
        }
        Some(StatefulSetControllerAction::ControllerJoin(_)) => {
            panic!("got controller join whilst serving")
        }
        None => Err(ErrorResponse::NoOperation),
    }
}

#[tracing::instrument(skip_all)]
async fn job(Json(payload): Json<JobRequest>) -> Result<Json<JobResponse>, ErrorResponse> {
    let s = JobController;
    debug!("Got job controller request");
    println!("{}", serde_yaml::to_string(&payload).unwrap());
    let controller_id = 0;
    let state_view = StateView {
        revision: Revision::default(),
        jobs: vec![payload.job].into(),
        pods: payload.pods.into(),
        controllers: btreeset![controller_id],
        ..Default::default()
    };
    let mut local_state = JobControllerState;
    let operation = s.step(controller_id, &state_view, &mut local_state);
    debug!(?operation, "Got operation");
    match operation {
        Some(JobControllerAction::UpdateJobStatus(job)) => {
            Ok(Json(JobResponse::UpdateJobStatus { job }))
        }
        Some(JobControllerAction::CreatePod(pod)) => Ok(Json(JobResponse::CreatePod { pod })),
        Some(JobControllerAction::UpdatePod(pod)) => Ok(Json(JobResponse::UpdatePod { pod })),
        Some(JobControllerAction::DeletePod(pod)) => Ok(Json(JobResponse::DeletePod { pod })),
        Some(JobControllerAction::ControllerJoin(_)) => {
            panic!("got controller join whilst serving")
        }
        None => Err(ErrorResponse::NoOperation),
    }
}

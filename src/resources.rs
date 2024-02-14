use diff::Diff;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt::Display,
    iter::Sum,
    ops::{Add, AddAssign, Sub, SubAssign},
};

pub trait Meta {
    fn metadata(&self) -> &Metadata;
    fn metadata_mut(&mut self) -> &mut Metadata;
}

macro_rules! impl_meta {
    ($r:ident) => {
        impl Meta for $r {
            fn metadata(&self) -> &Metadata {
                &self.metadata
            }
            fn metadata_mut(&mut self) -> &mut Metadata {
                &mut self.metadata
            }
        }
    };
}

impl_meta!(Pod);
impl_meta!(Job);
impl_meta!(Deployment);
impl_meta!(ReplicaSet);
impl_meta!(StatefulSet);
impl_meta!(ControllerRevision);
impl_meta!(PersistentVolumeClaim);
impl_meta!(Node);

pub trait ObservedGeneration {
    fn observed_generation(&self) -> u64;
}

macro_rules! impl_observed_generation {
    ($r:ident) => {
        impl ObservedGeneration for $r {
            fn observed_generation(&self) -> u64 {
                self.status.observed_generation
            }
        }
    };
}

// impl_observed_generation!(Pod);
// impl_observed_generation!(Job);
impl_observed_generation!(Deployment);
impl_observed_generation!(ReplicaSet);
impl_observed_generation!(StatefulSet);
// impl_observed_generation!(ControllerRevision);
// impl_observed_generation!(PersistentVolumeClaim);
// impl_observed_generation!(Node);

/// Get the desired state of the resource, typically the `spec`.
pub trait Spec {
    type Spec: PartialEq;
    fn spec(&self) -> &Self::Spec;
}

macro_rules! impl_spec {
    ($r:ident, $spec:ident) => {
        impl Spec for $r {
            type Spec = $spec;
            fn spec(&self) -> &Self::Spec {
                &self.spec
            }
        }
    };
}

impl_spec!(Pod, PodSpec);
impl_spec!(Job, JobSpec);
impl_spec!(Deployment, DeploymentSpec);
impl_spec!(ReplicaSet, ReplicaSetSpec);
impl_spec!(StatefulSet, StatefulSetSpec);
impl_spec!(PersistentVolumeClaim, PersistentVolumeClaimSpec);
impl_spec!(Node, NodeSpec);

impl Spec for ControllerRevision {
    type Spec = ();
    fn spec(&self) -> &Self::Spec {
        &()
    }
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    // Name must be unique within a namespace. Is required when creating resources, although some
    // resources may allow a client to request the generation of an appropriate name automatically.
    // Name is primarily intended for creation idempotence and configuration definition. Cannot be
    // updated. More info:
    // https://kubernetes.io/docs/concepts/overview/working-with-objects/names#names
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,

    // GenerateName is an optional prefix, used by the server, to generate a unique name ONLY IF the Name field has not been provided. If this field is used, the name returned to the client will be different than the name passed. This value will also be combined with a unique suffix. The provided value has the same validation rules as the Name field, and may be truncated by the length of the suffix required to make the value unique on the server.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub generate_name: String,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub namespace: String,

    // CreationTimestamp is a timestamp representing the server time when this object was created.
    // It is not guaranteed to be set in happens-before order across separate operations. Clients
    // may not set this value. It is represented in RFC3339 form and is in UTC.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creation_timestamp: Option<Time>,

    // DeletionTimestamp is RFC 3339 date and time at which this resource will be deleted. This field is set by the server when a graceful deletion is requested by the user, and is not directly settable by a client. The resource is expected to be deleted (no longer visible from resource lists, and not reachable by name) after the time in this field, once the finalizers list is empty. As long as the finalizers list contains items, deletion is blocked. Once the deletionTimestamp is set, this value may not be unset or be set further into the future, although it may be shortened or the resource may be deleted prior to this time. For example, a user may request that a pod is deleted in 30 seconds. The Kubelet will react by sending a graceful termination signal to the containers in the pod. After that 30 seconds, the Kubelet will send a hard termination signal (SIGKILL) to the container and after cleanup, remove the pod from the API. In the presence of network partitions, this object may still exist after this timestamp, until an administrator or automated process can determine the resource is fully terminated. If not set, graceful deletion of the object has not been requested.

    // Populated by the system when a graceful deletion is requested. Read-only. More info: https://git.k8s.io/community/contributors/devel/sig-architecture/api-conventions.md#metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deletion_timestamp: Option<Time>,

    // Number of seconds allowed for this object to gracefully terminate before it will be removed from the system. Only set when deletionTimestamp is also set. May only be shortened. Read-only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deletion_grace_period_seconds: Option<u64>,

    // Map of string keys and values that can be used to organize and categorize (scope and select) objects. May match selectors of replication controllers and services. More info: https://kubernetes.io/docs/concepts/overview/working-with-objects/labels
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,

    // ManagedFields maps workflow-id and version to the set of fields that are managed by that workflow. This is mostly for internal housekeeping, and users typically shouldn't need to set or understand this field. A workflow can be the user's name, a controller's name, or the name of a specific apply path like "ci-cd". The set of fields is always in the version that the workflow used when modifying the object.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub managed_fields: Vec<ManagedFieldsEntry>,

    // List of objects depended by this object. If ALL objects in the list have been deleted, this object will be garbage collected. If this object is managed by a controller, then an entry in this list will point to this controller, with the controller field set to true. There cannot be more than one managing controller.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub owner_references: Vec<OwnerReference>,

    // UID is the unique in time and space value for this object. It is typically generated by the server on successful creation of a resource and is not allowed to change on PUT operations.
    //
    // Populated by the system. Read-only. More info: https://kubernetes.io/docs/concepts/overview/working-with-objects/names#uids
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub uid: String,

    // Annotations is an unstructured key value map stored with a resource that may be set by external tools to store and retrieve arbitrary metadata. They are not queryable and should be preserved when modifying objects. More info: https://kubernetes.io/docs/concepts/overview/working-with-objects/annotations
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub annotations: BTreeMap<String, String>,

    // A sequence number representing a specific generation of the desired state (spec). Populated by the system. Read-only.
    #[serde(default, skip_serializing_if = "u64_is_zero")]
    pub generation: u64,

    // An opaque value that represents the internal version of this object that can be used by clients to determine when objects have changed. May be used for optimistic concurrency, change detection, and the watch operation on a resource or set of resources. Clients must treat these values as opaque and passed unmodified back to the server. They may only be valid for a particular resource or set of resources.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub resource_version: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub finalizers: Vec<String>,
}

fn u64_is_zero(val: &u64) -> bool {
    *val == 0
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct ManagedFieldsEntry {
    #[serde(default)]
    pub api_version: String,

    #[serde(default)]
    pub fields_type: String,

    pub fields_v1: Option<FieldsV1>,

    #[serde(default)]
    pub manager: String,

    #[serde(default)]
    pub operation: String,

    #[serde(default)]
    pub subresource: String,

    pub time: Option<Time>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase", untagged)]
pub enum FieldsV1 {
    Map(BTreeMap<String, FieldsV1>),
    List(Vec<FieldsV1>),
    Str(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct OwnerReference {
    pub api_version: String,

    pub kind: String,

    pub name: String,

    pub uid: String,

    #[serde(default)]
    pub block_owner_deletion: bool,

    #[serde(default)]
    pub controller: bool,
}

#[derive(
    Default, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub struct Pod {
    pub metadata: Metadata,
    pub spec: PodSpec,
    pub status: PodStatus,
}

impl Pod {
    pub const GVK: GroupVersionKind = GroupVersionKind {
        group: "",
        version: "v1",
        kind: "Pod",
    };
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct PodSpec {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduler_name: Option<String>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub containers: Vec<Container>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub init_containers: Vec<Container>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub termination_grace_period_seconds: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart_policy: Option<PodRestartPolicy>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_deadline_seconds: Option<u64>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volumes: Vec<Volume>,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hostname: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub subdomain: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tolerations: Vec<Toleration>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub node_selector: BTreeMap<String, String>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub enum PodRestartPolicy {
    Never,
    OnFailure,
    Always,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct Toleration {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub operator: Option<Operator>,
    #[serde(default)]
    pub value: Option<String>,
    pub effect: Option<TaintEffect>,
    #[serde(default)]
    pub toleration_seconds: Option<u64>,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub enum Operator {
    #[default]
    Equal,
    Exists,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub enum TaintEffect {
    NoSchedule,
    PreferNoSchedule,
    NoExecute,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    pub name: String,
    pub persistent_volume_claim: Option<PersistentVolumeClaimVolumeSource>,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct PersistentVolumeClaimVolumeSource {
    pub claim_name: String,
    #[serde(default)]
    pub read_only: bool,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub struct Container {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub image: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub resources: ResourceRequirements,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<EnvVar>,
}

fn is_default<D: Default + PartialEq>(val: &D) -> bool {
    val == &D::default()
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct EnvVar {
    pub name: String,
    pub value: Option<String>,
    pub value_from: Option<EnvVarSource>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct EnvVarSource {
    // pub config_map_key_ref: Option<ConfigMapKeySelector>,
    pub field_ref: Option<ObjectFieldSelector>,
    // pub resource_field_ref: Option<ResourceFieldSelector>,
    // pub secret_key_ref: Option<SecretKeySelector>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct ObjectFieldSelector {
    pub field_path: String,
    pub api_version: Option<String>,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct PodStatus {
    // The phase of a Pod is a simple, high-level summary of where the Pod is in its lifecycle. The conditions array, the reason and message fields, and the individual container status arrays contain more detail about the pod's status. There are five possible phase values.
    #[serde(default)]
    pub phase: PodPhase,

    #[serde(default)]
    pub conditions: Vec<PodCondition>,

    // Status for any ephemeral containers that have run in this pod.
    #[serde(default)]
    pub container_statuses: Vec<ContainerStatus>,

    #[serde(default)]
    pub init_container_statuses: Vec<ContainerStatus>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct PodCondition {
    // Status of the condition, one of True, False, Unknown.
    pub status: ConditionStatus,
    // Type of deployment condition.
    pub r#type: PodConditionType,
    // Last time we probed the condition.
    pub last_probe_time: Option<Time>,
    // Last time the condition transitioned from one status to another.
    pub last_transition_time: Option<Time>,
    // A human readable message indicating details about the transition.
    pub message: Option<String>,
    // The reason for the condition's last transition.
    pub reason: Option<String>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub enum PodConditionType {
    DisruptionTarget,
    PodScheduled,
    PodReadyToStartContainers,
    ContainersReady,
    Initialized,
    Ready,
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub enum PodPhase {
    // Pending: The pod has been accepted by the Kubernetes system, but one or more of the container images has not been created. This includes time before being scheduled as well as time spent downloading images over the network, which could take a while.
    #[default]
    Pending,
    // Unknown: For some reason the state of the pod could not be obtained, typically due to an error in communicating with the host of the pod.
    Unknown,
    // Running: The pod has been bound to a node, and all of the containers have been created. At least one container is still running, or is in the process of starting or restarting.
    Running,
    // Succeeded: All containers in the pod have terminated in success, and will not be restarted.
    Succeeded,
    // Failed: All containers in the pod have terminated, and at least one container has terminated in failure. The container either exited with non-zero status or was terminated by the system.
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct ContainerStatus {
    pub name: String,
    #[serde(default)]
    pub state: ContainerState,
    #[serde(default)]
    pub last_termination_state: ContainerState,
    pub ready: bool,
    pub restart_count: u32,
    pub image: String,
    #[serde(rename = "imageID")]
    pub image_id: String,
    #[serde(default)]
    pub container_id: String,
    #[serde(default)]
    pub started: bool,
    #[serde(default)]
    pub allocated_resources: BTreeMap<String, Quantity>,
    #[serde(default)]
    pub resources: ResourceRequirements,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct ContainerState {
    pub waiting: Option<ContainerStateWaiting>,
    pub running: Option<ContainerStateRunning>,
    pub terminated: Option<ContainerStateTerminated>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct ContainerStateWaiting {
    #[serde(default)]
    reason: String,
    #[serde(default)]
    message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct ContainerStateRunning {
    started_at: Option<Time>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct ContainerStateTerminated {
    pub exit_code: u32,
    #[serde(default)]
    pub signal: u32,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub message: String,
    pub started_at: Option<Time>,
    pub finished_at: Option<Time>,
    #[serde(default)]
    pub container_id: String,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub struct ResourceRequirements {
    /// What a pod/container is guaranteed to have (minimums).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requests: Option<ResourceQuantities>,
    /// What a pod/container cannot use more than (maximums).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limits: Option<ResourceQuantities>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claims: Vec<ResourceClaim>,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub struct ResourceQuantities {
    // catch other resource types that we haven't included here yet
    #[serde(flatten)]
    pub others: BTreeMap<String, Quantity>,
}

impl Add<ResourceQuantities> for ResourceQuantities {
    type Output = ResourceQuantities;

    fn add(self, rhs: ResourceQuantities) -> Self::Output {
        let mut others = self.others;
        for (res, q) in rhs.others {
            *others.entry(res).or_default() += q;
        }
        Self { others }
    }
}

impl<'a> Sum<&'a ResourceQuantities> for ResourceQuantities {
    fn sum<I: Iterator<Item = &'a Self>>(iter: I) -> Self {
        iter.fold(ResourceQuantities::default(), |acc, v| acc + v.clone())
    }
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub struct ResourceClaim {
    pub name: String,
}

impl Sub for ResourceQuantities {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        let mut others = self.others;
        for (res, q) in rhs.others {
            *others.entry(res).or_default() -= q;
        }
        Self {
            others: BTreeMap::new(),
        }
    }
}

impl SubAssign for ResourceQuantities {
    fn sub_assign(&mut self, rhs: Self) {
        *self = self.clone() - rhs;
    }
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub metadata: Metadata,
    pub spec: JobSpec,
    pub status: JobStatus,
}

impl Job {
    pub const GVK: GroupVersionKind = GroupVersionKind {
        group: "batch",
        version: "v1",
        kind: "Job",
    };
}

fn u32_one() -> u32 {
    1
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct JobSpec {
    pub template: PodTemplateSpec,
    #[serde(default = "u32_one")]
    pub parallelism: u32,
    pub completions: Option<u32>,
    #[serde(default)]
    pub completion_mode: JobCompletionMode,
    pub backoff_limit: Option<u32>,
    pub active_deadline_seconds: Option<u64>,
    pub ttl_seconds_after_finished: Option<u64>,
    #[serde(default)]
    pub suspend: bool,
    pub selector: LabelSelector,

    pub pod_failure_policy: Option<JobPodFailurePolicy>,
}

impl Default for JobSpec {
    fn default() -> Self {
        Self {
            template: Default::default(),
            parallelism: 1,
            completions: Default::default(),
            completion_mode: Default::default(),
            backoff_limit: Default::default(),
            active_deadline_seconds: Default::default(),
            ttl_seconds_after_finished: Default::default(),
            suspend: Default::default(),
            selector: Default::default(),
            pod_failure_policy: Default::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct JobPodFailurePolicy {
    pub rules: Vec<JobPodFailurePolicyRule>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct JobPodFailurePolicyRule {
    pub action: JobPodFailurePolicyRuleAction,
    pub on_pod_conditions: Option<Vec<JobPodFailurePolicyRuleOnPodConditionsPattern>>,
    pub on_exit_codes: Option<JobPodFailurePolicyRuleOnExitCodesRequirement>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub enum JobPodFailurePolicyRuleAction {
    Ignore,
    FailIndex,
    Count,
    FailJob,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct JobPodFailurePolicyRuleOnPodConditionsPattern {
    pub status: ConditionStatus,
    pub r#type: PodConditionType,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct JobPodFailurePolicyRuleOnExitCodesRequirement {
    pub operator: JobPodFailurePolicyRuleOnExitCodesRequirementOperator,
    pub values: Vec<u32>,
    pub container_name: Option<String>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub enum JobPodFailurePolicyRuleOnExitCodesRequirementOperator {
    In,
    NotIn,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub enum JobCompletionMode {
    #[default]
    NonIndexed,
    Indexed,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct JobStatus {
    pub start_time: Option<Time>,
    pub completion_time: Option<Time>,
    #[serde(default)]
    pub active: u32,
    #[serde(default)]
    pub failed: u32,
    #[serde(default)]
    pub succeeded: u32,
    #[serde(default)]
    pub completed_indexes: String,
    #[serde(default)]
    pub conditions: Vec<JobCondition>,
    #[serde(default)]
    pub uncounted_terminated_pods: UncountedTerminatedPods,
    // The number of pods which have a Ready condition.
    #[serde(default)]
    pub ready: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct JobCondition {
    pub status: ConditionStatus,
    pub r#type: JobConditionType,
    pub last_probe_time: Option<Time>,
    pub last_transition_time: Option<Time>,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub reason: String,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub enum JobConditionType {
    Suspended,
    Complete,
    Failed,
    FailureTarget,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct UncountedTerminatedPods {
    #[serde(default)]
    pub failed: Vec<String>,
    #[serde(default)]
    pub succeeded: Vec<String>,
}

#[derive(
    Default, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub struct ReplicaSet {
    pub metadata: Metadata,
    pub spec: ReplicaSetSpec,
    pub status: ReplicaSetStatus,
}

impl ReplicaSet {
    pub const GVK: GroupVersionKind = GroupVersionKind {
        group: "apps",
        version: "v1",
        kind: "ReplicaSet",
    };

    pub fn pods(&self) -> Vec<String> {
        (0..self.status.replicas)
            .map(|i| format!("{}-{}", self.metadata.name, i))
            .collect()
    }
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct ReplicaSetSpec {
    // Label selector for pods. Existing ReplicaSets whose pods are selected by this will be the ones affected by this deployment. It must match the pod template's labels.
    pub selector: LabelSelector,
    pub template: PodTemplateSpec,
    pub replicas: Option<u32>,
    // Minimum number of seconds for which a newly created pod should be ready without any of its container crashing, for it to be considered available. Defaults to 0 (pod will be considered available as soon as it is ready)
    #[serde(default)]
    pub min_ready_seconds: u32,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub struct PodTemplateSpec {
    pub metadata: Metadata,
    pub spec: PodSpec,
}

#[derive(
    Clone, Default, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct ReplicaSetStatus {
    pub replicas: u32,

    // The number of available replicas (ready for at least minReadySeconds) for this replica set.
    #[serde(default)]
    pub available_replicas: u32,

    // readyReplicas is the number of pods targeted by this ReplicaSet with a Ready Condition.
    #[serde(default)]
    pub ready_replicas: u32,

    // The number of pods that have labels matching the labels of the pod template of the replicaset.
    #[serde(default)]
    pub fully_labeled_replicas: u32,

    // ObservedGeneration reflects the generation of the most recently observed ReplicaSet.
    #[serde(default)]
    pub observed_generation: u64,

    #[serde(default)]
    pub conditions: Vec<ReplicaSetCondition>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct ReplicaSetCondition {
    // Status of the condition, one of True, False, Unknown.
    pub status: ConditionStatus,
    // Type of deployment condition.
    pub r#type: ReplicaSetConditionType,
    // Last time the condition transitioned from one status to another.
    pub last_transition_time: Option<Time>,
    // A human readable message indicating details about the transition.
    pub message: Option<String>,
    // The reason for the condition's last transition.
    pub reason: Option<String>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub enum ReplicaSetConditionType {
    // ReplicaSetReplicaFailure is added in a replica set when one of its pods fails to be created
    // due to insufficient quota, limit ranges, pod security policy, node selectors, etc. or deleted
    // due to kubelet being down or finalizers are failing.
    ReplicaFailure,
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub enum ConditionStatus {
    True,
    False,
    #[default]
    Unknown,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub struct Deployment {
    pub metadata: Metadata,
    pub spec: DeploymentSpec,
    pub status: DeploymentStatus,
}

impl Deployment {
    pub const GVK: GroupVersionKind = GroupVersionKind {
        group: "apps",
        version: "v1",
        kind: "Deployment",
    };

    pub fn replicasets(&self) -> Vec<String> {
        vec![self.metadata.name.clone()]
    }
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct DeploymentSpec {
    #[serde(default)]
    pub replicas: u32,

    // Label selector for pods. Existing ReplicaSets whose pods are selected by this will be the ones affected by this deployment. It must match the pod template's labels.
    pub selector: LabelSelector,

    pub template: PodTemplateSpec,

    // The maximum time in seconds for a deployment to make progress before it is considered to be failed. The deployment controller will continue to process failed deployments and a condition with a ProgressDeadlineExceeded reason will be surfaced in the deployment status. Note that progress will not be estimated during the time a deployment is paused. Defaults to 600s.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_deadline_seconds: Option<u32>,

    // Minimum number of seconds for which a newly created pod should be ready without any of its container crashing, for it to be considered available. Defaults to 0 (pod will be considered available as soon as it is ready)
    #[serde(default)]
    pub min_ready_seconds: u32,

    // The number of old ReplicaSets to retain to allow rollback. This is a pointer to distinguish between explicit zero and not specified. Defaults to 10.
    #[serde(default = "default_revision_history_limit")]
    pub revision_history_limit: u32,

    #[serde(default, skip_serializing_if = "bool_is_false")]
    pub paused: bool,

    // The deployment strategy to use to replace existing pods with new ones.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy: Option<DeploymentStrategy>,
}

fn default_revision_history_limit() -> u32 {
    10
}

fn bool_is_false(val: &bool) -> bool {
    !val
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct DeploymentStrategy {
    #[serde(default)]
    pub r#type: DeploymentStrategyType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rolling_update: Option<RollingUpdate>,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct RollingUpdate {
    // The maximum number of pods that can be scheduled above the desired number of pods. Value can be an absolute number (ex: 5) or a percentage of desired pods (ex: 10%). This can not be 0 if MaxUnavailable is 0. Absolute number is calculated from percentage by rounding up. Defaults to 25%. Example: when this is set to 30%, the new ReplicaSet can be scaled up immediately when the rolling update starts, such that the total number of old and new pods do not exceed 130% of desired pods. Once old pods have been killed, new ReplicaSet can be scaled up further, ensuring that total number of pods running at any time during the update is at most 130% of desired pods.
    pub max_surge: Option<IntOrString>,
    // The maximum number of pods that can be unavailable during the update. Value can be an absolute number (ex: 5) or a percentage of desired pods (ex: 10%). Absolute number is calculated from percentage by rounding down. This can not be 0 if MaxSurge is 0. Defaults to 25%. Example: when this is set to 30%, the old ReplicaSet can be scaled down to 70% of desired pods immediately when the rolling update starts. Once new pods are ready, old ReplicaSet can be scaled down further, followed by scaling up the new ReplicaSet, ensuring that the total number of pods available at all times during the update is at least 70% of desired pods.
    pub max_unavailable: Option<IntOrString>,
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub enum DeploymentStrategyType {
    #[default]
    RollingUpdate,
    Recreate,
}

#[derive(
    Clone, Default, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct DeploymentStatus {
    // Total number of non-terminated pods targeted by this deployment (their labels match the selector).
    #[serde(default)]
    pub replicas: u32,
    // Total number of available pods (ready for at least minReadySeconds) targeted by this deployment.
    #[serde(default)]
    pub updated_replicas: u32,
    // readyReplicas is the number of pods targeted by this Deployment with a Ready Condition.
    #[serde(default)]
    pub ready_replicas: u32,
    // Total number of unavailable pods targeted by this deployment. This is the total number of pods that are still required for the deployment to have 100% available capacity. They may either be pods that are running but not yet available or pods that still have not been created.
    #[serde(default)]
    pub unavailable_replicas: u32,
    // Total number of available pods (ready for at least minReadySeconds) targeted by this deployment.
    #[serde(default)]
    pub available_replicas: u32,
    // Count of hash collisions for the Deployment. The Deployment controller uses this field as a collision avoidance mechanism when it needs to create the name for the newest ReplicaSet.
    #[serde(default)]
    pub collision_count: u32,
    // Represents the latest available observations of a deployment's current state.
    #[serde(default)]
    pub conditions: Vec<DeploymentCondition>,
    // The generation observed by the deployment controller.
    #[serde(default)]
    pub observed_generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct DeploymentCondition {
    // Status of the condition, one of True, False, Unknown.
    pub status: ConditionStatus,
    // Type of deployment condition.
    pub r#type: DeploymentConditionType,
    // Last time the condition transitioned from one status to another.
    pub last_transition_time: Option<Time>,
    // The last time this condition was updated.
    pub last_update_time: Option<Time>,
    // A human readable message indicating details about the transition.
    pub message: Option<String>,
    // The reason for the condition's last transition.
    pub reason: Option<String>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub enum DeploymentConditionType {
    // Progressing means the deployment is progressing. Progress for a deployment is
    // considered when a new replica set is created or adopted, and when new pods scale
    // up or old pods scale down. Progress is not estimated for paused deployments or
    // when progressDeadlineSeconds is not specified.
    Progressing,
    // Available means the deployment is available, ie. at least the minimum available
    // replicas required are up and running for at least minReadySeconds.
    Available,
    // ReplicaFailure is added in a deployment when one of its pods fails to be created
    // or deleted.
    ReplicaFailure,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct LabelSelector {
    // matchLabels is a map of {key,value} pairs. A single {key,value} in the matchLabels map is equivalent to an element of matchExpressions, whose key field is "key", the operator is "In", and the values array contains only "value". The requirements are ANDed.
    #[serde(default)]
    pub match_labels: BTreeMap<String, String>,
}

impl LabelSelector {
    pub fn matches(&self, labels: &BTreeMap<String, String>) -> bool {
        self.match_labels
            .iter()
            .all(|(k, v)| labels.get(k).map_or(false, |lv| v == lv))
    }
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct ControllerRevision {
    pub metadata: Metadata,
    pub revision: u64,
    pub data: String,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct StatefulSet {
    pub metadata: Metadata,
    pub spec: StatefulSetSpec,
    pub status: StatefulSetStatus,
}

impl StatefulSet {
    pub const GVK: GroupVersionKind = GroupVersionKind {
        group: "apps",
        version: "v1",
        kind: "StatefulSet",
    };
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct StatefulSetSpec {
    pub service_name: String,
    pub selector: LabelSelector,
    pub template: PodTemplateSpec,

    pub replicas: Option<u32>,
    #[serde(default)]
    pub update_strategy: StatefulSetUpdateStrategy,
    #[serde(default)]
    pub pod_management_policy: PodManagementPolicyType,
    pub revision_history_limit: Option<u32>,
    pub volume_claim_templates: Vec<PersistentVolumeClaim>,
    pub min_ready_seconds: Option<u32>,
    #[serde(default)]
    pub persistent_volume_claim_retention_policy: StatefulSetPersistentVolumeClaimRetentionPolicy,
    pub ordinals: Option<StatefulSetOrdinals>,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub enum PodManagementPolicyType {
    #[default]
    OrderedReady,
    Parallel,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct StatefulSetStatus {
    pub replicas: u32,
    #[serde(default)]
    pub ready_replicas: u32,
    #[serde(default)]
    pub current_replicas: u32,
    #[serde(default)]
    pub updated_replicas: u32,
    #[serde(default)]
    pub available_replicas: u32,
    #[serde(default)]
    pub collision_count: u32,
    #[serde(default)]
    pub conditions: Vec<StatefulSetCondition>,
    #[serde(default)]
    pub current_revision: String,
    #[serde(default)]
    pub update_revision: String,
    #[serde(default)]
    pub observed_generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct StatefulSetCondition {
    // Status of the condition, one of True, False, Unknown.
    pub status: ConditionStatus,
    // Type of deployment condition.
    pub r#type: StatefulSetConditionType,
    // Last time the condition transitioned from one status to another.
    pub last_transition_time: Option<Time>,
    // A human readable message indicating details about the transition.
    pub message: Option<String>,
    // The reason for the condition's last transition.
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub enum StatefulSetConditionType {
    Unknown,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct StatefulSetPersistentVolumeClaimRetentionPolicy {
    #[serde(default)]
    pub when_deleted: StatefulSetPersistentVolumeClaimRetentionPolicyType,
    #[serde(default)]
    pub when_scaled: StatefulSetPersistentVolumeClaimRetentionPolicyType,
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub enum StatefulSetPersistentVolumeClaimRetentionPolicyType {
    #[default]
    Retain,
    Delete,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct StatefulSetOrdinals {
    pub start: u32,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct StatefulSetUpdateStrategy {
    #[serde(default)]
    pub r#type: String,
    #[serde(default)]
    pub rolling_update: Option<RollingUpdateStatefulSetStrategy>,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct RollingUpdateStatefulSetStrategy {
    pub max_unavailable: Option<IntOrString>,
    pub partition: u32,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct PersistentVolumeClaim {
    pub metadata: Metadata,
    pub spec: PersistentVolumeClaimSpec,
    pub status: PersistentVolumeClaimStatus,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct PersistentVolumeClaimSpec {
    #[serde(default)]
    pub selector: LabelSelector,
    #[serde(default)]
    pub access_modes: Vec<String>,

    #[serde(default)]
    pub resources: ResourceRequirements,

    pub volume_name: Option<String>,
    pub storage_class_name: Option<String>,
    pub volume_mode: Option<String>,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct PersistentVolumeClaimStatus {
    #[serde(default)]
    pub access_modes: Vec<String>,
}

#[derive(
    Clone, Default, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub struct Node {
    pub metadata: Metadata,
    pub spec: NodeSpec,
    pub status: NodeStatus,
}

#[derive(
    Clone, Default, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct NodeSpec {
    #[serde(default)]
    pub taints: Vec<Taint>,
    #[serde(default)]
    pub unschedulable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct Taint {
    pub effect: TaintEffect,
    pub key: String,
    pub time_added: Option<Time>,
    #[serde(default)]
    pub value: String,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub struct NodeStatus {
    /// The total resources of the node.
    #[serde(default)]
    pub capacity: ResourceQuantities,

    /// The total resources of the node.
    pub allocatable: Option<ResourceQuantities>,

    #[serde(default)]
    pub conditions: Vec<NodeCondition>,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(rename_all = "camelCase")]
pub struct NodeCondition {
    pub r#type: NodeConditionType,
    pub status: ConditionStatus,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub message: String,
    pub last_heartbeat_time: Option<Time>,
    pub last_transition_time: Option<Time>,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff,
)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub enum NodeConditionType {
    #[default]
    Ready,
    DiskPressure,
    MemoryPressure,
    PIDPressure,
    NetworkUnavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(untagged)]
pub enum Quantity {
    Str(String),
    Num(u64),
}

impl Default for Quantity {
    fn default() -> Self {
        Self::Num(0)
    }
}

fn split_quantity(s: &str) -> (String, String) {
    if let Some(alpha_pos) = s.chars().position(char::is_alphabetic) {
        (
            s.chars().take(alpha_pos).collect(),
            s.chars().skip(alpha_pos).collect(),
        )
    } else {
        (s.to_owned(), String::new())
    }
}

impl Quantity {
    pub fn to_num(&self) -> u64 {
        match self {
            Quantity::Str(s) => {
                let (digit, unit) = split_quantity(s);
                let num: u64 = digit.parse().unwrap();
                match unit.as_str() {
                    "" => num,
                    "m" => num / 1000,
                    "k" => num * 1000,
                    u => panic!("unhandled unit {u:?} when splitting {s:?}"),
                }
            }
            Quantity::Num(i) => *i,
        }
    }
}

impl Display for Quantity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Quantity::Str(s) => s.clone(),
            Quantity::Num(n) => n.to_string(),
        };
        f.write_str(&s)
    }
}

impl From<u32> for Quantity {
    fn from(value: u32) -> Self {
        Quantity::Num(value.into())
    }
}

impl From<u64> for Quantity {
    fn from(value: u64) -> Self {
        Quantity::Num(value)
    }
}

impl Add<Quantity> for Quantity {
    type Output = Quantity;
    fn add(self, rhs: Quantity) -> Self::Output {
        (self.to_num() + rhs.to_num()).into()
    }
}

impl AddAssign<Quantity> for Quantity {
    fn add_assign(&mut self, rhs: Quantity) {
        *self = self.clone() + rhs;
    }
}

impl Sub<Quantity> for Quantity {
    type Output = Quantity;
    fn sub(self, rhs: Quantity) -> Self::Output {
        (self.to_num() - rhs.to_num()).into()
    }
}

impl SubAssign<Quantity> for Quantity {
    fn sub_assign(&mut self, rhs: Quantity) {
        *self = self.clone() - rhs;
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
#[serde(untagged)]
pub enum IntOrString {
    Int(u32),
    Str(String),
}

impl IntOrString {
    pub fn scaled_value(&self, total: u32, round_up: bool) -> u32 {
        match self {
            IntOrString::Int(i) => *i,
            IntOrString::Str(s) => {
                if let Some(s) = s.strip_suffix('%') {
                    let v = s.parse::<u32>().unwrap();
                    if round_up {
                        (v as f64 * total as f64 / 100.).ceil() as u32
                    } else {
                        (v as f64 * total as f64 / 100.).floor() as u32
                    }
                } else {
                    panic!("not a percentage")
                }
            }
        }
    }
}

impl From<u32> for IntOrString {
    fn from(value: u32) -> Self {
        IntOrString::Int(value)
    }
}
impl From<String> for IntOrString {
    fn from(value: String) -> Self {
        IntOrString::Str(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Time(#[serde(with = "time::serde::rfc3339")] pub time::OffsetDateTime);

impl Diff for Time {
    type Repr = Option<time::OffsetDateTime>;
    fn diff(&self, other: &Self) -> Self::Repr {
        if self != other {
            Some(other.0)
        } else {
            None
        }
    }
    fn apply(&mut self, diff: &Self::Repr) {
        if let Some(diff) = diff {
            *self = Time(*diff)
        }
    }
    fn identity() -> Self {
        Time(time::OffsetDateTime::UNIX_EPOCH)
    }
}

pub struct GroupVersionKind {
    pub group: &'static str,
    pub version: &'static str,
    pub kind: &'static str,
}

impl GroupVersionKind {
    pub fn group_version(&self) -> GroupVersion {
        GroupVersion {
            group: self.group,
            version: self.version,
        }
    }

    pub fn api_version(&self) -> String {
        match (self.group, self.version) {
            ("", "") => "".to_owned(),
            ("", version) => version.to_owned(),
            (group, "") => group.to_owned(),
            (group, version) => {
                format!("{}/{}", group, version)
            }
        }
    }
}

impl Display for GroupVersionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}, Kind={}", self.group, self.version, self.kind)
    }
}

pub struct GroupVersion {
    pub group: &'static str,
    pub version: &'static str,
}

impl Display for GroupVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.group.is_empty() {
            write!(f, "{}", self.version)
        } else {
            write!(f, "{}/{}", self.group, self.version)
        }
    }
}

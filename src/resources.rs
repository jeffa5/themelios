use serde::{Deserialize, Serialize};
use std::ops::{Sub, SubAssign};

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Metadata {
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PodResource {
    pub metadata: Metadata,
    pub spec: PodSpec,
    pub status: PodStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PodSpec {
    pub node_name: Option<String>,
    pub scheduler_name: Option<String>,
    /// The resources that the pod will use
    /// This is a simplification, really this should be per container in the pod, but that doesn't
    /// impact things really.
    pub resources: Option<ResourceRequirements>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PodStatus {}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ResourceRequirements {
    /// What a pod/container is guaranteed to have (minimums).
    pub requests: Option<ResourceQuantities>,
    /// What a pod/container cannot use more than (maximums).
    pub limits: Option<ResourceQuantities>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ResourceQuantities {
    /// Number of cpu cores.
    pub cpu_cores: Option<Quantity>,
    /// Amount of memory (in megabytes).
    pub memory_mb: Option<Quantity>,
    /// Number of pods.
    pub pods: Option<Quantity>,
}

impl Sub for ResourceQuantities {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            cpu_cores: Some(
                self.cpu_cores
                    .unwrap_or_default()
                    .to_int()
                    .saturating_sub(rhs.cpu_cores.unwrap_or_default().to_int())
                    .into(),
            ),
            memory_mb: Some(
                self.memory_mb
                    .unwrap_or_default()
                    .to_int()
                    .saturating_sub(rhs.memory_mb.unwrap_or_default().to_int())
                    .into(),
            ),
            pods: Some(
                self.pods
                    .unwrap_or_default()
                    .to_int()
                    .saturating_sub(rhs.pods.unwrap_or_default().to_int())
                    .into(),
            ),
        }
    }
}

impl SubAssign for ResourceQuantities {
    fn sub_assign(&mut self, rhs: Self) {
        *self = self.clone() - rhs;
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReplicaSetResource {
    pub metadata: Metadata,
    pub replicas: u32,
}

impl ReplicaSetResource {
    pub fn pods(&self) -> Vec<String> {
        (0..self.replicas)
            .map(|i| format!("{}-{}", self.metadata.name, i))
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeploymentResource {
    pub metadata: Metadata,
    pub replicas: u32,
}

impl DeploymentResource {
    pub fn replicasets(&self) -> Vec<String> {
        vec![self.metadata.name.clone()]
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StatefulSetResource {
    pub metadata: Metadata,
    pub replicas: u32,
}

impl StatefulSetResource {
    pub fn pods(&self) -> Vec<String> {
        (0..self.replicas)
            .map(|i| format!("{}-{}", self.metadata.name, i))
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NodeResource {
    pub metadata: Metadata,
    pub spec: NodeSpec,
    pub status: NodeStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NodeSpec {}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NodeStatus {
    /// The total resources of the node.
    #[serde(default)]
    pub capacity: ResourceQuantities,

    /// The total resources of the node.
    #[serde(default)]
    pub allocatable: ResourceQuantities,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Quantity {
    Str(String),
    Int(u32),
}

impl Default for Quantity {
    fn default() -> Self {
        Self::Int(0)
    }
}

impl Quantity {
    pub fn to_int(&self) -> u32 {
        match self {
            Quantity::Str(s) => {
                let (digit, unit) = s.split_once(char::is_alphabetic).unwrap();
                let digit = digit.parse().unwrap();
                match unit {
                    u => panic!("unhandled unit {u}"),
                };
                digit
            }
            Quantity::Int(i) => *i,
        }
    }
}

impl From<u32> for Quantity {
    fn from(value: u32) -> Self {
        Quantity::Int(value)
    }
}

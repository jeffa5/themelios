use std::{
    collections::BTreeSet,
    ops::{Sub, SubAssign},
};

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PodResource {
    pub name: String,
    pub node_name: Option<String>,
    /// The resources that the pod will use
    /// This is a simplification, really this should be per container in the pod, but that doesn't
    /// impact things really.
    pub resources: Option<ResourceRequirements>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ResourceRequirements {
    /// What a pod/container is guaranteed to have (minimums).
    pub requests: Option<ResourceQuantities>,
    /// What a pod/container cannot use more than (maximums).
    pub limits: Option<ResourceQuantities>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ResourceQuantities {
    /// Number of cpu cores.
    pub cpu_cores: Option<u32>,
    /// Amount of memory (in megabytes).
    pub memory_mb: Option<u32>,
}

impl Sub for ResourceQuantities {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            cpu_cores: Some(
                self.cpu_cores
                    .unwrap_or_default()
                    .saturating_sub(rhs.cpu_cores.unwrap_or_default()),
            ),
            memory_mb: Some(
                self.memory_mb
                    .unwrap_or_default()
                    .saturating_sub(rhs.memory_mb.unwrap_or_default()),
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
    pub id: String,
    pub replicas: u32,
}

impl ReplicaSetResource {
    pub fn pods(&self) -> Vec<String> {
        (0..self.replicas)
            .map(|i| format!("{}-{}", self.id, i))
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeploymentResource {
    pub id: String,
    pub replicas: u32,
}

impl DeploymentResource {
    pub fn replicasets(&self) -> Vec<String> {
        vec![self.id.clone()]
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StatefulSetResource {
    pub id: String,
    pub replicas: u32,
}

impl StatefulSetResource {
    pub fn pods(&self) -> Vec<String> {
        (0..self.replicas)
            .map(|i| format!("{}-{}", self.id, i))
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeResource {
    pub name: String,
    pub running: BTreeSet<String>,
    pub ready: bool,
    /// The total resources of the node.
    pub capacity: ResourceQuantities,
}

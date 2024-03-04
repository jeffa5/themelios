use stateright::{Expectation, Property};

use crate::{
    abstract_model::AbstractModel,
    controller::{
        job::JobController, podgc::PodGCController, Controllers, DeploymentController,
        NodeController, ReplicaSetController, SchedulerController, StatefulSetController,
    },
    state::State,
};

pub mod deployment;
pub mod job;
pub mod node;
pub mod podgc;
pub mod replicaset;
pub mod scheduler;
pub mod statefulset;

pub trait ControllerProperties {
    fn properties() -> Properties;
}

impl ControllerProperties for Controllers {
    fn properties() -> Properties {
        let mut properties = Properties::default();
        properties.append(&mut NodeController::properties());
        properties.append(&mut SchedulerController::properties());
        properties.append(&mut ReplicaSetController::properties());
        properties.append(&mut DeploymentController::properties());
        properties.append(&mut StatefulSetController::properties());
        properties.append(&mut JobController::properties());
        properties.append(&mut PodGCController::properties());
        properties
    }
}

#[derive(Default)]
pub struct Properties(Vec<Property<AbstractModel>>);

impl Properties {
    pub fn add(
        &mut self,
        expectation: Expectation,
        name: &'static str,
        condition: fn(&AbstractModel, &State) -> bool,
    ) {
        self.0.push(Property {
            expectation,
            name,
            condition,
        })
    }

    pub fn append(&mut self, other: &mut Properties) {
        self.0.append(&mut other.0)
    }
}

impl IntoIterator for Properties {
    type Item = Property<AbstractModel>;

    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

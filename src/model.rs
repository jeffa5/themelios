use stateright::{Expectation, Property};

use crate::{
    abstract_model::{AbstractModel, AbstractModelCfg},
    controller::{
        job::JobController, podgc::PodGCController, Controllers, DeploymentController,
        NodeController, ReplicaSetController, SchedulerController, StatefulSetController,
    },
    controller_properties::ControllerProperties,
    state::{history::ConsistencySetup, RawState, State},
};

#[derive(derivative::Derivative)]
#[derivative(Debug)]
#[derive(Clone, Default)]
pub struct OrchestrationModelCfg {
    /// The initial state.
    pub initial_state: RawState,
    /// The consistency level of the state.
    pub consistency_level: ConsistencySetup,
    /// The number of schedulers to run.
    pub schedulers: usize,
    /// The number of nodes to run.
    pub nodes: usize,
    /// The number of replicaset controllers to run.
    pub replicaset_controllers: usize,
    pub deployment_controllers: usize,
    pub statefulset_controllers: usize,
    pub job_controllers: usize,
    pub podgc_controllers: usize,

    #[derivative(Debug = "ignore")]
    pub properties: Vec<Property<AbstractModel>>,
}

impl OrchestrationModelCfg {
    pub fn new(
        initial_state: RawState,
        consistency_level: ConsistencySetup,
        controllers: usize,
    ) -> Self {
        Self {
            initial_state,
            consistency_level,
            schedulers: controllers,
            nodes: controllers,
            replicaset_controllers: controllers,
            deployment_controllers: controllers,
            statefulset_controllers: controllers,
            job_controllers: controllers,
            podgc_controllers: controllers,
            properties: Vec::new(),
        }
    }

    pub fn into_abstract_model(mut self) -> AbstractModel {
        self.auto_add_properties();

        let mut cfg = AbstractModelCfg {
            controllers: Vec::new(),
            initial_state: self.initial_state,
            consistency_level: self.consistency_level,
            properties: self.properties,
        };

        for i in 0..self.nodes {
            cfg.controllers.push(Controllers::Node(NodeController {
                name: format!("node-{i}"),
            }));
        }

        for _ in 0..self.schedulers {
            cfg.controllers
                .push(Controllers::Scheduler(SchedulerController));
        }

        for _ in 0..self.replicaset_controllers {
            cfg.controllers
                .push(Controllers::ReplicaSet(ReplicaSetController));
        }

        for _ in 0..self.deployment_controllers {
            cfg.controllers
                .push(Controllers::Deployment(DeploymentController));
        }

        for _ in 0..self.statefulset_controllers {
            cfg.controllers
                .push(Controllers::StatefulSet(StatefulSetController));
        }

        for _ in 0..self.job_controllers {
            cfg.controllers.push(Controllers::Job(JobController));
        }

        for _ in 0..self.podgc_controllers {
            cfg.controllers.push(Controllers::PodGC(PodGCController));
        }

        AbstractModel::new(cfg)
    }

    pub fn add_property(
        &mut self,
        expectation: Expectation,
        name: &'static str,
        condition: fn(&AbstractModel, &State) -> bool,
    ) {
        self.properties.push(Property {
            expectation,
            name,
            condition,
        })
    }

    pub fn add_properties(
        &mut self,
        properties: impl IntoIterator<Item = Property<AbstractModel>>,
    ) {
        self.properties.extend(properties)
    }

    fn auto_add_properties(&mut self) {
        if self.replicaset_controllers > 0 {
            self.add_properties(ReplicaSetController::properties())
        }
        if self.deployment_controllers > 0 {
            self.add_properties(DeploymentController::properties())
        }
        if self.statefulset_controllers > 0 {
            self.add_properties(StatefulSetController::properties())
        }
        if self.job_controllers > 0 {
            self.add_properties(JobController::properties())
        }
        if self.podgc_controllers > 0 {
            self.add_properties(PodGCController::properties())
        }
        if self.nodes > 0 {
            self.add_properties(NodeController::properties())
        }
        if self.schedulers > 0 {
            self.add_properties(SchedulerController::properties())
        }
    }
}

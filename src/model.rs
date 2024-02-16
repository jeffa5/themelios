use stateright::{Expectation, Property};

use crate::{
    abstract_model::AbstractModelCfg,
    controller::{
        client::ClientState, job::JobController, podgc::PodGCController, Controllers,
        DeploymentController, NodeController, ReplicaSetController, SchedulerController,
        StatefulSetController,
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
    /// The number of datastores to run.
    pub datastores: usize,
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

    /// Set up clients with specific actions to perform.
    pub client_state: ClientState,

    #[derivative(Debug = "ignore")]
    pub properties: Vec<Property<AbstractModelCfg>>,
}

impl OrchestrationModelCfg {
    pub fn into_abstract_model(mut self) -> AbstractModelCfg {
        self.auto_add_properties();

        let mut model = AbstractModelCfg {
            controllers: Vec::new(),
            clients: Vec::new(),
            initial_state: self.initial_state,
            consistency_level: self.consistency_level,
            properties: self.properties,
        };

        for i in 0..self.nodes {
            model.controllers.push(Controllers::Node(NodeController {
                name: format!("node-{i}"),
            }));
        }

        for _ in 0..self.schedulers {
            model
                .controllers
                .push(Controllers::Scheduler(SchedulerController));
        }

        for _ in 0..self.replicaset_controllers {
            model
                .controllers
                .push(Controllers::ReplicaSet(ReplicaSetController));
        }

        for _ in 0..self.deployment_controllers {
            model
                .controllers
                .push(Controllers::Deployment(DeploymentController));
            if !self.client_state.is_empty() {
                for deployment in model.initial_state.deployments.iter() {
                    model.clients.push(crate::controller::client::Client {
                        name: deployment.metadata.name.clone(),
                        initial_state: self.client_state.clone(),
                    })
                }
            }
        }

        for _ in 0..self.statefulset_controllers {
            model
                .controllers
                .push(Controllers::StatefulSet(StatefulSetController));
            if !self.client_state.is_empty() {
                for statefulset in model.initial_state.statefulsets.iter() {
                    model.clients.push(crate::controller::client::Client {
                        name: statefulset.metadata.name.clone(),
                        initial_state: self.client_state.clone(),
                    })
                }
            }
        }

        for _ in 0..self.job_controllers {
            model.controllers.push(Controllers::Job(JobController));
        }

        for _ in 0..self.podgc_controllers {
            model.controllers.push(Controllers::PodGC(PodGCController));
        }

        model
    }

    pub fn add_property(
        &mut self,
        expectation: Expectation,
        name: &'static str,
        condition: fn(&AbstractModelCfg, &State) -> bool,
    ) {
        self.properties.push(Property {
            expectation,
            name,
            condition,
        })
    }

    pub fn add_properties(
        &mut self,
        properties: impl IntoIterator<Item = Property<AbstractModelCfg>>,
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

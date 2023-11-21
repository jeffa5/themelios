use stateright::{
    actor::{ActorModel, Network},
    Expectation, Property,
};

use crate::{
    abstract_model::AbstractModelCfg,
    actor_model::{ActorModelCfg, ActorState, Actors, ControllerActor, Datastore},
    controller::{
        client::{ClientAction, ClientState, ClientStateAuto, ClientStateManual},
        Controllers, DeploymentController, NodeController, ReplicaSetController,
        SchedulerController, StatefulSetController,
    },
    state::{ConsistencySetup, State, StateView},
};

#[derive(derivative::Derivative)]
#[derivative(Debug)]
#[derive(Clone, Default)]
pub struct OrchestrationModelCfg {
    /// The initial state.
    pub initial_state: StateView,
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

    /// Whether to enable clients with some default params.
    pub clients: bool,

    /// Set up clients with specific actions to perform.
    pub client_actions: Vec<ClientAction>,

    #[derivative(Debug = "ignore")]
    pub properties: Vec<Property<AbstractModelCfg>>,
}

impl OrchestrationModelCfg {
    /// Instantiate a new actor model based on this config.
    pub fn into_actor_model(self) -> ActorModel<Actors, ActorModelCfg, ()> {
        let mut model = ActorModel::new(
            ActorModelCfg {
                initial_pods: self.initial_state.pods.len(),
            },
            (),
        );

        assert!(self.datastores > 0);
        for _ in 0..self.datastores {
            model = model.actor(Actors::Datastore(Datastore {
                initial_state: self.initial_state.clone(),
            }));
        }

        for i in 0..self.nodes {
            model = model.actor(Actors::Node(ControllerActor::new(NodeController {
                name: format!("node-{i}"),
            })));
        }

        for _ in 0..self.schedulers {
            model = model.actor(Actors::Scheduler(ControllerActor::new(SchedulerController)));
        }

        for _ in 0..self.replicaset_controllers {
            model = model.actor(Actors::ReplicaSet(ControllerActor::new(
                ReplicaSetController,
            )));
        }

        for _ in 0..self.deployment_controllers {
            model = model.actor(Actors::Deployment(ControllerActor::new(
                DeploymentController,
            )));
        }

        for _ in 0..self.replicaset_controllers {
            model = model.actor(Actors::StatefulSet(ControllerActor::new(
                StatefulSetController,
            )));
        }

        model = model.init_network(Network::new_unordered_nonduplicating(vec![]));

        model.property(
            // TODO: eventually properties don't seem to work with timers, even though they may be
            // steady state.
            stateright::Expectation::Eventually,
            "every application gets scheduled",
            |model, state| {
                let mut any = false;
                let total_apps = model.cfg.initial_pods;
                if let ActorState::Datastore(datastore) = &**state.actor_states.first().unwrap() {
                    let datastore_state = datastore.view_at(datastore.max_revision());
                    let all_apps_scheduled = datastore_state
                        .pods
                        .iter()
                        .all(|a| a.spec.node_name.is_some());
                    let num_scheduled_apps = datastore_state.pods.len();
                    if all_apps_scheduled && num_scheduled_apps == total_apps {
                        any = true;
                    }
                }
                any
            },
        )
    }

    pub fn into_abstract_model(self) -> AbstractModelCfg {
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
            if self.clients {
                for deployment in model.initial_state.deployments.iter() {
                    model.clients.push(crate::controller::client::Client {
                        deployment_name: deployment.metadata.name.clone(),
                        initial_state: ClientState::Auto(ClientStateAuto {
                            change_image: 1,
                            scale_up: 1,
                            scale_down: 1,
                        }),
                    });
                }
            }
            if !self.client_actions.is_empty() {
                for deployment in model.initial_state.deployments.iter() {
                    model.clients.push(crate::controller::client::Client {
                        deployment_name: deployment.metadata.name.clone(),
                        initial_state: ClientState::Manual(ClientStateManual {
                            actions: self.client_actions.clone(),
                        }),
                    })
                }
            }
        }

        for _ in 0..self.statefulset_controllers {
            model
                .controllers
                .push(Controllers::StatefulSet(StatefulSetController));
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
}

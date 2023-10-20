use stateright::actor::{ActorModel, Network};

use crate::{
    abstract_model::AbstractModelCfg,
    actor_model::{ActorModelCfg, ActorState, Actors, ControllerActor, Datastore},
    controller::{Controllers, Deployment, Node, ReplicaSet, Scheduler, StatefulSet},
    state::{ConsistencySetup, StateView},
};

#[derive(Clone, Debug)]
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
            model = model.actor(Actors::Node(ControllerActor::new(Node {
                name: format!("node-{i}"),
            })));
        }

        for _ in 0..self.schedulers {
            model = model.actor(Actors::Scheduler(ControllerActor::new(Scheduler)));
        }

        for _ in 0..self.replicaset_controllers {
            model = model.actor(Actors::ReplicaSet(ControllerActor::new(ReplicaSet)));
        }

        for _ in 0..self.deployment_controllers {
            model = model.actor(Actors::Deployment(ControllerActor::new(Deployment)));
        }

        for _ in 0..self.replicaset_controllers {
            model = model.actor(Actors::StatefulSet(ControllerActor::new(StatefulSet)));
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
                    let all_apps_scheduled =
                        datastore_state.pods.values().all(|a| a.spec.node_name.is_some());
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
            initial_state: self.initial_state,
            consistency_level: self.consistency_level,
        };

        assert!(self.datastores > 0);

        for i in 0..self.nodes {
            model.controllers.push(Controllers::Node(Node {
                name: format!("node-{i}"),
            }));
        }

        for _ in 0..self.schedulers {
            model.controllers.push(Controllers::Scheduler(Scheduler));
        }

        for _ in 0..self.replicaset_controllers {
            model.controllers.push(Controllers::ReplicaSet(ReplicaSet));
        }

        for _ in 0..self.deployment_controllers {
            model.controllers.push(Controllers::Deployment(Deployment));
        }

        for _ in 0..self.statefulset_controllers {
            model
                .controllers
                .push(Controllers::StatefulSet(StatefulSet));
        }

        model
    }
}

use crate::actor::Actors;
use crate::actor::ControllerActor;
use crate::actor::Datastore;

use crate::controller::Controllers;
use crate::model::ModelCfg;
use crate::controller::node::Node;
use crate::controller::replicaset::ReplicaSet;
use crate::controller::scheduler::Scheduler;
use stateright::actor::ActorModel;
use stateright::actor::Network;

#[derive(Clone, Debug)]
pub struct OrchestrationModelCfg {
    /// The number of static pods to start with.
    pub initial_pods: u32,
    /// The number of schedulers to run.
    pub schedulers: usize,
    /// The number of nodes to run.
    pub nodes: usize,
    /// The number of datastores to run.
    pub datastores: usize,
    /// The number of replicaset controllers to run.
    pub replicaset_controllers: usize,
    /// The number of replicasets to create.
    pub replicasets: u32,
    /// The number of pods each replicaset manages.
    pub pods_per_replicaset: u32,
}

pub struct ActorModelCfg {
    pub initial_pods: u32,
}

impl OrchestrationModelCfg {
    /// Instantiate a new actor model based on this config.
    pub fn into_actor_model(self) -> ActorModel<Actors, ActorModelCfg, ()> {
        let mut model = ActorModel::new(
            ActorModelCfg {
                initial_pods: self.initial_pods,
            },
            (),
        );

        assert!(self.datastores > 0);
        for _ in 0..self.datastores {
            model = model.actor(Actors::Datastore(Datastore {
                initial_pods: self.initial_pods,
                initial_replicasets: self.replicasets,
                pods_per_replicaset: self.pods_per_replicaset,
            }));
        }

        for _ in 0..self.nodes {
            model = model.actor(Actors::Node(ControllerActor::new(Node)));
        }

        for _ in 0..self.schedulers {
            model = model.actor(Actors::Scheduler(ControllerActor::new(Scheduler)));
        }

        for _ in 0..self.replicaset_controllers {
            model = model.actor(Actors::ReplicaSet(ControllerActor::new(ReplicaSet)));
        }

        model = model.init_network(Network::new_unordered_nonduplicating(vec![]));

        model.property(
            // TODO: eventually properties don't seem to work with timers, even though they may be
            // steady state.
            stateright::Expectation::Eventually,
            "every application gets scheduled",
            |model, state| {
                let mut any = false;
                let total_apps = model.cfg.initial_pods as usize;
                let datastore_state = state.actor_states.first().unwrap();
                let all_apps_scheduled =
                    datastore_state.pods.values().all(|a| a.node_name.is_some());
                let num_scheduled_apps = datastore_state.pods.len();
                if all_apps_scheduled && num_scheduled_apps == total_apps {
                    any = true;
                }
                any
            },
        )
    }

    pub fn into_model(self) -> ModelCfg {
        let mut model = ModelCfg {
            controllers: Vec::new(),
            initial_pods: self.initial_pods,
            initial_replicasets: self.replicasets,
            pods_per_replicaset: self.pods_per_replicaset,
        };

        assert!(self.datastores > 0);

        for _ in 0..self.nodes {
            model.controllers.push(Controllers::Node(Node));
        }

        for _ in 0..self.schedulers {
            model.controllers.push(Controllers::Scheduler(Scheduler));
        }

        for _ in 0..self.replicaset_controllers {
            model.controllers.push(Controllers::ReplicaSet(ReplicaSet));
        }

        model
    }
}

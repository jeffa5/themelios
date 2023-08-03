use crate::client;
use crate::controller::ControllerType;
use crate::model::ModelCfg;
use crate::node;
use crate::root::RootState;
use stateright::actor::ActorModel;
use stateright::actor::Id;
use stateright::actor::Network;

use crate::datastore;
use crate::root::Root;
use crate::scheduler;

#[derive(Clone, Debug)]
pub struct ActorModelCfg {
    /// The number of apps each client should create.
    pub apps_per_client: u32,
    /// The number of clients to run.
    pub clients: usize,
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

impl ActorModelCfg {
    /// Instantiate a new actor model based on this config.
    pub fn into_actor_model(self) -> ActorModel<Root, Self, ()> {
        let mut model = ActorModel::new(self.clone(), ());

        let datastore_id = Id::from(0);
        assert!(self.datastores > 0);
        for _ in 0..self.datastores {
            model = model.actor(Root::Datastore(datastore::Datastore {}));
        }

        for _ in 0..self.nodes {
            model = model.actor(Root::Node(node::Node {
                datastore: datastore_id,
            }));
        }

        for _ in 0..self.schedulers {
            model = model.actor(Root::Scheduler(scheduler::Scheduler {
                datastore: datastore_id,
            }));
        }

        for _ in 0..self.clients {
            model = model.actor(Root::Client(client::Client {
                datastore: datastore_id,
                initial_apps: self.apps_per_client,
            }));
        }

        model = model.init_network(Network::new_unordered_nonduplicating(vec![]));

        model.property(
            // TODO: eventually properties don't seem to work with timers, even though they may be
            // steady state.
            stateright::Expectation::Eventually,
            "every application gets scheduled",
            |model, state| {
                let mut any = false;
                let total_apps = model.cfg.apps_per_client as usize * model.cfg.clients;
                for actor in &state.actor_states {
                    if let RootState::Datastore(d) = &**actor {
                        if d.unscheduled_apps.is_empty() && d.scheduled_apps.len() == total_apps {
                            any = true;
                        }
                    }
                }
                any
            },
        )
    }

    pub fn into_model(self) -> ModelCfg {
        let mut model = ModelCfg {
            controllers: Vec::new(),
            initial_pods: self.clients as u32 * self.apps_per_client,
            initial_replicasets: self.replicasets,
            pods_per_replicaset: self.pods_per_replicaset,
        };

        assert!(self.datastores > 0);

        for _ in 0..self.nodes {
            model.controllers.push(ControllerType::Node);
        }

        for _ in 0..self.schedulers {
            model.controllers.push(ControllerType::Scheduler);
        }

        for _ in 0..self.replicaset_controllers {
            model.controllers.push(ControllerType::ReplicaSet);
        }

        model
    }
}

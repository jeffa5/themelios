use crate::client;
use crate::node;
use stateright::actor::ActorModel;
use stateright::actor::Id;
use stateright::actor::Network;

use crate::datastore;
use crate::root::Root;
use crate::scheduler;

pub struct ModelCfg {
    /// The number of apps each client should create.
    pub apps_per_client: usize,
    /// The number of clients to run.
    pub clients: usize,
    /// The number of schedulers to run.
    pub schedulers: usize,
    /// The number of nodes to run.
    pub nodes: usize,
    /// The number of datastores to run.
    pub datastores: usize,
}

impl ModelCfg {
    /// Instantiate a new actor model based on this config.
    pub fn into_actor_model(self) -> ActorModel<Root, (), ()> {
        let mut model = ActorModel::new((), ());

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
                initial_apps: 2,
            }));
        }

        model.init_network(Network::new_ordered(vec![]))
    }
}

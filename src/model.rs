use crate::node;
use stateright::actor::ActorModel;
use stateright::actor::Id;
use stateright::actor::Network;

use crate::datastore;
use crate::root::Root;
use crate::scheduler;

pub struct ModelCfg {
    pub schedulers: usize,
    pub nodes: usize,
    pub datastores: usize,
}

impl ModelCfg {
    pub fn into_actor_model(self) -> ActorModel<Root, (), ()> {
        let mut model = ActorModel::new((), ());

        let datastore_id = Id::from(0);
        assert!(self.datastores > 0);
        for _ in 0..self.datastores {
            model = model.actor(Root::Datastore(datastore::Datastore { initial_apps: 2 }));
        }

        for _ in 0..self.schedulers {
            model = model.actor(Root::Scheduler(scheduler::Scheduler {
                datastore: datastore_id,
            }));
        }

        for _ in 0..self.nodes {
            model = model.actor(Root::Node(node::Node {
                datastore: datastore_id,
            }));
        }

        model.init_network(Network::new_ordered(vec![]))
    }
}

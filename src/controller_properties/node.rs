use std::collections::BTreeSet;

use stateright::Expectation;

use crate::controller::{ControllerStates, NodeController};

use super::{ControllerProperties, Properties};

impl ControllerProperties for NodeController {
    fn properties() -> Properties {
        let mut properties = Properties::default();
        properties.add(
            Expectation::Always,
            "node: pods on nodes are unique",
            |model, state| {
                let mut node_pods = BTreeSet::new();
                for c in 0..model.controllers.len() {
                    let cstate = state.get_controller(c);
                    if let ControllerStates::Node(n) = cstate {
                        for node in &n.running {
                            if !node_pods.insert(node) {
                                return false;
                            }
                        }
                    }
                }
                true
            },
        );
        properties
    }
}

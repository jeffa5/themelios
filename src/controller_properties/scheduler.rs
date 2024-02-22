use stateright::Expectation;

use crate::controller::SchedulerController;

use super::{ControllerProperties, Properties};

impl ControllerProperties for SchedulerController {
    fn properties() -> Properties {
        let mut properties = Properties::default();
        // properties.add(
        //     Expectation::Eventually,
        //     "sched: every pod gets scheduled",
        //     |_model, state| {
        //         let state = state.latest();
        //         state.pods.iter().all(|pod| pod.spec.node_name.is_some())
        //     },
        // );
        properties
    }
}

use crate::controller::PodGCController;

use super::{ControllerProperties, Properties};

impl ControllerProperties for PodGCController {
    fn properties() -> Properties {
        Properties::default()
    }
}

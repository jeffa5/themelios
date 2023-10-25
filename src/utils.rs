use crate::resources::Time;
use time::OffsetDateTime;

use crate::resources::Metadata;

pub fn new_uid() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn now() -> Time {
    Time(OffsetDateTime::now_utc())
}

pub fn metadata(name: String) -> Metadata {
    Metadata {
        name,
        generate_name: String::new(),
        namespace: "default".to_owned(),
        creation_timestamp: None,
        deletion_timestamp: None,
        generation: 0,
        uid: new_uid(),
        labels: Default::default(),
        annotations: Default::default(),
        deletion_grace_period_seconds: None,
        managed_fields: Vec::new(),
        owner_references: Vec::new(),
        resource_version: String::new(),
        finalizers: Vec::new(),
    }
}

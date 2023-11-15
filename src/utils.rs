use crate::resources::Time;
use time::OffsetDateTime;

use crate::resources::Metadata;

#[cfg(feature = "serve")]
pub fn new_uid(_name:&str) -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(feature = "model")]
pub fn new_uid(name:&str) -> String {
    name.to_owned()
}

#[cfg(feature = "serve")]
pub fn now() -> Time {
    Time(OffsetDateTime::now_utc())
}

#[cfg(feature = "model")]
pub fn now() -> Time {
    Time(OffsetDateTime::UNIX_EPOCH)
}

pub fn metadata(name: String) -> Metadata {
    let uid = new_uid(&name);
    Metadata {
        name,
        generate_name: String::new(),
        namespace: "default".to_owned(),
        creation_timestamp: None,
        deletion_timestamp: None,
        generation: 0,
        uid,
        labels: Default::default(),
        annotations: Default::default(),
        deletion_grace_period_seconds: None,
        managed_fields: Vec::new(),
        owner_references: Vec::new(),
        resource_version: String::new(),
        finalizers: Vec::new(),
    }
}

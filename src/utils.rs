use crate::{resources::Time, state::revision::Revision};
use time::OffsetDateTime;

use crate::resources::Metadata;

#[cfg(feature = "serve")]
pub fn new_uid(_name: &str) -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(not(feature = "serve"))]
pub fn new_uid(name: &str) -> String {
    name.to_owned()
}

#[cfg(feature = "serve")]
pub fn now() -> Time {
    Time(OffsetDateTime::now_utc())
}

#[cfg(not(feature = "serve"))]
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
        resource_version: Revision::default(),
        finalizers: Vec::new(),
    }
}

pub trait LogicalBoolExt {
    fn implies(self, other: bool) -> bool;
    fn implies_then(self, other: impl Fn() -> bool) -> bool;
}

impl LogicalBoolExt for bool {
    fn implies(self, other: bool) -> bool {
        // P => Q == not(P) \/ Q
        !self || other
    }

    fn implies_then(self, other: impl Fn() -> bool) -> bool {
        // P => Q == not(P) \/ Q
        !self || other()
    }
}

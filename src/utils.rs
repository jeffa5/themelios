use time::OffsetDateTime;

use crate::resources::Metadata;

pub fn new_uid() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn now() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}

pub fn metadata(name: String) -> Metadata {
    Metadata {
        name,
        namespace:"default".to_owned(),
        creation_timestamp: None,
        deletion_timestamp:None,
        generation:0,
        uid: new_uid(),
        labels: Default::default(),
        annotations: Default::default(),
    }
}

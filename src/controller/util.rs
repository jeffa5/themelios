use crate::{
    abstract_model::Operation,
    resources::{GroupVersionKind, Metadata, OwnerReference},
};

pub enum ResourceOrOp<R> {
    Resource(R),
    Op(Operation),
}

pub fn new_controller_ref(owner: &Metadata, gvk: &GroupVersionKind) -> OwnerReference {
    OwnerReference {
        api_version: gvk.group_version().to_string(),
        kind: gvk.kind.to_owned(),
        name: owner.name.clone(),
        uid: owner.uid.clone(),
        block_owner_deletion: true,
        controller: true,
    }
}

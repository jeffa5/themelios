use crate::{
    abstract_model::Operation,
    resources::{GroupVersionKind, Metadata, OwnerReference, PodResource, PodTemplateSpec, PodStatus},
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

pub fn get_pod_from_template(
    metadata: &Metadata,
    template: &PodTemplateSpec,
    controller_kind: &GroupVersionKind,
) -> PodResource {
    let desired_labels = template.metadata.labels.clone();
    let desired_finalizers = template.metadata.finalizers.clone();
    let desired_annotations = template.metadata.annotations.clone();
    let prefix = get_pods_prefix(&metadata.name);
    let mut pod = PodResource {
        metadata: Metadata {
            generate_name: prefix,
            namespace: metadata.namespace.clone(),
            labels: desired_labels,
            annotations: desired_annotations,
            finalizers: desired_finalizers,
            ..Default::default()
        },
        spec: template.spec.clone(),
        status: PodStatus::default(),
    };
    pod.metadata
        .owner_references
        .push(new_controller_ref(&metadata, controller_kind));
    pod
}

fn get_pods_prefix(controller_name: &str) -> String {
    // use the dash (if the name isn't too long) to make the pod name a bit prettier
    let prefix = format!("{}-", controller_name);
    // TODO: validate pod name and maybe remove dash
    prefix
}

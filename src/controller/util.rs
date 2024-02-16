use std::collections::BTreeMap;

use crate::resources::{
    ConditionStatus, GroupVersionKind, Meta, Metadata, NodeCondition, NodeConditionType,
    OwnerReference, Pod, PodConditionType, PodPhase, PodStatus, PodTemplateSpec,
};

pub enum ValOrOp<V, O> {
    Resource(V),
    Op(O),
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
) -> Pod {
    let desired_labels = template.metadata.labels.clone();
    let desired_finalizers = template.metadata.finalizers.clone();
    let desired_annotations = template.metadata.annotations.clone();
    let prefix = get_pods_prefix(&metadata.name);
    let mut pod = Pod {
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
        .push(new_controller_ref(metadata, controller_kind));
    pod
}

fn get_pods_prefix(controller_name: &str) -> String {
    // use the dash (if the name isn't too long) to make the pod name a bit prettier
    let prefix = format!("{}-", controller_name);
    // TODO: validate pod name and maybe remove dash
    prefix
}

pub fn get_node_condition(
    conditions: &[NodeCondition],
    cond_type: NodeConditionType,
) -> Option<&NodeCondition> {
    conditions.iter().find(|c| c.r#type == cond_type)
}

pub fn filter_active_pods<'a>(pods: &[&'a Pod]) -> Vec<&'a Pod> {
    pods.iter()
        .filter_map(|pod| if is_pod_active(pod) { Some(*pod) } else { None })
        .collect()
}

pub fn is_pod_ready(pod: &Pod) -> bool {
    pod.status
        .conditions
        .iter()
        .find(|c| c.r#type == PodConditionType::Ready)
        .map_or(false, |c| c.status == ConditionStatus::True)
}

pub fn is_pod_active(pod: &Pod) -> bool {
    pod.status.phase != PodPhase::Succeeded
        && pod.status.phase != PodPhase::Failed
        && pod.metadata.deletion_timestamp.is_none()
}

pub fn filter_terminating_pods<'a>(pods: &[&'a Pod]) -> Vec<&'a Pod> {
    pods.iter()
        .filter(|p| is_pod_terminating(p))
        .copied()
        .collect()
}

pub fn is_pod_terminating(pod: &Pod) -> bool {
    !(pod.status.phase == PodPhase::Failed || pod.status.phase == PodPhase::Succeeded)
        && pod.metadata.deletion_timestamp.is_some()
}

// Check that the annotations on resource `a` are all set on resource `b`.
pub fn annotations_subset<T, U>(a: &T, b: &U) -> bool
where
    T: Meta,
    U: Meta,
{
    subset(&a.metadata().annotations, &b.metadata().annotations)
}

fn subset(m1: &BTreeMap<String, String>, m2: &BTreeMap<String, String>) -> bool {
    m1.iter().all(|(k, v)| m2.get(k).map_or(false, |w| v == w))
}

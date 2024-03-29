use k8s_openapi::apimachinery::pkg::apis::meta::v1::APIResource;
use k8s_openapi::NamespaceResourceScope;
use k8s_openapi::Resource;
use serde::Serialize;

use crate::resources::Deployment;
use crate::resources::Node;
use crate::resources::Pod;
use crate::resources::ReplicaSet;
use crate::resources::Scale;

pub trait APIObject: Resource {
    fn api_resource() -> APIResource;
}

macro_rules! impl_resource {
    ($r:ident, $scope:ident, $apiversion:expr, $group:expr, $kind:expr, $version:expr, $urlpathsegment:expr) => {
        impl Resource for $r {
            type Scope = $scope;

            const API_VERSION: &'static str = $apiversion;
            const GROUP: &'static str = $group;
            const KIND: &'static str = $kind;
            const VERSION: &'static str = $version;
            const URL_PATH_SEGMENT: &'static str = $urlpathsegment;
        }
    };
}

impl_resource!(
    Pod,
    NamespaceResourceScope,
    "v1",
    "core",
    "Pod",
    "v1",
    "pods"
);
// impl_resource!(Job, "JobList");
impl_resource!(
    Deployment,
    NamespaceResourceScope,
    "apps/v1",
    "apps",
    "Deployment",
    "v1",
    "deployments"
);
impl_resource!(
    ReplicaSet,
    NamespaceResourceScope,
    "apps/v1",
    "apps",
    "ReplicaSet",
    "v1",
    "replicasets"
);
// impl_resource!(StatefulSet, "StatefulSetList");
// impl_resource!(PersistentVolumeClaim, "PersistentVolumeClaimList");
impl_resource!(
    Node,
    NamespaceResourceScope,
    "v1",
    "core",
    "Node",
    "v1",
    "nodes"
);

macro_rules! impl_listable {
    ($r:ident, $kind:expr) => {
        impl k8s_openapi::ListableResource for $r {
            const LIST_KIND: &'static str = $kind;
        }
    };
}

impl_listable!(Pod, "PodList");
// impl_listable!(Job, "JobList");
impl_listable!(Deployment, "DeploymentList");
impl_listable!(ReplicaSet, "ReplicaSetList");
// impl_listable!(StatefulSet, "StatefulSetList");
// impl_listable!(PersistentVolumeClaim, "PersistentVolumeClaimList");
impl_listable!(Node, "NodeList");
//
macro_rules! impl_api_object {
    ($r:ident) => {
        impl APIObject for $r {
            fn api_resource() -> APIResource {
                let singular_name = $r::KIND.to_lowercase();
                let plural_name = format!("{singular_name}s");
                APIResource {
                    categories: None,
                    // group: Some($r::GROUP.to_owned()),
                    group: None,
                    kind: $r::KIND.to_owned(),
                    name: plural_name,
                    namespaced: true,
                    short_names: None,
                    singular_name: $r::KIND.to_lowercase(),
                    storage_version_hash: None,
                    verbs: vec![
                        "get".to_owned(),
                        "list".to_owned(),
                        "create".to_owned(),
                        "update".to_owned(),
                        "patch".to_owned(),
                        "delete".to_owned(),
                        "deletecollection".to_owned(),
                    ],
                    version: None,
                }
            }
        }
    };
}

impl_api_object!(Pod);
// impl_api_object!(Job);
impl_api_object!(Deployment);
impl_api_object!(ReplicaSet);
// impl_api_object!(StatefulSet);
// impl_api_object!(PersistentVolumeClaim);
impl_api_object!(Node);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SerializableResource<R: Resource> {
    api_version: String,
    kind: String,
    #[serde(flatten)]
    resource: R,
}

impl<R: Resource> SerializableResource<R> {
    pub fn new(resource: R) -> Self {
        Self {
            api_version: R::API_VERSION.to_owned(),
            kind: R::KIND.to_owned(),
            resource,
        }
    }
}

impl<R: Resource> k8s_openapi::Resource for SerializableResource<R> {
    const API_VERSION: &'static str = R::API_VERSION;
    const GROUP: &'static str = R::GROUP;
    const KIND: &'static str = R::KIND;
    const VERSION: &'static str = R::VERSION;
    const URL_PATH_SEGMENT: &'static str = R::URL_PATH_SEGMENT;
    type Scope = R::Scope;
}

impl<R: Resource + k8s_openapi::ListableResource> k8s_openapi::ListableResource
    for SerializableResource<R>
{
    const LIST_KIND: &'static str = R::LIST_KIND;
}

impl Scale {
    pub fn api_resource<K: Resource>() -> APIResource {
        APIResource {
            categories: None,
            group: Some("autoscaling".to_owned()),
            kind: "Scale".to_owned(),
            name: format!("{}/scale", K::URL_PATH_SEGMENT),
            namespaced: true,
            short_names: None,
            singular_name: "".to_owned(),
            storage_version_hash: None,
            verbs: vec!["get".to_owned(), "patch".to_owned(), "update".to_owned()],
            version: Some("v1".to_owned()),
        }
    }
}

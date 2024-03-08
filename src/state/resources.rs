use std::sync::Arc;

use tracing::warn;

use crate::{
    resources::{LabelSelector, Meta, Spec},
    utils::now,
};

use super::revision::Revision;

/// A data structure that ensures the resources are unique by name, and kept in sorted order for
/// efficient lookup and deterministic ordering.
#[derive(derivative::Derivative)]
#[derivative(PartialEq, Hash)]
#[derive(Clone, Debug, Eq, PartialOrd, Ord)]
pub struct Resources<T>(imbl::Vector<Arc<T>>);

impl<T> Default for Resources<T> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<T: Meta + Spec + Clone> Resources<T> {
    pub fn upsert(&mut self, res: T, revision: Revision)
    where
        T: PartialEq,
    {
        match self.create(res.clone(), revision.clone()) {
            Ok(_) => {}
            Err(_) => {
                self.update(res, revision).map_err(|_| ()).unwrap();
            }
        }
    }

    pub fn create(&mut self, mut res: T, revision: Revision) -> Result<(), T> {
        if self.has(&res.metadata().name) {
            return Err(res);
        }
        // set the uid if not set already
        if res.metadata().uid.is_empty() {
            res.metadata_mut().uid = revision.to_string();
        }
        // default the generation to 1
        if res.metadata().generation == 0 {
            res.metadata_mut().generation = 1;
        }
        // set the creation timestamp
        if res.metadata().creation_timestamp.is_none() {
            res.metadata_mut().creation_timestamp = Some(now());
        }
        // set the namespace
        if res.metadata().namespace.is_empty() {
            res.metadata_mut().namespace = "default".to_owned();
        }
        // set resource version to mod revision as per https://github.com/kubernetes/community/blob/master/contributors/devel/sig-architecture/api-conventions.md#concurrency-control-and-consistency
        res.metadata_mut().resource_version = revision;
        let pos = self.get_insertion_pos(&res.metadata().name);
        self.0.insert(pos, Arc::new(res));
        Ok(())
    }

    pub fn update(&mut self, mut res: T, revision: Revision) -> Result<(), T>
    where
        T: PartialEq,
    {
        if let Some(existing_pos) = self.get_pos(&res.metadata().name) {
            let existing = &self.0[existing_pos];
            if existing.metadata().deletion_timestamp.is_some() {
                // can only remove finalizers on terminating resources
                let mut ex = (**existing).clone();
                ex.metadata_mut().finalizers.clear();
                let mut r = res.clone();
                r.metadata_mut().finalizers.clear();
                if r != ex {
                    warn!("Tried to update resource that is terminating, only removing finalizers is allowed");
                    return Err(res);
                }
            }
            if existing.metadata().uid != res.metadata().uid {
                // TODO: update this to have some conflict-reconciliation thing?
                warn!(
                    "Different uids! {} vs {}",
                    existing.metadata().uid,
                    res.metadata().uid
                );
                Err(res)
            } else if existing.metadata().resource_version > res.metadata().resource_version {
                // ignore changes to resources when resource version is specified but the resource
                // being inserted is old
                let existing = &existing.metadata().resource_version;
                let new = &res.metadata().resource_version;
                warn!(?existing, ?new, "Old resource");
                Err(res)
            } else {
                // set resource version to mod revision as per https://github.com/kubernetes/community/blob/master/contributors/devel/sig-architecture/api-conventions.md#concurrency-control-and-consistency
                // Update the generation of the resource if the spec (desired state) has changed.
                let mut new_meta_without_generation = res.metadata().clone();
                new_meta_without_generation.generation = 0;
                let mut existing_meta_without_generation = existing.metadata().clone();
                existing_meta_without_generation.generation = 0;
                if res.spec() != existing.spec() ||
                // TODO: this should be able to be removed now that we have the
                // observed_revision field
                // THEMELIOS: changing metadata does not change generation normally, but this
                // eliminates the way to check for stability (that a controller has observed the
                // updates)
                    new_meta_without_generation != existing_meta_without_generation
                {
                    res.metadata_mut().generation += 1;
                }
                res.metadata_mut().resource_version = revision;
                self.0[existing_pos] = Arc::new(res);
                Ok(())
            }
        } else {
            Err(res)
        }
    }

    fn get_insertion_pos(&self, k: &str) -> usize {
        match self.0.binary_search_by(|v| k.cmp(&v.metadata().name)) {
            Ok(p) => p,
            Err(p) => p,
        }
    }

    fn get_pos(&self, k: &str) -> Option<usize> {
        self.0.binary_search_by(|v| k.cmp(&v.metadata().name)).ok()
    }

    pub fn has(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    pub fn get(&self, name: &str) -> Option<&T> {
        self.get_pos(name)
            .and_then(|p| self.0.get(p).map(|r| r.as_ref()))
    }

    pub fn iter(&self) -> ResourcesIter<'_, T> {
        ResourcesIter {
            iter: self.0.iter(),
        }
    }

    pub fn remove(&mut self, res: &T) -> Option<T> {
        // in order to remove a resource it must have the same uid.
        if let Some(existing_pos) = self.get_pos(&res.metadata().name) {
            let existing = &self.0[existing_pos];
            if existing.metadata().uid == res.metadata().uid {
                return Some((*self.0.remove(existing_pos)).clone());
            }
        }
        None
    }

    pub fn retain(&mut self, f: impl Fn(&T) -> bool) {
        self.0.retain(|r| f(r))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn for_controller<'a>(&'a self, uid: &'a str) -> impl Iterator<Item = &T> + 'a {
        self.0
            .iter()
            .filter(move |t| t.metadata().owner_references.iter().any(|or| or.uid == uid))
            .map(|r| r.as_ref())
    }

    pub fn matching<'a>(&'a self, selector: &'a LabelSelector) -> impl Iterator<Item = &T> + 'a {
        self.0
            .iter()
            .filter(move |t| selector.matches(&t.metadata().labels))
            .map(|r| r.as_ref())
    }

    pub fn to_vec(&self) -> Vec<&T> {
        self.iter().collect()
    }

    pub fn merge(&mut self, other: &Self) {
        for resource in &other.0 {
            if let Some(existing_pos) = self.get_pos(&resource.metadata().name) {
                let existing = &self.0[existing_pos];
                let new_revision = &resource.metadata().resource_version;
                let existing_revision = &existing.metadata().resource_version;
                if new_revision > existing_revision {
                    self.0[existing_pos] = Arc::clone(resource);
                }
            } else {
                let pos = self.get_insertion_pos(&resource.metadata().name);
                self.0.insert(pos, Arc::clone(resource));
            }
        }
    }
}

impl<T: Meta + Spec + Clone> From<Vec<T>> for Resources<T> {
    fn from(value: Vec<T>) -> Self {
        let mut rv = Resources::default();
        for v in value {
            let revision = v.metadata().resource_version.clone();
            rv.create(v, revision).map_err(|_| ()).unwrap();
        }
        rv
    }
}

impl<T: Meta + Spec + Clone> FromIterator<T> for Resources<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut rv = Resources::default();
        for v in iter {
            let revision = v.metadata().resource_version.clone();
            rv.create(v, revision).map_err(|_| ()).unwrap();
        }
        rv
    }
}

pub struct ResourcesIter<'a, T> {
    iter: imbl::vector::Iter<'a, Arc<T>>,
}

impl<'a, T> Iterator for ResourcesIter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|v| v.as_ref())
    }
}

impl<'a, T> IntoIterator for &'a Resources<T> {
    type Item = &'a T;

    type IntoIter = ResourcesIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        ResourcesIter {
            iter: self.0.iter(),
        }
    }
}

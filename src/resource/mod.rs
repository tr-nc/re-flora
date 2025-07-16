use std::any::Any;
use std::ops::{Deref, DerefMut};

pub trait ResourceContainer {
    const RESOURCE_NAMES: &'static [&'static str];

    fn get_resource<T: 'static>(&self, name: &str) -> Option<&T>;
    fn get_resource_names() -> Vec<&'static str>;
}

pub struct Resource<T> {
    inner: T,
}

impl<T> Resource<T> {
    pub fn new(resource: T) -> Self {
        Self { inner: resource }
    }

    pub fn get(&self) -> &T {
        &self.inner
    }
}

impl<T: 'static> Resource<T> {
    pub fn as_any(&self) -> &dyn Any {
        &self.inner
    }
}

impl<T> Deref for Resource<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for Resource<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

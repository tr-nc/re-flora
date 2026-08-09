use crate::{AccelStruct, Buffer, Texture};
use std::ops::{Deref, DerefMut};

/// A resource kind that can be supplied to a reflected descriptor binding.
///
/// Numeric descriptor locations remain inside the reflected descriptor runtime.
#[derive(Clone, Copy)]
pub enum DescriptorResource<'a> {
    Buffer(&'a Buffer),
    Texture(&'a Texture),
    AccelerationStructure(&'a AccelStruct),
}

/// The result of resolving one semantic resource name within a container tree.
#[derive(Clone, Copy)]
pub enum ResourceLookup<'a> {
    Missing,
    Unique(DescriptorResource<'a>),
    Ambiguous { providers: usize },
}

impl<'a> ResourceLookup<'a> {
    /// Merges independent provider trees without introducing first-match priority.
    #[doc(hidden)]
    pub fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::Missing, lookup) | (lookup, Self::Missing) => lookup,
            (Self::Unique(_), Self::Unique(_)) => Self::Ambiguous { providers: 2 },
            (Self::Unique(_), Self::Ambiguous { providers })
            | (Self::Ambiguous { providers }, Self::Unique(_)) => Self::Ambiguous {
                providers: providers + 1,
            },
            (
                Self::Ambiguous {
                    providers: left,
                },
                Self::Ambiguous {
                    providers: right,
                },
            ) => Self::Ambiguous {
                providers: left + right,
            },
        }
    }
}

pub trait ResourceContainer {
    fn resolve_resource(&self, name: &str) -> ResourceLookup<'_>;
}

pub struct Resource<T> {
    inner: T,
}

impl<T> Resource<T> {
    pub fn new(resource: T) -> Self {
        Self { inner: resource }
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

#[cfg(test)]
mod lookup_tests {
    use super::ResourceLookup;

    #[test]
    fn missing_lookup_is_the_merge_identity() {
        let ambiguous = ResourceLookup::Ambiguous { providers: 2 };
        assert!(matches!(
            ResourceLookup::Missing.merge(ResourceLookup::Missing),
            ResourceLookup::Missing
        ));
        assert!(matches!(
            ResourceLookup::Missing.merge(ambiguous),
            ResourceLookup::Ambiguous { providers: 2 }
        ));
    }

    #[test]
    fn ambiguous_lookup_counts_nested_provider_trees() {
        let left = ResourceLookup::Ambiguous { providers: 2 };
        let right = ResourceLookup::Ambiguous { providers: 3 };
        assert!(matches!(
            left.merge(right),
            ResourceLookup::Ambiguous { providers: 5 }
        ));
    }
}

#[derive(Clone, Debug)]
pub struct PingPong<T> {
    ping: T,
    pong: T,
}

impl<T> PingPong<T> {
    pub fn new(ping: T, pong: T) -> Self {
        Self { ping, pong }
    }

    pub fn ping(&self) -> &T {
        &self.ping
    }

    pub fn ping_mut(&mut self) -> &mut T {
        &mut self.ping
    }

    pub fn pong(&self) -> &T {
        &self.pong
    }

    pub fn pong_mut(&mut self) -> &mut T {
        &mut self.pong
    }

    pub fn into_parts(self) -> (T, T) {
        (self.ping, self.pong)
    }
}

#[derive(Clone, Debug)]
pub struct CurrentPrevious<T> {
    current: T,
    previous: T,
}

impl<T> CurrentPrevious<T> {
    pub fn new(current: T, previous: T) -> Self {
        Self { current, previous }
    }

    pub fn current(&self) -> &T {
        &self.current
    }

    pub fn current_mut(&mut self) -> &mut T {
        &mut self.current
    }

    pub fn previous(&self) -> &T {
        &self.previous
    }

    pub fn previous_mut(&mut self) -> &mut T {
        &mut self.previous
    }

    pub fn into_parts(self) -> (T, T) {
        (self.current, self.previous)
    }
}

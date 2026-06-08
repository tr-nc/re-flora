use crate::{Buffer, Texture};
use std::any::Any;
use std::ops::{Deref, DerefMut};

pub trait ResourceContainer {
    fn get_buffer(&self, name: &str) -> Option<&Buffer>;
    fn get_texture(&self, name: &str) -> Option<&Texture>;
    fn get_resource_names(&self) -> Vec<&'static str>;
}

pub struct Resource<T> {
    inner: T,
}

impl<T> Resource<T> {
    pub fn new(resource: T) -> Self {
        Self { inner: resource }
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

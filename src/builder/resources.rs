use crate::vkn::{Allocator, Device};

pub struct BuilderResources {}

impl BuilderResources {
    pub fn new(device: Device, allocator: Allocator, resolution: u32) -> Self {
        Self {}
    }
}

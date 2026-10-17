use anyhow::{ensure, Result};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use re_flora_vkn::vk;
use re_flora_vkn::{Allocator, Buffer, BufferUsage, Device, MemoryLocation};

use super::sprinkler_mesh::build_sprinkler_mesh;
use crate::resource::Resource;

pub const SPRINKLER_RENDER_CAPACITY: usize = 256;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct SprinklerVertex {
    pub(crate) position: [f32; 3],
    pub(crate) surface_position: [f32; 3],
    pub(crate) shading_normal: [f32; 3],
    pub(crate) color_srgb: [f32; 3],
    pub(crate) animation_direction: [f32; 3],
}

impl SprinklerVertex {
    pub(crate) fn new(
        position: Vec3,
        surface_position: Vec3,
        shading_normal: Vec3,
        color_srgb: Vec3,
        animation_direction: Vec3,
    ) -> Self {
        Self {
            position: position.to_array(),
            surface_position: surface_position.to_array(),
            shading_normal: shading_normal.to_array(),
            color_srgb: color_srgb.to_array(),
            animation_direction: animation_direction.to_array(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct SprinklerInstanceGpu {
    base_position: [f32; 3],
    animation_phase: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct SprinklerRenderInstance {
    pub base_position: Vec3,
    pub animation_phase: f32,
}

pub struct SprinklerRendererResources {
    pub vertices: Resource<Buffer>,
    pub indices: Resource<Buffer>,
    pub indices_len: u32,
    pub instances: Resource<Buffer>,
    pub instance_count: u32,
}

impl SprinklerRendererResources {
    pub fn new(device: Device, allocator: Allocator) -> Self {
        let (vertices_data, indices_data) = build_sprinkler_mesh();
        let vertices = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::VERTEX_BUFFER),
            MemoryLocation::CpuToGpu,
            std::mem::size_of_val(vertices_data.as_slice()) as u64,
        );
        vertices.fill(&vertices_data).unwrap();

        let indices = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::INDEX_BUFFER),
            MemoryLocation::CpuToGpu,
            std::mem::size_of_val(indices_data.as_slice()) as u64,
        );
        indices.fill(&indices_data).unwrap();

        let instances = Buffer::new_sized(
            device,
            allocator,
            BufferUsage::from_flags(vk::BufferUsageFlags::VERTEX_BUFFER),
            MemoryLocation::CpuToGpu,
            (std::mem::size_of::<SprinklerInstanceGpu>() * SPRINKLER_RENDER_CAPACITY) as u64,
        );

        Self {
            vertices: Resource::new(vertices),
            indices: Resource::new(indices),
            indices_len: indices_data.len() as u32,
            instances: Resource::new(instances),
            instance_count: 0,
        }
    }

    pub fn upload(&mut self, render_instances: &[SprinklerRenderInstance]) -> Result<()> {
        ensure!(
            render_instances.len() <= SPRINKLER_RENDER_CAPACITY,
            "sprinkler render capacity exceeded: {} > {}",
            render_instances.len(),
            SPRINKLER_RENDER_CAPACITY,
        );
        let instances = render_instances
            .iter()
            .map(|instance| SprinklerInstanceGpu {
                base_position: instance.base_position.to_array(),
                animation_phase: instance.animation_phase.rem_euclid(1.0),
            })
            .collect::<Vec<_>>();
        if !instances.is_empty() {
            self.instances.fill(&instances)?;
        }
        self.instance_count = instances.len() as u32;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprinkler_vertex_layout_matches_shader_locations() {
        assert_eq!(std::mem::size_of::<SprinklerVertex>(), 15 * 4);
        assert_eq!(std::mem::size_of::<SprinklerInstanceGpu>(), 4 * 4);
    }
}

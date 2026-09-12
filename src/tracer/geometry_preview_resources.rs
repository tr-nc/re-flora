use anyhow::{anyhow, ensure, Result};
use bytemuck::{Pod, Zeroable};
use glam::{Vec3, Vec4};
use re_flora_vkn::vk;
use re_flora_vkn::{Allocator, Buffer, BufferUsage, Device, MemoryLocation};

use crate::resource::Resource;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct GeometryPreviewVertex {
    pub position: [f32; 3],
    pub color_alpha: [f32; 4],
}

impl GeometryPreviewVertex {
    pub fn new(position: Vec3, color_alpha: Vec4) -> Self {
        Self {
            position: position.to_array(),
            color_alpha: color_alpha.to_array(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GeometryPreviewMesh {
    pub vertices: Vec<GeometryPreviewVertex>,
    pub indices: Vec<u32>,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct GeometryPreviewInstanceGpu {
    base_position: [f32; 3],
    tint: [f32; 4],
}

impl GeometryPreviewInstanceGpu {
    fn new(base_position: Vec3, tint: Vec4) -> Self {
        Self {
            base_position: base_position.to_array(),
            tint: tint.to_array(),
        }
    }
}

pub struct GeometryPreviewMeshResources {
    device: Device,
    allocator: Allocator,
    vertex_capacity: usize,
    index_capacity: usize,
    pub vertices: Resource<Buffer>,
    pub indices: Resource<Buffer>,
    pub indices_len: u32,
    pub instances: Resource<Buffer>,
    pub instance_count: u32,
}

impl GeometryPreviewMeshResources {
    fn new(device: Device, allocator: Allocator) -> Self {
        let vertices = Self::new_buffer::<GeometryPreviewVertex>(
            device.clone(),
            allocator.clone(),
            vk::BufferUsageFlags::VERTEX_BUFFER,
            1,
        );
        let indices = Self::new_buffer::<u32>(
            device.clone(),
            allocator.clone(),
            vk::BufferUsageFlags::INDEX_BUFFER,
            1,
        );
        let instances = Self::new_buffer::<GeometryPreviewInstanceGpu>(
            device.clone(),
            allocator.clone(),
            vk::BufferUsageFlags::VERTEX_BUFFER,
            1,
        );
        Self {
            device,
            allocator,
            vertex_capacity: 1,
            index_capacity: 1,
            vertices: Resource::new(vertices),
            indices: Resource::new(indices),
            indices_len: 0,
            instances: Resource::new(instances),
            instance_count: 0,
        }
    }

    fn new_buffer<T>(
        device: Device,
        allocator: Allocator,
        usage: vk::BufferUsageFlags,
        capacity: usize,
    ) -> Buffer {
        Buffer::new_sized(
            device,
            allocator,
            BufferUsage::from_flags(usage),
            MemoryLocation::CpuToGpu,
            (std::mem::size_of::<T>() * capacity) as u64,
        )
    }

    pub fn upload(&mut self, mesh: &GeometryPreviewMesh) -> Result<()> {
        ensure!(
            mesh.vertices.len() <= u32::MAX as usize,
            "geometry preview vertex count exceeds u32 index range"
        );
        ensure!(
            mesh.indices
                .iter()
                .all(|index| (*index as usize) < mesh.vertices.len()),
            "geometry preview contains an out-of-range index"
        );
        self.ensure_capacity(mesh.vertices.len(), mesh.indices.len())?;
        if !mesh.vertices.is_empty() {
            self.vertices.fill(&mesh.vertices)?;
            self.indices.fill(&mesh.indices)?;
        }
        self.indices_len = mesh.indices.len() as u32;
        if self.indices_len == 0 {
            self.instance_count = 0;
        }
        Ok(())
    }

    pub fn show(&mut self, base_position: Vec3, tint: Vec4) -> Result<()> {
        ensure!(base_position.is_finite(), "preview position must be finite");
        ensure!(tint.is_finite(), "preview tint must be finite");
        self.instances
            .fill(&[GeometryPreviewInstanceGpu::new(base_position, tint)])?;
        self.instance_count = u32::from(self.indices_len > 0);
        Ok(())
    }

    pub fn clear(&mut self) {
        self.instance_count = 0;
    }

    fn ensure_capacity(&mut self, vertex_count: usize, index_count: usize) -> Result<()> {
        if vertex_count > self.vertex_capacity {
            self.vertex_capacity = vertex_count
                .checked_next_power_of_two()
                .ok_or_else(|| anyhow!("geometry preview vertex capacity overflow"))?;
            *self.vertices = Self::new_buffer::<GeometryPreviewVertex>(
                self.device.clone(),
                self.allocator.clone(),
                vk::BufferUsageFlags::VERTEX_BUFFER,
                self.vertex_capacity,
            );
        }
        if index_count > self.index_capacity {
            self.index_capacity = index_count
                .checked_next_power_of_two()
                .ok_or_else(|| anyhow!("geometry preview index capacity overflow"))?;
            *self.indices = Self::new_buffer::<u32>(
                self.device.clone(),
                self.allocator.clone(),
                vk::BufferUsageFlags::INDEX_BUFFER,
                self.index_capacity,
            );
        }
        Ok(())
    }
}

pub struct GeometryPreviewRendererResources {
    pub debug: GeometryPreviewMeshResources,
    pub tree: GeometryPreviewMeshResources,
}

impl GeometryPreviewRendererResources {
    pub fn new(device: Device, allocator: Allocator) -> Self {
        Self {
            debug: GeometryPreviewMeshResources::new(device.clone(), allocator.clone()),
            tree: GeometryPreviewMeshResources::new(device, allocator),
        }
    }

    pub fn has_visible_mesh(&self) -> bool {
        self.debug.instance_count > 0 || self.tree.instance_count > 0
    }
}

pub(crate) fn append_box(mesh: &mut GeometryPreviewMesh, min: Vec3, max: Vec3, color_alpha: Vec4) {
    let faces = [
        [
            Vec3::new(max.x, min.y, min.z),
            Vec3::new(max.x, max.y, min.z),
            Vec3::new(max.x, max.y, max.z),
            Vec3::new(max.x, min.y, max.z),
        ],
        [
            Vec3::new(min.x, min.y, max.z),
            Vec3::new(min.x, max.y, max.z),
            Vec3::new(min.x, max.y, min.z),
            Vec3::new(min.x, min.y, min.z),
        ],
        [
            Vec3::new(min.x, max.y, min.z),
            Vec3::new(min.x, max.y, max.z),
            Vec3::new(max.x, max.y, max.z),
            Vec3::new(max.x, max.y, min.z),
        ],
        [
            Vec3::new(min.x, min.y, max.z),
            Vec3::new(min.x, min.y, min.z),
            Vec3::new(max.x, min.y, min.z),
            Vec3::new(max.x, min.y, max.z),
        ],
        [
            Vec3::new(max.x, min.y, max.z),
            Vec3::new(max.x, max.y, max.z),
            Vec3::new(min.x, max.y, max.z),
            Vec3::new(min.x, min.y, max.z),
        ],
        [
            Vec3::new(min.x, min.y, min.z),
            Vec3::new(min.x, max.y, min.z),
            Vec3::new(max.x, max.y, min.z),
            Vec3::new(max.x, min.y, min.z),
        ],
    ];
    for positions in faces {
        let base = mesh.vertices.len() as u32;
        mesh.vertices
            .extend(positions.map(|position| GeometryPreviewVertex::new(position, color_alpha)));
        mesh.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use super::*;

    #[test]
    fn preview_instance_layout_stays_compact() {
        assert_eq!(size_of::<GeometryPreviewInstanceGpu>(), 28);
    }
}

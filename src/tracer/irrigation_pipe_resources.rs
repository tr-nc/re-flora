use anyhow::{ensure, Result};
use glam::Vec3;
use re_flora_vkn::vk;
use re_flora_vkn::{Allocator, Buffer, BufferUsage, Device, MemoryLocation};

use crate::{
    resource::Resource,
    tracer::{SprinklerInstanceGpu, SprinklerVertex},
};

const PIPE_SEGMENT_CAPACITY: usize = 1_024;
const VERTICES_PER_BOX: usize = 24;
const INDICES_PER_BOX: usize = 36;
const PIPE_RADIUS: f32 = 1.5 / 256.0;
const SOURCE_HALF_EXTENT: f32 = 1.5 / 256.0;
const PIPE_COLOR_SRGB: Vec3 = Vec3::new(0.56, 0.59, 0.60);
const SOURCE_COLOR_SRGB: Vec3 = Vec3::new(0.34, 0.48, 0.52);

#[derive(Clone, Copy, Debug)]
pub struct IrrigationPipeRenderSegment {
    pub start: Vec3,
    pub end: Vec3,
}

#[derive(Clone, Debug, Default)]
pub struct IrrigationPipeRenderData {
    pub source_position: Option<Vec3>,
    pub segments: Vec<IrrigationPipeRenderSegment>,
}

pub struct IrrigationPipeRendererResources {
    pub vertices: Resource<Buffer>,
    pub indices: Resource<Buffer>,
    pub indices_len: u32,
    pub instances: Resource<Buffer>,
    pub instance_count: u32,
}

impl IrrigationPipeRendererResources {
    pub fn new(device: Device, allocator: Allocator) -> Self {
        let box_capacity = PIPE_SEGMENT_CAPACITY + 1;
        let vertices = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::VERTEX_BUFFER),
            MemoryLocation::CpuToGpu,
            (std::mem::size_of::<SprinklerVertex>() * box_capacity * VERTICES_PER_BOX) as u64,
        );
        let indices = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::INDEX_BUFFER),
            MemoryLocation::CpuToGpu,
            (std::mem::size_of::<u32>() * box_capacity * INDICES_PER_BOX) as u64,
        );
        let instances = Buffer::new_sized(
            device,
            allocator,
            BufferUsage::from_flags(vk::BufferUsageFlags::VERTEX_BUFFER),
            MemoryLocation::CpuToGpu,
            std::mem::size_of::<SprinklerInstanceGpu>() as u64,
        );
        instances
            .fill(&[SprinklerInstanceGpu::static_mode()])
            .expect("fill irrigation pipe draw instance");

        Self {
            vertices: Resource::new(vertices),
            indices: Resource::new(indices),
            indices_len: 0,
            instances: Resource::new(instances),
            instance_count: 0,
        }
    }

    pub fn upload(&mut self, data: &IrrigationPipeRenderData) -> Result<()> {
        ensure!(
            data.segments.len() <= PIPE_SEGMENT_CAPACITY,
            "irrigation pipe render capacity exceeded: {} > {} segments",
            data.segments.len(),
            PIPE_SEGMENT_CAPACITY,
        );
        let box_count = data.segments.len() + usize::from(data.source_position.is_some());
        let mut vertices = Vec::with_capacity(box_count * VERTICES_PER_BOX);
        let mut indices = Vec::with_capacity(box_count * INDICES_PER_BOX);

        if let Some(source) = data.source_position {
            append_box(
                &mut vertices,
                &mut indices,
                source - Vec3::splat(SOURCE_HALF_EXTENT),
                source + Vec3::splat(SOURCE_HALF_EXTENT),
                SOURCE_COLOR_SRGB,
            );
        }
        for segment in &data.segments {
            let (min, max) = pipe_segment_bounds(*segment);
            append_box(&mut vertices, &mut indices, min, max, PIPE_COLOR_SRGB);
        }

        if !vertices.is_empty() {
            self.vertices.fill(&vertices)?;
            self.indices.fill(&indices)?;
        }
        self.indices_len = indices.len() as u32;
        self.instance_count = u32::from(!indices.is_empty());
        Ok(())
    }
}

fn pipe_segment_bounds(segment: IrrigationPipeRenderSegment) -> (Vec3, Vec3) {
    (
        segment.start.min(segment.end) - Vec3::splat(PIPE_RADIUS),
        segment.start.max(segment.end) + Vec3::splat(PIPE_RADIUS),
    )
}

fn append_box(
    vertices: &mut Vec<SprinklerVertex>,
    indices: &mut Vec<u32>,
    min: Vec3,
    max: Vec3,
    color: Vec3,
) {
    let center = (min + max) * 0.5;
    let faces = [
        (
            Vec3::X,
            [
                Vec3::new(max.x, min.y, min.z),
                Vec3::new(max.x, max.y, min.z),
                Vec3::new(max.x, max.y, max.z),
                Vec3::new(max.x, min.y, max.z),
            ],
        ),
        (
            -Vec3::X,
            [
                Vec3::new(min.x, min.y, max.z),
                Vec3::new(min.x, max.y, max.z),
                Vec3::new(min.x, max.y, min.z),
                Vec3::new(min.x, min.y, min.z),
            ],
        ),
        (
            Vec3::Y,
            [
                Vec3::new(min.x, max.y, min.z),
                Vec3::new(min.x, max.y, max.z),
                Vec3::new(max.x, max.y, max.z),
                Vec3::new(max.x, max.y, min.z),
            ],
        ),
        (
            -Vec3::Y,
            [
                Vec3::new(min.x, min.y, max.z),
                Vec3::new(min.x, min.y, min.z),
                Vec3::new(max.x, min.y, min.z),
                Vec3::new(max.x, min.y, max.z),
            ],
        ),
        (
            Vec3::Z,
            [
                Vec3::new(max.x, min.y, max.z),
                Vec3::new(max.x, max.y, max.z),
                Vec3::new(min.x, max.y, max.z),
                Vec3::new(min.x, min.y, max.z),
            ],
        ),
        (
            -Vec3::Z,
            [
                Vec3::new(min.x, min.y, min.z),
                Vec3::new(min.x, max.y, min.z),
                Vec3::new(max.x, max.y, min.z),
                Vec3::new(max.x, min.y, min.z),
            ],
        ),
    ];

    for (normal, positions) in faces {
        let base = vertices.len() as u32;
        vertices.extend(
            positions
                .map(|position| SprinklerVertex::new(position, center, normal, color, Vec3::ZERO)),
        );
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipe_segment_is_rendered_as_one_metal_box() {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        append_box(
            &mut vertices,
            &mut indices,
            Vec3::ZERO,
            Vec3::new(1.0, 0.1, 0.1),
            PIPE_COLOR_SRGB,
        );

        assert_eq!(vertices.len(), VERTICES_PER_BOX);
        assert_eq!(indices.len(), INDICES_PER_BOX);
        assert!(vertices
            .iter()
            .all(|vertex| vertex.color_srgb == PIPE_COLOR_SRGB.to_array()));
    }

    #[test]
    fn pipe_segment_is_three_voxels_wide() {
        let (min, max) = pipe_segment_bounds(IrrigationPipeRenderSegment {
            start: Vec3::ZERO,
            end: Vec3::X,
        });

        let width_voxels = (max - min) * 256.0;
        assert_eq!(width_voxels.y, 3.0);
        assert_eq!(width_voxels.z, 3.0);
    }
}

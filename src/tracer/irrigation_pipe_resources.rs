use anyhow::{anyhow, ensure, Result};
use glam::{IVec3, Vec3};
use re_flora_vkn::vk;
use re_flora_vkn::{Allocator, Buffer, BufferUsage, Device, MemoryLocation};
use std::collections::BTreeMap;

use crate::{
    resource::Resource,
    tracer::{SprinklerInstanceGpu, SprinklerVertex},
};

const PIPE_SEGMENT_CAPACITY: usize = 1_024;
const VERTICES_PER_BOX: usize = 24;
const INDICES_PER_BOX: usize = 36;
const INITIAL_PIPE_VOXEL_CAPACITY: usize = PIPE_SEGMENT_CAPACITY + 1;
const VOXEL_SCALE: f32 = 1.0 / 256.0;
const VOXEL_HALF_EXTENT: f32 = VOXEL_SCALE * 0.5;
const PIPE_CROSS_SECTION_RADIUS_VOXELS: i32 = 1;
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
    device: Device,
    allocator: Allocator,
    voxel_capacity: usize,
    pub vertices: Resource<Buffer>,
    pub indices: Resource<Buffer>,
    pub indices_len: u32,
    pub instances: Resource<Buffer>,
    pub instance_count: u32,
}

impl IrrigationPipeRendererResources {
    pub fn new(device: Device, allocator: Allocator) -> Self {
        let vertices = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::VERTEX_BUFFER),
            MemoryLocation::CpuToGpu,
            (std::mem::size_of::<SprinklerVertex>()
                * INITIAL_PIPE_VOXEL_CAPACITY
                * VERTICES_PER_BOX) as u64,
        );
        let indices = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::INDEX_BUFFER),
            MemoryLocation::CpuToGpu,
            (std::mem::size_of::<u32>() * INITIAL_PIPE_VOXEL_CAPACITY * INDICES_PER_BOX) as u64,
        );
        let instances = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::VERTEX_BUFFER),
            MemoryLocation::CpuToGpu,
            std::mem::size_of::<SprinklerInstanceGpu>() as u64,
        );
        instances
            .fill(&[SprinklerInstanceGpu::static_mode()])
            .expect("fill irrigation pipe draw instance");

        Self {
            device,
            allocator,
            voxel_capacity: INITIAL_PIPE_VOXEL_CAPACITY,
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
        let (vertices, indices, voxel_count) = build_pipe_voxel_mesh(data)?;
        self.ensure_voxel_capacity(voxel_count)?;

        if !vertices.is_empty() {
            self.vertices.fill(&vertices)?;
            self.indices.fill(&indices)?;
        }
        self.indices_len = indices.len() as u32;
        self.instance_count = u32::from(!indices.is_empty());
        Ok(())
    }

    fn ensure_voxel_capacity(&mut self, required_capacity: usize) -> Result<()> {
        if required_capacity <= self.voxel_capacity {
            return Ok(());
        }

        let new_capacity = required_capacity
            .checked_next_power_of_two()
            .ok_or_else(|| anyhow!("irrigation pipe voxel capacity overflow"))?;
        *self.vertices = Buffer::new_sized(
            self.device.clone(),
            self.allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::VERTEX_BUFFER),
            MemoryLocation::CpuToGpu,
            (std::mem::size_of::<SprinklerVertex>() * new_capacity * VERTICES_PER_BOX) as u64,
        );
        *self.indices = Buffer::new_sized(
            self.device.clone(),
            self.allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::INDEX_BUFFER),
            MemoryLocation::CpuToGpu,
            (std::mem::size_of::<u32>() * new_capacity * INDICES_PER_BOX) as u64,
        );
        self.voxel_capacity = new_capacity;
        Ok(())
    }
}

fn build_pipe_voxel_mesh(
    data: &IrrigationPipeRenderData,
) -> Result<(Vec<SprinklerVertex>, Vec<u32>, usize)> {
    let voxels = collect_pipe_voxels(data)?;
    ensure!(
        voxels.len() <= u32::MAX as usize / VERTICES_PER_BOX,
        "irrigation pipe voxel mesh exceeds u32 index range"
    );
    let mut vertices = Vec::with_capacity(voxels.len() * VERTICES_PER_BOX);
    let mut indices = Vec::with_capacity(voxels.len() * INDICES_PER_BOX);
    for ((x, y, z), color) in &voxels {
        let center = IVec3::new(*x, *y, *z).as_vec3() * VOXEL_SCALE;
        append_box(
            &mut vertices,
            &mut indices,
            center - Vec3::splat(VOXEL_HALF_EXTENT),
            center + Vec3::splat(VOXEL_HALF_EXTENT),
            *color,
        );
    }
    Ok((vertices, indices, voxels.len()))
}

fn collect_pipe_voxels(data: &IrrigationPipeRenderData) -> Result<BTreeMap<(i32, i32, i32), Vec3>> {
    let mut voxels = BTreeMap::new();

    if let Some(source) = data.source_position {
        let source = world_position_to_voxel(source)?;
        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    insert_voxel(&mut voxels, source + IVec3::new(x, y, z), SOURCE_COLOR_SRGB);
                }
            }
        }
    }

    for segment in &data.segments {
        let start = world_position_to_voxel(segment.start)?;
        let end = world_position_to_voxel(segment.end)?;
        let delta = end - start;
        let changed_axes =
            i32::from(delta.x != 0) + i32::from(delta.y != 0) + i32::from(delta.z != 0);
        ensure!(
            changed_axes == 1,
            "irrigation pipe render segment must be non-empty and axis aligned: {:?} -> {:?}",
            start,
            end
        );

        let direction = delta.signum();
        let length = delta.abs().max_element();
        for step in 0..=length {
            let centerline = start + direction * step;
            for first in -PIPE_CROSS_SECTION_RADIUS_VOXELS..=PIPE_CROSS_SECTION_RADIUS_VOXELS {
                for second in -PIPE_CROSS_SECTION_RADIUS_VOXELS..=PIPE_CROSS_SECTION_RADIUS_VOXELS {
                    let offset = if delta.x != 0 {
                        IVec3::new(0, first, second)
                    } else if delta.y != 0 {
                        IVec3::new(first, 0, second)
                    } else {
                        IVec3::new(first, second, 0)
                    };
                    insert_voxel(&mut voxels, centerline + offset, PIPE_COLOR_SRGB);
                }
            }
        }
    }

    Ok(voxels)
}

fn world_position_to_voxel(position: Vec3) -> Result<IVec3> {
    ensure!(
        position.is_finite(),
        "irrigation pipe position must be finite"
    );
    let position_voxels = position / VOXEL_SCALE;
    let rounded = position_voxels.round();
    ensure!(
        (position_voxels - rounded).abs().max_element() <= 1.0e-4,
        "irrigation pipe position must lie on the voxel grid: {:?}",
        position
    );
    Ok(rounded.as_ivec3())
}

fn insert_voxel(voxels: &mut BTreeMap<(i32, i32, i32), Vec3>, position: IVec3, color: Vec3) {
    voxels
        .entry((position.x, position.y, position.z))
        .or_insert(color);
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
    use std::collections::BTreeSet;

    fn x_axis_pipe(length_voxels: i32) -> IrrigationPipeRenderData {
        IrrigationPipeRenderData {
            source_position: None,
            segments: vec![IrrigationPipeRenderSegment {
                start: Vec3::ZERO,
                end: Vec3::X * length_voxels as f32 * VOXEL_SCALE,
            }],
        }
    }

    #[test]
    fn unit_pipe_voxel_is_rendered_as_one_metal_box() {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        append_box(
            &mut vertices,
            &mut indices,
            Vec3::ZERO,
            Vec3::splat(VOXEL_SCALE),
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
        let voxels = collect_pipe_voxels(&x_axis_pipe(2)).unwrap();
        let min_y = voxels.keys().map(|(_, y, _)| *y).min().unwrap();
        let max_y = voxels.keys().map(|(_, y, _)| *y).max().unwrap();
        let min_z = voxels.keys().map(|(_, _, z)| *z).min().unwrap();
        let max_z = voxels.keys().map(|(_, _, z)| *z).max().unwrap();

        assert_eq!(max_y - min_y + 1, 3);
        assert_eq!(max_z - min_z + 1, 3);
    }

    #[test]
    fn pipe_mesh_uses_unit_voxels_with_independent_light_samples() {
        let (vertices, indices, voxel_count) = build_pipe_voxel_mesh(&x_axis_pipe(2)).unwrap();

        assert_eq!(voxel_count, 3 * 3 * 3);
        assert_eq!(vertices.len(), voxel_count * VERTICES_PER_BOX);
        assert_eq!(indices.len(), voxel_count * INDICES_PER_BOX);

        let mut light_sample_centers = BTreeSet::new();
        for voxel in vertices.chunks_exact(VERTICES_PER_BOX) {
            let center = voxel[0].voxel_center;
            assert!(voxel.iter().all(|vertex| vertex.voxel_center == center));
            light_sample_centers.insert((
                center[0].to_bits(),
                center[1].to_bits(),
                center[2].to_bits(),
            ));

            for axis in 0..3 {
                let min = voxel
                    .iter()
                    .map(|vertex| vertex.position[axis])
                    .fold(f32::INFINITY, f32::min);
                let max = voxel
                    .iter()
                    .map(|vertex| vertex.position[axis])
                    .fold(f32::NEG_INFINITY, f32::max);
                assert!(((max - min) - VOXEL_SCALE).abs() < 1.0e-6);
            }
        }

        assert_eq!(light_sample_centers.len(), voxel_count);
    }

    #[test]
    fn source_uses_27_unit_voxels_and_pipe_overlap_is_deduplicated() {
        let mut data = x_axis_pipe(2);
        data.source_position = Some(Vec3::ZERO);
        let voxels = collect_pipe_voxels(&data).unwrap();

        assert_eq!(voxels.len(), 36);
        assert_eq!(
            voxels
                .values()
                .filter(|color| **color == SOURCE_COLOR_SRGB)
                .count(),
            27
        );
        assert_eq!(
            voxels
                .values()
                .filter(|color| **color == PIPE_COLOR_SRGB)
                .count(),
            9
        );
    }

    #[test]
    fn non_axis_aligned_pipe_segment_is_rejected() {
        let data = IrrigationPipeRenderData {
            source_position: None,
            segments: vec![IrrigationPipeRenderSegment {
                start: Vec3::ZERO,
                end: Vec3::new(1.0, 1.0, 0.0) * VOXEL_SCALE,
            }],
        };

        assert!(collect_pipe_voxels(&data).is_err());
    }
}

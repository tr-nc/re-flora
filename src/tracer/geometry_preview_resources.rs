use anyhow::{anyhow, ensure, Result};
use bytemuck::{Pod, Zeroable};
use glam::{Quat, Vec3, Vec4};
use re_flora_vkn::vk;
use re_flora_vkn::{Allocator, Buffer, BufferUsage, Device, MemoryLocation};

use crate::{resource::Resource, util::stable_perpendicular_basis};

use super::{
    IrrigationPipeRenderData, IrrigationPipeRenderSegment, IRRIGATION_PIPE_END_CAP_VOXELS,
    IRRIGATION_PIPE_RADIUS_VOXELS,
};

const VOXEL_SCALE: f32 = 1.0 / 256.0;
const PIPE_RADIUS: f32 = IRRIGATION_PIPE_RADIUS_VOXELS * VOXEL_SCALE;
const PIPE_END_CAP: f32 = IRRIGATION_PIPE_END_CAP_VOXELS * VOXEL_SCALE;
const PIPE_PREVIEW_SIDE_COUNT: u32 = 12;
const PIPE_PREVIEW_COLOR: Vec4 = Vec4::new(0.08, 0.62, 1.0, 0.38);
const PIPE_SOURCE_PREVIEW_COLOR: Vec4 = Vec4::new(0.12, 0.82, 1.0, 0.46);
const PROBE_APPLE_BOTTOM_COLOR: Vec4 = Vec4::new(0.48, 0.025, 0.018, 1.0);
const PROBE_APPLE_TOP_COLOR: Vec4 = Vec4::new(0.95, 0.06, 0.035, 1.0);

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
    rotation: [f32; 4],
}

impl GeometryPreviewInstanceGpu {
    fn new(base_position: Vec3, tint: Vec4, rotation: Quat) -> Self {
        Self {
            base_position: base_position.to_array(),
            tint: tint.to_array(),
            rotation: rotation.to_array(),
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
        self.show_transform(base_position, Quat::IDENTITY, tint)
    }

    pub fn show_transform(
        &mut self,
        base_position: Vec3,
        rotation: Quat,
        tint: Vec4,
    ) -> Result<()> {
        ensure!(base_position.is_finite(), "preview position must be finite");
        ensure!(rotation.is_finite(), "preview rotation must be finite");
        ensure!(
            rotation.length_squared() > f32::EPSILON,
            "preview rotation must have non-zero length"
        );
        ensure!(tint.is_finite(), "preview tint must be finite");
        let rotation = rotation.normalize();
        self.instances.fill(&[GeometryPreviewInstanceGpu::new(
            base_position,
            tint,
            rotation,
        )])?;
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
    pub pipe: GeometryPreviewMeshResources,
    pub tree: GeometryPreviewMeshResources,
    pub collision_probe: GeometryPreviewMeshResources,
}

impl GeometryPreviewRendererResources {
    pub fn new(device: Device, allocator: Allocator) -> Self {
        Self {
            pipe: GeometryPreviewMeshResources::new(device.clone(), allocator.clone()),
            tree: GeometryPreviewMeshResources::new(device.clone(), allocator.clone()),
            collision_probe: GeometryPreviewMeshResources::new(device, allocator),
        }
    }

    pub fn has_visible_mesh(&self) -> bool {
        self.pipe.instance_count > 0
            || self.tree.instance_count > 0
            || self.collision_probe.instance_count > 0
    }
}

pub fn build_collision_probe_apple_mesh() -> GeometryPreviewMesh {
    let mut mesh = GeometryPreviewMesh::default();
    for voxel in super::voxel_apple_offsets() {
        let color_t = ((voxel.y + 2) as f32 / 3.0).clamp(0.0, 1.0);
        let color = PROBE_APPLE_BOTTOM_COLOR.lerp(PROBE_APPLE_TOP_COLOR, color_t);
        let min = voxel.as_vec3() * VOXEL_SCALE;
        append_box(&mut mesh, min, min + Vec3::splat(VOXEL_SCALE), color);
    }
    mesh
}

pub fn build_pipe_preview_mesh(data: &IrrigationPipeRenderData) -> Result<GeometryPreviewMesh> {
    let mut mesh = GeometryPreviewMesh::default();
    if let Some(source) = data.source_position {
        ensure!(source.is_finite(), "pipe preview source must be finite");
        append_box(
            &mut mesh,
            source - Vec3::splat(PIPE_RADIUS),
            source + Vec3::splat(PIPE_RADIUS),
            PIPE_SOURCE_PREVIEW_COLOR,
        );
    }
    for segment in &data.segments {
        append_pipe_segment(&mut mesh, *segment)?;
    }
    Ok(mesh)
}

fn append_pipe_segment(
    mesh: &mut GeometryPreviewMesh,
    segment: IrrigationPipeRenderSegment,
) -> Result<()> {
    ensure!(
        segment.start.is_finite() && segment.end.is_finite(),
        "pipe preview segment position must be finite"
    );
    let delta = segment.end - segment.start;
    let changed_axes = u32::from(delta.x.abs() > f32::EPSILON)
        + u32::from(delta.y.abs() > f32::EPSILON)
        + u32::from(delta.z.abs() > f32::EPSILON);
    ensure!(
        changed_axes == 1,
        "pipe preview segment must be non-empty and axis aligned: {:?} -> {:?}",
        segment.start,
        segment.end
    );

    let direction = delta.normalize();
    append_cylinder(
        mesh,
        segment.start - direction * PIPE_END_CAP,
        segment.end + direction * PIPE_END_CAP,
        PIPE_RADIUS,
        PIPE_PREVIEW_COLOR,
    );
    Ok(())
}

fn append_cylinder(
    mesh: &mut GeometryPreviewMesh,
    start: Vec3,
    end: Vec3,
    radius: f32,
    color_alpha: Vec4,
) {
    let direction = (end - start).normalize();
    let (tangent, bitangent) = stable_perpendicular_basis(direction);
    let start_base = mesh.vertices.len() as u32;
    for side in 0..PIPE_PREVIEW_SIDE_COUNT {
        let angle = std::f32::consts::TAU * side as f32 / PIPE_PREVIEW_SIDE_COUNT as f32;
        let radial = tangent * angle.cos() + bitangent * angle.sin();
        mesh.vertices.push(GeometryPreviewVertex::new(
            start + radial * radius,
            color_alpha,
        ));
    }
    let end_base = mesh.vertices.len() as u32;
    for side in 0..PIPE_PREVIEW_SIDE_COUNT {
        let angle = std::f32::consts::TAU * side as f32 / PIPE_PREVIEW_SIDE_COUNT as f32;
        let radial = tangent * angle.cos() + bitangent * angle.sin();
        mesh.vertices.push(GeometryPreviewVertex::new(
            end + radial * radius,
            color_alpha,
        ));
    }
    let start_center = mesh.vertices.len() as u32;
    mesh.vertices
        .push(GeometryPreviewVertex::new(start, color_alpha));
    let end_center = mesh.vertices.len() as u32;
    mesh.vertices
        .push(GeometryPreviewVertex::new(end, color_alpha));

    for side in 0..PIPE_PREVIEW_SIDE_COUNT {
        let next = (side + 1) % PIPE_PREVIEW_SIDE_COUNT;
        let start_current = start_base + side;
        let start_next = start_base + next;
        let end_current = end_base + side;
        let end_next = end_base + next;
        mesh.indices.extend([
            start_current,
            end_next,
            end_current,
            start_current,
            start_next,
            end_next,
            start_center,
            start_next,
            start_current,
            end_center,
            end_current,
            end_next,
        ]);
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
        assert_eq!(size_of::<GeometryPreviewInstanceGpu>(), 44);
    }

    #[test]
    fn collision_probe_mesh_matches_shared_apple_description() {
        let mesh = build_collision_probe_apple_mesh();
        assert_eq!(mesh.vertices.len(), 32 * 24);
        assert_eq!(mesh.indices.len(), 32 * 36);

        let positions = mesh
            .vertices
            .iter()
            .map(|vertex| Vec3::from_array(vertex.position))
            .collect::<Vec<_>>();
        let min = positions.iter().copied().reduce(Vec3::min).unwrap();
        let max = positions.iter().copied().reduce(Vec3::max).unwrap();
        assert_eq!(min, Vec3::splat(-2.0 * VOXEL_SCALE));
        assert_eq!(max, Vec3::splat(2.0 * VOXEL_SCALE));
    }

    #[test]
    fn pipe_preview_uses_one_cylinder_per_route_segment() {
        let data = IrrigationPipeRenderData {
            source_position: Some(Vec3::ZERO),
            segments: vec![
                IrrigationPipeRenderSegment {
                    start: Vec3::ZERO,
                    end: Vec3::X,
                },
                IrrigationPipeRenderSegment {
                    start: Vec3::X,
                    end: Vec3::X + Vec3::Z,
                },
            ],
        };
        let mesh = build_pipe_preview_mesh(&data).unwrap();

        let cylinder_vertex_count = (PIPE_PREVIEW_SIDE_COUNT * 2 + 2) as usize;
        let cylinder_index_count = (PIPE_PREVIEW_SIDE_COUNT * 12) as usize;
        assert_eq!(mesh.vertices.len(), 24 + 2 * cylinder_vertex_count);
        assert_eq!(mesh.indices.len(), 36 + 2 * cylinder_index_count);
    }

    #[test]
    fn pipe_preview_preserves_three_voxel_cross_section() {
        let mesh = build_pipe_preview_mesh(&IrrigationPipeRenderData {
            source_position: None,
            segments: vec![IrrigationPipeRenderSegment {
                start: Vec3::ZERO,
                end: Vec3::X,
            }],
        })
        .unwrap();
        let positions = mesh
            .vertices
            .iter()
            .map(|vertex| Vec3::from_array(vertex.position))
            .collect::<Vec<_>>();
        let min = positions.iter().copied().reduce(Vec3::min).unwrap();
        let max = positions.iter().copied().reduce(Vec3::max).unwrap();

        assert!((max.y - min.y - 3.0 * VOXEL_SCALE).abs() < 1.0e-6);
        assert!((max.z - min.z - 3.0 * VOXEL_SCALE).abs() < 1.0e-6);
    }

    #[test]
    fn pipe_preview_cross_section_vertices_lie_on_a_circle() {
        let mesh = build_pipe_preview_mesh(&IrrigationPipeRenderData {
            source_position: None,
            segments: vec![IrrigationPipeRenderSegment {
                start: Vec3::ZERO,
                end: Vec3::X,
            }],
        })
        .unwrap();

        for vertex in mesh
            .vertices
            .iter()
            .take((PIPE_PREVIEW_SIDE_COUNT * 2) as usize)
        {
            let position = Vec3::from_array(vertex.position);
            let radial_distance = Vec3::new(0.0, position.y, position.z).length();
            assert!((radial_distance - PIPE_RADIUS).abs() < 1.0e-6);
        }
    }
}

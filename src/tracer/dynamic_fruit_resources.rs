use anyhow::{ensure, Result};
use bytemuck::{Pod, Zeroable};
use glam::{Quat, Vec3, Vec4};
use re_flora_vkn::vk;
use re_flora_vkn::{Allocator, Buffer, BufferUsage, Device, MemoryLocation};

use crate::{
    resource::Resource,
    tracer::voxel_geometry::{CUBE_INDICES, VOXEL_VERTICES},
};

const VOXEL_SCALE: f32 = 1.0 / 256.0;
const APPLE_BOTTOM_COLOR_SRGB: Vec3 = Vec3::new(0.48, 0.025, 0.018);
const APPLE_TOP_COLOR_SRGB: Vec3 = Vec3::new(0.95, 0.06, 0.035);

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct DynamicFruitVertex {
    position: [f32; 3],
    voxel_center: [f32; 3],
    shading_normal: [f32; 3],
    color_srgb: [f32; 3],
}

impl DynamicFruitVertex {
    fn new(position: Vec3, voxel_center: Vec3, shading_normal: Vec3, color_srgb: Vec3) -> Self {
        Self {
            position: position.to_array(),
            voxel_center: voxel_center.to_array(),
            shading_normal: shading_normal.to_array(),
            color_srgb: color_srgb.to_array(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct DynamicFruitInstanceGpu {
    base_position: [f32; 3],
    tint: [f32; 4],
    rotation: [f32; 4],
}

impl DynamicFruitInstanceGpu {
    fn new(base_position: Vec3, tint: Vec4, rotation: Quat) -> Self {
        Self {
            base_position: base_position.to_array(),
            tint: tint.to_array(),
            rotation: rotation.to_array(),
        }
    }
}

pub struct DynamicFruitRendererResources {
    pub vertices: Resource<Buffer>,
    pub indices: Resource<Buffer>,
    pub indices_len: u32,
    pub instances: Resource<Buffer>,
    pub instance_count: u32,
    last_transform: Option<(Vec3, Quat)>,
    shadow_changed: bool,
}

impl DynamicFruitRendererResources {
    pub fn new(device: Device, allocator: Allocator) -> Self {
        let (vertices_data, indices_data) = build_collision_probe_apple_mesh();
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
            std::mem::size_of::<DynamicFruitInstanceGpu>() as u64,
        );

        Self {
            vertices: Resource::new(vertices),
            indices: Resource::new(indices),
            indices_len: indices_data.len() as u32,
            instances: Resource::new(instances),
            instance_count: 0,
            last_transform: None,
            shadow_changed: false,
        }
    }

    pub fn show(&mut self, position: Vec3, rotation: Quat) -> Result<()> {
        ensure!(
            position.is_finite(),
            "dynamic fruit position must be finite"
        );
        ensure!(
            rotation.is_finite(),
            "dynamic fruit rotation must be finite"
        );
        ensure!(
            rotation.length_squared() > f32::EPSILON,
            "dynamic fruit rotation must have non-zero length"
        );
        let rotation = rotation.normalize();
        self.shadow_changed |= transform_changed(self.last_transform, position, rotation);
        self.last_transform = Some((position, rotation));
        self.instances
            .fill(&[DynamicFruitInstanceGpu::new(position, Vec4::ONE, rotation)])?;
        self.instance_count = 1;
        Ok(())
    }

    pub fn clear(&mut self) {
        self.shadow_changed |= self.instance_count > 0;
        self.instance_count = 0;
        self.last_transform = None;
    }

    pub fn take_shadow_changed(&mut self) -> bool {
        std::mem::take(&mut self.shadow_changed)
    }
}

fn transform_changed(last: Option<(Vec3, Quat)>, position: Vec3, rotation: Quat) -> bool {
    const POSITION_EPSILON_SQUARED: f32 = 1.0e-10;
    const ROTATION_DOT_EPSILON: f32 = 1.0e-7;
    let Some((last_position, last_rotation)) = last else {
        return true;
    };
    position.distance_squared(last_position) > POSITION_EPSILON_SQUARED
        || 1.0 - rotation.dot(last_rotation).abs() > ROTATION_DOT_EPSILON
}

fn build_collision_probe_apple_mesh() -> (Vec<DynamicFruitVertex>, Vec<u32>) {
    let offsets = super::collision_probe_apple_offsets();
    let mut vertices = Vec::with_capacity(offsets.len() * VOXEL_VERTICES.len());
    let mut indices = Vec::with_capacity(offsets.len() * CUBE_INDICES.len());
    for voxel in offsets {
        let voxel_min = voxel.as_vec3();
        let voxel_center_voxels = voxel_min + Vec3::splat(0.5);
        let voxel_center = voxel_center_voxels * VOXEL_SCALE;
        let shading_normal = voxel_center_voxels.normalize_or_zero();
        let color_t = ((voxel.y + 4) as f32 / 7.0).clamp(0.0, 1.0);
        let color = APPLE_BOTTOM_COLOR_SRGB.lerp(APPLE_TOP_COLOR_SRGB, color_t);
        let base = vertices.len() as u32;
        vertices.extend(VOXEL_VERTICES.map(|offset| {
            DynamicFruitVertex::new(
                (voxel_min + offset.as_vec3()) * VOXEL_SCALE,
                voxel_center,
                shading_normal,
                color,
            )
        }));
        indices.extend(CUBE_INDICES.map(|index| base + index));
    }
    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_fruit_layout_matches_shader_locations() {
        assert_eq!(std::mem::size_of::<DynamicFruitVertex>(), 12 * 4);
        assert_eq!(std::mem::size_of::<DynamicFruitInstanceGpu>(), 11 * 4);
    }

    #[test]
    fn collision_probe_mesh_uses_shared_unit_voxel_description() {
        let expected_voxels = super::super::collision_probe_apple_offsets().len();
        let (vertices, indices) = build_collision_probe_apple_mesh();
        assert_eq!(vertices.len(), expected_voxels * VOXEL_VERTICES.len());
        assert_eq!(indices.len(), expected_voxels * CUBE_INDICES.len());

        let min = vertices
            .iter()
            .map(|vertex| Vec3::from_array(vertex.position))
            .reduce(Vec3::min)
            .unwrap();
        let max = vertices
            .iter()
            .map(|vertex| Vec3::from_array(vertex.position))
            .reduce(Vec3::max)
            .unwrap();
        assert_eq!(min, Vec3::splat(-4.0 * VOXEL_SCALE));
        assert_eq!(max, Vec3::splat(4.0 * VOXEL_SCALE));
    }

    #[test]
    fn shadow_history_reset_only_tracks_visible_transform_changes() {
        let position = Vec3::new(1.0, 2.0, 3.0);
        let rotation = Quat::from_rotation_y(0.4);
        assert!(transform_changed(None, position, rotation));
        assert!(!transform_changed(
            Some((position, rotation)),
            position,
            -rotation
        ));
        assert!(transform_changed(
            Some((position, rotation)),
            position + Vec3::splat(1.0e-3),
            rotation
        ));
        assert!(transform_changed(
            Some((position, rotation)),
            position,
            Quat::from_rotation_y(0.5)
        ));
    }

    #[test]
    fn rust_buffers_match_both_dynamic_fruit_shader_inputs() {
        let color_shader = include_str!("../../shader/slang/dynamic_fruit.vert.slang");
        let shadow_shader = include_str!("../../shader/slang/dynamic_fruit_shadow.vert.slang");
        for shader in [color_shader, shadow_shader] {
            for declaration in [
                "[[vk::location(0)]] float3 position",
                "[[vk::location(1)]] float3 voxel_center",
                "[[vk::location(2)]] float3 shading_normal",
                "[[vk::location(3)]] float3 color_srgb",
                "[[vk::location(4)]] float3 base_position",
                "[[vk::location(5)]] float4 tint",
                "[[vk::location(6)]] float4 rotation",
            ] {
                assert!(shader.contains(declaration), "missing `{declaration}`");
            }
        }
        assert!(color_shader.contains("rotateByQuaternion(input.position"));
        assert!(color_shader.contains("rotateByQuaternion(input.voxel_center"));
        assert!(color_shader.contains("input.shading_normal, input.rotation"));
        assert!(shadow_shader.contains("rotateByQuaternion(input.position"));
    }
}

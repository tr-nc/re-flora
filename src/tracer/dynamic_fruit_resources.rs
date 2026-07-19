use anyhow::{anyhow, ensure, Result};
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
    fn new(instance: DynamicFruitRenderInstance) -> Self {
        Self {
            base_position: instance.position.to_array(),
            tint: Vec4::new(1.0, 1.0, 1.0, instance.scale).to_array(),
            rotation: instance.rotation.to_array(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DynamicFruitRenderInstance {
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: f32,
}

impl DynamicFruitRenderInstance {
    pub fn new(position: Vec3, rotation: Quat, scale: f32) -> Self {
        Self {
            position,
            rotation,
            scale,
        }
    }
}

pub struct DynamicFruitRendererResources {
    device: Device,
    allocator: Allocator,
    instance_capacity: usize,
    pub vertices: Resource<Buffer>,
    pub indices: Resource<Buffer>,
    pub indices_len: u32,
    pub instances: Resource<Buffer>,
    pub instance_count: u32,
    last_instances: Vec<DynamicFruitRenderInstance>,
    shadow_changed: bool,
}

impl DynamicFruitRendererResources {
    pub fn new(device: Device, allocator: Allocator) -> Self {
        let (vertices_data, indices_data) = build_dynamic_apple_mesh();
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
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::VERTEX_BUFFER),
            MemoryLocation::CpuToGpu,
            std::mem::size_of::<DynamicFruitInstanceGpu>() as u64,
        );

        Self {
            device,
            allocator,
            instance_capacity: 1,
            vertices: Resource::new(vertices),
            indices: Resource::new(indices),
            indices_len: indices_data.len() as u32,
            instances: Resource::new(instances),
            instance_count: 0,
            last_instances: Vec::new(),
            shadow_changed: false,
        }
    }

    pub fn show(&mut self, instances: &[DynamicFruitRenderInstance]) -> Result<()> {
        ensure!(
            instances.len() <= u32::MAX as usize,
            "dynamic fruit instance count exceeds u32 draw range"
        );
        let mut normalized = Vec::with_capacity(instances.len());
        for instance in instances {
            ensure!(
                instance.position.is_finite(),
                "dynamic fruit position must be finite"
            );
            ensure!(
                instance.rotation.is_finite(),
                "dynamic fruit rotation must be finite"
            );
            ensure!(
                instance.rotation.length_squared() > f32::EPSILON,
                "dynamic fruit rotation must have non-zero length"
            );
            ensure!(
                instance.scale.is_finite() && instance.scale > 0.0,
                "dynamic fruit scale must be finite and positive"
            );
            normalized.push(DynamicFruitRenderInstance {
                rotation: instance.rotation.normalize(),
                ..*instance
            });
        }

        self.ensure_instance_capacity(normalized.len())?;
        self.shadow_changed |= instances_changed(&self.last_instances, &normalized);
        self.last_instances.clone_from(&normalized);
        if !normalized.is_empty() {
            let gpu_instances = normalized
                .iter()
                .copied()
                .map(DynamicFruitInstanceGpu::new)
                .collect::<Vec<_>>();
            self.instances.fill(&gpu_instances)?;
        }
        self.instance_count = normalized.len() as u32;
        Ok(())
    }

    pub fn clear(&mut self) {
        self.shadow_changed |= self.instance_count > 0;
        self.instance_count = 0;
        self.last_instances.clear();
    }

    pub fn take_shadow_changed(&mut self) -> bool {
        std::mem::take(&mut self.shadow_changed)
    }

    fn ensure_instance_capacity(&mut self, required: usize) -> Result<()> {
        let required = required.max(1);
        if required <= self.instance_capacity {
            return Ok(());
        }
        self.instance_capacity = required
            .checked_next_power_of_two()
            .ok_or_else(|| anyhow!("dynamic fruit instance capacity overflow"))?;
        *self.instances = Buffer::new_sized(
            self.device.clone(),
            self.allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::VERTEX_BUFFER),
            MemoryLocation::CpuToGpu,
            (std::mem::size_of::<DynamicFruitInstanceGpu>() * self.instance_capacity) as u64,
        );
        Ok(())
    }
}

fn instances_changed(
    last: &[DynamicFruitRenderInstance],
    current: &[DynamicFruitRenderInstance],
) -> bool {
    const POSITION_EPSILON_SQUARED: f32 = 1.0e-10;
    const ROTATION_DOT_EPSILON: f32 = 1.0e-7;
    const SCALE_EPSILON: f32 = 1.0e-6;
    if last.len() != current.len() {
        return true;
    }
    last.iter().zip(current).any(|(last, current)| {
        last.position.distance_squared(current.position) > POSITION_EPSILON_SQUARED
            || 1.0 - current.rotation.dot(last.rotation).abs() > ROTATION_DOT_EPSILON
            || (current.scale - last.scale).abs() > SCALE_EPSILON
    })
}

fn build_dynamic_apple_mesh() -> (Vec<DynamicFruitVertex>, Vec<u32>) {
    let offsets = super::voxel_apple_offsets();
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
    fn dynamic_mesh_uses_the_regular_attached_apple_description() {
        let expected_voxels = super::super::voxel_apple_offsets().len();
        let (vertices, indices) = build_dynamic_apple_mesh();
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
        assert_eq!(min, Vec3::splat(-2.0 * VOXEL_SCALE));
        assert_eq!(max, Vec3::splat(2.0 * VOXEL_SCALE));
    }

    #[test]
    fn shadow_history_reset_only_tracks_visible_instance_changes() {
        let position = Vec3::new(1.0, 2.0, 3.0);
        let rotation = Quat::from_rotation_y(0.4);
        let instance = DynamicFruitRenderInstance::new(position, rotation, 1.0);
        assert!(instances_changed(&[], &[instance]));
        assert!(!instances_changed(
            &[instance],
            &[DynamicFruitRenderInstance::new(position, -rotation, 1.0)]
        ));
        assert!(instances_changed(
            &[instance],
            &[DynamicFruitRenderInstance::new(
                position + Vec3::splat(1.0e-3),
                rotation,
                1.0,
            )]
        ));
        assert!(instances_changed(
            &[instance],
            &[DynamicFruitRenderInstance::new(
                position,
                Quat::from_rotation_y(0.5),
                1.0,
            )]
        ));
        assert!(instances_changed(
            &[instance],
            &[DynamicFruitRenderInstance::new(position, rotation, 0.5)]
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
        assert!(color_shader.contains("input.position * input.tint.a"));
        assert!(color_shader.contains("rotateByQuaternion(input.voxel_center"));
        assert!(color_shader.contains("input.shading_normal, input.rotation"));
        assert!(shadow_shader.contains("rotateByQuaternion(input.position"));
        assert!(shadow_shader.contains("input.position * input.tint.a"));
    }
}

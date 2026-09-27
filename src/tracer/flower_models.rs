//! GPU geometry for the shared authored flower bank. Tiles and ready-frame
//! lifetimes remain ModelPixelFrame-owned, just like apples and particles.
use crate::{flora::models, resource::Resource};
use bytemuck::{Pod, Zeroable};
use re_flora_vkn::{vk, Allocator, Buffer, BufferUsage, Device, MemoryLocation};
use resource_container_derive::ResourceContainer;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuTriangle {
    a: [f32; 4],
    e1: [f32; 4],
    e2: [f32; 4],
    normals: [[f32; 4]; 3],
    color: [f32; 4],
    unused: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuPart {
    range: [u32; 4],
    center_radius: [f32; 4],
}

fn geometry() -> (Vec<GpuTriangle>, Vec<GpuPart>, u32) {
    let mut triangles = Vec::new();
    let mut parts = Vec::new();
    let mut max_stem_vertices = 0;
    for (model, flower) in models::flowers().iter().enumerate() {
        let first = triangles.len() as u32;
        for triangle in &flower.triangles {
            let [a, b, c] = triangle.positions;
            triangles.push(GpuTriangle {
                a: a.extend(0.).to_array(),
                e1: (b - a).extend(0.).to_array(),
                e2: (c - a).extend(0.).to_array(),
                normals: [triangle.normal.extend(0.).to_array(); 3],
                color: [
                    f32::from(triangle.color[0]) / 255.,
                    f32::from(triangle.color[1]) / 255.,
                    f32::from(triangle.color[2]) / 255.,
                    0.,
                ],
                unused: [0.; 4],
            });
        }
        for i in 0..=models::MAX_HEADS {
            let part = if i == 0 {
                Some(&flower.whole)
            } else {
                flower.heads.get(i - 1)
            };
            parts.push(part.map_or(GpuPart::zeroed(), |part| GpuPart {
                range: [
                    first + part.triangles.start,
                    part.triangles.end - part.triangles.start,
                    super::model_pixel_cache::flower_source(model, i),
                    flower.heads.len() as u32,
                ],
                center_radius: part.center.extend(part.radius).to_array(),
            }));
        }
        max_stem_vertices = max_stem_vertices.max(flower.stem_triangles * 3);
    }
    (triangles, parts, max_stem_vertices)
}
#[derive(ResourceContainer)]
pub struct FlowerModelResources {
    pub flower_triangles: Resource<Buffer>,
    pub flower_parts: Resource<Buffer>,
    pub flower_stem_vertices: Resource<Buffer>,
    pub flower_stem_indices: Resource<Buffer>,
}
impl FlowerModelResources {
    pub fn new(device: Device, allocator: Allocator) -> Self {
        let (triangles, parts, max_stem_vertices) = geometry();
        let make = |bytes: &[u8], flags| {
            let buffer = Buffer::new_sized(
                device.clone(),
                allocator.clone(),
                BufferUsage::from_flags(flags),
                MemoryLocation::CpuToGpu,
                bytes.len() as u64,
            );
            buffer
                .fill_range_with_raw_u8(0, bytes)
                .expect("shared flower geometry upload");
            Resource::new(buffer)
        };
        let stem_indices = (0..max_stem_vertices).collect::<Vec<_>>();
        log::info!("[FLOWER_MODELS] assets={} triangles={} source=shared_recipe heads=complete stem_pose=shared",models::flowers().len(),triangles.len());
        Self {
            flower_triangles: make(
                bytemuck::cast_slice(&triangles),
                vk::BufferUsageFlags::STORAGE_BUFFER,
            ),
            flower_parts: make(
                bytemuck::cast_slice(&parts),
                vk::BufferUsageFlags::STORAGE_BUFFER,
            ),
            flower_stem_vertices: make(
                bytemuck::cast_slice(&stem_indices),
                vk::BufferUsageFlags::VERTEX_BUFFER,
            ),
            flower_stem_indices: make(
                bytemuck::cast_slice(&stem_indices),
                vk::BufferUsageFlags::INDEX_BUFFER,
            ),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gpu_ranges_cover_each_flower_without_cross_species_or_per_petal_tiles() {
        let (triangles, parts, stem_count) = geometry();
        assert_eq!(std::mem::size_of::<GpuTriangle>(), 128);
        assert_eq!(std::mem::size_of::<GpuPart>(), 32);
        assert_eq!(
            std::mem::size_of::<crate::generated::gpu_structs::PushConstantFlowerPixel>(),
            48
        );
        assert_eq!(parts.len(), models::MODEL_COUNT * 4);
        let mut end = 0;
        for (index, flower) in models::flowers().iter().enumerate() {
            let base = index * 4;
            assert_eq!(parts[base].range[0], end);
            assert_eq!(parts[base].range[1], flower.triangles.len() as u32);
            assert!(stem_count >= flower.stem_triangles * 3);
            for part in 0..=flower.heads.len() {
                assert_eq!(
                    parts[base + part].range[2],
                    super::super::model_pixel_cache::flower_source(index, part)
                );
            }
            let mut head_end = end + flower.stem_triangles;
            for i in 1..=flower.heads.len() {
                assert_eq!(parts[base + i].range[0], head_end);
                head_end += parts[base + i].range[1];
            }
            end += flower.triangles.len() as u32;
            assert_eq!(head_end, end);
        }
        assert_eq!(end as usize, triangles.len());
    }
}

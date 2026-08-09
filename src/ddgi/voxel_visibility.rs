//! Exact voxel-occupancy visibility used as the DDGI query gate.
//!
//! The GPU representation packs the authoritative voxel atlas along X, one bit per voxel.  The
//! small CPU model below is deliberately kept equivalent to the shader contract so boundary,
//! optical-boundary, revision, and fail-closed behavior can be tested without a Vulkan device.

use crate::generated::gpu_structs::DdgiVoxelVisibilityInfo;
use crate::resource::{DescriptorResource, Resource, ResourceContainer, ResourceLookup};
use anyhow::{ensure, Result};
use glam::UVec3;
#[cfg(test)]
use glam::Vec3;
use re_flora_vkn::vk;
use re_flora_vkn::{
    Allocator, Buffer, Extent3D, ImageDesc, SamplerDesc, ShaderModule, Texture, TextureLayout,
    VulkanContext,
};

pub const DDGI_VOXEL_VISIBILITY_MAX_STEPS: u32 = 2048;
pub const DDGI_VOXEL_VISIBILITY_BLOCK_SIZE: u32 = 8;

/// Deep owner of the exact voxel-occupancy publication consumed by every production DDGI query.
///
/// A single fixed-size bit volume is safe because rebuilding is synchronous. `ready` is cleared
/// before packing, and is only republished with the exact geometry revision after the GPU job has
/// completed.
pub struct DdgiVoxelVisibility {
    word_dimensions: UVec3,
    block_dimensions: UVec3,
    info_snapshot: DdgiVoxelVisibilityInfo,
    published_revision: Option<u32>,
    pub ddgi_voxel_visibility_bits: Resource<Texture>,
    pub ddgi_voxel_visibility_blocks: Resource<Texture>,
    pub ddgi_voxel_visibility_info: Resource<Buffer>,
}

impl DdgiVoxelVisibility {
    pub fn new(
        vulkan_ctx: &VulkanContext,
        allocator: Allocator,
        dimensions: UVec3,
        voxels_per_world_unit: UVec3,
        pack_shader: &ShaderModule,
    ) -> Result<Self> {
        ensure!(
            dimensions.min_element() > 0,
            "DDGI voxel visibility dimensions must be nonzero"
        );
        ensure!(
            voxels_per_world_unit.min_element() > 0,
            "DDGI voxel visibility world scale must be nonzero",
        );
        let word_dimensions = packed_word_dimensions(dimensions);
        let block_dimensions = UVec3::new(
            dimensions.x.div_ceil(DDGI_VOXEL_VISIBILITY_BLOCK_SIZE),
            dimensions.y.div_ceil(DDGI_VOXEL_VISIBILITY_BLOCK_SIZE),
            dimensions.z.div_ceil(DDGI_VOXEL_VISIBILITY_BLOCK_SIZE),
        );
        let max_steps = dimensions.element_sum().saturating_add(3);
        ensure!(
            max_steps <= DDGI_VOXEL_VISIBILITY_MAX_STEPS,
            "DDGI voxel visibility grid needs {max_steps} steps, exceeding shader budget {DDGI_VOXEL_VISIBILITY_MAX_STEPS}",
        );
        let max_image_dimension = unsafe {
            vulkan_ctx
                .instance()
                .as_raw()
                .get_physical_device_properties(vulkan_ctx.physical_device().as_raw())
                .limits
                .max_image_dimension3_d
        };
        ensure!(
            word_dimensions.max_element() <= max_image_dimension,
            "DDGI voxel visibility volume {}x{}x{} exceeds device 3D texture limit {max_image_dimension}",
            word_dimensions.x,
            word_dimensions.y,
            word_dimensions.z,
        );
        ensure!(
            block_dimensions.max_element() <= max_image_dimension,
            "DDGI voxel visibility block volume {}x{}x{} exceeds device 3D texture limit {max_image_dimension}",
            block_dimensions.x,
            block_dimensions.y,
            block_dimensions.z,
        );

        let texture = Texture::new(
            vulkan_ctx.device().clone(),
            allocator.clone(),
            &ImageDesc {
                extent: Extent3D::new(word_dimensions.x, word_dimensions.y, word_dimensions.z),
                format: vk::Format::R32_UINT,
                usage: vk::ImageUsageFlags::STORAGE,
                initial_layout: TextureLayout::UNDEFINED,
                aspect: vk::ImageAspectFlags::COLOR,
                ..Default::default()
            },
            &SamplerDesc::default(),
        );
        let block_texture = Texture::new(
            vulkan_ctx.device().clone(),
            allocator.clone(),
            &ImageDesc {
                extent: Extent3D::new(block_dimensions.x, block_dimensions.y, block_dimensions.z),
                format: vk::Format::R8_UINT,
                usage: vk::ImageUsageFlags::STORAGE,
                initial_layout: TextureLayout::UNDEFINED,
                aspect: vk::ImageAspectFlags::COLOR,
                ..Default::default()
            },
            &SamplerDesc::default(),
        );
        let info = Buffer::from_uniform_layout(
            vulkan_ctx.device().clone(),
            allocator,
            pack_shader
                .get_buffer_layout("U_DdgiVoxelVisibilityInfo")
                .expect("voxel visibility pack shader must reflect its info uniform")
                .clone(),
        );
        let info_snapshot = DdgiVoxelVisibilityInfo {
            voxel_dimensions: dimensions.to_array(),
            geometry_revision: 0,
            packed_word_dimensions: word_dimensions.to_array(),
            ready: 0,
            world_to_voxel_scale: voxels_per_world_unit.as_vec3().to_array(),
            max_steps,
        };
        info.fill_uniform(&info_snapshot)?;

        let packed_bytes =
            word_dimensions.element_product() as u64 * std::mem::size_of::<u32>() as u64;
        let block_bytes = block_dimensions.element_product() as u64;
        let bytes = packed_bytes + block_bytes;
        log::info!(
            "[DDGI][VOXEL_VISIBILITY] allocated voxels={}x{}x{} packed={}x{}x{} blocks={}x{}x{} bytes={} mib={:.2} max_steps={}",
            dimensions.x,
            dimensions.y,
            dimensions.z,
            word_dimensions.x,
            word_dimensions.y,
            word_dimensions.z,
            block_dimensions.x,
            block_dimensions.y,
            block_dimensions.z,
            bytes,
            bytes as f64 / (1024.0 * 1024.0),
            max_steps,
        );

        Ok(Self {
            word_dimensions,
            block_dimensions,
            info_snapshot,
            published_revision: None,
            ddgi_voxel_visibility_bits: Resource::new(texture),
            ddgi_voxel_visibility_blocks: Resource::new(block_texture),
            ddgi_voxel_visibility_info: Resource::new(info),
        })
    }

    pub fn word_dimensions(&self) -> UVec3 {
        self.word_dimensions
    }

    pub fn block_dimensions(&self) -> UVec3 {
        self.block_dimensions
    }

    pub fn published_revision(&self) -> Option<u32> {
        self.published_revision
    }

    pub fn begin_pack(&mut self, geometry_revision: u32) -> Result<()> {
        self.info_snapshot.geometry_revision = geometry_revision;
        self.info_snapshot.ready = 0;
        self.published_revision = None;
        self.ddgi_voxel_visibility_info
            .fill_uniform(&self.info_snapshot)
    }

    pub fn publish_pack(&mut self, geometry_revision: u32) -> Result<()> {
        ensure!(
            self.info_snapshot.geometry_revision == geometry_revision,
            "cannot publish DDGI voxel visibility revision {geometry_revision}; packing revision {}",
            self.info_snapshot.geometry_revision,
        );
        self.info_snapshot.ready = 1;
        self.ddgi_voxel_visibility_info
            .fill_uniform(&self.info_snapshot)?;
        self.published_revision = Some(geometry_revision);
        Ok(())
    }
}

impl ResourceContainer for DdgiVoxelVisibility {
    fn resolve_resource(&self, name: &str) -> ResourceLookup<'_> {
        match name {
            "ddgi_voxel_visibility_bits" => ResourceLookup::Unique(DescriptorResource::Texture(
                &self.ddgi_voxel_visibility_bits,
            )),
            "ddgi_voxel_visibility_blocks" => ResourceLookup::Unique(DescriptorResource::Texture(
                &self.ddgi_voxel_visibility_blocks,
            )),
            "ddgi_voxel_visibility_info" => {
                ResourceLookup::Unique(DescriptorResource::Buffer(&self.ddgi_voxel_visibility_info))
            }
            _ => ResourceLookup::Missing,
        }
    }
}

#[cfg(test)]
const ENDPOINT_CLIP_GRACE_VOXELS: f32 = 0.5;
#[cfg(test)]
const OPEN_SEGMENT_EPSILON_VOXELS: f32 = 1.0e-4;

#[cfg(test)]
#[derive(Clone, Debug)]
struct CpuVisibilityVolume {
    dimensions: UVec3,
    geometry_revision: u32,
    ready: bool,
    words: Vec<u32>,
}

#[cfg(test)]
impl CpuVisibilityVolume {
    fn from_occupancy(
        dimensions: UVec3,
        geometry_revision: u32,
        ready: bool,
        occupied: impl IntoIterator<Item = UVec3>,
    ) -> Self {
        let word_dimensions = packed_word_dimensions(dimensions);
        let mut words = vec![0; word_dimensions.element_product() as usize];
        for coordinate in occupied {
            assert!(coordinate.cmplt(dimensions).all());
            let (word_index, bit) = packed_word_index(dimensions, coordinate).unwrap();
            words[word_index] |= bit;
        }
        Self {
            dimensions,
            geometry_revision,
            ready,
            words,
        }
    }

    fn occupied(&self, coordinate: UVec3) -> bool {
        packed_word_index(self.dimensions, coordinate)
            .is_some_and(|(word_index, bit)| self.words[word_index] & bit != 0)
    }

    fn segment_visible(
        &self,
        expected_geometry_revision: u32,
        start_voxels: Vec3,
        end_voxels: Vec3,
        max_steps: u32,
    ) -> bool {
        if !self.ready || self.geometry_revision != expected_geometry_revision {
            return false;
        }
        optical_open_segment_visible(self, start_voxels, end_voxels, max_steps, false)
    }

    fn segment_visible_with_empty_block_skips(
        &self,
        expected_geometry_revision: u32,
        start_voxels: Vec3,
        end_voxels: Vec3,
        max_steps: u32,
    ) -> bool {
        if !self.ready || self.geometry_revision != expected_geometry_revision {
            return false;
        }
        optical_open_segment_visible(self, start_voxels, end_voxels, max_steps, true)
    }
}

fn packed_word_dimensions(dimensions: UVec3) -> UVec3 {
    UVec3::new(dimensions.x.div_ceil(32), dimensions.y, dimensions.z)
}

#[cfg(test)]
fn packed_word_index(dimensions: UVec3, coordinate: UVec3) -> Option<(usize, u32)> {
    if dimensions.min_element() == 0 || !coordinate.cmplt(dimensions).all() {
        return None;
    }
    let words = packed_word_dimensions(dimensions);
    let word_coordinate = UVec3::new(coordinate.x >> 5, coordinate.y, coordinate.z);
    let linear = word_coordinate.x + words.x * (word_coordinate.y + words.y * word_coordinate.z);
    Some((linear as usize, 1 << (coordinate.x & 31)))
}

#[cfg(test)]
fn optical_open_segment_visible(
    volume: &CpuVisibilityVolume,
    start: Vec3,
    end: Vec3,
    max_steps: u32,
    skip_empty_blocks: bool,
) -> bool {
    let dimensions = volume.dimensions.as_vec3();
    if volume.dimensions.min_element() == 0
        || !start.is_finite()
        || !end.is_finite()
        || endpoint_is_grossly_outside(start, dimensions)
        || endpoint_is_grossly_outside(end, dimensions)
    {
        return false;
    }

    let segment = end - start;
    let segment_length = segment.length();
    if segment_length <= OPEN_SEGMENT_EPSILON_VOXELS {
        return true;
    }

    let Some((entry_t, exit_t)) = clip_segment_to_voxel_domain(start, segment, dimensions) else {
        return false;
    };
    let parameter_epsilon =
        (OPEN_SEGMENT_EPSILON_VOXELS / segment_length).min((exit_t - entry_t) * 0.25);
    let entry_t = entry_t + parameter_epsilon;
    let exit_t = exit_t - parameter_epsilon;
    if entry_t >= exit_t {
        return true;
    }

    let domain_max = dimensions - Vec3::splat(OPEN_SEGMENT_EPSILON_VOXELS);
    let clipped_start = (start + segment * entry_t).clamp(Vec3::ZERO, domain_max);
    let clipped_end = (start + segment * exit_t).clamp(Vec3::ZERO, domain_max);
    let clipped_segment = clipped_end - clipped_start;
    if clipped_segment.length_squared() <= OPEN_SEGMENT_EPSILON_VOXELS.powi(2) {
        return true;
    }

    let mut cell = clipped_start.floor().as_ivec3();
    let end_cell = clipped_end.floor().as_ivec3();
    if occupied_cell(volume, cell) {
        return false;
    }
    if cell == end_cell {
        return true;
    }

    let step = clipped_segment.signum().as_ivec3();
    let delta = Vec3::new(
        axis_delta(clipped_segment.x),
        axis_delta(clipped_segment.y),
        axis_delta(clipped_segment.z),
    );
    let mut next = Vec3::new(
        axis_next(clipped_start.x, clipped_segment.x, cell.x, step.x),
        axis_next(clipped_start.y, clipped_segment.y, cell.y, step.y),
        axis_next(clipped_start.z, clipped_segment.z, cell.z, step.z),
    );

    for _ in 0..max_steps {
        let block = voxel_block_coordinate(cell);
        let block_occupied = !skip_empty_blocks || voxel_block_occupied(volume, block);
        if block_occupied {
            if occupied_cell(volume, cell) {
                return false;
            }
        } else if block == voxel_block_coordinate(end_cell) {
            return true;
        }
        if cell == end_cell {
            return true;
        }

        let crossing_by_axis = if block_occupied {
            next
        } else {
            Vec3::new(
                block_axis_next(next.x, delta.x, cell.x, step.x),
                block_axis_next(next.y, delta.y, cell.y, step.y),
                block_axis_next(next.z, delta.z, cell.z, step.z),
            )
        };
        let crossing = crossing_by_axis.min_element();
        if !crossing.is_finite() || crossing > 1.0 + OPEN_SEGMENT_EPSILON_VOXELS {
            return false;
        }
        if block_occupied {
            let tie = [
                (next.x - crossing).abs() <= OPEN_SEGMENT_EPSILON_VOXELS,
                (next.y - crossing).abs() <= OPEN_SEGMENT_EPSILON_VOXELS,
                (next.z - crossing).abs() <= OPEN_SEGMENT_EPSILON_VOXELS,
            ];
            if tie[0] {
                cell.x += step.x;
                next.x += delta.x;
            }
            if tie[1] {
                cell.y += step.y;
                next.y += delta.y;
            }
            if tie[2] {
                cell.z += step.z;
                next.z += delta.z;
            }
        } else {
            advance_axis_across_empty_block(crossing, step.x, delta.x, &mut cell.x, &mut next.x);
            advance_axis_across_empty_block(crossing, step.y, delta.y, &mut cell.y, &mut next.y);
            advance_axis_across_empty_block(crossing, step.z, delta.z, &mut cell.z, &mut next.z);
        }
    }
    false
}

#[cfg(test)]
fn voxel_block_coordinate(cell: glam::IVec3) -> glam::IVec3 {
    cell / DDGI_VOXEL_VISIBILITY_BLOCK_SIZE as i32
}

#[cfg(test)]
fn voxel_block_occupied(volume: &CpuVisibilityVolume, block: glam::IVec3) -> bool {
    let first = block * DDGI_VOXEL_VISIBILITY_BLOCK_SIZE as i32;
    let end = (first + glam::IVec3::splat(DDGI_VOXEL_VISIBILITY_BLOCK_SIZE as i32))
        .min(volume.dimensions.as_ivec3());
    for z in first.z..end.z {
        for y in first.y..end.y {
            for x in first.x..end.x {
                if volume.occupied(UVec3::new(x as u32, y as u32, z as u32)) {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
fn cells_to_block_exit(cell: i32, step: i32) -> i32 {
    if step == 0 {
        return 0;
    }
    let block_start =
        (cell / DDGI_VOXEL_VISIBILITY_BLOCK_SIZE as i32) * DDGI_VOXEL_VISIBILITY_BLOCK_SIZE as i32;
    if step > 0 {
        block_start + DDGI_VOXEL_VISIBILITY_BLOCK_SIZE as i32 - cell
    } else {
        cell - block_start + 1
    }
}

#[cfg(test)]
fn block_axis_next(next: f32, delta: f32, cell: i32, step: i32) -> f32 {
    let cells = cells_to_block_exit(cell, step);
    if cells == 0 {
        f32::INFINITY
    } else {
        next + delta * (cells - 1) as f32
    }
}

#[cfg(test)]
fn advance_axis_across_empty_block(
    crossing: f32,
    step: i32,
    delta: f32,
    cell: &mut i32,
    next: &mut f32,
) {
    if step == 0 || *next > crossing + OPEN_SEGMENT_EPSILON_VOXELS {
        return;
    }
    let advances = 1
        + ((crossing + OPEN_SEGMENT_EPSILON_VOXELS - *next) / delta)
            .max(0.0)
            .floor() as i32;
    *cell += step * advances;
    *next += delta * advances as f32;
}

#[cfg(test)]
fn occupied_cell(volume: &CpuVisibilityVolume, cell: glam::IVec3) -> bool {
    if cell.min_element() < 0 {
        return true;
    }
    let coordinate = cell.as_uvec3();
    !coordinate.cmplt(volume.dimensions).all() || volume.occupied(coordinate)
}

#[cfg(test)]
fn endpoint_is_grossly_outside(point: Vec3, dimensions: Vec3) -> bool {
    let grace = Vec3::splat(ENDPOINT_CLIP_GRACE_VOXELS);
    (point.cmplt(-grace) | point.cmpgt(dimensions + grace)).any()
}

#[cfg(test)]
fn clip_segment_to_voxel_domain(
    start: Vec3,
    segment: Vec3,
    dimensions: Vec3,
) -> Option<(f32, f32)> {
    let mut entry: f32 = 0.0;
    let mut exit: f32 = 1.0;
    for axis in 0..3 {
        let origin = start[axis];
        let direction = segment[axis];
        if direction.abs() <= f32::EPSILON {
            if origin < 0.0 || origin > dimensions[axis] {
                return None;
            }
            continue;
        }
        let first = -origin / direction;
        let second = (dimensions[axis] - origin) / direction;
        entry = entry.max(first.min(second));
        exit = exit.min(first.max(second));
        if entry > exit {
            return None;
        }
    }
    Some((entry.max(0.0), exit.min(1.0)))
}

#[cfg(test)]
fn axis_delta(direction: f32) -> f32 {
    if direction.abs() <= f32::EPSILON {
        f32::INFINITY
    } else {
        direction.abs().recip()
    }
}

#[cfg(test)]
fn axis_next(origin: f32, direction: f32, cell: i32, step: i32) -> f32 {
    if step == 0 {
        f32::INFINITY
    } else {
        let boundary = if step > 0 { cell + 1 } else { cell } as f32;
        (boundary - origin) / direction
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REVISION: u32 = 7;

    fn volume(dimensions: UVec3, occupied: &[UVec3]) -> CpuVisibilityVolume {
        CpuVisibilityVolume::from_occupancy(dimensions, REVISION, true, occupied.iter().copied())
    }

    #[test]
    fn x_word_packing_handles_bits_31_32_and_zeroes_tail_bits() {
        let volume = volume(
            UVec3::new(35, 1, 1),
            &[
                UVec3::new(31, 0, 0),
                UVec3::new(32, 0, 0),
                UVec3::new(34, 0, 0),
            ],
        );
        assert_eq!(
            packed_word_dimensions(volume.dimensions),
            UVec3::new(2, 1, 1)
        );
        assert_eq!(volume.words, vec![1 << 31, 0b101]);
        assert!(volume.occupied(UVec3::new(31, 0, 0)));
        assert!(volume.occupied(UVec3::new(32, 0, 0)));
        assert!(!volume.occupied(UVec3::new(33, 0, 0)));
        assert!(volume.occupied(UVec3::new(34, 0, 0)));
        assert!(packed_word_index(volume.dimensions, UVec3::new(35, 0, 0)).is_none());
    }

    #[test]
    fn rust_and_shader_empty_block_sizes_match() {
        let shader_types = include_str!("../../shader/slang/ddgi_types.slang");
        assert!(shader_types.contains(&format!(
            "DDGI_VOXEL_VISIBILITY_BLOCK_SIZE = {DDGI_VOXEL_VISIBILITY_BLOCK_SIZE}u;"
        )));
    }

    #[test]
    fn axis_segment_blocks_on_an_occupied_cell() {
        let volume = volume(UVec3::splat(4), &[UVec3::new(2, 1, 1)]);
        assert!(!volume.segment_visible(
            REVISION,
            Vec3::new(0.25, 1.25, 1.25),
            Vec3::new(3.75, 1.25, 1.25),
            16,
        ));
    }

    #[test]
    fn diagonal_edge_tie_ignores_zero_area_side_contact() {
        let volume = volume(UVec3::splat(4), &[UVec3::new(1, 0, 0)]);
        assert!(volume.segment_visible(
            REVISION,
            Vec3::new(0.25, 0.25, 0.5),
            Vec3::new(2.75, 2.75, 0.5),
            16,
        ));
    }

    #[test]
    fn diagonal_corner_tie_only_checks_the_entered_cell() {
        for blocker in [
            UVec3::new(1, 0, 0),
            UVec3::new(0, 1, 0),
            UVec3::new(0, 0, 1),
            UVec3::new(1, 1, 0),
            UVec3::new(1, 0, 1),
            UVec3::new(0, 1, 1),
        ] {
            let volume = volume(UVec3::splat(4), &[blocker]);
            assert!(
                volume.segment_visible(REVISION, Vec3::splat(0.25), Vec3::splat(2.75), 16,),
                "zero-area corner contact blocked on {blocker:?}",
            );
        }
        let diagonal = volume(UVec3::splat(4), &[UVec3::new(1, 1, 1)]);
        assert!(!diagonal.segment_visible(REVISION, Vec3::splat(0.25), Vec3::splat(2.75), 16,));
    }

    #[test]
    fn exact_world_boundary_endpoints_and_small_surface_bias_overshoot_are_clipped() {
        let empty = volume(UVec3::splat(4), &[]);
        assert!(empty.segment_visible(
            REVISION,
            Vec3::new(-0.25, 1.5, 1.5),
            Vec3::new(4.0, 1.5, 1.5),
            16,
        ));
        let blocked = volume(UVec3::splat(4), &[UVec3::new(3, 1, 1)]);
        assert!(!blocked.segment_visible(
            REVISION,
            Vec3::new(-0.25, 1.5, 1.5),
            Vec3::new(4.0, 1.5, 1.5),
            16,
        ));
    }

    #[test]
    fn not_ready_revision_mismatch_gross_out_of_bounds_and_budget_exhaustion_fail_closed() {
        let mut volume = volume(UVec3::splat(4), &[]);
        volume.ready = false;
        assert!(!volume.segment_visible(REVISION, Vec3::splat(0.25), Vec3::splat(3.75), 16));
        volume.ready = true;
        assert!(!volume.segment_visible(REVISION + 1, Vec3::splat(0.25), Vec3::splat(3.75), 16));
        assert!(!volume.segment_visible(
            REVISION,
            Vec3::new(-0.75, 1.0, 1.0),
            Vec3::new(3.0, 1.0, 1.0),
            16,
        ));
        assert!(!volume.segment_visible(REVISION, Vec3::splat(0.25), Vec3::splat(3.75), 0));
    }

    #[test]
    fn empty_block_skips_match_exact_voxel_traversal() {
        let dimensions = UVec3::new(35, 27, 19);
        let occupied = (0..dimensions.z)
            .flat_map(|z| {
                (0..dimensions.y).flat_map(move |y| {
                    (0..dimensions.x)
                        .filter(move |&x| (x * 17 + y * 31 + z * 43) % 67 == 0)
                        .map(move |x| UVec3::new(x, y, z))
                })
            })
            .collect::<Vec<_>>();
        let volume = volume(dimensions, &occupied);
        let max_steps = dimensions.element_sum() + 3;
        let mut state = 0x1234_5678_u32;
        let mut random_unit = || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 8) as f32 / (u32::MAX >> 8) as f32
        };

        for case in 0..4_096 {
            let start = Vec3::new(
                random_unit() * dimensions.x as f32,
                random_unit() * dimensions.y as f32,
                random_unit() * dimensions.z as f32,
            );
            let end = Vec3::new(
                random_unit() * dimensions.x as f32,
                random_unit() * dimensions.y as f32,
                random_unit() * dimensions.z as f32,
            );
            let exact = volume.segment_visible(REVISION, start, end, max_steps);
            let accelerated =
                volume.segment_visible_with_empty_block_skips(REVISION, start, end, max_steps);
            assert_eq!(
                accelerated, exact,
                "empty-block traversal diverged for case {case}: {start:?} -> {end:?}",
            );
        }
    }
}

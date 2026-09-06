//! Stateful surface vegetation motion. Ordinary grass shares one world-space
//! response grid; authored plants have lifetime-keyed state. Rendering only sees
//! four held poses, never the continuous integrator velocity.
use crate::{builder::SurfaceResources, geom::UAabb3};
use anyhow::Result;
use bytemuck::{Pod, Zeroable};
use re_flora_vkn::{
    vk, Allocator, Buffer, BufferUsage, CommandBuffer, ComputePipeline, DescriptorResource, Device,
    Extent3D, MemoryLocation,
};
use std::collections::HashMap;

mod fruit_handoff;
mod validation;
pub(super) use validation::validate_gpu;

const GRID_SPACING_VOXELS: u32 = 16;
const NO_PREVIOUS: u32 = u32::MAX;
const STATE_BYTES: u64 = 80;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
struct ResponseInput {
    root: [f32; 4],
    identity: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ResponseInfo {
    grid: [f32; 4],
    shape: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ResponseStep {
    start_time: f32,
    end_time: f32,
    tick_seconds: f32,
    count: u32,
    controls: [f32; 4],
}

struct FrameBuffers {
    inputs: Buffer,
    info: Buffer,
    outputs: [Buffer; 2],
    output_index: usize,
    capacity: usize,
}

impl FrameBuffers {
    fn new(device: Device, allocator: Allocator, count: usize) -> Self {
        let capacity = count.max(1).next_power_of_two();
        let storage = vk::BufferUsageFlags::STORAGE_BUFFER;
        let make = |bytes, flags, location| {
            Buffer::new_sized(
                device.clone(),
                allocator.clone(),
                BufferUsage::from_flags(flags),
                location,
                bytes,
            )
        };
        Self {
            inputs: make(capacity as u64 * 32, storage, MemoryLocation::CpuToGpu),
            info: make(
                32,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
                MemoryLocation::CpuToGpu,
            ),
            outputs: std::array::from_fn(|_| {
                make(
                    capacity as u64 * STATE_BYTES,
                    storage | vk::BufferUsageFlags::TRANSFER_SRC,
                    MemoryLocation::GpuOnly,
                )
            }),
            output_index: 0,
            capacity,
        }
    }
}

pub(super) struct VegetationResponse {
    pub enabled: bool,
    pub controls: [f32; 4],
    pub pose_hz: f32,
    comparison: String,
    grid: ResponseInfo,
    grid_inputs: Vec<ResponseInput>,
    previous_plants: HashMap<u64, u32>,
    flower_offsets: Vec<[u32; 5]>,
    frames: Vec<Option<FrameBuffers>>,
    previous_output: Option<Buffer>,
    current_frame: Option<usize>,
    last_time: Option<f32>,
    validation_pending: bool,
    validation_enabled: bool,
    validation_draw_mask: u32,
    validation_reset_count: u32,
    validation_tree_mask: u32,
    last_controls: Option<([f32; 4], f32)>,
}

impl VegetationResponse {
    pub fn new(bounds: UAabb3) -> Self {
        let origin = bounds.min();
        let extent = bounds.max() - origin;
        let width = (extent.x * 256).div_ceil(GRID_SPACING_VOXELS) + 1;
        let depth = (extent.z * 256).div_ceil(GRID_SPACING_VOXELS) + 1;
        let spacing = GRID_SPACING_VOXELS as f32 / 256.0;
        let grid = ResponseInfo {
            grid: [origin.x as f32, origin.z as f32, spacing, 3.0],
            shape: [width, depth, 1, width * depth],
        };
        let mut grid_inputs = Vec::with_capacity((width * depth) as usize);
        let comparison = std::env::var("RE_FLORA_VEGETATION_RESPONSE_BENCH").unwrap_or_default();
        let field_species: &[u32] = match comparison.as_str() {
            "legacy" => &[],
            "surface" => &[0],
            _ => &[0, 5, 6],
        };
        let mut grid = grid;
        grid.grid[3] = field_species.len() as f32;
        for &species in field_species {
            for z in 0..depth {
                for x in 0..width {
                    grid_inputs.push(ResponseInput {
                        root: [
                            origin.x as f32 + x as f32 * spacing,
                            0.0,
                            origin.z as f32 + z as f32 * spacing,
                            0.0,
                        ],
                        identity: [NO_PREVIOUS, species, 0, 0],
                    });
                }
            }
        }
        log::info!("[VEGETATION_RESPONSE] grid={}x{} spacing_voxels={} grid_state_bytes={} per_plant_state_bytes={}",
            width, depth, GRID_SPACING_VOXELS, width as u64 * depth as u64 * STATE_BYTES, STATE_BYTES);
        Self {
            enabled: true,
            controls: [1.5, 1., 1., 0.],
            pose_hz: 5.,
            comparison,
            grid,
            grid_inputs,
            previous_plants: HashMap::new(),
            flower_offsets: Vec::new(),
            frames: Vec::new(),
            previous_output: None,
            current_frame: None,
            last_time: None,
            validation_pending: std::env::var_os("RE_FLORA_VEGETATION_RESPONSE_VALIDATE").is_some(),
            validation_enabled: std::env::var_os("RE_FLORA_VEGETATION_RESPONSE_VALIDATE").is_some(),
            validation_draw_mask: 0,
            validation_reset_count: 0,
            validation_tree_mask: 0,
            last_controls: None,
        }
    }

    pub fn validate_once(
        &mut self,
        context: &re_flora_vkn::VulkanContext,
        allocator: Allocator,
        resources: &crate::tracer::TracerResources,
    ) -> Result<()> {
        if std::mem::take(&mut self.validation_pending) {
            validate_gpu(context, allocator, resources)?;
        }
        Ok(())
    }

    /// Called even when flora is hidden or the legacy comparison is selected.
    /// Existing frame fences protect host-written inputs; descriptor snapshots
    /// and command resource uses retain replaced GPU buffer allocations.
    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &mut self,
        device: Device,
        allocator: Allocator,
        pipeline: &ComputePipeline,
        cmdbuf: &CommandBuffer,
        surface: &SurfaceResources,
        frame_slot: usize,
        time: f32,
        tick_seconds: f32,
    ) -> Result<()> {
        // A process-start-only old-path baseline: no solver dispatch, no plant
        // remapping and no full response allocations. One dummy element remains
        // per in-flight frame to satisfy the shared shader descriptor interface.
        if self.comparison == "legacy" {
            if self.frames.len() <= frame_slot {
                self.frames.resize_with(frame_slot + 1, || None);
            }
            if self.frames[frame_slot].is_none() {
                self.frames[frame_slot] = Some(FrameBuffers::new(device, allocator, 1));
                log::info!("[VEGETATION_RESPONSE][MEMORY] mode=legacy frame_slot={frame_slot} capacity=1 buffer_bytes=224 solver_dispatches=0 residual_descriptor_buffers=true");
            }
            let mut info = self.grid;
            info.shape[2] = 0;
            self.frames[frame_slot]
                .as_ref()
                .unwrap()
                .info
                .fill_uniform(&info)?;
            self.current_frame = Some(frame_slot);
            self.flower_offsets
                .resize(surface.instances.chunk_flora_instances.len(), [0; 5]);
            return Ok(());
        }
        let (start, reset) = response_interval(self.last_time, time);
        if reset {
            self.validation_reset_count += 1;
            self.previous_plants.clear();
        }
        let mut inputs = self.grid_inputs.clone();
        for (index, input) in inputs.iter_mut().enumerate() {
            input.identity[0] = if reset { NO_PREVIOUS } else { index as u32 };
        }
        let mut next_plants = HashMap::new();
        self.flower_offsets.clear();
        for (_, chunk) in &surface.instances.chunk_flora_instances {
            let offsets = append_chunk_plants(
                &mut inputs,
                &chunk.authored_response_instances,
                &self.previous_plants,
                &mut next_plants,
            );
            for species in 2..5 {
                let count = chunk
                    .authored_response_instances
                    .iter()
                    .filter(|plant| plant.species_index == species as u32)
                    .count() as u32;
                anyhow::ensure!(count == chunk.species_len(species),
                    "authored response/draw stream mismatch: species={species} response={count} draw={}",
                    chunk.species_len(species));
            }
            self.flower_offsets.push(offsets);
        }
        if self.previous_plants.len() != next_plants.len() {
            log::info!(
                "[VEGETATION_RESPONSE] authored_plants={} retained={} state_count={}",
                next_plants.len(),
                next_plants
                    .keys()
                    .filter(|id| self.previous_plants.contains_key(id))
                    .count(),
                inputs.len()
            );
        }
        self.previous_plants = next_plants;
        if self.frames.len() <= frame_slot {
            self.frames.resize_with(frame_slot + 1, || None);
        }
        if self.frames[frame_slot]
            .as_ref()
            .is_none_or(|frame| frame.capacity < inputs.len())
        {
            self.frames[frame_slot] = Some(FrameBuffers::new(device, allocator, inputs.len()));
            let capacity = self.frames[frame_slot].as_ref().unwrap().capacity;
            log::info!("[VEGETATION_RESPONSE][MEMORY] mode={} frame_slot={frame_slot} capacity={capacity} buffer_bytes={} active_states={} fields={}",
                if self.comparison.is_empty() { "all" } else { &self.comparison }, capacity * 192 + 32, inputs.len(), self.grid.grid[3]);
        }
        let frame = self.frames[frame_slot].as_mut().unwrap();
        frame.output_index ^= 1;
        let output = &frame.outputs[frame.output_index];
        let previous = self
            .previous_output
            .as_ref()
            .unwrap_or(&frame.outputs[frame.output_index ^ 1]);
        anyhow::ensure!(
            previous.as_raw() != output.as_raw(),
            "response ping-pong aliases"
        );
        frame
            .inputs
            .fill_range_with_raw_u8(0, bytemuck::cast_slice(&inputs))?;
        let mut info = self.grid;
        info.shape[2] = u32::from(self.enabled || !self.comparison.is_empty());
        info.shape[3] = inputs.len() as u32;
        frame.info.fill_uniform(&info)?;
        let step = ResponseStep {
            start_time: start,
            end_time: time,
            tick_seconds: if !self.comparison.is_empty() {
                tick_seconds
            } else {
                1. / (4. * self.pose_hz.clamp(2.5, 20.))
            },
            count: inputs.len() as u32,
            controls: if !self.comparison.is_empty() {
                [1., 1., 1., 0.]
            } else {
                self.controls
            },
        };
        let settings = (step.controls, step.tick_seconds);
        if self.last_controls != Some(settings) {
            log::info!(
                "[VEGETATION_RESPONSE][SETTINGS] controls={:?} pose_hz={} states={} reset_count={}",
                step.controls,
                1. / (4. * step.tick_seconds),
                step.count,
                self.validation_reset_count
            );
            self.last_controls = Some(settings);
        }
        pipeline.begin_transient_descriptor_frame(frame_slot);
        pipeline.record_with_descriptors(
            cmdbuf,
            &[
                ("response_inputs", DescriptorResource::Buffer(&frame.inputs)),
                ("response_previous", DescriptorResource::Buffer(previous)),
                ("response_output", DescriptorResource::Buffer(output)),
            ],
            Extent3D::new(step.count, 1, 1),
            Some(bytemuck::bytes_of(&step)),
        )?;
        self.previous_output = Some(output.clone());
        self.current_frame = Some(frame_slot);
        self.last_time = Some(time);
        Ok(())
    }

    pub fn descriptors(&self) -> [(&'static str, DescriptorResource<'_>); 2] {
        let frame = self.frames[self
            .current_frame
            .expect("response prepass must precede flora draws")]
        .as_ref()
        .unwrap();
        [
            (
                "vegetation_response",
                DescriptorResource::Buffer(&frame.outputs[frame.output_index]),
            ),
            (
                "vegetation_response_info",
                DescriptorResource::Buffer(&frame.info),
            ),
        ]
    }

    pub fn flower_offset(&self, chunk: usize, species: usize) -> u32 {
        self.flower_offsets[chunk][species]
    }

    pub fn observe_draws(&mut self, plan: &super::flora_frame_plan::FloraFramePlan) {
        if !self.validation_enabled || !self.enabled {
            return;
        }
        for batch in plan.batches() {
            let lod = u32::from(batch.lod_state() == super::LodState::Lod1);
            let bit = 1 << (batch.species_index() as u32 + lod * 5);
            if self.validation_draw_mask & bit == 0 {
                self.validation_draw_mask |= bit;
                log::info!(
                    "[VEGETATION_RESPONSE][DRAW] species={} lod={} instances={} coverage=0x{:03x}",
                    batch.species_index(),
                    lod,
                    batch.instance_count(),
                    self.validation_draw_mask
                );
            }
        }
    }

    pub fn validate_draw_coverage(&self) -> Result<()> {
        anyhow::ensure!(
            self.validation_tree_mask == 15,
            "incomplete attached leaf/fruit LOD coverage: {}",
            self.validation_tree_mask
        );
        anyhow::ensure!(
            self.validation_draw_mask == 0x3ff,
            "incomplete grass/flower C draw coverage: 0x{:03x}",
            self.validation_draw_mask
        );
        anyhow::ensure!(
            self.validation_reset_count == 1,
            "response was reset during lifecycle replay: {} resets",
            self.validation_reset_count
        );
        log::info!("[VEGETATION_RESPONSE][DRAW] all_species_both_lods=passed attached_leaf_fruit_both_lods=passed state_resets=1");
        Ok(())
    }

    pub fn observe_tree_draw(&mut self, is_apple: bool, lod: bool, count: u32) {
        if !self.validation_enabled || !self.enabled || count == 0 {
            return;
        }
        let bit = 1 << (u32::from(is_apple) * 2 + u32::from(lod));
        if self.validation_tree_mask & bit == 0 {
            self.validation_tree_mask |= bit;
            log::info!("[VEGETATION_RESPONSE][TREE_DRAW] apple={is_apple} lod={} instances={count} mask={}", u32::from(lod), self.validation_tree_mask);
        }
    }
}

fn response_interval(previous: Option<f32>, time: f32) -> (f32, bool) {
    match previous {
        Some(previous) if time >= previous => (previous, false),
        _ => (time, true), // New world or an explicit backwards timeline seek.
    }
}

fn append_chunk_plants(
    inputs: &mut Vec<ResponseInput>,
    plants: &[crate::builder::AuthoredFloraInstance],
    previous: &HashMap<u64, u32>,
    next: &mut HashMap<u64, u32>,
) -> [u32; 5] {
    let mut offsets = [0; 5];
    for species in 2..5 {
        offsets[species] = inputs.len() as u32;
        for plant in plants
            .iter()
            .filter(|plant| plant.species_index == species as u32)
        {
            let previous = previous
                .get(&plant.response_id)
                .copied()
                .unwrap_or(NO_PREVIOUS);
            next.insert(plant.response_id, inputs.len() as u32);
            let root = (plant.base_world_vox + glam::UVec3::Y).as_vec3() / 256.0;
            inputs.push(ResponseInput {
                root: [root.x, root.y, root.z, 0.0],
                identity: [previous, species as u32, 0, 0],
            });
        }
    }
    offsets
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::UVec3;

    #[test]
    fn grid_covers_chunk_seams_and_nonzero_world_origins() {
        let response =
            VegetationResponse::new(UAabb3::new(UVec3::new(3, 0, 4), UVec3::new(5, 2, 6)));
        assert_eq!(response.grid.shape[..2], [33, 33]);
        assert_eq!(response.grid_inputs.len(), 1089 * 3);
        assert_eq!(response.grid_inputs.first().unwrap().root, [3., 0., 4., 0.]);
        assert_eq!(response.grid_inputs.last().unwrap().root, [5., 0., 6., 0.]);
        assert_eq!(response.grid_inputs[16].root[0], 4.);
    }

    #[test]
    fn normal_frames_and_pause_preserve_state_only_time_seeks_reset() {
        assert_eq!(response_interval(Some(1.), 1.016), (1., false));
        assert_eq!(response_interval(Some(1.), 1.), (1., false));
        assert_eq!(response_interval(Some(2.), 1.), (1., true));
        assert_eq!(response_interval(Some(1.), 5.), (1., false));
    }

    #[test]
    fn gpu_input_layouts_remain_aligned() {
        assert_eq!(std::mem::size_of::<ResponseInput>(), 32);
        assert_eq!(std::mem::size_of::<ResponseInfo>(), 32);
        assert_eq!(std::mem::size_of::<ResponseStep>(), 32);
        assert_eq!(
            std::mem::size_of::<crate::generated::gpu_structs::ManualResponseInputs>(),
            32
        );
        assert_eq!(
            std::mem::size_of::<crate::generated::gpu_structs::ManualResponseOutput>(),
            STATE_BYTES as usize
        );
        assert_eq!(
            std::mem::size_of::<crate::generated::gpu_structs::PushConstantFlora>(),
            128
        );
    }

    #[test]
    fn plant_identity_survives_reorder_growth_and_delete_without_reusing_velocity() {
        let plant = |id, species| crate::builder::AuthoredFloraInstance {
            response_id: id,
            species_index: species,
            base_world_vox: UVec3::new(10, 20, 30),
            growth_progress: 255,
            spawn_start_ms: 0,
            seed: 123,
        };
        let mut inputs = vec![];
        let mut previous = HashMap::new();
        let offsets = append_chunk_plants(
            &mut inputs,
            &[plant(1, 2), plant(2, 2), plant(3, 4)],
            &HashMap::new(),
            &mut previous,
        );
        assert_eq!(offsets[2..], [0, 2, 2]);
        assert!(inputs.iter().all(|input| input.identity[0] == NO_PREVIOUS));
        inputs.clear();
        let mut next = HashMap::new();
        let mut retained = plant(2, 2);
        retained.growth_progress = 20;
        append_chunk_plants(
            &mut inputs,
            &[plant(3, 4), retained, plant(4, 2)],
            &previous,
            &mut next,
        );
        assert_eq!(
            inputs
                .iter()
                .map(|input| input.identity[0])
                .collect::<Vec<_>>(),
            [1, NO_PREVIOUS, 2]
        );
        assert!(!next.contains_key(&1));
        assert_eq!(inputs[1].root, inputs[0].root); // Same position, genuinely new lifetime.
        inputs.clear();
        append_chunk_plants(
            &mut inputs,
            &[plant(2, 2)],
            &HashMap::new(),
            &mut HashMap::new(),
        );
        assert_eq!(inputs[0].identity[0], NO_PREVIOUS); // Reappearing after an unloaded frame.
    }
}

//! Batched event-only transfer of actual attached leaf poses to CPU flight.
//! No per-leaf queue waits, new release randomness, or second CPU sway solver.
use super::*;
use crate::leaf_lifecycle::LeafDetachment;
use crate::particles::AttachedLeafRelease;
use bytemuck::{Pod, Zeroable};
use glam::{IVec3, Quat, Vec4};
use re_flora_vkn::{execute_one_time_command, BufferUse};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Input {
    world: [u32; 4],
    state: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Output {
    position_size: [f32; 4],
    velocity: [f32; 4],
    normal: [f32; 4],
    color: [f32; 4],
    rotation: [f32; 4],
    angular_velocity: [f32; 4],
}

impl Tracer {
    pub(crate) fn gather_leaf_handoffs(
        &self,
        requests: &[(LeafDetachment, IVec3)],
        colors: FloraHeightColorTables,
    ) -> Result<Vec<(LeafDetachment, AttachedLeafRelease)>> {
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        let started = Instant::now();
        let mut inputs = Vec::with_capacity(requests.len());
        let mut ready = Vec::with_capacity(requests.len());
        let mut time = 0.0;
        for &(event, local) in requests {
            let packed = Self::pack_tree_leaf_voxel_local_pos(local)?;
            // Newly planted/rebuilt leaves have not been published yet. They
            // cannot transfer a pose until that topology has actually rendered.
            let Some((index, published_time)) =
                self.vegetation_response
                    .published_leaf(event.id.tree, event.world_voxel, packed)
            else {
                continue;
            };
            inputs.push(Input {
                world: [
                    event.world_voxel.x,
                    event.world_voxel.y,
                    event.world_voxel.z,
                    packed,
                ],
                state: [index, event.growth.to_bits(), 0, 0],
            });
            ready.push((event, local));
            time = published_time;
        }
        if inputs.is_empty() {
            return Ok(Vec::new());
        }
        let input = Buffer::new_sized(
            self.vulkan_ctx.device().clone(),
            self.allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            MemoryLocation::CpuToGpu,
            std::mem::size_of_val(inputs.as_slice()) as u64,
        );
        input.fill(&inputs)?;
        let bytes = inputs.len() as u64 * std::mem::size_of::<Output>() as u64;
        let output = Buffer::new_sized(
            self.vulkan_ctx.device().clone(),
            self.allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            MemoryLocation::GpuToCpu,
            bytes,
        );
        let mut pc = flora_push_constant(time, LEAF_INSTANCE_TYPE, UVec3::ZERO, colors);
        pc.instance_ty = flora_lighting_cache_instance_ty(LEAF_INSTANCE_TYPE, inputs.len() as u32);
        let pipeline = &self.pipeline_topology.compute().leaf_handoff_ppl;
        pipeline.begin_transient_descriptor_frame(0);
        execute_one_time_command(
            self.vulkan_ctx.device(),
            self.vulkan_ctx.command_pool(),
            &self.vulkan_ctx.get_general_queue(),
            |cmd| -> Result<()> {
                pipeline.record_with_descriptors(
                    cmd,
                    &[
                        ("leaf_handoff_inputs", DescriptorResource::Buffer(&input)),
                        ("leaf_handoff_outputs", DescriptorResource::Buffer(&output)),
                        self.vegetation_response.descriptors()[0],
                        self.vegetation_response.descriptors()[1],
                    ],
                    Extent3D::new(inputs.len() as u32, 1, 1),
                    Some(bytemuck::bytes_of(&pc)),
                )?;
                cmd.use_buffer(&output, BufferUse::HostRead);
                Ok(())
            },
        )?;
        let data = output.read_back_range(0, bytes)?;
        let poses: &[Output] = bytemuck::try_cast_slice(&data)
            .map_err(|e| anyhow::anyhow!("leaf handoff readback: {e}"))?;
        let mut result = Vec::with_capacity(ready.len());
        for ((event, local), pose) in ready.into_iter().zip(poses) {
            let position = Vec3::from_slice(&pose.position_size);
            let mut velocity = Vec3::from_slice(&pose.velocity);
            let mut angular_velocity = Vec3::from_slice(&pose.angular_velocity);
            let normal = Vec3::from_slice(&pose.normal);
            let rotation = Quat::from_array(pose.rotation);
            let anchor = (event.world_voxel.as_ivec3() - local).as_uvec3();
            // Add the published branch's rigid velocity; shader supplied the
            // per-leaf motion relative to that branch.
            if self.raster_trees.posed_surface.is_some() {
                if let Some(index) = self
                    .raster_trees
                    .attachments
                    .iter()
                    .position(|a| a.anchor == anchor)
                {
                    if let (Some(current), Some(previous)) = (
                        self.raster_trees.attachment_poses.get(index),
                        self.raster_trees.previous_attachment_poses.get(index),
                    ) {
                        let dt = self.raster_trees.attachment_dt;
                        if dt > 1e-6 {
                            let rest =
                                current.rotation.conjugate() * (position - current.translation);
                            velocity += (position - previous.transform_point(rest)) / dt;
                            angular_velocity += (current.rotation * previous.rotation.conjugate())
                                .to_scaled_axis()
                                / dt;
                        }
                    }
                }
            }
            anyhow::ensure!(
                position.is_finite()
                    && velocity.is_finite()
                    && normal.is_finite()
                    && normal.length_squared() > 0.5
                    && rotation.is_finite()
                    && pose.color.iter().all(|v| v.is_finite())
                    && angular_velocity.is_finite(),
                "invalid attached leaf handoff {:?}",
                event.id
            );
            result.push((
                event,
                AttachedLeafRelease {
                    position,
                    velocity,
                    normal,
                    angular_velocity,
                    geometry_rotation: rotation,
                    color: Vec4::from_array(pose.color),
                    size: pose.position_size[3],
                    seed: event.seed,
                },
            ));
        }
        log::info!("[LEAF_LIFECYCLE][HANDOFF] leaves={} bytes={} batch_waits=1 elapsed_us={} source=published_leaf_pose",
            result.len(), bytes, started.elapsed().as_micros());
        Ok(result)
    }
}

//! Small GPU scene-query answers, including exact barycentric rest-space wood.
//! Full wood vertices never cross back to the CPU for picking.
use super::{TerrainRayQuery, Tracer};
use anyhow::{ensure, Result};
use glam::Vec3;
use re_flora_vkn::{BufferUse, CommandBuffer, Extent3D};

pub(super) const PICK_BYTES: u64 = 48;
const NO_PRIMITIVE: u32 = u32::MAX;

// Structured-buffer element ABI in terrain_query.slang::B_SurfacePickResult.
#[repr(C, align(16))]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct PickRaw {
    pub position: [f32; 4],
    pub rest_position: [f32; 4],
    pub metadata: [u32; 4],
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SurfacePickHit {
    pub world_position: Vec3,
    pub rest_position: Vec3,
    pub distance: f32,
    pub voxel_type: u32,
    pub tree_primitive: Option<u32>,
}
impl SurfacePickHit {
    pub fn tree_rest_position(self) -> Option<Vec3> {
        self.tree_primitive.map(|_| self.rest_position)
    }
}

fn decode(raw: PickRaw) -> Result<Option<SurfacePickHit>> {
    if raw.metadata[0] == 0 {
        return Ok(None);
    }
    let hit = SurfacePickHit {
        world_position: Vec3::from_slice(&raw.position),
        rest_position: Vec3::from_slice(&raw.rest_position),
        distance: raw.position[3],
        voxel_type: raw.metadata[2],
        tree_primitive: (raw.metadata[1] != NO_PRIMITIVE).then_some(raw.metadata[1]),
    };
    ensure!(
        raw.metadata[0] == 1
            && hit.world_position.is_finite()
            && hit.rest_position.is_finite()
            && hit.distance.is_finite()
            && hit.distance >= 0.
            && (1..=15).contains(&hit.voxel_type),
        "invalid GPU surface pick result"
    );
    ensure!(
        hit.tree_primitive.is_none() || hit.voxel_type == 5,
        "GPU tree pick has non-wood material"
    );
    Ok(Some(hit))
}

impl Tracer {
    /// Explicit, on-demand query for edits/validation. Hover will use a frame
    /// readback instead; never call this blocking path once per hover frame.
    pub(crate) fn query_surface_pick_now(
        &mut self,
        ray: TerrainRayQuery,
    ) -> Result<Option<SurfacePickHit>> {
        if !ray.origin.is_finite()
            || !ray.direction.is_finite()
            || ray.direction.length_squared() < 1e-8
        {
            return Ok(None);
        }
        self.resources
            .terrain_query
            .terrain_query_count
            .fill(&[1u32, 0, 0, 0])?;
        self.resources.terrain_query.terrain_query_info.fill(&[[
            ray.origin.x,
            ray.origin.y,
            ray.origin.z,
            -1.,
            ray.direction.x,
            ray.direction.y,
            ray.direction.z,
            0.,
        ]])?;
        let command = CommandBuffer::new(self.vulkan_ctx.device(), self.vulkan_ctx.command_pool());
        command.begin(true);
        self.record_tree_skin(&command);
        self.record_surface_pick_query(&command);
        self.resources
            .terrain_query
            .terrain_pick_result
            .record_copy_to_buffer(
                &command,
                &self.resources.terrain_query.terrain_pick_readback,
                PICK_BYTES,
                0,
                0,
            );
        command.use_buffer(
            &self.resources.terrain_query.terrain_pick_readback,
            BufferUse::HostRead,
        );
        command.end();
        command
            .submit_gpu_job(&self.vulkan_ctx.get_general_queue(), "surface.pick.edit")?
            .wait_complete()?;
        let bytes = self
            .resources
            .terrain_query
            .terrain_pick_readback
            .read_back_range(0, PICK_BYTES)?;
        let hit = decode(bytemuck::pod_read_unaligned(&bytes))?;
        if let Some(primitive) = hit.and_then(|h| h.tree_primitive) {
            ensure!(
                (primitive as usize) < self.raster_trees.scene.primitives.len(),
                "GPU pick primitive outside publication"
            );
        }
        Ok(hit)
    }

    fn record_surface_pick_query(&self, command: &CommandBuffer) {
        for buffer in [
            &self.resources.terrain_query.terrain_query_count,
            &self.resources.terrain_query.terrain_query_info,
        ] {
            command.use_buffer(buffer, BufferUse::HostWrite);
        }
        self.pipeline_topology.compute().terrain_query_ppl.record(
            command,
            Extent3D::new(1, 1, 1),
            None,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, offset_of, size_of};
    #[test]
    fn pick_layout_and_decode_preserve_world_rest_and_integer_identity() {
        assert_eq!(size_of::<PickRaw>(), PICK_BYTES as usize);
        assert_eq!(align_of::<PickRaw>(), 16);
        assert_eq!(offset_of!(PickRaw, rest_position), 16);
        assert_eq!(offset_of!(PickRaw, metadata), 32);
        let raw = PickRaw {
            position: [1., 2., 3., 4.],
            rest_position: [5., 6., 7., 0.],
            metadata: [1, 340443, 5, 0],
        };
        let hit = decode(raw).unwrap().unwrap();
        assert_eq!(hit.world_position, Vec3::new(1., 2., 3.));
        assert_eq!(hit.tree_rest_position(), Some(Vec3::new(5., 6., 7.)));
        assert_eq!(hit.tree_primitive, Some(340443));
        let voxel = decode(PickRaw {
            metadata: [1, NO_PRIMITIVE, 2, 0],
            ..raw
        })
        .unwrap()
        .unwrap();
        assert!(voxel.tree_rest_position().is_none());
        assert!(decode(PickRaw {
            metadata: [0; 4],
            ..raw
        })
        .unwrap()
        .is_none());
        assert!(decode(PickRaw {
            position: [f32::NAN; 4],
            ..raw
        })
        .is_err());
        assert!(decode(PickRaw {
            metadata: [1, 0, 2, 0],
            ..raw
        })
        .is_err());
    }
}

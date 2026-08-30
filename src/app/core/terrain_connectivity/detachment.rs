use super::{prepare_detached_voxel_clear, PreparedAtlasWrite};
use crate::app::core::{visible_terrain::VisibleTerrainPublication, App, VisibleTerrainChange};
use crate::app::world_edits::BuildEdit;
use crate::builder::PlainBuilder;
use crate::geom::UAabb3;
use anyhow::Context;
use glam::UVec3;
use std::time::Instant;

struct AtlasWriteRequest {
    origin: UVec3,
    dim: UVec3,
    data: Vec<u8>,
}

pub(super) struct TerrainDetachmentRequest {
    world_dim: UVec3,
    atlas_writes: Vec<AtlasWriteRequest>,
    publication_edits: Vec<BuildEdit>,
    visual_voxels: Vec<(UVec3, u8)>,
    detached_voxels: usize,
}

impl TerrainDetachmentRequest {
    pub(super) fn single_region(
        world_dim: UVec3,
        origin: UVec3,
        dim: UVec3,
        data: Vec<u8>,
        visual_voxels: Vec<(UVec3, u8)>,
        detached_voxels: usize,
        affected_bound: UAabb3,
    ) -> Self {
        Self {
            world_dim,
            atlas_writes: vec![AtlasWriteRequest { origin, dim, data }],
            publication_edits: vec![BuildEdit::RebuildMeshWithoutFlora(affected_bound)],
            visual_voxels,
            detached_voxels,
        }
    }

    fn from_regions(
        world_dim: UVec3,
        atlas_writes: Vec<(UVec3, UVec3, Vec<u8>)>,
        affected_bound: UAabb3,
        visual_voxels: Vec<(UVec3, u8)>,
        detached_voxels: usize,
    ) -> Self {
        Self {
            world_dim,
            atlas_writes: atlas_writes
                .into_iter()
                .map(|(origin, dim, data)| AtlasWriteRequest { origin, dim, data })
                .collect(),
            publication_edits: vec![BuildEdit::RebuildMeshWithoutFlora(affected_bound)],
            visual_voxels,
            detached_voxels,
        }
    }
}

pub(super) struct RejectedTerrainDetachment {
    request: TerrainDetachmentRequest,
    error: anyhow::Error,
}

impl RejectedTerrainDetachment {
    pub(super) fn into_error(self) -> anyhow::Error {
        self.error
    }

    pub(super) fn into_single_region(self) -> (Vec<u8>, Vec<(UVec3, u8)>, anyhow::Error) {
        let TerrainDetachmentRequest {
            mut atlas_writes,
            visual_voxels,
            ..
        } = self.request;
        assert_eq!(
            atlas_writes.len(),
            1,
            "single-region detachment rejection changed request shape"
        );
        let atlas = atlas_writes.pop().expect("one atlas region was checked");
        (atlas.data, visual_voxels, self.error)
    }
}

pub(super) struct PreparedTerrainDetachment {
    atlas_writes: Vec<PreparedAtlasWrite>,
    publication: VisibleTerrainPublication,
    visual_voxels: Vec<(UVec3, u8)>,
    detached_voxels: usize,
}

pub(super) struct CommittedTerrainDetachment {
    pub(super) invalidation_us: f64,
    pub(super) publication_us: f64,
    pub(super) particle_spawn_us: f64,
    pub(super) detached_voxels: usize,
    pub(super) spawned_particles: usize,
}

impl PreparedTerrainDetachment {
    pub(super) fn prepare(
        request: TerrainDetachmentRequest,
    ) -> Result<Self, RejectedTerrainDetachment> {
        let prepared = (|| {
            anyhow::ensure!(
                !request.atlas_writes.is_empty(),
                "terrain detachment requires at least one prepared atlas write"
            );
            request
                .atlas_writes
                .iter()
                .try_for_each(|write| -> anyhow::Result<()> {
                    PreparedAtlasWrite::validate(
                        request.world_dim,
                        write.origin,
                        write.dim,
                        &write.data,
                    )
                })?;
            anyhow::ensure!(
                request.visual_voxels.len() <= request.detached_voxels,
                "terrain detachment cannot visualize more voxels than it clears"
            );
            let change = VisibleTerrainChange::from_build_edits(request.publication_edits.clone())?
                .context("terrain detachment has no visible terrain chunks")?;
            Ok(VisibleTerrainPublication::edit(change)?)
        })();
        let publication = match prepared {
            Ok(publication) => publication,
            Err(error) => return Err(RejectedTerrainDetachment { request, error }),
        };
        Ok(Self {
            atlas_writes: request
                .atlas_writes
                .into_iter()
                .map(|write| {
                    PreparedAtlasWrite::prepare(
                        request.world_dim,
                        write.origin,
                        write.dim,
                        write.data,
                    )
                    .expect("atlas write was validated before detachment publication")
                })
                .collect(),
            publication,
            visual_voxels: request.visual_voxels,
            detached_voxels: request.detached_voxels,
        })
    }

    pub(super) fn from_all_selected_voxels(
        plain_builder: &mut PlainBuilder,
        world_dim: UVec3,
        selected_voxels: Vec<(UVec3, u8)>,
        affected_bound: UAabb3,
    ) -> anyhow::Result<Self> {
        let detached_voxels = selected_voxels.len();
        let atlas_writes =
            prepare_detached_voxel_clear(plain_builder, world_dim, &selected_voxels)?;
        let request = TerrainDetachmentRequest::from_regions(
            world_dim,
            atlas_writes,
            affected_bound,
            selected_voxels,
            detached_voxels,
        );
        Self::prepare(request).map_err(RejectedTerrainDetachment::into_error)
    }

    pub(super) fn from_cleared_and_visual_voxels(
        plain_builder: &mut PlainBuilder,
        world_dim: UVec3,
        cleared_voxels: &[(UVec3, u8)],
        visual_voxels: Vec<(UVec3, u8)>,
        affected_bound: UAabb3,
    ) -> anyhow::Result<Self> {
        let request = TerrainDetachmentRequest::from_regions(
            world_dim,
            prepare_detached_voxel_clear(plain_builder, world_dim, cleared_voxels)?,
            affected_bound,
            visual_voxels,
            cleared_voxels.len(),
        );
        Self::prepare(request).map_err(RejectedTerrainDetachment::into_error)
    }

    pub(super) fn commit(self, app: &mut App) -> CommittedTerrainDetachment {
        self.commit_atlas(app)
            .commit_publication(app)
            .commit_visuals(app)
    }

    fn commit_atlas(self, app: &mut App) -> AtlasCommittedTerrainDetachment {
        let invalidation_started = Instant::now();
        for write in self.atlas_writes {
            app.plain_builder
                .write_chunk_atlas_region(write.origin, write.dim, &write.data)
                .unwrap_or_else(|error| {
                    panic!(
                        "terrain connectivity atlas commit failed after entering non-rollbackable state: {error:#}"
                    )
                });
        }
        AtlasCommittedTerrainDetachment {
            publication: self.publication,
            visual_voxels: self.visual_voxels,
            detached_voxels: self.detached_voxels,
            invalidation_us: invalidation_started.elapsed().as_secs_f64() * 1_000_000.0,
        }
    }
}

struct AtlasCommittedTerrainDetachment {
    publication: VisibleTerrainPublication,
    visual_voxels: Vec<(UVec3, u8)>,
    detached_voxels: usize,
    invalidation_us: f64,
}

impl AtlasCommittedTerrainDetachment {
    fn commit_publication(self, app: &mut App) -> PublishedTerrainDetachment {
        let publication_started = Instant::now();
        app.commit_prepared_visible_terrain(self.publication);
        PublishedTerrainDetachment {
            visual_voxels: self.visual_voxels,
            detached_voxels: self.detached_voxels,
            invalidation_us: self.invalidation_us,
            publication_us: publication_started.elapsed().as_secs_f64() * 1_000_000.0,
        }
    }
}

struct PublishedTerrainDetachment {
    visual_voxels: Vec<(UVec3, u8)>,
    detached_voxels: usize,
    invalidation_us: f64,
    publication_us: f64,
}

impl PublishedTerrainDetachment {
    fn commit_visuals(self, app: &mut App) -> CommittedTerrainDetachment {
        let particle_started = Instant::now();
        let spawned_particles = app.spawn_detached_terrain_voxel_particles(&self.visual_voxels);
        let particle_spawn_us = particle_started.elapsed().as_secs_f64() * 1_000_000.0;
        assert_eq!(
            spawned_particles,
            self.visual_voxels.len(),
            "terrain connectivity cleared {} voxels but spawned only {spawned_particles} of {} visual particles",
            self.detached_voxels,
            self.visual_voxels.len(),
        );
        CommittedTerrainDetachment {
            invalidation_us: self.invalidation_us,
            publication_us: self.publication_us,
            particle_spawn_us,
            detached_voxels: self.detached_voxels,
            spawned_particles,
        }
    }
}

::static_assertions::assert_not_impl_any!(PreparedTerrainDetachment: Clone, Copy);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::UAabb3;
    use glam::UVec3;

    #[test]
    fn sealed_prepare_returns_the_exact_rejected_region_payload() {
        let world_dim = UVec3::splat(8);
        let atlas_data = vec![7; 7];
        let atlas_address = atlas_data.as_ptr();
        let visual_voxels = vec![(UVec3::ONE, 7)];
        let visual_address = visual_voxels.as_ptr();
        let request = TerrainDetachmentRequest::single_region(
            world_dim,
            UVec3::ZERO,
            UVec3::splat(2),
            atlas_data,
            visual_voxels,
            1,
            UAabb3::new(UVec3::ZERO, UVec3::ONE),
        );

        let rejected = match PreparedTerrainDetachment::prepare(request) {
            Ok(_) => panic!("invalid atlas byte count unexpectedly prepared"),
            Err(rejected) => rejected,
        };

        assert_eq!(
            rejected.request.atlas_writes[0].data.as_ptr(),
            atlas_address
        );
        assert_eq!(rejected.request.visual_voxels.as_ptr(), visual_address);
        assert_eq!(rejected.request.atlas_writes[0].data, vec![7; 7]);
    }

    #[test]
    fn sealed_prepare_rejects_empty_and_out_of_world_atlas_regions() {
        let cases = [
            (
                UVec3::ZERO,
                UVec3::ZERO,
                Vec::new(),
                "cannot prepare an empty atlas write",
            ),
            (
                UVec3::splat(7),
                UVec3::splat(2),
                vec![7; 8],
                "atlas write is outside the world",
            ),
        ];

        for (origin, dim, atlas_data, expected_error) in cases {
            let request = TerrainDetachmentRequest::single_region(
                UVec3::splat(8),
                origin,
                dim,
                atlas_data,
                Vec::new(),
                0,
                UAabb3::new(UVec3::ZERO, UVec3::ONE),
            );
            let rejected = match PreparedTerrainDetachment::prepare(request) {
                Ok(_) => panic!("invalid atlas region unexpectedly prepared"),
                Err(rejected) => rejected,
            };

            assert!(
                rejected.error.to_string().contains(expected_error),
                "unexpected atlas validation error: {:#}",
                rejected.error,
            );
        }
    }

    #[test]
    fn sealed_prepare_owns_the_exact_valid_region_payload() {
        let world_dim = UVec3::splat(8);
        let atlas_data = vec![7; 8];
        let atlas_address = atlas_data.as_ptr();
        let visual_voxels = vec![(UVec3::ONE, 7)];
        let visual_address = visual_voxels.as_ptr();
        let request = TerrainDetachmentRequest::single_region(
            world_dim,
            UVec3::ZERO,
            UVec3::splat(2),
            atlas_data,
            visual_voxels,
            1,
            UAabb3::new(UVec3::ZERO, UVec3::ONE),
        );

        let prepared = match PreparedTerrainDetachment::prepare(request) {
            Ok(prepared) => prepared,
            Err(rejected) => panic!("valid region was rejected: {:#}", rejected.error),
        };

        assert_eq!(prepared.atlas_writes[0].data.as_ptr(), atlas_address);
        assert_eq!(prepared.visual_voxels.as_ptr(), visual_address);
    }

    #[test]
    fn sealed_prepare_returns_the_exact_payload_when_publication_is_invalid() {
        let atlas_data = vec![7; 8];
        let atlas_address = atlas_data.as_ptr();
        let visual_voxels = vec![(UVec3::ONE, 7)];
        let visual_address = visual_voxels.as_ptr();
        let mut request = TerrainDetachmentRequest::single_region(
            UVec3::splat(8),
            UVec3::ZERO,
            UVec3::splat(2),
            atlas_data,
            visual_voxels,
            1,
            UAabb3::new(UVec3::ZERO, UVec3::ONE),
        );
        request.publication_edits.clear();

        let rejected = match PreparedTerrainDetachment::prepare(request) {
            Ok(_) => panic!("missing publication edit unexpectedly prepared"),
            Err(rejected) => rejected,
        };

        assert_eq!(
            rejected.request.atlas_writes[0].data.as_ptr(),
            atlas_address
        );
        assert_eq!(rejected.request.visual_voxels.as_ptr(), visual_address);
    }
}

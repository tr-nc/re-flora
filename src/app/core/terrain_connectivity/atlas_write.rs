use crate::builder::PlainBuilder;
use crate::geom::UAabb3;
use glam::UVec3;

type AtlasRegion = (UVec3, UVec3, Vec<u8>);

struct PreparedAtlasRegion {
    origin: UVec3,
    dim: UVec3,
    data: Vec<u8>,
}

pub(super) struct PreparedAtlasWrite {
    regions: Vec<PreparedAtlasRegion>,
}

pub(super) struct CommittedAtlasWrite;

impl PreparedAtlasWrite {
    pub(super) fn prepare(
        world_dim: UVec3,
        regions: Vec<AtlasRegion>,
    ) -> Result<Self, (Vec<AtlasRegion>, anyhow::Error)> {
        let validated = (|| {
            anyhow::ensure!(
                !regions.is_empty(),
                "connectivity requires at least one atlas write"
            );
            for (origin, dim, data) in &regions {
                anyhow::ensure!(
                    dim.cmpgt(UVec3::ZERO).all(),
                    "connectivity cannot prepare an empty atlas write"
                );
                anyhow::ensure!(
                    origin.cmple(world_dim).all() && dim.cmple(world_dim - *origin).all(),
                    "connectivity atlas write is outside the world: origin={origin:?} dim={dim:?} world={world_dim:?}",
                );
                let expected =
                    usize::try_from(super::voxel_count(UAabb3::new(*origin, *origin + *dim)))?;
                anyhow::ensure!(
                    data.len() == expected,
                    "connectivity atlas write has {} bytes, expected {expected}",
                    data.len(),
                );
            }
            Ok(())
        })();
        if let Err(error) = validated {
            return Err((regions, error));
        }
        Ok(Self {
            regions: regions
                .into_iter()
                .map(|(origin, dim, data)| PreparedAtlasRegion { origin, dim, data })
                .collect(),
        })
    }

    pub(super) fn commit(self, builder: &mut PlainBuilder) -> anyhow::Result<CommittedAtlasWrite> {
        for region in self.regions {
            builder.write_chunk_atlas_region(region.origin, region.dim, &region.data)?;
        }
        Ok(CommittedAtlasWrite)
    }

    #[cfg(test)]
    pub(super) fn data_ptr(&self, index: usize) -> *const u8 {
        self.regions[index].data.as_ptr()
    }
}

::static_assertions::assert_not_impl_any!(PreparedAtlasWrite: Clone, Copy);

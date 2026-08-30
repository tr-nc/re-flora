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
}

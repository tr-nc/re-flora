use anyhow::{ensure, Result};
use glam::{UVec2, UVec3, Vec3};

pub const DDGI_IRRADIANCE_INTERIOR_SIDE: u32 = 8;
pub const DDGI_VISIBILITY_INTERIOR_SIDE: u32 = 16;
pub const DDGI_TILE_GUTTER: u32 = 1;
pub const DDGI_IRRADIANCE_STORED_SIDE: u32 = DDGI_IRRADIANCE_INTERIOR_SIDE + 2 * DDGI_TILE_GUTTER;
pub const DDGI_VISIBILITY_STORED_SIDE: u32 = DDGI_VISIBILITY_INTERIOR_SIDE + 2 * DDGI_TILE_GUTTER;
pub const DDGI_RELOCATION_WORKGROUP_SIZE: u32 = 64;
pub const DDGI_TRACE_WORKGROUP_SIZE: u32 = 64;
pub const DDGI_GUTTER_WORKGROUP_SIZE: u32 = 64;

const _: () = assert!(super::DDGI_RAYS_PER_PROBE > 0);
const _: () = assert!(super::DDGI_RAYS_PER_PROBE.is_multiple_of(DDGI_TRACE_WORKGROUP_SIZE));

pub const SUPPORTED_DDGI_SPACINGS_VOXELS: [u32; 4] = [64, 32, 16, 8];
pub const DEFAULT_DDGI_SPACING_VOXELS: u32 = 32;

/// A supported DDGI Probe spacing at the runtime seam.
///
/// Raw voxel counts belong to configuration, diagnostics, and capture wire formats. Runtime
/// scheduling and physical Volume ownership carry this type so an invalid spacing cannot enter a
/// queued or in-flight build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdgiProbeSpacing(u32);

impl DdgiProbeSpacing {
    pub const fn voxels(self) -> u32 {
        self.0
    }
}

impl TryFrom<u32> for DdgiProbeSpacing {
    type Error = String;

    fn try_from(spacing_voxels: u32) -> Result<Self, Self::Error> {
        if SUPPORTED_DDGI_SPACINGS_VOXELS.contains(&spacing_voxels) {
            Ok(Self(spacing_voxels))
        } else {
            Err(format!(
                "Unsupported DDGI spacing {spacing_voxels}. Supported values: {}",
                supported_ddgi_spacings_label()
            ))
        }
    }
}

pub fn validate_ddgi_spacing(spacing_voxels: u32) -> Result<u32, String> {
    DdgiProbeSpacing::try_from(spacing_voxels).map(DdgiProbeSpacing::voxels)
}

pub fn supported_ddgi_spacings_label() -> String {
    SUPPORTED_DDGI_SPACINGS_VOXELS
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdgiVolumeGrid {
    world_extent_voxels: UVec3,
    spacing: DdgiProbeSpacing,
    dimensions: UVec3,
    probe_count: u32,
}

impl DdgiVolumeGrid {
    pub fn new(world_extent_voxels: UVec3, spacing: DdgiProbeSpacing) -> Result<Self> {
        let spacing_voxels = spacing.voxels();
        ensure!(
            world_extent_voxels.cmpgt(UVec3::ZERO).all(),
            "DDGI world extent must be non-zero"
        );
        ensure!(
            world_extent_voxels % UVec3::splat(spacing_voxels) == UVec3::ZERO,
            "DDGI spacing {spacing_voxels} must divide world extent {world_extent_voxels:?}"
        );

        let dimensions = world_extent_voxels / spacing_voxels + UVec3::ONE;
        let probe_count = dimensions.x as u64 * dimensions.y as u64 * dimensions.z as u64;
        ensure!(
            probe_count <= u32::MAX as u64,
            "DDGI probe count exceeds u32"
        );

        Ok(Self {
            world_extent_voxels,
            spacing,
            dimensions,
            probe_count: probe_count as u32,
        })
    }

    #[allow(dead_code)]
    pub fn world_extent_voxels(self) -> UVec3 {
        self.world_extent_voxels
    }

    #[allow(dead_code)]
    pub fn spacing_voxels(self) -> u32 {
        self.spacing.voxels()
    }

    pub(crate) fn spacing(self) -> DdgiProbeSpacing {
        self.spacing
    }

    pub fn dimensions(self) -> UVec3 {
        self.dimensions
    }

    pub fn probe_count(self) -> u32 {
        self.probe_count
    }

    #[allow(dead_code)]
    pub fn flatten(self, coordinate: UVec3) -> Option<u32> {
        coordinate.cmplt(self.dimensions).all().then(|| {
            coordinate.x + self.dimensions.x * (coordinate.y + self.dimensions.y * coordinate.z)
        })
    }

    #[allow(dead_code)]
    pub fn unflatten(self, index: u32) -> Option<UVec3> {
        if index >= self.probe_count {
            return None;
        }

        let layer_size = self.dimensions.x * self.dimensions.y;
        let z = index / layer_size;
        let layer_index = index % layer_size;
        Some(UVec3::new(
            layer_index % self.dimensions.x,
            layer_index / self.dimensions.x,
            z,
        ))
    }

    #[allow(dead_code)]
    pub fn nominal_voxel_position(self, index: u32) -> Option<Vec3> {
        self.unflatten(index)
            .map(|coordinate| coordinate.as_vec3() * self.spacing.voxels() as f32)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdgiAtlasLayout {
    probe_count: u32,
    interior_side: u32,
    stored_side: u32,
    tile_columns: u32,
    tile_rows: u32,
}

impl DdgiAtlasLayout {
    pub fn new(probe_count: u32, interior_side: u32) -> Result<Self> {
        ensure!(probe_count > 0, "DDGI atlas requires at least one probe");
        ensure!(interior_side > 0, "DDGI atlas interior must be non-zero");

        let floor_root = probe_count.isqrt();
        let tile_columns = floor_root + u32::from(floor_root * floor_root < probe_count);
        let tile_rows = probe_count.div_ceil(tile_columns);
        let stored_side = interior_side
            .checked_add(2 * DDGI_TILE_GUTTER)
            .ok_or_else(|| anyhow::anyhow!("DDGI stored tile side exceeds u32"))?;

        ensure!(
            tile_columns.checked_mul(stored_side).is_some()
                && tile_rows.checked_mul(stored_side).is_some(),
            "DDGI atlas extent exceeds u32"
        );

        Ok(Self {
            probe_count,
            interior_side,
            stored_side,
            tile_columns,
            tile_rows,
        })
    }

    #[allow(dead_code)]
    pub fn probe_count(self) -> u32 {
        self.probe_count
    }

    #[allow(dead_code)]
    pub fn interior_side(self) -> u32 {
        self.interior_side
    }

    #[allow(dead_code)]
    pub fn stored_side(self) -> u32 {
        self.stored_side
    }

    pub fn tile_grid(self) -> UVec2 {
        UVec2::new(self.tile_columns, self.tile_rows)
    }

    pub fn extent(self) -> UVec2 {
        self.tile_grid() * self.stored_side
    }

    pub fn tile_origin(self, probe_index: u32) -> Option<UVec2> {
        (probe_index < self.probe_count).then(|| {
            UVec2::new(
                probe_index % self.tile_columns,
                probe_index / self.tile_columns,
            ) * self.stored_side
        })
    }

    #[allow(dead_code)]
    pub fn interior_origin(self, probe_index: u32) -> Option<UVec2> {
        self.tile_origin(probe_index)
            .map(|origin| origin + UVec2::splat(DDGI_TILE_GUTTER))
    }

    #[allow(dead_code)]
    pub fn stored_texel(self, probe_index: u32, coordinate: UVec2) -> Option<UVec2> {
        if coordinate.cmplt(UVec2::splat(self.stored_side)).all() {
            self.tile_origin(probe_index)
                .map(|origin| origin + coordinate)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_spacing_accepts_only_supported_runtime_values() {
        for spacing_voxels in SUPPORTED_DDGI_SPACINGS_VOXELS {
            let spacing = DdgiProbeSpacing::try_from(spacing_voxels).unwrap();
            assert_eq!(spacing.voxels(), spacing_voxels);
        }

        for spacing_voxels in [0, 1, 7, 15, 24, 65, u32::MAX] {
            assert!(DdgiProbeSpacing::try_from(spacing_voxels).is_err());
        }
    }

    #[test]
    fn planned_volume_dimensions_and_counts_are_exact() {
        for (spacing, side, count) in [
            (64, 9, 729),
            (32, 17, 4_913),
            (16, 33, 35_937),
            (8, 65, 274_625),
        ] {
            let grid = DdgiVolumeGrid::new(
                UVec3::splat(512),
                DdgiProbeSpacing::try_from(spacing).unwrap(),
            )
            .unwrap();
            assert_eq!(grid.world_extent_voxels(), UVec3::splat(512));
            assert_eq!(grid.spacing_voxels(), spacing);
            assert_eq!(grid.dimensions(), UVec3::splat(side));
            assert_eq!(grid.probe_count(), count);
        }
    }

    #[test]
    fn flatten_and_unflatten_round_trip_every_spacing_16_probe() {
        let grid = DdgiVolumeGrid::new(UVec3::splat(512), DdgiProbeSpacing::try_from(16).unwrap())
            .unwrap();
        for index in 0..grid.probe_count() {
            let coordinate = grid.unflatten(index).unwrap();
            assert_eq!(grid.flatten(coordinate), Some(index));
            assert_eq!(
                grid.nominal_voxel_position(index),
                Some(coordinate.as_vec3() * 16.0)
            );
        }
        assert_eq!(grid.unflatten(grid.probe_count()), None);
        assert_eq!(grid.flatten(grid.dimensions()), None);
    }

    #[test]
    fn atlas_tiles_are_dense_unique_and_in_bounds() {
        let probe_count = 35_937;
        let layout = DdgiAtlasLayout::new(probe_count, DDGI_VISIBILITY_INTERIOR_SIDE).unwrap();
        assert_eq!(layout.stored_side(), DDGI_VISIBILITY_STORED_SIDE);
        assert_eq!(layout.tile_grid(), UVec2::new(190, 190));
        assert_eq!(layout.extent(), UVec2::new(3_420, 3_420));

        for index in 0..probe_count {
            let origin = layout.tile_origin(index).unwrap();
            assert!(origin.cmplt(layout.extent()).all());
            if index > 0 {
                assert_ne!(origin, layout.tile_origin(index - 1).unwrap());
            }
            assert_eq!(layout.interior_origin(index), Some(origin + UVec2::ONE));
        }
        assert_eq!(layout.tile_origin(probe_count), None);
    }

    #[test]
    fn planned_spacing_8_atlases_fit_in_expected_extents() {
        let count = DdgiVolumeGrid::new(UVec3::splat(512), DdgiProbeSpacing::try_from(8).unwrap())
            .unwrap()
            .probe_count();
        let irradiance = DdgiAtlasLayout::new(count, DDGI_IRRADIANCE_INTERIOR_SIDE).unwrap();
        let visibility = DdgiAtlasLayout::new(count, DDGI_VISIBILITY_INTERIOR_SIDE).unwrap();

        assert_eq!(irradiance.stored_side(), DDGI_IRRADIANCE_STORED_SIDE);
        assert_eq!(irradiance.tile_grid(), UVec2::new(525, 524));
        assert_eq!(irradiance.extent(), UVec2::new(5_250, 5_240));
        assert_eq!(visibility.extent(), UVec2::new(9_450, 9_432));
    }
}

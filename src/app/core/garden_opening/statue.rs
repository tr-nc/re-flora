//! Editable temple sculpture and its saved, material-only live comparison.
//! Terrain remains authoritative: toggling never repairs holes or overwrites foreign materials.
use super::super::App;
use crate::{
    app::world_edits::{TerrainBrushEdit, VoxelEdit, WorldEditTransaction},
    builder::{VOXEL_TYPE_GLASS, VOXEL_TYPE_LIMESTONE, VOXEL_TYPE_MASK},
    gameplay::CameraPose,
    geom::UAabb3,
};
use anyhow::{ensure, Result};
use glam::{UVec3, Vec3, Vec3Swizzles};
use serde::{Deserialize, Serialize};

mod origin {
    use super::*;
    pub fn serialize<S: serde::Serializer>(
        value: &Vec3,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        value.to_array().serialize(serializer)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Vec3, D::Error> {
        <[f32; 3]>::deserialize(deserializer).map(Vec3::from_array)
    }
}
mod geometry;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::app::core) struct TempleStatue {
    #[serde(with = "origin")]
    origin: Vec3,
    scale: f32,
    pub glass: bool,
}
impl TempleStatue {
    pub(super) fn new(origin: Vec3, scale: f32, glass: bool) -> Self {
        Self {
            origin,
            scale,
            glass,
        }
    }
    pub(in crate::app::core) fn validate(&self, world: UVec3) -> Result<()> {
        ensure!(
            self.origin.is_finite() && self.scale.is_finite() && (0.5..=1.).contains(&self.scale),
            "invalid temple sculpture placement"
        );
        let min = self.origin + geometry::MIN * self.scale;
        let max = self.origin + geometry::MAX * self.scale;
        ensure!(
            min.cmpge(Vec3::ZERO).all() && max.cmple(world.as_vec3()).all(),
            "temple sculpture outside world"
        );
        Ok(())
    }
    fn bound(&self) -> UAabb3 {
        UAabb3::new(
            (self.origin + geometry::MIN * self.scale)
                .floor()
                .as_uvec3(),
            (self.origin + geometry::MAX * self.scale).ceil().as_uvec3(),
        )
    }
    fn map(&self, mut visit: impl FnMut(usize, u8)) {
        let shape = geometry::Sculpture::new();
        let bound = self.bound();
        let dim = bound.max() - bound.min();
        for z in 0..dim.z {
            for y in 0..dim.y {
                for x in 0..dim.x {
                    let voxel = bound.min() + UVec3::new(x, y, z);
                    let local = (voxel.as_vec3() + Vec3::splat(0.5) - self.origin) / self.scale;
                    visit(
                        (x + dim.x * (y + dim.y * z)) as usize,
                        shape.material(local),
                    );
                }
            }
        }
    }
    pub(super) fn initial_edit(&self) -> VoxelEdit {
        let b = self.bound();
        let mut bytes = vec![0; (b.max() - b.min()).element_product() as usize];
        self.map(|i, material| {
            bytes[i] = if material == VOXEL_TYPE_LIMESTONE as u8 && self.glass {
                VOXEL_TYPE_GLASS as u8
            } else {
                material
            }
        });
        self.edit(bytes)
    }
    fn edit(&self, bytes: Vec<u8>) -> VoxelEdit {
        let b = self.bound();
        VoxelEdit::RestoreAtlasRegion {
            origin: b.min(),
            dim: b.max() - b.min(),
            data: bytes.into(),
        }
    }
    fn recolor(&self, bytes: &mut [u8], glass: bool) -> Result<usize> {
        let b = self.bound();
        ensure!(
            bytes.len() == (b.max() - b.min()).element_product() as usize,
            "invalid statue material readback"
        );
        let from = if self.glass {
            VOXEL_TYPE_GLASS
        } else {
            VOXEL_TYPE_LIMESTONE
        } as u8;
        let to = if glass {
            VOXEL_TYPE_GLASS
        } else {
            VOXEL_TYPE_LIMESTONE
        } as u8;
        let mut changed = 0;
        self.map(|i, material| {
            if material == VOXEL_TYPE_LIMESTONE as u8 && bytes[i] & VOXEL_TYPE_MASK == from {
                bytes[i] = to;
                changed += 1;
            }
        });
        Ok(changed)
    }
    pub(super) fn camera(&self) -> CameraPose {
        let eye = (self.origin + Vec3::new(0., 37., 75.) * self.scale) / 256.;
        let target = (self.origin + Vec3::new(0., 34., -25.) * self.scale) / 256.;
        let d = target - eye;
        CameraPose {
            position: eye,
            yaw_deg: d.x.atan2(-d.z).to_degrees(),
            pitch_deg: d.y.atan2(d.xz().length()).to_degrees(),
            fov_deg: 56.,
        }
    }
}

impl App {
    pub(in crate::app::core) fn sync_temple_statue_material(&mut self) -> Result<()> {
        let glass = self.debug_settings.adjustables.temple_statue_glass.value;
        let Some(statue) = self.temple_statue.clone() else {
            return Ok(());
        };
        if statue.glass == glass || !self.terrain_persistence.can_start_operation() {
            return Ok(());
        }
        let b = statue.bound();
        let mut bytes = self
            .plain_builder
            .read_chunk_atlas_region(b.min(), b.max() - b.min())?;
        let changed = statue.recolor(&mut bytes, glass)?;
        if changed > 0 {
            self.execute_world_edit(WorldEditTransaction::terrain_change(
                vec![statue.edit(bytes)],
                b,
            ))?;
        }
        self.temple_statue.as_mut().unwrap().glass = glass;
        log::info!("[TEMPLE_STATUE] glass={glass} changed_voxels={changed} editable=true preserved_holes=true material_only=true");
        Ok(())
    }

    pub(in crate::app::core) fn review_temple_statue(&mut self) -> Result<()> {
        let Ok(mode) = std::env::var("RE_FLORA_GARDEN_SCULPTURE") else {
            return Ok(());
        };
        ensure!(
            matches!(mode.as_str(), "glass" | "audit"),
            "statue review must be glass or audit"
        );
        let statue = self
            .temple_statue
            .clone()
            .ok_or_else(|| anyhow::anyhow!("statue review needs a temple sculpture"))?;
        let b = statue.bound();
        if mode == "audit" {
            // The real Dig interface, not a synthetic missing-voxel fixture.
            self.debug_settings.adjustables.temple_statue_glass.value = false;
            self.sync_temple_statue_material()?;
            let point = (statue.origin + Vec3::new(0.5, 50.5, -12.5) * statue.scale) / 256.;
            let result = self.apply_surface_terrain_removal(
                TerrainBrushEdit {
                    start: point,
                    end: point,
                    radius: 1.6 / 256.,
                },
                Some(VOXEL_TYPE_LIMESTONE),
                None,
                None,
            )?;
            ensure!(
                result.stats.count_removed(VOXEL_TYPE_LIMESTONE) > 0,
                "sculpture Dig removed no stone"
            );
            let before = self
                .plain_builder
                .read_chunk_atlas_region(b.min(), b.max() - b.min())?;
            let ray_origin = (statue.origin + Vec3::new(12.5, 35.5, 20.5) * statue.scale) / 256.;
            // CPU picking is a derived asynchronous cache; inspect its published Ready state.
            self.contree_builder.flush_cpu_chunk_cache_jobs();
            let opaque_hit = self
                .contree_builder
                .query_terrain_ray_cpu(ray_origin, -Vec3::Z)
                .ok_or_else(|| {
                    anyhow::anyhow!("opaque sculpture is absent from physical queries")
                })?;
            ensure!(
                opaque_hit.voxel_type == VOXEL_TYPE_LIMESTONE,
                "opaque sculpture has the wrong physical material"
            );
            self.debug_settings.adjustables.temple_statue_glass.value = true;
            self.sync_temple_statue_material()?;
            let glass = self
                .plain_builder
                .read_chunk_atlas_region(b.min(), b.max() - b.min())?;
            self.contree_builder.flush_cpu_chunk_cache_jobs();
            let glass_hit = self
                .contree_builder
                .query_terrain_ray_cpu(ray_origin, -Vec3::Z)
                .ok_or_else(|| {
                    anyhow::anyhow!("Glass sculpture is absent from physical queries")
                })?;
            ensure!(
                glass_hit.voxel_type == VOXEL_TYPE_GLASS
                    && glass_hit.position.distance(opaque_hit.position) < 1e-6,
                "material preview changed physical geometry or left stale physical material"
            );
            ensure!(
                before.iter().zip(&glass).all(|(&a, &b)| a != 0 || b == 0),
                "Glass preview repaired a hole"
            );
            self.debug_settings.adjustables.temple_statue_glass.value = false;
            self.sync_temple_statue_material()?;
            let after = self
                .plain_builder
                .read_chunk_atlas_region(b.min(), b.max() - b.min())?;
            ensure!(
                before == after,
                "sculpture material round trip changed geometry"
            );
            log::info!("[TEMPLE_STATUE_AUDIT] dig_removed={} opaque_glass_opaque=byte_identical holes_preserved=true collision_and_render_publication=true", result.stats.count_removed(VOXEL_TYPE_LIMESTONE));
        } else {
            self.debug_settings.adjustables.temple_statue_glass.value = true;
            self.sync_temple_statue_material()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn material_preview_preserves_carved_air_and_foreign_edits_without_rebuilding() {
        for scale in [0.5, 1.] {
            let mut s = TempleStatue::new(Vec3::new(256., 180., 256.), scale, false);
            let VoxelEdit::RestoreAtlasRegion { data, .. } = s.initial_edit() else {
                unreachable!()
            };
            let mut bytes = data.to_vec();
            for (i, voxel) in bytes.iter_mut().enumerate() {
                if i % 53 == 0 {
                    *voxel = 0;
                } else if i % 79 == 0 {
                    *voxel = crate::builder::VOXEL_TYPE_OAK_WOOD as u8;
                }
            }
            let before = bytes.clone();
            assert!(s.recolor(&mut bytes, true).unwrap() > 500);
            assert!(before
                .iter()
                .zip(&bytes)
                .all(|(&a, &b)| a == VOXEL_TYPE_LIMESTONE as u8 || a == b));
            s.glass = true;
            s.recolor(&mut bytes, false).unwrap();
            assert_eq!(bytes, before);
            let persisted = serde_json::to_vec(&s).unwrap();
            let restored: TempleStatue = serde_json::from_slice(&persisted).unwrap();
            restored.validate(UVec3::new(1024, 512, 1024)).unwrap();
            assert_eq!(restored.bound(), s.bound());
        }
    }
    #[test]
    fn invalid_persisted_statue_cannot_address_outside_the_world() {
        for s in [
            TempleStatue::new(Vec3::splat(f32::NAN), 1., false),
            TempleStatue::new(Vec3::ZERO, 1., false),
            TempleStatue::new(Vec3::splat(512.), 1., false),
            TempleStatue::new(Vec3::splat(256.), 10., false),
        ] {
            assert!(s.validate(UVec3::new(1024, 512, 1024)).is_err());
        }
    }
}

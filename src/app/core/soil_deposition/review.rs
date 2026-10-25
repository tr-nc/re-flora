//! Opt-in GPU regression: Soil may remove buried roots, never untouched neighboring grass.
use super::*;
use anyhow::ensure;
use std::collections::BTreeSet;

fn grass_roots(snapshot: &crate::builder::FloraSnapshot) -> Result<BTreeSet<[u32; 3]>> {
    let json = serde_json::to_value(snapshot)?;
    let mut roots = BTreeSet::new();
    for chunk in json["chunks"].as_array().unwrap() {
        let c = chunk["coordinate"].as_array().unwrap();
        let origin = UVec3::new(
            c[0].as_u64().unwrap() as u32,
            c[1].as_u64().unwrap() as u32,
            c[2].as_u64().unwrap() as u32,
        ) * 256;
        for species in chunk["species"].as_array().unwrap().iter().take(2) {
            for instance in species["instances"].as_array().unwrap() {
                let p = instance[0].as_u64().unwrap() as u32;
                roots.insert(
                    (origin + UVec3::new(p & 255, (p >> 8) & 255, (p >> 16) & 255)).to_array(),
                );
            }
        }
    }
    Ok(roots)
}

impl App {
    pub(in crate::app::core) fn review_soil_grass_preservation(&mut self) -> Result<()> {
        ensure!(
            self.rooftop_scene.is_some(),
            "soil grass review requires --rooftop-poc"
        );
        for fast in [false, true] {
            self.debug_settings
                .adjustables
                .soil_deposition_fast_path
                .value = fast;
            for (offset, volume, spread) in [
                (UVec3::new(352, rooftop_scene::SOIL_MIN.y, 352), 1, 0.),
                (UVec3::new(464, 255, 464), 64, 13.),
            ] {
                self.review_soil_grass_case(offset, volume, spread)?;
            }
        }
        log::info!("[SOIL_GRASS_REVIEW] passed cases=4");
        Ok(())
    }

    fn review_soil_grass_case(&mut self, offset: UVec3, volume: u32, spread: f32) -> Result<()> {
        let dim = UVec3::new(96, 8, 96);
        let mut terrain = vec![0u8; dim.element_product() as usize];
        for z in 0..dim.z {
            for x in 0..dim.x {
                terrain[((z * dim.y) * dim.x + x) as usize] = crate::builder::VOXEL_TYPE_DIRT as u8;
            }
        }
        if offset.y > rooftop_scene::SOIL_MIN.y {
            let base = UVec3::new(offset.x, rooftop_scene::SOIL_MIN.y, offset.z);
            let support_dim = UVec3::new(dim.x, offset.y - base.y, dim.z);
            self.plain_builder.write_chunk_atlas_region(
                base,
                support_dim,
                &vec![
                    crate::builder::VOXEL_TYPE_DIRT as u8;
                    support_dim.element_product() as usize
                ],
            )?;
        }
        self.plain_builder
            .write_chunk_atlas_region(offset, dim, &terrain)?;
        self.plain_builder.mark_all_solid_workgroups_dirty();
        let center = (offset + UVec3::new(48, 0, 48)).as_vec3() / 256.;
        self.publish_visible_terrain(VisibleTerrainChange::preserving_flora(
            UAabb3::new(offset, offset + dim - UVec3::ONE),
            world_ops::FloraBrushEdit {
                start: center,
                end: center,
                radius: 0.,
                tick: 0,
                spawn_time_ms: 0,
            },
            true,
        ))?;
        let mut json = serde_json::to_value(self.surface_builder.capture_flora_snapshot(0)?)?;
        for chunk in json["chunks"].as_array_mut().unwrap() {
            let c = chunk["coordinate"].as_array().unwrap();
            let origin = UVec3::new(
                c[0].as_u64().unwrap() as u32,
                c[1].as_u64().unwrap() as u32,
                c[2].as_u64().unwrap() as u32,
            ) * 256;
            for species in chunk["species"].as_array_mut().unwrap() {
                species["instances"] = serde_json::json!([]);
                species["authored"] = serde_json::json!([]);
            }
            for z in (4..92).step_by(2) {
                for x in (4..92).step_by(2) {
                    let root = offset + UVec3::new(x, 0, z);
                    if root / 256 != origin / 256 {
                        continue;
                    }
                    let local = root - origin;
                    let packed = local.x | (local.y << 8) | (local.z << 16) | (255 << 24);
                    chunk["species"][((x + z) / 2 % 2) as usize]["instances"]
                        .as_array_mut()
                        .unwrap()
                        .push(serde_json::json!([packed, u32::MAX]));
                }
            }
        }
        let fixture = serde_json::from_value(json)?;
        self.surface_builder.restore_flora_snapshot(&fixture, 0)?;
        let before = grass_roots(&self.surface_builder.capture_flora_snapshot(0)?)?;
        ensure!(
            before.len() == 1936,
            "soil review must have dense grass witnesses"
        );
        self.debug_settings
            .adjustables
            .soil_deposition_clump_radius
            .value = 0;
        let result = self.apply_soil_deposition(
            TerrainBrushEdit {
                start: center,
                end: center,
                radius: spread / 256.,
            },
            volume,
            42,
        )?;
        ensure!(
            result.added == volume,
            "review must deposit the requested voxels"
        );
        let after = grass_roots(&self.surface_builder.capture_flora_snapshot(0)?)?;
        let actual = self.plain_builder.read_chunk_atlas_region(offset, dim)?;
        let unchanged = |root: [u32; 3]| {
            let p = UVec3::from_array(root) - offset;
            [p, p + UVec3::Y].iter().all(|p| {
                let i = ((p.z * dim.y + p.y) * dim.x + p.x) as usize;
                terrain[i] == actual[i]
            })
        };
        let untouched_removed = before.difference(&after).filter(|&&p| unchanged(p)).count();
        log::info!(
            "[SOIL_GRASS_REVIEW] added={} halo={} before={} after={} untouched_removed={}",
            result.added,
            result.sampling_halo,
            before.len(),
            after.len(),
            untouched_removed
        );
        ensure!(untouched_removed == 0, "Soil removed {untouched_removed} grass roots whose support and headroom did not change");
        let buried: BTreeSet<_> = result
            .deposited_voxels
            .iter()
            .map(|p| (*p - UVec3::Y).to_array())
            .collect();
        let expected: BTreeSet<_> = before.difference(&buried).copied().collect();
        ensure!(
            after == expected,
            "only roots buried by actual deposits may disappear"
        );
        Ok(())
    }
}

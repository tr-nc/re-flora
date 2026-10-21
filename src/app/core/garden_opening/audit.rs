//! Opt-in native acceptance through the same collision, Dig, Grow and mow paths as play.
use super::*;
use crate::{
    gameplay::camera::PlayerWalkMovementRequest,
    terrain_persistence::{TerrainSnapshotMetadata, TerrainSnapshotWriter},
};
use glam::{UVec2, UVec3};

impl App {
    pub(in crate::app::core) fn audit_garden_opening(&mut self) -> anyhow::Result<()> {
        let Some(output) = std::env::var_os("RE_FLORA_GARDEN_AUDIT") else {
            return Ok(());
        };
        anyhow::ensure!(
            self.fresh_garden_seed.is_some(),
            "garden audit requires a fresh ordinary launch"
        );
        let output = std::path::PathBuf::from(output);
        anyhow::ensure!(
            output.starts_with("target/garden-delivery/world")
                && !output
                    .components()
                    .any(|c| c == std::path::Component::ParentDir),
            "garden audit output must be beneath target/garden-delivery/world"
        );
        std::fs::create_dir_all(&output)?;
        let size = self.world_chunk_dim.as_vec3();
        let ground = |p: Vec2, app: &Self| {
            let xz = p * Vec2::new(size.x, size.z);
            Vec3::new(xz.x, app.query_terrain_height_cpu(xz), xz.y)
        };
        let mut eye = ground(layout::ARRIVAL, self) + Vec3::Y * 0.085;
        eye = self
            .terrain_physics
            .recover_walk_entry(eye, 0.08)?
            .ok_or_else(|| anyhow::anyhow!("arrival capsule is blocked"))?;
        let mut steps = 0;
        for destination in &layout::ROUTE[1..] {
            let target = ground(*destination, self);
            for _ in 0..300 {
                let horizontal = Vec3::new(target.x - eye.x, 0., target.z - eye.z);
                if horizontal.length() < 0.01 {
                    break;
                }
                let desired =
                    horizontal.normalize() * horizontal.length().min(0.004) - Vec3::Y * 0.006;
                let result = self.terrain_physics.move_player_capsule(
                    PlayerWalkMovementRequest {
                        camera_position: eye,
                        camera_height: 0.08,
                        desired_translation: desired,
                    },
                    1. / 60.,
                )?;
                eye += result.translation;
                steps += 1;
            }
            anyhow::ensure!(
                Vec2::new(eye.x - target.x, eye.z - target.z).length() < 0.012,
                "corridor capsule blocked before {target:?}: eye={eye:?}"
            );
        }
        log::info!("[GARDEN_AUDIT] arrival_and_corridor=walked capsule=true steps={steps} final_eye={eye:?}");

        // Ordinary edit paths in a temporary generated world, well away from arrival.
        let patch = ground(Vec2::new(0.45, 0.78), self);
        let dig = self.apply_surface_terrain_removal(
            TerrainBrushEdit {
                start: patch,
                end: patch,
                radius: 0.025,
            },
            Some(crate::builder::VOXEL_TYPE_DIRT),
            None,
            None,
        )?;
        anyhow::ensure!(
            dig.stats.count_removed(crate::builder::VOXEL_TYPE_DIRT) > 0,
            "Dig did not change garden soil"
        );
        let column = UVec2::new((patch.x * 256.) as u32, (patch.z * 256.) as u32);
        let anchor = self
            .resolve_plantable_surface_column(column)
            .map_err(|e| anyhow::anyhow!("dug soil no longer plantable: {e:?}"))?;
        let species = crate::flora::species::species()
            .iter()
            .position(|s| s.key == layout::FLOWER_KEYS[0])
            .unwrap() as u32;
        let mut batch = AuthoredFloraPlacementBatch::new();
        anyhow::ensure!(
            self.try_place_authored_flora(&mut batch, species, anchor, 255, 0, 0xabc123),
            "Grow did not accept dug soil"
        );
        self.finish_authored_flora_placement(batch)?;
        let planted = self.surface_builder.authored_flora_base_positions().count();
        self.perform_startup_terrain_save(&output.join("dug-planted.rflterrain"))?;
        let point = anchor.surface_position_ws();
        self.apply_surface_flora_removal(TerrainBrushEdit {
            start: point,
            end: point,
            radius: 0.035,
        })?;
        anyhow::ensure!(
            self.surface_builder.authored_flora_base_positions().count() < planted,
            "mow did not remove planted flora"
        );
        self.perform_startup_terrain_save(&output.join("dug-mown.rflterrain"))?;
        log::info!(
            "[GARDEN_AUDIT] dig_removed={} grow=accepted mow=removed ordinary_edit_paths=true",
            dig.stats.count_removed(crate::builder::VOXEL_TYPE_DIRT)
        );

        // A valid minimal saved world: empty vegetation plus fully empty editable terrain.
        // This fixture deliberately contains no recipe metadata: loading emptiness is final.
        let metadata = TerrainSnapshotMetadata::new(
            self.world_chunk_dim.to_array(),
            VOXEL_DIM_PER_CHUNK.to_array(),
        );
        let mut writer =
            TerrainSnapshotWriter::create(output.join("minimal-empty.rflterrain"), metadata)?;
        let mut chunks = Vec::new();
        let empty = vec![
            0;
            (VOXEL_DIM_PER_CHUNK.x * VOXEL_DIM_PER_CHUNK.y * VOXEL_DIM_PER_CHUNK.z)
                as usize
        ];
        for z in 0..self.world_chunk_dim.z {
            for y in 0..self.world_chunk_dim.y {
                for x in 0..self.world_chunk_dim.x {
                    let coordinate = UVec3::new(x, y, z).to_array();
                    writer.write_chunk(coordinate, &empty)?;
                    chunks.push(serde_json::json!({"coordinate": coordinate, "species": crate::flora::species::species().iter()
                .map(|s| serde_json::json!({"key": s.key, "instances": [], "authored": []})).collect::<Vec<_>>() }));
                }
            }
        }
        chunks.sort_by_key(|chunk| {
            let c = chunk["coordinate"].as_array().unwrap();
            [
                c[0].as_u64().unwrap(),
                c[1].as_u64().unwrap(),
                c[2].as_u64().unwrap(),
            ]
        });
        writer.set_garden_data(serde_json::to_vec(&serde_json::json!({
            "flora": {"chunks": chunks}, "trees": {"age": 1.0, "fruit_cycle": 0.4, "trees": []},
            "growth_override_enabled": false, "growth_override": 1.0,
        }))?)?;
        writer.finish()?;
        Ok(())
    }
}

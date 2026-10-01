//! The unsaved rooftop proof: fixed triangle models and a bounded, initially empty soil layer.
use super::vegetation::SurfaceOccupantClearPath;
use super::*;
use crate::app::world_edits::{TerrainBrushEdit, TerrainRemovalEdit};
use crate::builder::ChunkModifyReadback;
use crate::geom::{build_bvh, Aabb3, Sphere};
use crate::tracer::StaticSceneMesh;

pub(super) const SOIL_MIN: UVec3 = UVec3::new(72, 192, 84);
pub(super) const SOIL_MAX: UVec3 = UVec3::new(436, 384, 408);

#[derive(Clone, Copy, Debug)]
struct SceneBox {
    min: Vec3,
    max: Vec3,
    color: Vec4,
}

pub(super) struct RooftopScene {
    boxes: Vec<SceneBox>,
    pub(super) material: voxel_backpack::BackpackVoxel,
}

impl RooftopScene {
    pub(super) fn new() -> Self {
        let mut boxes = Vec::new();
        let mut add = |min: [f32; 3], max: [f32; 3], rgb: [f32; 3]| {
            boxes.push(SceneBox {
                min: Vec3::from_array(min) / 256.0,
                max: Vec3::from_array(max) / 256.0,
                color: Vec3::from_array(rgb).extend(1.0),
            });
        };
        // Modelled stage, single-storey shell, flat roof and parapets. No atlas stamps.
        add([16., 66., 20.], [492., 72., 492.], [0.61, 0.64, 0.52]);
        add([60., 72., 72.], [448., 176., 80.], [0.79, 0.72, 0.57]);
        add([60., 72., 424.], [448., 176., 432.], [0.89, 0.83, 0.68]);
        add([60., 72., 80.], [68., 176., 424.], [0.87, 0.80, 0.64]);
        add([440., 72., 80.], [448., 176., 424.], [0.83, 0.76, 0.61]);
        add([60., 72., 72.], [448., 78., 432.], [0.52, 0.43, 0.32]);
        add([52., 176., 64.], [456., 192., 440.], [0.67, 0.69, 0.58]);
        add([52., 192., 64.], [72., 205., 440.], [0.84, 0.83, 0.70]);
        add([436., 192., 64.], [456., 205., 440.], [0.84, 0.83, 0.70]);
        add([72., 192., 64.], [436., 205., 84.], [0.84, 0.83, 0.70]);
        add([72., 192., 408.], [436., 205., 440.], [0.84, 0.83, 0.70]);
        // Recess-looking warm windows, frames and a modest cafe sign; no transparent-pane promise.
        for x in [88., 170., 302., 384.] {
            add([x, 104., 432.], [x + 52., 158., 436.], [0.26, 0.39, 0.30]);
            add(
                [x + 5., 109., 436.],
                [x + 47., 153., 437.],
                [0.75, 0.61, 0.36],
            );
            add(
                [x + 24., 109., 437.],
                [x + 28., 153., 439.],
                [0.26, 0.39, 0.30],
            );
        }
        add([238., 78., 432.], [284., 148., 438.], [0.26, 0.35, 0.26]);
        add([233., 151., 436.], [289., 166., 440.], [0.66, 0.32, 0.20]);
        // A fixed vent on the parapet, deliberately outside the plantable area.
        add([439., 205., 90.], [451., 238., 102.], [0.38, 0.48, 0.39]);
        add([436., 230., 86.], [455., 241., 106.], [0.47, 0.58, 0.47]);
        Self {
            boxes,
            material: voxel_backpack::BackpackVoxel::Dirt,
        }
    }

    pub(super) fn mesh(&self) -> StaticSceneMesh {
        let mut mesh = StaticSceneMesh::default();
        for b in &self.boxes {
            mesh.append_box(b.min, b.max, b.color);
        }
        mesh
    }

    pub(super) fn ray_hit(&self, origin: Vec3, direction: Vec3) -> Option<Vec3> {
        self.boxes
            .iter()
            .filter_map(|b| ray_box_distance(origin, direction, b.min, b.max))
            .min_by(f32::total_cmp)
            .map(|t| origin + direction * t)
    }

    pub(super) fn allows_soil(point: Vec3) -> bool {
        let min = SOIL_MIN.as_vec3() / 256.0;
        let max = SOIL_MAX.as_vec3() / 256.0;
        point.is_finite()
            && point.x >= min.x
            && point.x < max.x
            && point.z >= min.z
            && point.z < max.z
            && point.y >= min.y - 1e-5
            && point.y < max.y
    }
}

fn ray_box_distance(origin: Vec3, direction: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    if !origin.is_finite() || !direction.is_finite() || direction.length_squared() < 1e-12 {
        return None;
    }
    let mut near: f32 = f32::NEG_INFINITY;
    let mut far: f32 = f32::INFINITY;
    for axis in 0..3 {
        if direction[axis].abs() < 1e-8 {
            if origin[axis] < min[axis] || origin[axis] > max[axis] {
                return None;
            }
        } else {
            let a = (min[axis] - origin[axis]) / direction[axis];
            let b = (max[axis] - origin[axis]) / direction[axis];
            near = near.max(a.min(b));
            far = far.min(a.max(b));
        }
    }
    if far < near || far < 0.0 {
        None
    } else {
        Some(if near >= 0.0 { near } else { far })
    }
}

impl App {
    pub(super) fn finish_rooftop_scene(&mut self) -> Result<()> {
        let scene = RooftopScene::new();
        let mesh = scene.mesh();
        self.terrain_physics.publish_fixed_scene(&mesh)?;
        self.tracer.upload_static_scene(&mesh)?;
        self.rooftop_scene = Some(scene);
        self.set_manual_time_of_day(0.36);
        self.player_tools.select_item_panel_slot(SHOVEL_SLOT_INDEX);
        self.player_tools.terrain_edit_radius = 0.045;
        if self.launch_owners.snapshot_name().is_none() {
            let focus = Vec3::new(254., 192., 246.) / 256.;
            let position = Vec3::new(665., 650., 710.) / 256.;
            self.tracer.set_camera_pose_looking_at(position, focus);
            let mut pose = self.tracer.camera_pose();
            pose.fov_deg = 60.0;
            self.tracer.apply_camera_pose(pose);
            self.camera_control.set_orbit_focus(focus);
            anyhow::ensure!(
                self.rooftop_scene
                    .as_ref()
                    .unwrap()
                    .ray_hit(self.tracer.camera_position(), self.tracer.camera_front(),)
                    .is_some_and(RooftopScene::allows_soil),
                "opening camera must frame the roof work area"
            );
        }
        log::info!("[ROOFTOP] ready initial_soil=0 fixed_model_triangles={} unlimited=true persistence=disabled soil_bounds={:?}..{:?} lighting=stepped_sun_sky pixelization=existing_nearest_postprocess", mesh.indices.len() / 3, SOIL_MIN, SOIL_MAX);
        if std::env::var_os("RE_FLORA_ROOFTOP_VALIDATE").is_some() {
            self.validate_rooftop_edits()?;
        }
        Ok(())
    }

    pub(super) fn apply_rooftop_soil(
        &mut self,
        edit: TerrainRemovalEdit,
        material: u32,
    ) -> Result<ChunkModifyReadback> {
        anyhow::ensure!(
            RooftopScene::allows_soil(edit.center),
            "soil brush outside rooftop work area"
        );
        anyhow::ensure!(
            edit.radius.is_finite() && edit.radius > 0.0,
            "invalid rooftop brush radius"
        );
        let sphere = Sphere::new(edit.center * 256., edit.radius * 256.);
        let min = sphere
            .aabb()
            .min()
            .floor()
            .max(SOIL_MIN.as_vec3())
            .as_uvec3();
        let max = sphere
            .aabb()
            .max()
            .ceil()
            .min(SOIL_MAX.as_vec3())
            .as_uvec3();
        let rebuild_bound = UAabb3::new(min, max);
        let bvh_nodes = build_bvh(&[Aabb3::new(min.as_vec3(), max.as_vec3())], &[0_u32])
            .map_err(anyhow::Error::msg)?;
        let stats = self.plain_builder.chunk_modify_bounded_spheres(
            &bvh_nodes,
            &[sphere],
            material,
            UAabb3::new(SOIL_MIN, SOIL_MAX),
        )?;
        let changed = stats.stats.added_counts.iter().any(|&n| n > 0)
            || stats.stats.removed_counts.iter().any(|&n| n > 0);
        if material == crate::builder::VOXEL_TYPE_EMPTY {
            self.clear_surface_occupants_in_brush(
                TerrainBrushEdit {
                    start: edit.center,
                    end: edit.center,
                    radius: edit.radius,
                },
                SurfaceOccupantClearPath::TerrainRebuild {
                    bound: rebuild_bound,
                    terrain_changed: changed,
                },
            )?;
        } else {
            self.publish_visible_terrain(VisibleTerrainChange::preserving_flora(
                rebuild_bound,
                world_ops::FloraBrushEdit {
                    start: edit.center,
                    end: edit.center,
                    radius: edit.radius,
                    tick: self.world_clock.flora_tick(),
                    spawn_time_ms: self.time_info.time_since_start_duration().as_millis() as u32,
                },
                changed,
            ))?;
        }
        Ok(stats)
    }

    fn validate_rooftop_edits(&mut self) -> Result<()> {
        anyhow::ensure!(
            !self.set_orbit_mouse_drag_state(MouseButton::Right, ElementState::Pressed,),
            "orbit rotation captured the Edit placement button"
        );
        self.modifiers = ModifiersState::ALT;
        anyhow::ensure!(
            self.set_orbit_mouse_drag_state(MouseButton::Right, ElementState::Pressed),
            "Alt + RMB must retain orbit rotation"
        );
        self.set_orbit_mouse_drag_state(MouseButton::Right, ElementState::Released);
        self.modifiers = ModifiersState::empty();
        let center = Vec3::new(240., 192., 244.) / 256.;
        // The roof is not a terrain stamp: no initial atlas/Contree soil to hit.
        anyhow::ensure!(
            self.contree_builder
                .query_terrain_ray_cpu(center + Vec3::Y * 0.3, Vec3::NEG_Y)
                .is_none(),
            "roof must start with zero voxel terrain"
        );
        let ray_hit = self
            .query_terrain_ray_cpu(center + Vec3::Y * 0.3, Vec3::NEG_Y)
            .context("bare roof is not pickable")?;
        anyhow::ensure!(
            (ray_hit.position.y - center.y).abs() < 1e-5,
            "bare roof picking mismatch"
        );
        let edit = TerrainRemovalEdit {
            center,
            radius: 0.065,
        };
        let add =
            self.apply_surface_terrain_placement(edit, crate::builder::VOXEL_TYPE_DIRT, u32::MAX)?;
        anyhow::ensure!(
            add.stats.count_added(crate::builder::VOXEL_TYPE_DIRT) > 0,
            "first soil placement wrote nothing"
        );
        let second =
            self.apply_surface_terrain_placement(edit, crate::builder::VOXEL_TYPE_DIRT, u32::MAX)?;
        anyhow::ensure!(
            second.stats.count_added(crate::builder::VOXEL_TYPE_DIRT) > 0,
            "first dab filled the entire brush instead of gradual surface placement"
        );
        // Production queries publish asynchronously. This opt-in end-to-end fixture
        // must settle the real CPU source before inspecting the result of each edit.
        self.contree_builder.flush_cpu_chunk_cache_jobs();
        let below = self
            .contree_builder
            .query_terrain_ray_cpu(center - Vec3::Y / 256., Vec3::NEG_Y);
        anyhow::ensure!(below.is_none(), "soil crossed the model roof floor");
        let hit = self
            .contree_builder
            .query_terrain_ray_cpu(center + Vec3::Y * 0.3, Vec3::NEG_Y)
            .context("placed soil not published")?;
        let (selection_index, species_index) = species::PLAYER_FLORA_PAINT_SELECTIONS
            .iter()
            .enumerate()
            .find_map(|(i, s)| {
                if let species::FloraPaintSelection::Species(sp) = s {
                    species::is_authored_plant_species_index(*sp).then_some((i, *sp))
                } else {
                    None
                }
            })
            .context("no authored plant for rooftop Grow check")?;
        self.select_flora_paint_selection_index(selection_index);
        self.apply_surface_flora_regeneration(
            TerrainBrushEdit {
                start: hit.position,
                end: hit.position,
                radius: 0.06,
            },
            1,
            true,
        )?;
        let planted = self
            .surface_builder
            .authored_flora_base_positions_for_species(species_index)
            .len();
        anyhow::ensure!(planted > 0, "Grow placed no plants on the added soil");
        self.apply_surface_terrain_smooth(
            hit.position,
            0.065,
            super::TERRAIN_SMOOTH_STRENGTH,
            super::TERRAIN_SMOOTH_MAX_DELTA,
            super::TERRAIN_SMOOTH_DEADBAND,
        )?;
        let forbidden_slab = self
            .plain_builder
            .read_chunk_atlas_region(UVec3::new(215, 191, 219), UVec3::new(50, 1, 50))?;
        anyhow::ensure!(
            forbidden_slab.iter().all(|&v| v == 0),
            "smoothing wrote beneath fixed roof"
        );
        let remove = self.apply_surface_terrain_removal(edit, None, None, None)?;
        self.contree_builder.flush_cpu_chunk_cache_jobs();
        anyhow::ensure!(
            remove.stats.count_removed(crate::builder::VOXEL_TYPE_DIRT) > 0,
            "soil removal wrote nothing"
        );
        anyhow::ensure!(
            self.contree_builder
                .query_terrain_ray_cpu(center + Vec3::Y * 0.3, Vec3::NEG_Y)
                .is_none(),
            "deleted soil still visible in Contree"
        );
        anyhow::ensure!(
            self.surface_builder
                .authored_flora_base_positions_for_species(species_index)
                .is_empty(),
            "unsupported plants survived removal of all soil"
        );
        anyhow::ensure!(
            self.query_terrain_ray_cpu(center + Vec3::Y * 0.3, Vec3::NEG_Y)
                .is_some(),
            "editing destroyed the fixed roof"
        );
        let movement = self.terrain_physics.move_player_capsule(
            crate::gameplay::camera::PlayerWalkMovementRequest {
                camera_position: center + Vec3::Y * 0.081,
                camera_height: 0.08,
                desired_translation: Vec3::new(0.03, -0.01, 0.),
            },
            1. / 60.,
        )?;
        anyhow::ensure!(
            movement.grounded && movement.translation.y > -0.005,
            "fixed roof character collision failed: {movement:?}"
        );
        log::info!("[ROOFTOP][CHECK] first_soil={} second_soil={} removed={} planted={} bare_pick=true floor_clip=true smooth_clip=true grow_path=true roof_preserved=true grounded=true edit_pointer=true alt_orbit=true", add.stats.count_added(crate::builder::VOXEL_TYPE_DIRT), second.stats.count_added(crate::builder::VOXEL_TYPE_DIRT), remove.stats.count_removed(crate::builder::VOXEL_TYPE_DIRT), planted);
        // Drive the production pointer → semantic action → shovel placement path, too.
        // Leave its planted patch only for this opt-in fixture, never the normal opening.
        let opening_pose = self.tracer.camera_pose();
        let cursor_before = self.cursor_position_physical;
        let extent = self.window_state.window_extent();
        self.cursor_position_physical = Some(Vec2::new(
            extent.width as f32 / 2.,
            extent.height as f32 / 2.,
        ));
        self.tracer
            .set_camera_pose_looking_at(opening_pose.position, center);
        let radius_before = self.player_tools.terrain_edit_radius;
        self.player_tools.terrain_edit_radius = edit.radius;
        let inventory_before: Vec<_> = self
            .voxel_backpack
            .snapshot()
            .iter()
            .map(|e| e.count)
            .collect();
        self.set_tool_mouse_button_state(MouseButton::Right, ElementState::Pressed);
        let Some(PlayerToolPointerAction::Continuous(action)) =
            self.player_tools.begin_pointer_action(MouseButton::Right)
        else {
            anyhow::bail!("Edit RMB did not resolve to a continuous tool action");
        };
        self.execute_continuous_terrain_tool_action(action, Instant::now());
        self.set_tool_mouse_button_state(MouseButton::Right, ElementState::Released);
        self.refresh_terrain_edit_hold_from_mouse_buttons();
        anyhow::ensure!(
            self.voxel_backpack
                .snapshot()
                .iter()
                .map(|e| e.count)
                .collect::<Vec<_>>()
                == inventory_before,
            "unlimited Edit mutated the normal backpack"
        );
        self.player_tools.terrain_edit_radius = radius_before;
        self.cursor_position_physical = cursor_before;
        self.tracer.apply_camera_pose(opening_pose);
        self.contree_builder.flush_cpu_chunk_cache_jobs();
        let hit = self
            .contree_builder
            .query_terrain_ray_cpu(center + Vec3::Y * 0.3, Vec3::NEG_Y)
            .context("review soil missing")?;
        self.apply_surface_flora_regeneration(
            TerrainBrushEdit {
                start: hit.position,
                end: hit.position,
                radius: 0.045,
            },
            2,
            true,
        )?;
        log::info!("[ROOFTOP][CHECK] production_rmb_placement=true backpack_preserved=true");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bare_roof_has_real_model_hit_and_bounded_edit_area() {
        let scene = RooftopScene::new();
        let hit = scene.ray_hit(Vec3::new(1., 1.5, 1.), Vec3::NEG_Y).unwrap();
        assert!((hit.y - 192. / 256.).abs() < 1e-6);
        assert!(RooftopScene::allows_soil(hit));
        assert!(!RooftopScene::allows_soil(Vec3::new(0., hit.y, 0.)));
        assert!(!RooftopScene::allows_soil(hit - Vec3::Y / 256.));
        assert!(scene.ray_hit(Vec3::splat(4.), Vec3::Y).is_none());
    }
    #[test]
    fn fixed_models_never_overlap_the_editable_volume() {
        let scene = RooftopScene::new();
        let min = SOIL_MIN.as_vec3() / 256.;
        let max = SOIL_MAX.as_vec3() / 256.;
        for b in scene.boxes {
            assert!(
                !b.max.cmpgt(min).all() || !b.min.cmplt(max).all(),
                "fixed model overlaps editable soil region: {b:?}"
            );
        }
    }

    #[test]
    fn model_pick_and_collision_use_the_same_geometry() {
        let scene = RooftopScene::new();
        let mesh = scene.mesh();
        assert_eq!(mesh.indices.len(), scene.boxes.len() * 36);
        assert_eq!(mesh.vertices.len(), scene.boxes.len() * 24);
        let (points, tris) = mesh.collision_geometry();
        assert_eq!(points.len(), mesh.vertices.len());
        assert_eq!(tris.len(), mesh.indices.len() / 3);
    }
    #[test]
    fn ray_box_rejects_parallel_outside_and_nonfinite_inputs() {
        assert_eq!(
            ray_box_distance(Vec3::new(0.5, 2., 0.5), Vec3::NEG_Y, Vec3::ZERO, Vec3::ONE),
            Some(1.)
        );
        assert!(
            ray_box_distance(Vec3::new(2., 2., 0.5), Vec3::NEG_Y, Vec3::ZERO, Vec3::ONE).is_none()
        );
        assert!(ray_box_distance(Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, Vec3::ONE).is_none());
        assert!(ray_box_distance(Vec3::splat(f32::NAN), Vec3::Y, Vec3::ZERO, Vec3::ONE).is_none());
    }
}

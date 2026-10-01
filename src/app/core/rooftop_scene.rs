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
        // A grounded neighbourhood plinth, with a continuous level street and raised sidewalks.
        add([-180., -24., -160.], [692., 60., 700.], [0.34, 0.37, 0.31]);
        add([-180., 60., -160.], [692., 64., 700.], [0.28, 0.31, 0.32]);
        add([-36., 64., 8.], [544., 72., 506.], [0.68, 0.67, 0.60]);
        add([-180., 64., -160.], [-100., 70., 700.], [0.54, 0.59, 0.46]);
        add([612., 64., -160.], [692., 70., 700.], [0.54, 0.59, 0.46]);
        // Paving joints, crossing and lane markings are raised clear of the road surface.
        for x in (-20..540).step_by(40) {
            add(
                [x as f32, 72., 450.],
                [x as f32 + 1., 72.5, 504.],
                [0.51, 0.53, 0.49],
            );
        }
        for x in (180..340).step_by(28) {
            add(
                [x as f32, 64., 530.],
                [x as f32 + 14., 64.5, 602.],
                [0.87, 0.85, 0.72],
            );
        }
        for x in (-140..660).step_by(100) {
            add(
                [x as f32, 64., 652.],
                [x as f32 + 46., 64.5, 656.],
                [0.86, 0.80, 0.56],
            );
        }
        add([60., 72., 72.], [448., 176., 80.], [0.79, 0.72, 0.57]);
        // Front wall is built around real openings, not an opaque wall behind fake glass.
        add([60., 72., 424.], [238., 98., 432.], [0.89, 0.83, 0.68]);
        add([284., 72., 424.], [448., 98., 432.], [0.89, 0.83, 0.68]);
        add([60., 160., 424.], [448., 176., 432.], [0.89, 0.83, 0.68]);
        for (lo, hi) in [
            (60., 82.),
            (146., 164.),
            (228., 238.),
            (284., 296.),
            (360., 378.),
            (442., 448.),
        ] {
            add([lo, 98., 424.], [hi, 160., 432.], [0.89, 0.83, 0.68]);
        }
        add([238., 148., 424.], [284., 160., 432.], [0.89, 0.83, 0.68]);
        add([60., 72., 80.], [68., 176., 424.], [0.87, 0.80, 0.64]);
        add([440., 72., 80.], [448., 98., 424.], [0.83, 0.76, 0.61]);
        add([440., 160., 80.], [448., 176., 424.], [0.83, 0.76, 0.61]);
        for (lo, hi) in [(80., 112.), (192., 214.), (294., 316.), (396., 424.)] {
            add([440., 98., lo], [448., 160., hi], [0.83, 0.76, 0.61]);
        }
        // Terminate the floor slab halfway through the 8-voxel wall thickness.
        // Its sides are enclosed by the walls, not duplicate exterior wall faces.
        add([64., 72., 76.], [444., 78., 428.], [0.52, 0.43, 0.32]);
        add([52., 176., 64.], [456., 192., 440.], [0.67, 0.69, 0.58]);
        add([52., 192., 64.], [72., 205., 440.], [0.84, 0.83, 0.70]);
        add([436., 192., 64.], [456., 205., 440.], [0.84, 0.83, 0.70]);
        add([72., 192., 64.], [436., 205., 84.], [0.84, 0.83, 0.70]);
        add([72., 192., 408.], [436., 205., 440.], [0.84, 0.83, 0.70]);
        // Slender green frames leave the interior visible through glazed openings.
        for x in [82., 164., 296., 378.] {
            add([x, 98., 432.], [x + 64., 102., 436.], [0.22, 0.34, 0.29]);
            add([x, 156., 432.], [x + 64., 160., 436.], [0.22, 0.34, 0.29]);
            for dx in [0., 30., 60.] {
                add(
                    [x + dx, 102., 432.],
                    [x + dx + 4., 156., 436.],
                    [0.22, 0.34, 0.29],
                );
            }
        }
        for z in [112., 214., 316.] {
            for y in [98., 156.] {
                add([448., y, z], [452., y + 4., z + 80.], [0.22, 0.34, 0.29]);
            }
            for dz in [0., 38., 76.] {
                add(
                    [448., 102., z + dz],
                    [452., 156., z + dz + 4.],
                    [0.22, 0.34, 0.29],
                );
            }
        }
        for x in [238., 280.] {
            add([x, 78., 432.], [x + 4., 148., 436.], [0.22, 0.34, 0.29]);
        }
        add([242., 144., 432.], [280., 148., 436.], [0.22, 0.34, 0.29]);
        add([274., 106., 436.], [277., 119., 440.], [0.80, 0.69, 0.40]);
        add([231., 154., 436.], [291., 170., 440.], [0.66, 0.32, 0.20]);
        // Cafe sign lettering, striped awnings and a welcoming entrance step.
        for x in [239., 252., 265., 278.] {
            add([x, 158., 440.], [x + 6., 166., 441.], [0.96, 0.89, 0.67]);
        }
        for x in (82..442).step_by(12) {
            add(
                [x as f32, 165., 441.],
                [x as f32 + 12., 168., 456.],
                if (x / 12) % 2 == 0 {
                    [0.84, 0.76, 0.55]
                } else {
                    [0.25, 0.40, 0.33]
                },
            );
        }
        add([234., 72., 436.], [288., 78., 448.], [0.73, 0.70, 0.61]);
        // Dining tables, chair seats/backs, plates and pendant fixtures.
        for x in [112., 194., 330., 404.] {
            for z in [280., 374.] {
                add(
                    [x - 2., 78., z - 2.],
                    [x + 2., 106., z + 2.],
                    [0.28, 0.30, 0.25],
                );
                add(
                    [x - 20., 106., z - 14.],
                    [x + 20., 110., z + 14.],
                    [0.68, 0.43, 0.25],
                );
                for dx in [-28., 22.] {
                    add(
                        [x + dx, 78., z - 6.],
                        [x + dx + 6., 92., z + 6.],
                        [0.36, 0.44, 0.32],
                    );
                    add(
                        [x + dx, 92., z - 9.],
                        [x + dx + 10., 95., z + 9.],
                        [0.72, 0.40, 0.25],
                    );
                    add(
                        [x + dx, 95., z + 7.],
                        [x + dx + 10., 111., z + 10.],
                        [0.72, 0.40, 0.25],
                    );
                }
                add(
                    [x - 12., 110., z - 5.],
                    [x - 3., 111., z + 5.],
                    [0.92, 0.88, 0.74],
                );
                add(
                    [x + 7., 110., z - 3.],
                    [x + 11., 116., z + 1.],
                    [0.92, 0.88, 0.74],
                );
                add(
                    [x - 1., 157., z - 1.],
                    [x + 1., 176., z + 1.],
                    [0.27, 0.29, 0.23],
                );
                add(
                    [x - 9., 153., z - 7.],
                    [x + 9., 157., z + 7.],
                    [0.90, 0.70, 0.36],
                );
            }
        }
        // Order counter / till / pastry display; open kitchen behind it.
        add([96., 78., 202.], [398., 112., 224.], [0.35, 0.46, 0.37]);
        add([92., 112., 198.], [402., 117., 228.], [0.82, 0.75, 0.60]);
        add([350., 117., 205.], [371., 127., 218.], [0.20, 0.25, 0.25]);
        add([354., 127., 208.], [370., 137., 212.], [0.25, 0.52, 0.48]);
        for x in [112., 138., 164.] {
            add([x, 117., 208.], [x + 16., 121., 218.], [0.86, 0.60, 0.27]);
        }
        add([86., 78., 88.], [318., 110., 120.], [0.61, 0.64, 0.61]);
        add([82., 110., 84.], [322., 115., 124.], [0.79, 0.81, 0.75]);
        for x in [104., 138., 172., 206.] {
            add([x, 115., 92.], [x + 22., 117., 114.], [0.22, 0.25, 0.25]);
        }
        add([96., 146., 84.], [230., 158., 126.], [0.61, 0.65, 0.63]);
        add([354., 78., 90.], [418., 163., 124.], [0.72, 0.76, 0.71]);
        add([383., 83., 124.], [386., 158., 125.], [0.34, 0.40, 0.37]);
        add([392., 118., 125.], [396., 140., 127.], [0.30, 0.34, 0.31]);
        // Street trees occupy the perimeter, keeping the roof and storefront the focal point.
        for (x, z) in [
            (-68., 32.),
            (-68., 208.),
            (-68., 416.),
            (570., 32.),
            (570., 222.),
            (570., 438.),
            (92., -64.),
            (380., -64.),
        ] {
            add(
                [x - 22., 64., z - 22.],
                [x + 22., 78., z + 22.],
                [0.55, 0.53, 0.43],
            );
            add(
                [x - 18., 78., z - 18.],
                [x + 18., 79., z + 18.],
                [0.30, 0.36, 0.23],
            );
            add(
                [x - 5., 79., z - 5.],
                [x + 5., 172., z + 5.],
                [0.40, 0.29, 0.19],
            );
            add(
                [x - 33., 144., z - 29.],
                [x + 33., 192., z + 29.],
                [0.30, 0.46, 0.29],
            );
            add(
                [x - 25., 192., z - 23.],
                [x + 25., 218., z + 23.],
                [0.40, 0.55, 0.32],
            );
            add(
                [x - 17., 218., z - 15.],
                [x + 17., 230., z + 15.],
                [0.48, 0.60, 0.36],
            );
        }
        for x in [28., 480.] {
            add(
                [x - 4., 72., 476.],
                [x + 4., 178., 484.],
                [0.25, 0.31, 0.29],
            );
            add(
                [x - 12., 178., 468.],
                [x + 12., 193., 492.],
                [0.87, 0.78, 0.53],
            );
            add(
                [x - 14., 193., 466.],
                [x + 14., 197., 494.],
                [0.25, 0.31, 0.29],
            );
        }
        for x in [110., 370.] {
            add([x, 80., 470.], [x + 52., 85., 487.], [0.57, 0.37, 0.24]);
            add([x, 85., 485.], [x + 52., 103., 489.], [0.57, 0.37, 0.24]);
            for dx in [4., 42.] {
                add(
                    [x + dx, 72., 472.],
                    [x + dx + 5., 80., 484.],
                    [0.25, 0.31, 0.29],
                );
            }
        }
        // A fixed vent on the parapet, deliberately outside the plantable area.
        add([439., 205., 90.], [451., 238., 102.], [0.38, 0.48, 0.39]);
        add([436., 230., 86.], [455., 241., 106.], [0.47, 0.58, 0.47]);
        // Glass is part of the same physical shell, rendered in a sorted translucent pass.
        for x in [82., 164., 296., 378.] {
            boxes.push(SceneBox {
                min: Vec3::new(x + 4., 102., 430.) / 256.,
                max: Vec3::new(x + 60., 156., 431.) / 256.,
                color: Vec4::new(0.64, 0.82, 0.78, 0.16),
            });
        }
        for z in [112., 214., 316.] {
            boxes.push(SceneBox {
                min: Vec3::new(446., 102., z + 4.) / 256.,
                max: Vec3::new(447., 156., z + 76.) / 256.,
                color: Vec4::new(0.64, 0.82, 0.78, 0.16),
            });
        }
        boxes.push(SceneBox {
            min: Vec3::new(242., 78., 430.) / 256.,
            max: Vec3::new(280., 144., 431.) / 256.,
            color: Vec4::new(0.64, 0.82, 0.78, 0.16),
        });
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
            // Lower three-quarter view shows the glazed facade and surrounding streets,
            // while the center ray still lands on the editable rooftop.
            let position = Vec3::new(900., 510., 960.) / 256.;
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
        // A thick patch distinguishes gradual removal from one-shot volume clearing.
        for _ in 0..2 {
            self.apply_surface_terrain_placement(edit, crate::builder::VOXEL_TYPE_DIRT, u32::MAX)?;
        }
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
                .is_some(),
            "first removal dab cleared the entire thick patch instead of its surface"
        );
        let mut removed_total = remove.stats.count_removed(crate::builder::VOXEL_TYPE_DIRT);
        for _ in 0..32 {
            if self
                .contree_builder
                .query_terrain_ray_cpu(center + Vec3::Y * 0.3, Vec3::NEG_Y)
                .is_none()
            {
                break;
            }
            let next = self.apply_surface_terrain_removal(edit, None, None, None)?;
            removed_total += next.stats.count_removed(crate::builder::VOXEL_TYPE_DIRT);
            self.contree_builder.flush_cpu_chunk_cache_jobs();
        }
        anyhow::ensure!(
            self.contree_builder
                .query_terrain_ray_cpu(center + Vec3::Y * 0.3, Vec3::NEG_Y)
                .is_none(),
            "repeated removal did not clear soil"
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
        log::info!("[ROOFTOP][CHECK] first_soil={} second_soil={} first_removed={} removed_total={} planted={} bare_pick=true floor_clip=true smooth_clip=true grow_path=true roof_preserved=true grounded=true edit_pointer=true alt_orbit=true gradual_remove=true", add.stats.count_added(crate::builder::VOXEL_TYPE_DIRT), second.stats.count_added(crate::builder::VOXEL_TYPE_DIRT), remove.stats.count_removed(crate::builder::VOXEL_TYPE_DIRT), removed_total, planted);
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
    fn exposed_model_side_faces_do_not_overlap_on_the_same_plane() {
        let scene = RooftopScene::new();
        // Test the authored boxes used by BOTH the raster mesh and collision mesh.
        // Same-facing, externally visible coplanar rectangles compete for depth.
        for (i, a) in scene.boxes.iter().enumerate() {
            for (j, b) in scene.boxes.iter().enumerate().skip(i + 1) {
                for axis in [0, 2] {
                    let tangents = if axis == 0 { [1, 2] } else { [0, 1] };
                    for positive in [false, true] {
                        let plane_a = if positive { a.max[axis] } else { a.min[axis] };
                        let plane_b = if positive { b.max[axis] } else { b.min[axis] };
                        if plane_a != plane_b {
                            continue;
                        }
                        let lo = a.min.max(b.min);
                        let hi = a.max.min(b.max);
                        if tangents.iter().any(|&t| lo[t] >= hi[t]) {
                            continue;
                        }
                        let mut sample = (lo + hi) * 0.5;
                        sample[axis] = plane_a;
                        let mut normal = Vec3::ZERO;
                        normal[axis] = if positive { 1. } else { -1. };
                        let exposed = scene
                            .ray_hit(sample + normal * 3., -normal)
                            .is_some_and(|hit| hit.distance(sample) < 1e-5);
                        assert!(!exposed, "exposed coplanar side faces: boxes {i}/{j}, axis={axis}, positive={positive}, sample={sample:?}");
                    }
                }
            }
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
    fn street_is_grounded_and_windows_reveal_real_interior() {
        let scene = RooftopScene::new();
        let street = scene
            .ray_hit(Vec3::new(270., 300., 570.) / 256., Vec3::NEG_Y)
            .unwrap();
        assert!((street.y - 64.5 / 256.).abs() < 1e-6);
        assert!(scene
            .boxes
            .iter()
            .any(|b| b.min.y < 0. && b.max.y == 60. / 256.));
        let origin = Vec3::new(110., 130., 500.) / 256.;
        let glass = scene.ray_hit(origin, Vec3::NEG_Z).unwrap();
        assert!((glass.z - 431. / 256.).abs() < 1e-6);
        // Removing only translucent panes from this query exposes the room, not a fake wall.
        let opaque_hit = scene
            .boxes
            .iter()
            .filter(|b| b.color.w == 1.)
            .filter_map(|b| ray_box_distance(origin, Vec3::NEG_Z, b.min, b.max))
            .min_by(f32::total_cmp)
            .unwrap();
        assert!(opaque_hit > (500. - 424.) / 256.);
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

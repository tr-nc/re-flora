//! Press/scoop -> held parcels -> release/deposit. Input cancellation can discard
//! an intent, never the finite soil already removed from the world.
use super::*;
use crate::builder::VOXEL_TYPE_DIRT;
use crate::particles::{ParticleRenderKind, ParticleSnapshot};
use crate::soil_movement::CarriedSoil;
use crate::terrain_deposition::DepositionRequest;
use crate::tracer::TerrainRayQuery;

#[derive(Default)]
enum Gesture {
    #[default]
    Idle,
    Scooping(Option<TerrainRayQuery>),
    Holding,
    Dropping,
}
#[derive(Default)]
pub(super) struct State {
    pub carry: CarriedSoil,
    gesture: Gesture,
}
impl State {
    fn begin(&mut self) -> bool {
        let scoop = self.carry.count() == 0;
        self.gesture = if scoop {
            Gesture::Scooping(None)
        } else {
            Gesture::Holding
        };
        scoop
    }
    pub fn cancel_gesture(&mut self) {
        self.gesture = Gesture::Idle;
    }
    pub fn reconcile_pending_pick(&mut self, pending: bool) {
        if !pending && matches!(self.gesture, Gesture::Scooping(_) | Gesture::Dropping) {
            self.cancel_gesture();
        }
    }
    fn release(&mut self, ray: TerrainRayQuery) -> Option<TerrainRayQuery> {
        match &mut self.gesture {
            Gesture::Scooping(release) => {
                *release = Some(ray);
                None
            }
            Gesture::Holding => {
                self.gesture = Gesture::Dropping;
                Some(ray)
            }
            Gesture::Idle | Gesture::Dropping => None,
        }
    }
    fn scoop_finished(&mut self) -> Option<TerrainRayQuery> {
        let phase = std::mem::take(&mut self.gesture);
        if self.carry.count() == 0 {
            return None;
        }
        match phase {
            Gesture::Scooping(Some(ray)) => {
                self.gesture = Gesture::Dropping;
                Some(ray)
            }
            Gesture::Scooping(None) => {
                self.gesture = Gesture::Holding;
                None
            }
            _ => None,
        }
    }
}

impl App {
    pub(super) fn begin_soil_shovel(&mut self, now: Instant) {
        self.cancel_pointer_pick(); // a new press supersedes only uncommitted intent
        if self.soil_shovel.begin() {
            let Some((origin, direction)) = self.terrain_edit_ray() else {
                self.soil_shovel.cancel_gesture();
                return;
            };
            if !self.queue_captured_pointer_pick(
                PlayerToolPointerAction::ShovelScoop,
                TerrainRayQuery { origin, direction },
                now,
                false,
            ) {
                self.soil_shovel.cancel_gesture();
            }
        }
    }
    pub(super) fn release_soil_shovel(&mut self) {
        if self.player_tools.selected_tool() != PlayerTool::SoilShovel {
            return;
        }
        if !self.terrain_edit_pointer_available()
            || !self.terrain_persistence.allows_world_updates()
        {
            self.cancel_pointer_pick();
            return;
        }
        let Some((origin, direction)) = self.terrain_edit_ray() else {
            self.cancel_pointer_pick();
            return;
        };
        if let Some(ray) = self
            .soil_shovel
            .release(TerrainRayQuery { origin, direction })
        {
            self.queue_soil_shovel_drop(ray);
        }
    }
    fn queue_soil_shovel_drop(&mut self, ray: TerrainRayQuery) {
        if self.soil_shovel.carry.count() == 0
            || !self.queue_captured_pointer_pick(
                PlayerToolPointerAction::ShovelDrop,
                ray,
                Instant::now(),
                false,
            )
        {
            self.soil_shovel.cancel_gesture();
        }
    }
    pub(super) fn complete_soil_shovel_pick(&mut self, action: PlayerToolPointerAction) {
        let scoop = action == PlayerToolPointerAction::ShovelScoop;
        if (scoop && !matches!(self.soil_shovel.gesture, Gesture::Scooping(_)))
            || (!scoop && !matches!(self.soil_shovel.gesture, Gesture::Dropping))
        {
            return;
        }
        let hit = self.pointer_picking.active.and_then(|p| p.hit).filter(|h| {
            h.tree_primitive.is_none()
                && (0. ..=SHOVEL_RAY_QUERY_DISTANCE).contains(&h.distance)
                && (!scoop || h.voxel_type == VOXEL_TYPE_DIRT)
        });
        let result = match hit {
            Some(hit) if scoop => self.scoop_soil(hit.world_position),
            Some(hit) => self.drop_carried_soil(hit.world_position),
            None => Ok(0),
        };
        if let Err(e) = result {
            self.soil_shovel.cancel_gesture();
            log::error!(
                "[SOIL_SHOVEL] operation failed (committed parcels retain their owner): {e:#}"
            );
            return;
        }
        if scoop {
            if let Some(ray) = self.soil_shovel.scoop_finished() {
                self.queue_soil_shovel_drop(ray);
            }
        } else {
            self.soil_shovel.cancel_gesture();
        }
    }
    fn scoop_soil(&mut self, contact: Vec3) -> Result<u32> {
        if self.soil_shovel.carry.count() != 0 || !contact.is_finite() {
            return Ok(0);
        }
        let radius = (self.player_tools.terrain_edit_radius * 256.).clamp(2., 6.);
        let center = contact * 256. - Vec3::Y * (radius * 0.35);
        let (legal, _) = self.soil_tool_domain();
        if (contact * 256.).cmplt(legal.min().as_vec3()).any()
            || (contact * 256.).cmpge(legal.max().as_vec3()).any()
        {
            return Ok(0);
        }
        let origin = (center - Vec3::splat(radius + 1.))
            .floor()
            .max(legal.min().as_vec3())
            .as_uvec3();
        let max = (center + Vec3::splat(radius + 1.))
            .ceil()
            .min(legal.max().as_vec3())
            .as_uvec3();
        if origin.cmpge(max).any() {
            return Ok(0);
        }
        let bytes = self
            .plain_builder
            .read_chunk_atlas_region(origin, max - origin)?;
        let Some((plan, parcels)) =
            crate::soil_movement::scoop(&bytes, UAabb3::new(origin, max), center, radius)
        else {
            return Ok(0);
        };
        let moved = parcels.count();
        self.plain_builder
            .write_chunk_atlas_region(plan.origin, plan.dim, &plan.data)?;
        // The GPU mutation succeeded. Transfer ownership before any fallible
        // mesh/root/physics publication, so even a publication error cannot lose soil.
        self.soil_shovel.carry = parcels;
        self.publish_soil_movement(plan.origin, plan.dim, &[])?;
        log::info!(
            "[SOIL_SHOVEL] scooped={moved} carried={} finite_even_in_debug=true",
            self.soil_shovel.carry.count()
        );
        Ok(moved)
    }
    fn drop_carried_soil(&mut self, contact: Vec3) -> Result<u32> {
        let volume = self.soil_shovel.carry.count();
        let (legal, floor) = self.soil_tool_domain();
        let center = contact * 256.;
        if volume == 0
            || !center.is_finite()
            || center.cmplt(legal.min().as_vec3()).any()
            || center.cmpge(legal.max().as_vec3()).any()
        {
            return Ok(0);
        }
        let gui = &self.debug_settings.adjustables;
        let request = DepositionRequest {
            start: center,
            end: center,
            radius: (self.player_tools.terrain_edit_radius * 256.).clamp(2., 6.),
            volume,
            clump_radius: gui.soil_deposition_clump_radius.value,
            repose_slope: gui.soil_deposition_repose_slope.value,
            seed: self.visible_terrain_revision ^ 0x51a7,
        };
        let result = self.plain_builder.deposit_soil_parcels(
            request,
            legal,
            floor,
            gui.soil_deposition_fast_path.value,
            self.soil_shovel.carry.parcels(),
        )?;
        // Debit exact committed writes, not requested volume, before publication.
        self.soil_shovel.carry.debit_committed(result.added);
        if let Some(changed) = result.changed {
            self.publish_soil_movement(
                changed.min(),
                changed.max() - changed.min() + UVec3::ONE,
                &result.deposited_voxels,
            )?;
        }
        log::info!(
            "[SOIL_SHOVEL] deposited={} retained={} finite_even_in_debug=true",
            result.added,
            self.soil_shovel.carry.count()
        );
        Ok(result.added)
    }
    pub(super) fn append_soil_shovel_preview(&mut self) {
        if self.player_tools.selected_tool() != PlayerTool::SoilShovel
            || self.soil_shovel.carry.count() == 0
        {
            return;
        }
        let front = self.tracer.camera_front().normalize_or_zero();
        let right = front.cross(Vec3::Y).normalize_or_zero();
        let center = self.tracer.camera_position() + front * 0.22 + right * 0.06 - Vec3::Y * 0.035;
        let size = (self.soil_shovel.carry.count() as f32 / 19.).cbrt() / 256.;
        let rgb = self.debug_settings.adjustables.voxel_bare_dirt_color.value;
        let linear = |c: u8| {
            let c = c as f32 / 255.;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        let color = Vec3::new(linear(rgb.r()), linear(rgb.g()), linear(rgb.b()));
        // Nineteen bounded display blocks, not physical/resources-owning debris.
        for z in -1_i32..=1 {
            for y in -1_i32..=1 {
                for x in -1_i32..=1 {
                    if x * x + y * y + z * z > 2 {
                        continue;
                    }
                    self.particle_snapshots.push(ParticleSnapshot {
                        position_ws: center + Vec3::new(x as f32, y as f32, z as f32) * size,
                        velocity: Vec3::ZERO,
                        color: (color * (0.48 + 0.15 * (y + 1) as f32 + 0.08 * (1 - x) as f32))
                            .extend(1.),
                        size,
                        kind: ParticleRenderKind::TerrainVoxel,
                        palette_index: 0,
                        animation_phase_offset: 0.,
                        animation_sample_time: None,
                        butterfly_wingbeat: None,
                        butterfly_body_pose: None,
                        leaf_orientation: None,
                        leaf_shape_seed: None,
                        leaf_geometry: None,
                    });
                }
            }
        }
    }

    pub(super) fn seed_soil_shovel_review(&mut self) -> Result<()> {
        let Ok(mode) = std::env::var("RE_FLORA_SHOVEL_REVIEW") else {
            return Ok(());
        };
        let origin = UVec3::new(160, 112, 256);
        let dim = UVec3::new(96, 48, 96);
        let count = |data: &[u8]| data.iter().filter(|&&v| v & 15 == 2).count() as u32;
        self.player_tools
            .select_item_panel_slot(ui_style::SOIL_SHOVEL_SLOT_INDEX);
        self.player_tools.terrain_edit_radius = 6. / 256.;
        if mode != "load-drop" {
            let mut data = vec![0; dim.element_product() as usize];
            for z in 0..dim.z {
                for y in 0..12 {
                    for x in 0..dim.x {
                        data[((z * dim.y + y) * dim.x + x) as usize] = 2 | if mode == "moisture" {
                            (x % 4) as u8 * 16
                        } else {
                            0
                        };
                    }
                }
            }
            self.execute_world_edit(WorldEditTransaction::terrain_change(
                vec![crate::app::world_edits::VoxelEdit::RestoreAtlasRegion {
                    origin,
                    dim,
                    data: data.into(),
                }],
                UAabb3::new(origin, origin + dim - UVec3::ONE),
            ))?;
            let moved = self.scoop_soil(Vec3::new(184., 124., 292.) / 256.)?;
            anyhow::ensure!(moved > 100, "shovel review needs a real scoop");
            self.soil_shovel.cancel_gesture();
            anyhow::ensure!(
                self.drop_carried_soil(Vec3::splat(-1.))? == 0
                    && self.soil_shovel.carry.count() == moved,
                "invalid release lost soil"
            );
        }
        let before = count(&self.plain_builder.read_chunk_atlas_region(origin, dim)?);
        anyhow::ensure!(
            before + self.soil_shovel.carry.count() == 96 * 12 * 96
                && self.soil_shovel.carry.count() > 0,
            "held/saved shovel soil not conserved"
        );
        let mut outside_fixture = 0;
        if mode == "partial" {
            let roof = UVec3::new(216, self.soil_tool_domain().0.max().y - 4, 284);
            let roof_dim = UVec3::new(16, 4, 16);
            let mut bytes = vec![7; 1024];
            bytes[((8 * 4 + 3) * 16 + 8) as usize] = 0;
            self.execute_world_edit(WorldEditTransaction::terrain_change(
                vec![crate::app::world_edits::VoxelEdit::RestoreAtlasRegion {
                    origin: roof,
                    dim: roof_dim,
                    data: bytes.into(),
                }],
                UAabb3::new(roof, roof + roof_dim - UVec3::ONE),
            ))?;
            let clump = self
                .debug_settings
                .adjustables
                .soil_deposition_clump_radius
                .value;
            self.debug_settings
                .adjustables
                .soil_deposition_clump_radius
                .value = 0;
            self.player_tools.terrain_edit_radius = 2. / 256.;
            let result = self.drop_carried_soil((roof + UVec3::new(8, 3, 8)).as_vec3() / 256.);
            self.debug_settings
                .adjustables
                .soil_deposition_clump_radius
                .value = clump;
            self.player_tools.terrain_edit_radius = 6. / 256.;
            outside_fixture = result?;
            anyhow::ensure!(
                outside_fixture == 1,
                "partial release did not fill exactly one available cell: {outside_fixture}"
            );
            anyhow::ensure!(
                self.plain_builder
                    .read_chunk_atlas_region(roof + UVec3::new(8, 3, 8), UVec3::ONE)?[0]
                    & 15
                    == 2,
                "partial release absent from atlas"
            );
        }
        if mode == "load-drop" || mode == "drop" || mode == "moisture" {
            let added = self.drop_carried_soil(Vec3::new(224., 124., 292.) / 256.)?;
            anyhow::ensure!(
                added > 0 && self.soil_shovel.carry.count() == 0,
                "unobstructed release did not empty shovel"
            );
        }
        let final_bytes = self.plain_builder.read_chunk_atlas_region(origin, dim)?;
        let after = count(&final_bytes);
        if mode == "moisture" {
            let mut histogram = [0_u32; 4];
            for &b in &final_bytes {
                if b & 15 == 2 {
                    histogram[((b >> 4) & 3) as usize] += 1;
                }
            }
            anyhow::ensure!(
                histogram == [96 * 12 * 24; 4],
                "GPU parcel moisture changed: {histogram:?}"
            );
            log::info!("[SOIL_SHOVEL_REVIEW] GPU_moisture_histogram={histogram:?} preserved=true");
        }
        anyhow::ensure!(
            after + self.soil_shovel.carry.count() + outside_fixture == 96 * 12 * 96,
            "release did not conserve soil"
        );
        log::info!("[SOIL_SHOVEL_REVIEW] mode={mode} terrain={after} outside_fixture={outside_fixture} carry={} total={} conserved=true",self.soil_shovel.carry.count(),after+self.soil_shovel.carry.count()+outside_fixture);
        self.camera_control.apply_snapshot_mode(true);
        let target = Vec3::new(204., 124., 292.) / 256.;
        self.camera_control.set_orbit_focus(target);
        self.tracer
            .set_camera_pose_looking_at(Vec3::new(0.58, 0.82, 1.50), target);
        self.reset_camera_movement_input();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn loaded() -> CarriedSoil {
        serde_json::from_str("[2,18,34]").unwrap()
    }
    fn ray(x: f32) -> TerrainRayQuery {
        TerrainRayQuery {
            origin: Vec3::new(x, 1., 0.),
            direction: Vec3::NEG_Y,
        }
    }
    #[test]
    fn quick_release_keeps_its_ray_until_scoop_completes_and_never_duplicates() {
        let mut s = State::default();
        assert!(s.begin());
        assert!(s.release(ray(2.)).is_none());
        s.carry = loaded();
        assert_eq!(s.scoop_finished().unwrap().origin.x, 2.);
        assert!(s.scoop_finished().is_none());
        assert_eq!(s.carry.count(), 3);
    }
    #[test]
    fn cancellation_stale_pick_and_retry_keep_committed_soil() {
        let mut s = State {
            carry: loaded(),
            ..Default::default()
        };
        assert!(!s.begin());
        s.cancel_gesture();
        assert!(s.release(ray(1.)).is_none());
        assert_eq!(s.carry.count(), 3);
        assert!(!s.begin());
        assert!(s.release(ray(3.)).is_some());
        s.reconcile_pending_pick(false);
        assert_eq!(s.carry.count(), 3);
        assert!(!s.begin());
        assert_eq!(s.release(ray(4.)).unwrap().origin.x, 4.);
    }
    #[test]
    fn held_soil_serialization_and_partial_debit_preserve_exact_ownership() {
        let s = loaded();
        s.validate().unwrap();
        let mut s: CarriedSoil = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        let dim = UVec3::splat(4);
        let mut data = vec![7; 64];
        data[(1 * 4 + 1) * 4 + 1] = 0;
        let result = crate::terrain_deposition::deposit_in_bounds_fast(
            &mut data,
            UVec3::ZERO,
            dim,
            None,
            DepositionRequest {
                start: Vec3::ONE,
                end: Vec3::ONE,
                radius: 0.,
                volume: s.count(),
                clump_radius: 0,
                repose_slope: 1.1,
                seed: 1,
            },
            UAabb3::new(UVec3::ZERO, dim),
        );
        assert_eq!(result.added, 1);
        s.debit_committed(result.added);
        assert_eq!(s.count(), 2);
        assert_eq!(
            s.count() + data.iter().filter(|&&v| v & 15 == 2).count() as u32,
            3
        );
        let bad: CarriedSoil = serde_json::from_str("[7]").unwrap();
        assert!(bad.validate().is_err());
    }
}

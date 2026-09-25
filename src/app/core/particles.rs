use super::App;
use crate::builder::ChunkModifyStats;
use crate::particles::{
    ButterflyEmitter, ButterflyEmitterDesc, ButterflyFlightTuning, ButterflyFlightVariant,
    FallenLeafEmitter, LeafEmitterDesc, ParticleEmitter, ParticleHandle, ParticleRenderKind,
    ParticleSnapshot, ParticleSpawn, ParticleSystem, ParticleTickStep, ParticleUpdateConfig,
    PARTICLE_CAPACITY, STANDARD_PARTICLE_SIZE,
};
use crate::util::ClusterResult;
use egui::Color32;
use glam::{Vec2, Vec3, Vec4};
use std::{collections::HashMap, f32::consts::TAU, time::Instant};

const TERRAIN_HARVEST_PARTICLE_UPDATE: ParticleUpdateConfig = ParticleUpdateConfig::new(0.1, 2);

// bird-specific audio and control logic has been removed

#[allow(dead_code)]
const TERRAIN_HARVEST_MAX_PARTICLES_PER_EDIT: u32 = 4;
#[allow(dead_code)]
const TERRAIN_HARVEST_PARTICLE_SIZE: f32 = STANDARD_PARTICLE_SIZE;
const DEFAULT_WATER_DEBUG_PARTICLE_SIZE: f32 = 0.012;
const WATER_DEBUG_COLOR: Vec4 = Vec4::new(0.12, 0.45, 1.0, 1.0);
const BUTTERFLY_LIMIT_PER_WORLD_CHUNK: u64 = 2;
// Leaf-born visual particles may start inside branch voxels. B treats the canopy
// as permeable, while soil, rocks and constructed surfaces remain solid.
const BUTTERFLY_FLIGHT_SURFACE_MASK: u32 = u32::MAX
    & !(1 << crate::builder::VOXEL_TYPE_CHERRY_WOOD)
    & !(1 << crate::builder::VOXEL_TYPE_OAK_WOOD);
const DETACHED_TERRAIN_UPDATE: ParticleUpdateConfig = ParticleUpdateConfig::new(1.0 / 30.0, 2);

fn butterfly_world_limit(chunk_dim: glam::UVec3) -> usize {
    let chunk_count = u64::from(chunk_dim.x)
        .saturating_mul(u64::from(chunk_dim.y))
        .saturating_mul(u64::from(chunk_dim.z));
    usize::try_from(chunk_count.saturating_mul(BUTTERFLY_LIMIT_PER_WORLD_CHUNK))
        .unwrap_or(usize::MAX)
}

#[derive(Default)]
pub(super) struct ButterflyReview {
    frame: u32,
    subject_frame: Option<u32>,
}

fn terrain_harvest_rgb_for_voxel(voxel_type: u32) -> [u8; 3] {
    match voxel_type {
        crate::builder::VOXEL_TYPE_DIRT => super::voxel_backpack::BackpackVoxel::Dirt.color_rgb(),
        crate::builder::VOXEL_TYPE_CHERRY_WOOD => {
            super::voxel_backpack::BackpackVoxel::CherryWood.color_rgb()
        }
        crate::builder::VOXEL_TYPE_OAK_WOOD => {
            super::voxel_backpack::BackpackVoxel::OakWood.color_rgb()
        }
        crate::builder::VOXEL_TYPE_SAND => super::voxel_backpack::BackpackVoxel::Sand.color_rgb(),
        crate::builder::VOXEL_TYPE_STUCCO => {
            super::voxel_backpack::BackpackVoxel::Stucco.color_rgb()
        }
        crate::builder::VOXEL_TYPE_ROCK => super::voxel_backpack::BackpackVoxel::Rock.color_rgb(),
        crate::builder::VOXEL_TYPE_EMISSIVE => {
            super::voxel_backpack::BackpackVoxel::Emissive.color_rgb()
        }
        crate::builder::VOXEL_TYPE_LIMESTONE => {
            super::voxel_backpack::BackpackVoxel::Limestone.color_rgb()
        }
        crate::builder::VOXEL_TYPE_IVY => super::voxel_backpack::BackpackVoxel::Ivy.color_rgb(),
        crate::builder::VOXEL_TYPE_PETAL => super::voxel_backpack::BackpackVoxel::Petal.color_rgb(),
        _ => [210, 190, 140],
    }
}

fn harvest_distribution(
    stats: &ChunkModifyStats,
    material_mode: crate::voxel_material::VoxelMaterialMode,
) -> (u32, Vec<(u32, u32)>) {
    let mut removed_total = 0u32;
    let mut removed_types = Vec::new();
    for (voxel_type, count) in stats.removed_counts.iter().copied().enumerate() {
        if count == 0
            || crate::voxel_material::material_for(voxel_type as u32, material_mode).surface_class
                == crate::voxel_material::VoxelSurfaceClass::Dielectric
        {
            continue;
        }
        removed_total = removed_total.saturating_add(count);
        removed_types.push((voxel_type as u32, removed_total));
    }
    (removed_total, removed_types)
}

fn detached_terrain_voxel_spawn(world_voxel: glam::UVec3, color: Vec4) -> ParticleSpawn {
    let hash = world_voxel.x.wrapping_mul(73_856_093)
        ^ world_voxel.y.wrapping_mul(19_349_663)
        ^ world_voxel.z.wrapping_mul(83_492_791);
    let signed_unit = |bits: u32| -> f32 { (bits as f32 / u32::MAX as f32) * 2.0 - 1.0 };
    let position =
        (world_voxel.as_vec3() + Vec3::splat(0.5)) / super::VOXEL_DIM_PER_CHUNK.as_vec3();
    ParticleSpawn {
        position,
        velocity: Vec3::new(
            signed_unit(hash.wrapping_mul(0x9e37_79b9)) * 0.025,
            0.015,
            signed_unit(hash.rotate_left(13).wrapping_mul(0x85eb_ca6b)) * 0.025,
        ),
        color,
        size: STANDARD_PARTICLE_SIZE,
        lifetime: 4.0,
        wind_factor: 0.0,
        gravity_factor: 1.0,
        drift_direction: Vec3::ZERO,
        drift_strength: 0.0,
        drift_frequency: 1.0,
        speed_noise_offset: hash as f32 / u32::MAX as f32,
        motion_mode: crate::particles::MotionMode::Free,
        sink_on_lifetime: false,
        sink_speed: 0.1,
        palette_index: 0,
        render_kind: ParticleRenderKind::TerrainVoxel,
        despawn_on_lifetime: true,
        despawn_below_ground: true,
        update: DETACHED_TERRAIN_UPDATE,
    }
}

fn water_debug_particle_size(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.001, 0.1)
    } else {
        DEFAULT_WATER_DEBUG_PARTICLE_SIZE
    }
}

struct TreeLeafEmitter {
    tree_id: u32,
    emitter: FallenLeafEmitter,
}

impl TreeLeafEmitter {
    fn new(tree_id: u32, emitter: FallenLeafEmitter) -> Self {
        Self { tree_id, emitter }
    }

    fn tree_id(&self) -> u32 {
        self.tree_id
    }
}

pub(super) struct TreeLeafEmitterRuntime {
    emitters: Vec<TreeLeafEmitter>,
    indices_by_tree: HashMap<u32, Vec<usize>>,
    desc: LeafEmitterDesc,
}

impl TreeLeafEmitterRuntime {
    pub(super) fn new(desc: LeafEmitterDesc) -> Self {
        Self {
            emitters: Vec::new(),
            indices_by_tree: HashMap::new(),
            desc,
        }
    }

    pub(super) fn empty_like(&self) -> Self {
        Self::new(self.desc)
    }

    pub(super) fn upsert(&mut self, tree_id: u32, clusters: &[ClusterResult]) {
        self.remove(tree_id);

        let mut emitter_indices = Vec::with_capacity(clusters.len());
        for cluster in clusters {
            let mut emitter = FallenLeafEmitter::new(
                cluster.pos,
                Vec::new(),
                tree_id as u64 + cluster.pos.x as u64 + cluster.pos.y as u64 + cluster.pos.z as u64,
                &self.desc,
            );
            emitter.spawn_rate = self.desc.spawn_rate * (cluster.items_count as f32).sqrt();

            let index = self.emitters.len();
            self.emitters.push(TreeLeafEmitter::new(tree_id, emitter));
            emitter_indices.push(index);
        }

        if !emitter_indices.is_empty() {
            self.indices_by_tree.insert(tree_id, emitter_indices);
        }
    }

    pub(super) fn remove(&mut self, tree_id: u32) {
        let Some(mut indices) = self.indices_by_tree.remove(&tree_id) else {
            return;
        };
        indices.sort_unstable_by(|a, b| b.cmp(a));

        for index in indices {
            self.emitters.swap_remove(index);
            if let Some(swapped) = self.emitters.get(index) {
                if let Some(swapped_indices) = self.indices_by_tree.get_mut(&swapped.tree_id()) {
                    let old_index = self.emitters.len();
                    if let Some(position) =
                        swapped_indices.iter().position(|&entry| entry == old_index)
                    {
                        swapped_indices[position] = index;
                    }
                }
            }
        }
    }

    pub(super) fn advance(
        &mut self,
        particle_system: &mut ParticleSystem,
        dt: f32,
        wind: &crate::wind_field::WindFieldFrame,
        enabled: bool,
    ) {
        for emitter in &mut self.emitters {
            emitter.emitter.enabled = enabled;
            emitter.emitter.update(particle_system, dt, wind);
        }
    }

    pub(super) fn len(&self) -> usize {
        self.emitters.len()
    }
}

impl App {
    fn terrain_harvest_collection_target(&self) -> Vec3 {
        let screen_resolution = self.window_state.resolution();
        self.player_tools
            .backpack_summary_panel_screen_pos
            .and_then(|screen_pos| {
                self.tracer.project_screen_point_to_world(
                    screen_pos,
                    Vec2::new(screen_resolution[0], screen_resolution[1]),
                    0.22,
                )
            })
            .unwrap_or_else(|| {
                let camera_pos = self.tracer.camera_position();
                let camera_front = self.tracer.camera_front().normalize_or_zero();
                camera_pos + camera_front * 0.22 + Vec3::new(0.0, -0.04, 0.0)
            })
    }

    pub(super) fn terrain_harvest_color_for_voxel(&self, voxel_type: u32) -> Vec4 {
        fn srgb_to_linear(channel: u8) -> f32 {
            let srgb = channel as f32 / 255.0;
            if srgb <= 0.04045 {
                srgb / 12.92
            } else {
                ((srgb + 0.055) / 1.055).powf(2.4)
            }
        }

        let color_rgb = terrain_harvest_rgb_for_voxel(voxel_type);

        Vec4::new(
            srgb_to_linear(color_rgb[0]),
            srgb_to_linear(color_rgb[1]),
            srgb_to_linear(color_rgb[2]),
            1.0,
        )
    }

    pub(super) fn spawn_detached_terrain_voxel_particles(
        &mut self,
        voxels: &[(glam::UVec3, u8)],
    ) -> usize {
        let mut spawned = 0;
        for &(world_voxel, voxel_type) in voxels {
            let color = self.terrain_harvest_color_for_voxel(u32::from(voxel_type));
            let spawn = detached_terrain_voxel_spawn(world_voxel, color);
            if self.particle_system.spawn(spawn).is_none() {
                break;
            }
            spawned += 1;
        }
        spawned
    }

    #[allow(dead_code)]
    pub(super) fn spawn_terrain_harvest_particles(
        &mut self,
        center: Vec3,
        stats: &ChunkModifyStats,
        sampled_positions_world: &[Vec3],
    ) {
        if !self
            .debug_settings
            .adjustables
            .terrain_harvest_particles_enabled
            .value
        {
            return;
        }

        let (removed_total, removed_types) =
            harvest_distribution(stats, self.voxel_material_mode());
        if removed_total == 0 {
            return;
        }

        let spawn_count = removed_total.clamp(1, TERRAIN_HARVEST_MAX_PARTICLES_PER_EDIT);
        let fallback_base_pos = center + Vec3::new(0.0, 0.03, 0.0);
        let collection_target = self.terrain_harvest_collection_target();
        let flyback_speed = self
            .debug_settings
            .adjustables
            .terrain_harvest_flyback_speed
            .value
            .max(0.05);

        for i in 0..spawn_count {
            let base_pos = if sampled_positions_world.is_empty() {
                fallback_base_pos
            } else {
                sampled_positions_world[i as usize % sampled_positions_world.len()] + Vec3::Y * 0.01
            };
            let velocity = (collection_target - base_pos).normalize_or_zero() * flyback_speed;
            let sample = (((i as f32 + 0.5) / spawn_count as f32) * removed_total as f32)
                .clamp(0.0, removed_total as f32 - 0.001) as u32;
            let mut sampled_voxel_type = removed_types[removed_types.len() - 1].0;
            for (voxel_type, threshold) in &removed_types {
                if sample < *threshold {
                    sampled_voxel_type = *voxel_type;
                    break;
                }
            }
            let base_color = self.terrain_harvest_color_for_voxel(sampled_voxel_type);
            let rgb = base_color.truncate();

            let spawn = ParticleSpawn {
                position: base_pos,
                velocity,
                color: Vec4::new(rgb.x.min(1.0), rgb.y.min(1.0), rgb.z.min(1.0), 1.0),
                size: TERRAIN_HARVEST_PARTICLE_SIZE,
                lifetime: 1.35,
                wind_factor: 0.0,
                gravity_factor: 0.0,
                drift_direction: Vec3::ZERO,
                drift_strength: 0.0,
                drift_frequency: 0.0,
                speed_noise_offset: i as f32,
                motion_mode: crate::particles::MotionMode::Free,
                sink_on_lifetime: false,
                sink_speed: 0.0,
                palette_index: 0,
                render_kind: ParticleRenderKind::Leaf,
                despawn_on_lifetime: false,
                despawn_below_ground: false,
                update: TERRAIN_HARVEST_PARTICLE_UPDATE,
            };
            if let Some(handle) = self.particle_system.spawn(spawn) {
                self.terrain_harvest_particle_handles.push(handle);
            }
        }
    }

    #[allow(dead_code)]
    fn update_terrain_harvest_particle_collection(&mut self, dt: f32) {
        if dt <= 0.0
            || !self
                .debug_settings
                .adjustables
                .terrain_harvest_particles_enabled
                .value
            || self.terrain_harvest_particle_handles.is_empty()
        {
            return;
        }

        let collection_target = self.terrain_harvest_collection_target();
        let flyback_speed = self
            .debug_settings
            .adjustables
            .terrain_harvest_flyback_speed
            .value
            .max(0.05);

        self.terrain_harvest_particle_handles.retain(|handle| {
            if !self.particle_system.is_alive_handle(*handle) {
                return false;
            }

            let Some(position) = self.particle_system.position(*handle) else {
                return false;
            };

            let to_target = collection_target - position;
            let distance = to_target.length();
            if distance <= 0.01 || distance <= flyback_speed * dt {
                let _ = self
                    .particle_system
                    .set_position(*handle, collection_target);
                let _ = self.particle_system.despawn(*handle);
                return false;
            }

            let direction = to_target.normalize_or_zero();
            let _ = self
                .particle_system
                .set_velocity(*handle, direction * flyback_speed);

            true
        });
    }

    #[allow(dead_code)]
    pub(super) fn color32_to_vec4(color: Color32) -> Vec4 {
        Vec4::new(
            color.r() as f32 / 255.0,
            color.g() as f32 / 255.0,
            color.b() as f32 / 255.0,
            color.a() as f32 / 255.0,
        )
    }

    pub(super) fn butterfly_desc_from_gui_adjustables(
        gui_adjustables: &crate::app::GuiAdjustables,
        flight_variant: ButterflyFlightVariant,
        flight_tuning: ButterflyFlightTuning,
    ) -> ButterflyEmitterDesc {
        let (height_offset_min, height_offset_max) = {
            let min = gui_adjustables.butterfly_height_offset_min.value;
            let max = gui_adjustables.butterfly_height_offset_max.value;
            (min.min(max), min.max(max))
        };
        let (lifetime_min, lifetime_max) = {
            let min = gui_adjustables.butterfly_lifetime_min.value;
            let max = gui_adjustables.butterfly_lifetime_max.value;
            (min.min(max), min.max(max))
        };

        ButterflyEmitterDesc {
            enabled: gui_adjustables.butterflies_enabled.value,
            spawn_rate_per_source: gui_adjustables.butterfly_spawn_rate_per_source.value,
            max_active_butterflies: butterfly_world_limit(super::CHUNK_DIM),
            height_offset_min,
            height_offset_max,
            size: gui_adjustables.butterfly_size.value,
            lifetime_min,
            lifetime_max,
            color_low: Vec4::ONE,
            color_high: Vec4::ONE,
            worm_noise_frequency: gui_adjustables.butterfly_worm_noise_frequency.value,
            worm_noise_detail_frequency: gui_adjustables
                .butterfly_worm_noise_detail_frequency
                .value,
            worm_noise_detail_weight: gui_adjustables.butterfly_worm_noise_detail_weight.value,
            flight_variant,
            flight_tuning,
        }
    }

    pub(super) fn ensure_butterfly_emitter(&mut self) {
        if !self.butterfly_emitters.is_empty() {
            return;
        }

        self.butterfly_emitters
            .push(ButterflyEmitter::new(9_173, &self.butterfly_emitter_desc));
    }

    pub(super) fn update_particle_simulation(&mut self, dt: f32) {
        if dt <= 0.0 {
            return;
        }

        let total_start = Instant::now();
        self.prepare_fallen_leaf_review();
        let setup_start = Instant::now();
        let allows_ambient_emitters = self.launch_owners.allows_ambient_particle_emitters();
        if allows_ambient_emitters {
            self.butterfly_emitter_desc = Self::butterfly_desc_from_gui_adjustables(
                &self.debug_settings.adjustables,
                self.debug_settings.butterfly_flight.variant,
                self.debug_settings.butterfly_flight.tuning,
            );
            for emitter in &mut self.butterfly_emitters {
                emitter.apply_desc(&self.butterfly_emitter_desc);
            }
            self.ensure_butterfly_emitter();
        }
        let wind_time = self.time_info.time_since_start();
        self.particle_system
            .set_bucket_step_seconds(self.debug_settings.adjustables.world_tick_seconds.value);
        let setup_ms = setup_start.elapsed().as_secs_f32() * 1000.0;

        let emit_start = Instant::now();
        if allows_ambient_emitters {
            Self::drive_emitters(
                &mut self.butterfly_emitters,
                &mut self.particle_system,
                dt,
                wind_time,
            );
            self.trees.advance_leaf_emitters(
                &mut self.particle_system,
                dt,
                &self.wind_prototype.field.frame(),
                self.render_flags.enable_leaves,
            );
            let world_tick_seconds = self.debug_settings.adjustables.world_tick_seconds.value;
            self.sprinklers.advance_particles(
                &mut self.particle_system,
                dt,
                wind_time,
                self.world_clock.flora_tick(),
                world_tick_seconds,
            );
        }
        let emit_ms = emit_start.elapsed().as_secs_f32() * 1000.0;

        let sim_start = Instant::now();
        let wind = self.wind_prototype.field.frame();
        self.particle_system
            .update_with_wind(dt, self.particle_forces, &wind);
        let world_max =
            super::CHUNK_DIM.as_vec3() + Vec3::Y * crate::tracer::TERRARIUM_GLASS_TOP_PADDING_WORLD;
        let animation_time = self.butterfly_presentation_time();
        for emitter in &mut self.butterfly_emitters {
            emitter.synchronize_animation_clock(animation_time, dt);
            emitter.advance_guided_flight(
                &mut self.particle_system,
                dt,
                world_max,
                &wind,
                |origin, direction| {
                    self.contree_builder
                        .query_terrain_ray_cpu_filtered(
                            origin,
                            direction,
                            BUTTERFLY_FLIGHT_SURFACE_MASK,
                        )
                        .map(|hit| hit.position.distance(origin))
                },
            );
        }
        let sim_ms = sim_start.elapsed().as_secs_f32() * 1000.0;

        let collect_start = Instant::now();
        self.update_terrain_harvest_particle_collection(dt);
        let collect_ms = collect_start.elapsed().as_secs_f32() * 1000.0;

        let plan_start = Instant::now();
        let tick_step = self.particle_system.last_tick_step();
        if tick_step.did_step {
            self.particle_animation_time_sec += tick_step.step_seconds;
            self.plan_butterflies(tick_step);
        }
        let plan_ms = plan_start.elapsed().as_secs_f32() * 1000.0;

        let snapshot_start = Instant::now();
        self.advance_butterfly_review_settings();
        let frame = crate::particles::ButterflyFrame::at(
            self.butterfly_presentation_time(),
            self.debug_settings
                .adjustables
                .butterfly_animation_fps
                .value,
        );
        self.particle_system
            .write_snapshots_for_frame(&mut self.particle_snapshots, frame);
        self.review_butterfly_frame(dt);
        let sim_snapshot_count = self.particle_snapshots.len();
        self.log_fallen_leaf_review();
        self.append_water_debug_snapshots();
        self.append_butterfly_mesh_preview(frame);
        let snapshot_ms = snapshot_start.elapsed().as_secs_f32() * 1000.0;

        let upload_start = Instant::now();
        let settings = &self.debug_settings.adjustables;
        let butterfly_mesh = crate::tracer::ButterflyMeshSettings {
            resolution: settings.butterfly_pixel_resolution.value,
            fps: settings.butterfly_animation_fps.value,
            self_shadows: settings.butterfly_self_shadows.value,
            transmission: settings.butterfly_wing_transmission.value,
        };
        let leaf_model = crate::tracer::LeafModelSettings {
            enabled: settings.falling_leaf_mesh.value,
            resolution: settings.falling_leaf_pixel_resolution.value,
            size_scale: settings.falling_leaf_size_scale.value,
        };
        if let Err(err) =
            self.tracer
                .upload_particles(&self.particle_snapshots, butterfly_mesh, leaf_model)
        {
            log::error!("Failed to upload particles: {}", err);
        }
        let upload_ms = upload_start.elapsed().as_secs_f32() * 1000.0;

        if self.perf_logging {
            log::info!(
                "[PERF][PARTICLES] alive={} snapshots={} water_debug={} emitters butterflies={} leaves={} sprinklers={} tick_step={} dt={:.4} total={:.3}ms setup={:.3} emit={:.3} sim={:.3} collect={:.3} plan={:.3} snapshot={:.3} upload={:.3}",
                self.particle_system.alive_count(),
                self.particle_snapshots.len(),
                self.particle_snapshots.len().saturating_sub(sim_snapshot_count),
                self.butterfly_emitters.len(),
                self.trees.leaf_emitter_count(),
                self.sprinklers.len(),
                tick_step.did_step,
                dt,
                total_start.elapsed().as_secs_f32() * 1000.0,
                setup_ms,
                emit_ms,
                sim_ms,
                collect_ms,
                plan_ms,
                snapshot_ms,
                upload_ms,
            );
        }
    }

    fn append_water_debug_snapshots(&mut self) {
        if !self.water.terrain_status().is_initialized() {
            return;
        }

        let remaining_capacity = PARTICLE_CAPACITY.saturating_sub(self.particle_snapshots.len());
        if remaining_capacity == 0 {
            return;
        }

        let bounds = self.water.config().collider;
        let water_particle_size = water_debug_particle_size(
            self.debug_settings
                .adjustables
                .water_particle_quad_size
                .value,
        );
        let Some(frame) = self.water.latest_particle_frame() else {
            return;
        };
        for particle in frame
            .particles()
            .iter()
            .filter(|particle| {
                particle.position_ws.is_finite() && bounds.contains(particle.position_ws)
            })
            .take(remaining_capacity)
        {
            self.particle_snapshots.push(ParticleSnapshot {
                position_ws: particle.position_ws,
                velocity: particle.velocity,
                color: WATER_DEBUG_COLOR,
                size: water_particle_size,
                kind: ParticleRenderKind::Leaf,
                palette_index: 0,
                animation_phase_offset: 0.0,
                animation_sample_time: None,
                butterfly_wingbeat: None,
                leaf_orientation: None,
                leaf_shape_seed: None,
            });
        }
    }

    fn butterfly_presentation_time(&self) -> f32 {
        if std::env::var_os("RE_FLORA_BUTTERFLY_MESH_REVIEW").is_some() {
            0.237
        } else {
            self.time_info.time_since_start()
        }
    }

    /// Explicit hidden-app correctness fixture, not a saved setting or a perf
    /// path. Exercises real GUI-bound inputs, attachment motion and fruit drops.
    pub(super) fn prepare_apple_pixel_review(&mut self) {
        let Some(counter) = self.apple_pixel_review_frame.as_mut() else {
            return;
        };
        let frame = *counter;
        *counter = counter.saturating_add(1);
        let phase = (frame / 30).min(11);
        let dropped = phase >= 6;
        let stage = phase % 6;
        let enabled = stage != 0 && stage != 4;
        let n = match stage {
            1 => 8,
            3 => 64,
            _ => 32,
        };
        let settings = &mut self.debug_settings.adjustables;
        settings.apple_preview_model.value = enabled;
        settings.apple_pixel_resolution.value = n;
        settings.fruit_cycle.value = if dropped { 1.0 } else { 0.7 };
        if frame.is_multiple_of(30) && frame / 30 <= 11 {
            log::info!("[APPLE_PIXEL_REVIEW] phase={phase} dropped={dropped} enabled={enabled} resolution={n} diagnostic_only=true saved=false");
        }
    }

    /// Explicit Debug inspection fixture, not ecological spawns or simulation particles.
    /// Uses the exact same rendering path, palette selection, sun and scene depth.
    fn append_butterfly_mesh_preview(&mut self, frame: crate::particles::ButterflyFrame) {
        if !self.debug_settings.adjustables.butterfly_mesh_preview.value {
            return;
        }
        let origin = self.tracer.camera_position();
        let front = self.tracer.camera_front().normalize();
        let right = front.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(front).normalize();
        let time = frame.time_seconds();
        // Explicit renderer-only Release stress fixture. It exercises the same
        // production upload/tile/draw paths without readbacks or CPU oracles;
        // real flight correctness remains covered by fallen_leaf_review.
        if let Ok(value) = std::env::var("RE_FLORA_MODEL_PIXEL_STRESS_LEAVES") {
            let count = value
                .parse::<usize>()
                .expect("stress leaf count must be an integer");
            assert!(
                count <= crate::particles::PARTICLE_CAPACITY,
                "stress leaf count exceeds particle capacity"
            );
            static ANNOUNCE: std::sync::Once = std::sync::Once::new();
            ANNOUNCE.call_once(||log::info!("[MODEL_PIXEL_STRESS] leaves={count} butterflies=21 renderer_only=true readback=false saved=false"));
            let columns = (count as f32).sqrt().ceil().max(1.) as usize;
            for index in 0..count {
                let x = ((index % columns) as f32 + 0.5) / columns as f32 * 2. - 1.;
                let y = ((index / columns) as f32 + 0.5) / columns as f32 * 2. - 1.;
                let phase = index as f32 * 0.173;
                self.particle_snapshots.push(ParticleSnapshot {
                    position_ws: origin + front * 0.35 + right * (x * 0.13) + up * (y * 0.075),
                    velocity: Vec3::ZERO,
                    color: Vec4::new(0.7, 0.35, 0.12, 1.),
                    size: 0.008,
                    kind: ParticleRenderKind::Leaf,
                    palette_index: 0,
                    animation_phase_offset: 0.,
                    animation_sample_time: None,
                    butterfly_wingbeat: None,
                    leaf_orientation: Some(
                        glam::Quat::from_rotation_x(time * 0.7 + phase)
                            * glam::Quat::from_rotation_y(phase)
                            * glam::Quat::from_rotation_z(time * 0.4),
                    ),
                    leaf_shape_seed: Some(index as u32 * 137),
                });
            }
        }
        for (row, distance) in [0.25, 0.5, 1.0].into_iter().enumerate() {
            for preset in 0..crate::tracer::ButterflyPalettePreset::COUNT {
                let heading = preset as f32 * 0.35 + time * 0.3;
                self.particle_snapshots.push(ParticleSnapshot {
                    position_ws: origin
                        + front * distance
                        + right * ((preset as f32 - 3.) * 0.16 * distance)
                        + up * ((1. - row as f32) * 0.22 * distance),
                    velocity: Vec3::new(heading.sin(), 0., -heading.cos()) * 0.05,
                    color: Vec4::ONE,
                    size: 0.03,
                    kind: ParticleRenderKind::Butterfly,
                    palette_index: preset,
                    animation_phase_offset: 0.,
                    animation_sample_time: Some(time),
                    butterfly_wingbeat: None,
                    leaf_orientation: None,
                    leaf_shape_seed: None,
                });
            }
        }
    }

    /// Apply fixture settings before producing the single frame shared by all butterflies.
    fn advance_butterfly_review_settings(&mut self) {
        let Some(review) = self.butterfly_review.as_mut() else {
            return;
        };
        review.frame += 1;
        let frame = review.frame;
        if std::env::var("RE_FLORA_BUTTERFLY_REVIEW").as_deref() == Ok("wingbeat") {
            // Opt-in runtime diagnostic, using the same saved field as the A/B UI.
            // First enable, then exercise an off/on handoff after a subject appears.
            let elapsed = review.subject_frame.map(|start| frame - start);
            let enabled = match elapsed {
                _ if frame == 1 => Some(true),
                Some(120) => Some(false),
                Some(240) => Some(true),
                _ => None,
            };
            if let Some(enabled) = enabled {
                self.debug_settings
                    .butterfly_flight
                    .tuning
                    .wingbeat_coupling = enabled;
                log::info!("[BUTTERFLY_WINGBEAT_REVIEW] frame={frame} enabled={enabled}");
            }
        }
        if let Ok(mode) = std::env::var("RE_FLORA_BUTTERFLY_MESH_REVIEW") {
            let settings = &mut self.debug_settings.adjustables;
            settings.butterfly_mesh_preview.value = true;
            if mode == "sweep" {
                let stage = (frame / 60).min(6);
                settings.butterfly_wing_transmission.value = match stage {
                    5 => 0.5,
                    6 => 1.0,
                    _ => 0.0,
                };
                settings.butterfly_pixel_resolution.value = match stage {
                    1 => 8,
                    2 => 64,
                    _ => 22,
                };
                settings.butterfly_self_shadows.value = stage != 3;
                settings.butterfly_animation_fps.value = if stage == 1 { 2 } else { 60 };
            }
        }
    }

    /// Observe published natural flight; mesh fixtures only need the settings step above.
    fn review_butterfly_frame(&mut self, dt: f32) {
        if std::env::var_os("RE_FLORA_BUTTERFLY_MESH_REVIEW").is_some() {
            return;
        }
        let Some(review) = self.butterfly_review.as_mut() else {
            return;
        };
        let frame = review.frame;
        let height_review = std::env::var("RE_FLORA_BUTTERFLY_REVIEW").as_deref() == Ok("height");
        let terrain_y = |position: Vec3| {
            let origin = Vec3::new(
                position.x,
                super::CHUNK_DIM.y as f32 + crate::tracer::TERRARIUM_GLASS_TOP_PADDING_WORLD,
                position.z,
            );
            self.contree_builder
                .query_terrain_ray_cpu_filtered(origin, -Vec3::Y, BUTTERFLY_FLIGHT_SURFACE_MASK)
                .map(|hit| hit.position.y)
        };
        let butterflies = self
            .particle_snapshots
            .iter()
            .filter(|s| matches!(s.kind, ParticleRenderKind::Butterfly))
            .collect::<Vec<_>>();
        if std::env::var("RE_FLORA_BUTTERFLY_REVIEW").as_deref() == Ok("wingbeat")
            && frame.is_multiple_of(15)
        {
            log::info!(
                "[BUTTERFLY_WINGBEAT_REVIEW] frame={frame} samples={:?}",
                butterflies
                    .iter()
                    .take(4)
                    .map(|s| (s.position_ws, s.velocity, s.butterfly_wingbeat))
                    .collect::<Vec<_>>()
            );
        }
        if frame >= 240 && review.subject_frame.is_none() {
            if let Some(subject) = butterflies.iter().find(|s| {
                s.color.w >= 0.99
                    && (!height_review
                        || terrain_y(s.position_ws).is_some_and(|ground| {
                            (0.03..0.15).contains(&(s.position_ws.y - ground))
                        }))
            }) {
                let target = subject.position_ws;
                let camera = if height_review {
                    let mut camera = target + Vec3::new(0., 0., 0.45);
                    camera.z = camera.z.clamp(0.1, super::CHUNK_DIM.z as f32 - 0.1);
                    camera.y = terrain_y(camera).unwrap_or(target.y - 0.08) + 0.08;
                    camera
                } else {
                    target + Vec3::new(0.0, 0.30, 0.95)
                };
                self.tracer.set_camera_pose_looking_at(camera, target);
                review.subject_frame = Some(frame);
                log::info!("[BUTTERFLY_REVIEW] camera=fixed-natural-subject frame={frame} target={target:?}");
            }
        }
        if frame >= 240 {
            if height_review && frame.is_multiple_of(30) {
                log::info!(
                    "[BUTTERFLY_HEIGHT_REVIEW] frame={frame} requested={} samples={:?}",
                    self.debug_settings
                        .butterfly_flight
                        .tuning
                        .height_above_ground,
                    butterflies
                        .iter()
                        .map(|s| (
                            s.position_ws.to_array(),
                            terrain_y(s.position_ws).map(|ground| s.position_ws.y - ground),
                            s.color.w
                        ))
                        .collect::<Vec<_>>()
                );
            }
            log::info!(
                "[BUTTERFLY_REVIEW] frame={frame} dt={dt:.6} variant={:?} count={} particles={:?}",
                self.debug_settings.butterfly_flight.variant,
                butterflies.len(),
                butterflies
                    .iter()
                    .map(|s| (
                        s.position_ws.to_array(),
                        s.velocity.to_array(),
                        s.palette_index,
                        s.color.w
                    ))
                    .collect::<Vec<_>>()
            );
        }
        if std::env::var("RE_FLORA_BUTTERFLY_REVIEW").as_deref() == Ok("tuning") {
            let next = match review.subject_frame.map(|start| frame - start) {
                Some(90) => Some(ButterflyFlightTuning {
                    flight_frequency_hz: 0.0,
                    ..ButterflyFlightTuning::default()
                }),
                Some(180) => Some(ButterflyFlightTuning {
                    wingbeat_coupling: false,
                    flight_frequency_hz: 6.25,
                    height_above_ground: 0.08,
                    maneuver_tempo: 2.0,
                    vertical_strength: 3.0,
                    turn_sharpness: 2.0,
                    speed: 1.0,
                    wind_drift: 1.0,
                }),
                Some(270) => Some(ButterflyFlightTuning::default()),
                _ => None,
            };
            if let Some(next) = next {
                self.debug_settings.butterfly_flight.tuning = next;
                log::info!("[BUTTERFLY_REVIEW] scripted_tuning={next:?} frame={frame}");
            }
        }
        if std::env::var("RE_FLORA_BUTTERFLY_REVIEW").as_deref() == Ok("wind") {
            let wind = self.wind_prototype.field.frame();
            log::info!(
                "[BUTTERFLY_REVIEW][WIND] frame={frame} self_speed={} drift_gain={} samples={:?}",
                self.debug_settings.butterfly_flight.tuning.speed,
                self.debug_settings.butterfly_flight.tuning.wind_drift,
                butterflies
                    .iter()
                    .map(|s| wind.sample_world(s.position_ws).to_array())
                    .collect::<Vec<_>>()
            );
            let next_gain = match review.subject_frame.map(|start| frame - start) {
                Some(90) => Some(0.0),
                Some(180) => Some(2.0),
                Some(270) => Some(1.0),
                _ => None,
            };
            if let Some(gain) = next_gain {
                self.debug_settings.butterfly_flight.tuning.wind_drift = gain;
                log::info!("[BUTTERFLY_REVIEW] scripted_wind_gain={gain} frame={frame}");
            }
        }
        if std::env::var("RE_FLORA_BUTTERFLY_REVIEW").as_deref() == Ok("rhythm") {
            let next_frequency = match review.subject_frame.map(|start| frame - start) {
                Some(90) => Some(20.0),
                Some(180) => Some(40.0),
                Some(270) => Some(10.0),
                _ => None,
            };
            if let Some(frequency) = next_frequency {
                self.debug_settings
                    .butterfly_flight
                    .tuning
                    .flight_frequency_hz = frequency;
                log::info!("[BUTTERFLY_REVIEW] scripted_shared_frequency_hz={frequency} frame={frame} tuning={:?}", self.debug_settings.butterfly_flight.tuning);
            }
        }
    }

    pub(super) fn plan_butterflies(&mut self, tick_step: ParticleTickStep) {
        const MAX_RETRIES: usize = 3;
        const STEP_LEN: f32 = crate::particles::emitters::WORM_STEP_LEN;
        const RAY_EPSILON: f32 = 0.02;
        // Match the terrarium glass box top.
        let map_size = super::CHUNK_DIM.as_vec3();
        let butterfly_max_y = map_size.y + crate::tracer::TERRARIUM_GLASS_TOP_PADDING_WORLD;

        let mut all_handles: Vec<ParticleHandle> = Vec::new();
        let mut all_positions: Vec<Vec3> = Vec::new();
        let mut all_directions: Vec<Vec3> = Vec::new();
        let mut all_emerging: Vec<bool> = Vec::new();
        let mut all_emitter_indices: Vec<usize> = Vec::new();

        for emitter_idx in 0..self.butterfly_emitters.len() {
            if self.butterfly_emitters[emitter_idx]
                .flight_variant()
                .uses_darting_flight()
            {
                continue;
            }
            let (mut handles, mut positions, mut directions, mut emerging) = {
                let emitter = &mut self.butterfly_emitters[emitter_idx];
                let mut handles = Vec::new();
                let mut positions = Vec::new();
                let mut directions = Vec::new();
                let mut emerging = Vec::new();
                emitter.collect_butterfly_states(
                    &self.particle_system,
                    &mut handles,
                    &mut positions,
                    &mut directions,
                    &mut emerging,
                );
                (handles, positions, directions, emerging)
            };

            if tick_step.bucket_count > 1 {
                let active_bucket = tick_step.active_bucket;
                let mut filtered_handles = Vec::with_capacity(handles.len());
                let mut filtered_positions = Vec::with_capacity(positions.len());
                let mut filtered_directions = Vec::with_capacity(directions.len());
                let mut filtered_emerging = Vec::with_capacity(emerging.len());

                for (((handle, position), direction), is_emerging) in handles
                    .into_iter()
                    .zip(positions.into_iter())
                    .zip(directions.into_iter())
                    .zip(emerging.into_iter())
                {
                    if self.particle_system.handle_bucket(handle) == Some(active_bucket) {
                        filtered_handles.push(handle);
                        filtered_positions.push(position);
                        filtered_directions.push(direction);
                        filtered_emerging.push(is_emerging);
                    }
                }

                handles = filtered_handles;
                positions = filtered_positions;
                directions = filtered_directions;
                emerging = filtered_emerging;
            }

            all_emitter_indices.resize(all_emitter_indices.len() + handles.len(), emitter_idx);
            all_handles.extend(handles);
            all_positions.extend(positions);
            all_directions.extend(directions);
            all_emerging.extend(emerging);
        }

        if all_handles.is_empty() {
            return;
        }

        let n = all_handles.len();
        let mut successes = vec![false; n];
        let mut committed_dirs = all_directions.clone();
        let mut pending_retry: Vec<(usize, Vec3, Vec3)> = Vec::new();

        for attempt in 0..=MAX_RETRIES {
            let is_initial = attempt == 0;
            let pending_count = pending_retry.len();

            if pending_count == 0 && !is_initial {
                break;
            }

            let batch: Vec<(usize, Vec3, Vec3)> = if is_initial {
                all_positions
                    .iter()
                    .enumerate()
                    .map(|(i, pos)| (i, *pos, all_directions[i]))
                    .collect()
            } else {
                std::mem::take(&mut pending_retry)
            };

            if batch.is_empty() {
                continue;
            }

            for (idx, origin, dir) in batch.into_iter() {
                if successes[idx] {
                    continue;
                }

                let next_pos = origin + dir * STEP_LEN;

                let out_of_bounds = next_pos.x < 0.0
                    || next_pos.x > map_size.x
                    || next_pos.y < 0.0
                    || next_pos.y > butterfly_max_y
                    || next_pos.z < 0.0
                    || next_pos.z > map_size.z;

                if out_of_bounds {
                    if attempt < MAX_RETRIES {
                        let new_dir = {
                            let emitter_idx = all_emitter_indices[idx];
                            if let Some(em) = self.butterfly_emitters.get_mut(emitter_idx) {
                                let new_seed = (dir.x * 1000.0 + dir.z * 100.0 + idx as f32)
                                    + (attempt as f32 * 17.3);
                                let new_phase = dir.y * TAU + idx as f32 + attempt as f32 * 3.7;
                                crate::particles::emitters::generate_worm_direction(
                                    &em.worm_noise,
                                    &em.worm_noise_detail,
                                    em.worm_noise_detail_weight,
                                    new_seed,
                                    new_phase,
                                )
                            } else {
                                dir
                            }
                        };
                        pending_retry.push((idx, origin, new_dir));
                    } else {
                        if let Some(em) = self.butterfly_emitters.get_mut(all_emitter_indices[idx])
                        {
                            em.despawn_butterfly(all_handles[idx]);
                        }
                        let _ = self.particle_system.despawn(all_handles[idx]);
                    }
                    continue;
                }

                let blocked = if all_emerging[idx] {
                    false
                } else if let Some(hit) = self.query_terrain_ray_cpu(
                    origin + Vec3::new(0.0, RAY_EPSILON, 0.0),
                    dir.normalize_or_zero(),
                ) {
                    let hit_dist = (hit.position - origin).length();
                    hit_dist < STEP_LEN - RAY_EPSILON
                } else {
                    false
                };

                if blocked {
                    if attempt < MAX_RETRIES {
                        let new_dir = {
                            let emitter_idx = all_emitter_indices[idx];
                            if let Some(em) = self.butterfly_emitters.get_mut(emitter_idx) {
                                let new_seed = (dir.x * 1000.0 + dir.z * 100.0 + idx as f32)
                                    + (attempt as f32 * 17.3);
                                let new_phase = dir.y * TAU + idx as f32 + attempt as f32 * 3.7;
                                crate::particles::emitters::generate_worm_direction(
                                    &em.worm_noise,
                                    &em.worm_noise_detail,
                                    em.worm_noise_detail_weight,
                                    new_seed,
                                    new_phase,
                                )
                            } else {
                                dir
                            }
                        };
                        pending_retry.push((idx, origin, new_dir));
                    } else {
                        if let Some(em) = self.butterfly_emitters.get_mut(all_emitter_indices[idx])
                        {
                            em.despawn_butterfly(all_handles[idx]);
                        }
                        let _ = self.particle_system.despawn(all_handles[idx]);
                    }
                } else {
                    successes[idx] = true;
                    committed_dirs[idx] = dir;
                }
            }
        }

        for i in 0..n {
            if !successes[i] {
                continue;
            }
            let emitter_idx = all_emitter_indices[i];
            if let Some(em) = self.butterfly_emitters.get_mut(emitter_idx) {
                em.set_butterfly_state(all_handles[i], all_positions[i], committed_dirs[i]);
            }
            let _ = self
                .particle_system
                .set_velocity(all_handles[i], committed_dirs[i] * STEP_LEN);
        }
    }

    pub(super) fn drive_emitters<E: ParticleEmitter>(
        emitters: &mut [E],
        particle_system: &mut ParticleSystem,
        dt: f32,
        time: f32,
    ) {
        for emitter in emitters {
            emitter.update(particle_system, dt, time);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::gui_config::butterfly_flight::draw_butterfly_flight_tuning;

    #[test]
    fn butterfly_tuning_sliders_respond_to_pointer_input_without_reset() {
        let context = egui::Context::default();
        let mut saved = crate::app::gui_config_model::SavedCustomSettings::default();
        let mut draw = |events| {
            let mut rects = [egui::Rect::NOTHING; 5];
            let _ = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 700.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    let sliders = draw_butterfly_flight_tuning(
                        &mut crate::app::gui_config::saved_controls::SavedControls::for_test(
                            ui, &mut saved,
                        ),
                    );
                    rects = sliders.map(|response| response.rect);
                },
            );
            (rects, saved.butterfly_flight.tuning)
        };
        draw(Vec::new());
        let (_, initial) = draw(Vec::new());
        let click_events = |pos, pressed| {
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        for index in 0..5 {
            let (rects, _) = draw(Vec::new());
            let rect = rects[index];
            // Use current layout and click inside the track, not a possibly-default endpoint.
            let pos = egui::pos2(
                rect.left() + context.style().spacing.slider_width * 0.5,
                rect.center().y,
            );
            draw(click_events(pos, true));
            draw(click_events(pos, false));
        }
        let (_, edited) = draw(Vec::new());
        assert_eq!(edited.flight_frequency_hz, initial.flight_frequency_hz);
        assert_ne!(edited.height_above_ground, initial.height_above_ground);
        assert_eq!(edited.maneuver_tempo, initial.maneuver_tempo);
        assert_ne!(edited.vertical_strength, initial.vertical_strength);
        assert_ne!(edited.turn_sharpness, initial.turn_sharpness);
        assert_ne!(edited.speed, initial.speed);
        assert_ne!(edited.wind_drift, initial.wind_drift);
    }

    #[test]
    fn butterfly_world_limit_counts_all_world_chunks() {
        assert_eq!(butterfly_world_limit(glam::UVec3::new(2, 2, 2)), 16);
        assert_eq!(butterfly_world_limit(glam::UVec3::new(3, 2, 4)), 48);
    }

    fn cluster(x: f32) -> ClusterResult {
        ClusterResult {
            pos: Vec3::new(x, 1.0, 1.0),
            items_count: 4,
        }
    }

    #[test]
    fn tree_leaf_emitter_removal_repairs_swapped_tree_indices() {
        let mut runtime = TreeLeafEmitterRuntime::new(LeafEmitterDesc::default());
        runtime.upsert(1, &[cluster(1.0), cluster(2.0)]);
        runtime.upsert(2, &[cluster(3.0)]);
        assert_eq!(runtime.len(), 3);

        runtime.remove(1);
        assert_eq!(runtime.len(), 1);

        runtime.upsert(2, &[cluster(4.0), cluster(5.0)]);
        assert_eq!(runtime.len(), 2);
        runtime.remove(2);
        assert_eq!(runtime.len(), 0);
    }

    #[test]
    fn detached_voxel_particle_is_one_voxel_wide_and_falls_until_it_despawns() {
        let world_voxel = glam::UVec3::new(64, 96, 128);
        let color = Vec4::new(0.4, 0.3, 0.2, 1.0);

        let spawn = detached_terrain_voxel_spawn(world_voxel, color);

        assert_eq!(
            spawn.position,
            (world_voxel.as_vec3() + Vec3::splat(0.5))
                / crate::app::core::VOXEL_DIM_PER_CHUNK.as_vec3()
        );
        assert_eq!(spawn.size, STANDARD_PARTICLE_SIZE);
        assert_eq!(spawn.color, color);
        assert_eq!(spawn.motion_mode, crate::particles::MotionMode::Free);
        assert_eq!(spawn.render_kind, ParticleRenderKind::TerrainVoxel);
        assert!(spawn.gravity_factor > 0.0);
        assert!(spawn.despawn_on_lifetime);
        assert!(spawn.despawn_below_ground);
    }

    #[test]
    fn emissive_harvest_particles_use_the_backpack_material_color() {
        assert_eq!(
            terrain_harvest_rgb_for_voxel(crate::builder::VOXEL_TYPE_EMISSIVE),
            crate::lighting::EMISSIVE_VOXEL_COLOR_RGB8,
        );
    }

    #[test]
    fn experimental_glass_does_not_spawn_sand_harvest_particles() {
        let mut stats = ChunkModifyStats::default();
        stats.removed_counts[crate::builder::VOXEL_TYPE_SAND as usize] = 4;
        stats.removed_counts[crate::builder::VOXEL_TYPE_EMISSIVE as usize] = 2;

        let (standard_total, standard_types) =
            harvest_distribution(&stats, crate::voxel_material::VoxelMaterialMode::Standard);
        assert_eq!(standard_total, 6);
        assert_eq!(standard_types, vec![(3, 4), (8, 6)]);

        let (experiment_total, experiment_types) = harvest_distribution(
            &stats,
            crate::voxel_material::VoxelMaterialMode::GlassExperiment,
        );
        assert_eq!(experiment_total, 2);
        assert_eq!(experiment_types, vec![(8, 2)]);
    }
}

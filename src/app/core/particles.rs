use super::App;
use crate::builder::ChunkModifyStats;
use crate::particles::{
    ButterflyEmitter, ButterflyEmitterDesc, ButterflySpawnSource, FallenLeafEmitter,
    LeafEmitterDesc, ParticleEmitter, ParticleHandle, ParticleRenderKind, ParticleSnapshot,
    ParticleSpawn, ParticleSystem, ParticleTickStep, ParticleUpdateConfig, PARTICLE_CAPACITY,
    STANDARD_PARTICLE_SIZE,
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
const BUTTERFLY_SPAWN_SOURCE_REFRESH_SECONDS: f32 = 1.0;
const DETACHED_TERRAIN_UPDATE: ParticleUpdateConfig = ParticleUpdateConfig::new(1.0 / 30.0, 2);

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
        _ => [210, 190, 140],
    }
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
        texture_variant: 0,
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

impl ParticleEmitter for TreeLeafEmitter {
    fn update(&mut self, system: &mut ParticleSystem, dt: f32, time: f32) {
        self.emitter.update(system, dt, time);
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
        time: f32,
        enabled: bool,
    ) {
        for emitter in &mut self.emitters {
            emitter.emitter.enabled = enabled;
            emitter.update(particle_system, dt, time);
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

        let mut removed_total = 0u32;
        for count in stats.removed_counts {
            removed_total = removed_total.saturating_add(count);
        }
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

        let mut removed_types = Vec::new();
        let mut cumulative = 0u32;
        for (voxel_type, count) in stats.removed_counts.iter().enumerate() {
            if *count == 0 {
                continue;
            }
            cumulative = cumulative.saturating_add(*count);
            removed_types.push((voxel_type as u32, cumulative));
        }
        if removed_types.is_empty() {
            return;
        }

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
                texture_variant: 0,
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
        }
    }

    pub(super) fn ensure_butterfly_emitter(&mut self) {
        if !self.butterfly_emitters.is_empty() {
            return;
        }

        self.butterfly_emitters
            .push(ButterflyEmitter::new(9_173, &self.butterfly_emitter_desc));
    }

    fn refresh_butterfly_spawn_sources(&mut self, dt: f32) {
        self.butterfly_spawn_source_refresh_elapsed += dt.max(0.0);
        if self.butterfly_spawn_source_refresh_elapsed < BUTTERFLY_SPAWN_SOURCE_REFRESH_SECONDS {
            return;
        }
        self.butterfly_spawn_source_refresh_elapsed = 0.0;

        let ground_voxels = match self.surface_builder.flora_base_world_voxels() {
            Ok(positions) => positions,
            Err(err) => {
                log::warn!("Failed to refresh butterfly flora spawn sources: {err}");
                return;
            }
        };
        let ground_source_count = ground_voxels.len();
        let tree_spawn_positions = self.trees.butterfly_spawn_positions();
        let tree_source_count = tree_spawn_positions.len();
        let mut sources = Vec::with_capacity(ground_source_count + tree_source_count);
        let voxel_scale = super::VOXEL_DIM_PER_CHUNK.as_vec3();
        sources.extend(ground_voxels.into_iter().map(|position| {
            ButterflySpawnSource::ground_flora(
                (position.as_vec3() + Vec3::splat(0.5)) / voxel_scale,
            )
        }));
        sources.extend(
            tree_spawn_positions
                .into_iter()
                .map(ButterflySpawnSource::tree_leaf),
        );

        let previous_source_count = self
            .butterfly_emitters
            .first()
            .map_or(0, ButterflyEmitter::spawn_source_count);
        let total_source_count = sources.len();
        for emitter in &mut self.butterfly_emitters {
            emitter.set_spawn_sources(sources.clone());
        }
        if previous_source_count != total_source_count {
            log::info!(
                "[BUTTERFLY][SPAWN_SOURCES] ground_flora={} tree_leaves={} total={} rate_per_source_per_second={:.6}",
                ground_source_count,
                tree_source_count,
                total_source_count,
                self.butterfly_emitter_desc.spawn_rate_per_source,
            );
        }
    }

    pub(super) fn update_particle_simulation(&mut self, dt: f32) {
        if dt <= 0.0 {
            return;
        }

        let total_start = Instant::now();
        let setup_start = Instant::now();
        let diagnostic_capacity_isolation = self
            .launch_owners
            .connectivity_event()
            .isolates_particle_capacity();
        if !diagnostic_capacity_isolation {
            self.butterfly_emitter_desc =
                Self::butterfly_desc_from_gui_adjustables(&self.debug_settings.adjustables);
            for emitter in &mut self.butterfly_emitters {
                emitter.apply_desc(&self.butterfly_emitter_desc);
            }
            self.ensure_butterfly_emitter();
            self.refresh_butterfly_spawn_sources(dt);
        }
        let wind_time = self.time_info.time_since_start();
        self.particle_system
            .set_bucket_step_seconds(self.debug_settings.adjustables.world_tick_seconds.value);
        let setup_ms = setup_start.elapsed().as_secs_f32() * 1000.0;

        let emit_start = Instant::now();
        if !diagnostic_capacity_isolation {
            Self::drive_emitters(
                &mut self.butterfly_emitters,
                &mut self.particle_system,
                dt,
                wind_time,
            );
            self.trees.advance_leaf_emitters(
                &mut self.particle_system,
                dt,
                wind_time,
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
        self.particle_system.update(dt, self.particle_forces);
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
        self.particle_system
            .write_snapshots(&mut self.particle_snapshots);
        let sim_snapshot_count = self.particle_snapshots.len();
        self.append_water_debug_snapshots();
        let snapshot_ms = snapshot_start.elapsed().as_secs_f32() * 1000.0;

        let upload_start = Instant::now();
        if let Err(err) = self.tracer.upload_particles(&self.particle_snapshots) {
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
        if !self.water_terrain_status().is_initialized() {
            return;
        }

        let remaining_capacity = PARTICLE_CAPACITY.saturating_sub(self.particle_snapshots.len());
        if remaining_capacity == 0 {
            return;
        }

        let bounds = self.water_sim.config.collider;
        let water_particle_size = water_debug_particle_size(
            self.debug_settings
                .adjustables
                .water_particle_quad_size
                .value,
        );
        let Some(frame) = self.water_sim.latest_particle_frame() else {
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
                texture_variant: 0,
                animation_frame_offset: 0,
            });
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
}

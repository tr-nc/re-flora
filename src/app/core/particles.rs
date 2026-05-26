use super::App;
use crate::builder::ChunkModifyStats;
use crate::geom::UAabb3;
use crate::particles::{
    ButterflyEmitter, ButterflyEmitterDesc, FallenLeafEmitter, ParticleEmitter, ParticleHandle,
    ParticleRenderKind, ParticleSnapshot, ParticleSpawn, ParticleSystem, ParticleTickStep,
    PARTICLE_CAPACITY,
};
use crate::util::ClusterResult;
use egui::Color32;
use glam::{Vec2, Vec3, Vec4};
use std::{f32::consts::TAU, time::Instant};

// bird-specific audio and control logic has been removed

#[allow(dead_code)]
const TERRAIN_HARVEST_MAX_PARTICLES_PER_EDIT: u32 = 4;
#[allow(dead_code)]
const TERRAIN_HARVEST_PARTICLE_SIZE: f32 = 1.0 / 256.0;
const DEFAULT_WATER_DEBUG_PARTICLE_SIZE: f32 = 0.012;
const WATER_DEBUG_COLOR: Vec4 = Vec4::new(0.12, 0.45, 1.0, 1.0);

fn water_debug_particle_size(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.001, 0.1)
    } else {
        DEFAULT_WATER_DEBUG_PARTICLE_SIZE
    }
}

pub(super) struct TreeLeafEmitter {
    tree_id: u32,
    pub(super) emitter: FallenLeafEmitter,
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

    #[allow(dead_code)]
    fn terrain_harvest_color_for_voxel(&self, voxel_type: u32) -> Vec4 {
        fn srgb_to_linear(channel: u8) -> f32 {
            let srgb = channel as f32 / 255.0;
            if srgb <= 0.04045 {
                srgb / 12.92
            } else {
                ((srgb + 0.055) / 1.055).powf(2.4)
            }
        }

        let color32 = match voxel_type {
            crate::builder::VOXEL_TYPE_DIRT => super::ActiveVoxelType::Dirt.color(),
            crate::builder::VOXEL_TYPE_CHERRY_WOOD => super::ActiveVoxelType::CherryWood.color(),
            crate::builder::VOXEL_TYPE_OAK_WOOD => super::ActiveVoxelType::OakWood.color(),
            crate::builder::VOXEL_TYPE_SAND => super::ActiveVoxelType::Sand.color(),
            crate::builder::VOXEL_TYPE_ROCK => super::ActiveVoxelType::Rock.color(),
            _ => egui::Color32::from_rgb(210, 190, 140),
        };

        Vec4::new(
            srgb_to_linear(color32.r()),
            srgb_to_linear(color32.g()),
            srgb_to_linear(color32.b()),
            1.0,
        )
    }

    #[allow(dead_code)]
    pub(super) fn spawn_terrain_harvest_particles(
        &mut self,
        center: Vec3,
        stats: &ChunkModifyStats,
        sampled_positions_world: &[Vec3],
    ) {
        if !self.gui_adjustables.terrain_harvest_particles_enabled.value {
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
            .gui_adjustables
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
            };
            if let Some(handle) = self.particle_system.spawn(spawn) {
                self.terrain_harvest_particle_handles.push(handle);
            }
        }
    }

    #[allow(dead_code)]
    fn update_terrain_harvest_particle_collection(&mut self, dt: f32) {
        if dt <= 0.0
            || !self.gui_adjustables.terrain_harvest_particles_enabled.value
            || self.terrain_harvest_particle_handles.is_empty()
        {
            return;
        }

        let collection_target = self.terrain_harvest_collection_target();
        let flyback_speed = self
            .gui_adjustables
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

    pub(super) fn butterfly_count_from_per_chunk(butterflies_per_chunk: f32) -> u32 {
        let total_chunks = super::CHUNK_DIM.x.saturating_mul(super::CHUNK_DIM.z) as f32;
        (total_chunks * butterflies_per_chunk)
            .round()
            .clamp(0.0, u32::MAX as f32) as u32
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
            butterfly_count: Self::butterfly_count_from_per_chunk(
                gui_adjustables.butterflies_per_chunk.value,
            ),
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

    pub(super) fn upsert_tree_leaf_emitter(
        &mut self,
        tree_id: u32,
        tree_pos: Vec3,
        bound: &UAabb3,
        clusters: &[ClusterResult],
    ) {
        self.remove_leaf_emitter(tree_id);

        if clusters.is_empty() {
            return;
        }

        let mut emitter_indices = Vec::with_capacity(clusters.len());

        for cluster in clusters {
            let (_center, _extent) = Self::compute_leaf_emitter_region(tree_pos, bound);
            let cluster_center = cluster.pos;

            let mut emitter = FallenLeafEmitter::new(
                cluster_center,
                Vec::new(),
                tree_id as u64 + cluster.pos.x as u64 + cluster.pos.y as u64 + cluster.pos.z as u64,
                &self.leaf_emitter_desc,
            );

            let cluster_size_multiplier = (cluster.items_count as f32).sqrt();
            emitter.spawn_rate = self.leaf_emitter_desc.spawn_rate * cluster_size_multiplier;

            let idx = self.leaf_emitters.len();
            self.leaf_emitters
                .push(TreeLeafEmitter::new(tree_id, emitter));
            emitter_indices.push(idx);
        }

        self.tree_leaf_emitter_indices
            .insert(tree_id, emitter_indices);
    }

    pub(super) fn compute_leaf_emitter_region(tree_pos: Vec3, bound: &UAabb3) -> (Vec3, Vec3) {
        if bound.min() == bound.max() {
            return (
                tree_pos + Vec3::new(0.5, 1.3, 0.5),
                Vec3::new(2.0, 0.5, 2.0),
            );
        }

        let min = bound.min().as_vec3() / 256.0;
        let max = bound.max().as_vec3() / 256.0;
        let size = max - min;
        let center = min + size * 0.5;
        let extent = Vec3::new(
            (size.x * 0.5).max(0.75),
            (size.y * 0.25).max(0.5),
            (size.z * 0.5).max(0.75),
        );

        (center, extent)
    }

    pub(super) fn remove_leaf_emitter(&mut self, tree_id: u32) {
        if let Some(indices) = self.tree_leaf_emitter_indices.remove(&tree_id) {
            let mut sorted_indices = indices;
            sorted_indices.sort_unstable_by(|a, b| b.cmp(a));

            for index in sorted_indices {
                self.leaf_emitters.swap_remove(index);
                if let Some(swapped) = self.leaf_emitters.get(index) {
                    if let Some(tree_indices) =
                        self.tree_leaf_emitter_indices.get_mut(&swapped.tree_id())
                    {
                        if let Some(pos) = tree_indices
                            .iter()
                            .position(|&i| i == self.leaf_emitters.len())
                        {
                            tree_indices[pos] = index;
                        }
                    }
                }
            }
        }
    }

    pub(super) fn ensure_map_butterfly_emitter(&mut self) {
        if !self.butterfly_emitters.is_empty() {
            return;
        }

        let (center, extent) = Self::map_butterfly_region();
        self.butterfly_emitters.push(ButterflyEmitter::new(
            center,
            extent,
            9_173,
            &self.butterfly_emitter_desc,
        ));
    }

    pub(super) fn map_butterfly_region() -> (Vec3, Vec3) {
        let map_size = super::CHUNK_DIM.as_vec3();
        let center = Vec3::new(map_size.x * 0.5, 0.5, map_size.z * 0.5);
        let extent = Vec3::new(
            (map_size.x * 0.5).max(1.0),
            0.6,
            (map_size.z * 0.5).max(1.0),
        );
        (center, extent)
    }

    pub(super) fn update_particle_simulation(&mut self, dt: f32) {
        if dt <= 0.0 {
            return;
        }

        let total_start = Instant::now();
        let setup_start = Instant::now();
        self.butterfly_emitter_desc =
            Self::butterfly_desc_from_gui_adjustables(&self.gui_adjustables);
        for emitter in &mut self.butterfly_emitters {
            emitter.apply_desc(&self.butterfly_emitter_desc);
        }
        self.ensure_map_butterfly_emitter();
        let wind_time = self.time_info.time_since_start();
        self.particle_system
            .set_bucket_step_seconds(self.gui_adjustables.world_tick_seconds.value);
        let setup_ms = setup_start.elapsed().as_secs_f32() * 1000.0;

        let emit_start = Instant::now();
        Self::drive_emitters(
            &mut self.butterfly_emitters,
            &mut self.particle_system,
            dt,
            wind_time,
        );
        Self::drive_emitters(
            &mut self.leaf_emitters,
            &mut self.particle_system,
            dt,
            wind_time,
        );
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
                "[PERF][PARTICLES] alive={} snapshots={} water_debug={} emitters butterflies={} leaves={} tick_step={} dt={:.4} total={:.3}ms setup={:.3} emit={:.3} sim={:.3} collect={:.3} plan={:.3} snapshot={:.3} upload={:.3}",
                self.particle_system.alive_count(),
                self.particle_snapshots.len(),
                self.particle_snapshots.len().saturating_sub(sim_snapshot_count),
                self.butterfly_emitters.len(),
                self.leaf_emitters.len(),
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
        if !self.water_terrain_initialized {
            return;
        }

        let remaining_capacity = PARTICLE_CAPACITY.saturating_sub(self.particle_snapshots.len());
        if remaining_capacity == 0 {
            return;
        }

        let bounds = self.water_sim.config.collider;
        let water_particle_size =
            water_debug_particle_size(self.gui_adjustables.water_particle_quad_size.value);
        for particle in self
            .water_sim
            .latest_particles()
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
        const MAX_SPAWN_XZ_RETRIES: usize = 16;
        const STEP_LEN: f32 = crate::particles::emitters::WORM_STEP_LEN;
        const RAY_EPSILON: f32 = 0.02;

        let map_size = super::CHUNK_DIM.as_vec3();

        let mut all_handles: Vec<ParticleHandle> = Vec::new();
        let mut all_positions: Vec<Vec3> = Vec::new();
        let mut all_directions: Vec<Vec3> = Vec::new();
        let mut all_emitter_indices: Vec<usize> = Vec::new();

        for emitter_idx in 0..self.butterfly_emitters.len() {
            let pending_handles =
                self.butterfly_emitters[emitter_idx].drain_pending_placement_handles();
            for handle in pending_handles {
                if !self.particle_system.is_alive_handle(handle) {
                    continue;
                }

                let mut resolved_position = self.particle_system.position(handle);
                let mut found_valid_xz = false;
                let mut last_attempt_position = resolved_position;

                if let Some(position) = resolved_position {
                    if self
                        .query_terrain_ray_cpu(Vec3::new(position.x, 10.0, position.z), Vec3::NEG_Y)
                        .is_some()
                    {
                        found_valid_xz = true;
                    }
                }

                if !found_valid_xz {
                    for _ in 0..MAX_SPAWN_XZ_RETRIES {
                        let candidate =
                            self.butterfly_emitters[emitter_idx].random_spawn_position_candidate();

                        if self
                            .query_terrain_ray_cpu(
                                Vec3::new(candidate.x, 10.0, candidate.z),
                                Vec3::NEG_Y,
                            )
                            .is_some()
                        {
                            found_valid_xz = true;
                            resolved_position = Some(candidate);
                            break;
                        }

                        last_attempt_position = Some(candidate);
                    }
                }

                if found_valid_xz {
                    if let Some(position) = resolved_position {
                        let _ = self.particle_system.set_position(handle, position);
                    }
                } else {
                    let fallback = if let Some(last) = last_attempt_position {
                        Vec3::new(
                            last.x,
                            self.butterfly_emitters[emitter_idx].center.y,
                            last.z,
                        )
                    } else {
                        self.butterfly_emitters[emitter_idx].random_spawn_position_candidate()
                    };
                    let _ = self.particle_system.set_position(handle, fallback);
                }
            }

            let (mut handles, mut positions, mut directions) = {
                let emitter = &mut self.butterfly_emitters[emitter_idx];
                let mut handles = Vec::new();
                let mut positions = Vec::new();
                let mut directions = Vec::new();
                emitter.collect_butterfly_states(
                    &self.particle_system,
                    &mut handles,
                    &mut positions,
                    &mut directions,
                );
                (handles, positions, directions)
            };

            if tick_step.bucket_count > 1 {
                let active_bucket = tick_step.active_bucket;
                let mut filtered_handles = Vec::with_capacity(handles.len());
                let mut filtered_positions = Vec::with_capacity(positions.len());
                let mut filtered_directions = Vec::with_capacity(directions.len());

                for ((handle, position), direction) in handles
                    .into_iter()
                    .zip(positions.into_iter())
                    .zip(directions.into_iter())
                {
                    if self.particle_system.handle_bucket(handle) == Some(active_bucket) {
                        filtered_handles.push(handle);
                        filtered_positions.push(position);
                        filtered_directions.push(direction);
                    }
                }

                handles = filtered_handles;
                positions = filtered_positions;
                directions = filtered_directions;
            }

            all_emitter_indices.resize(all_emitter_indices.len() + handles.len(), emitter_idx);
            all_handles.extend(handles);
            all_positions.extend(positions);
            all_directions.extend(directions);
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

                let blocked = if let Some(hit) = self.query_terrain_ray_cpu(
                    origin + Vec3::new(0.0, RAY_EPSILON, 0.0),
                    dir.normalize_or_zero(),
                ) {
                    let hit_dist = (hit - origin).length();
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

use super::{App, VisibleTerrainChange, CHUNK_DIM, VOXEL_DIM_PER_CHUNK};
use crate::app::world_edits::BuildEdit;
use crate::builder::{PlainBuilder, VOXEL_TYPE_MASK};
use crate::geom::UAabb3;
use glam::UVec3;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::time::Instant;

pub(super) mod bench;

// Most releases resolve from one contiguous readback. Components that cross this
// fast-path halo continue through lazily loaded tiles below.
const ANALYSIS_HALO_VOXELS: u32 = 24;
const MAX_ANALYSIS_VOXELS: u64 = 16 * 1024 * 1024;
const CONNECTIVITY_TILE_DIM: u32 = 32;

#[derive(Clone, Copy, Debug)]
enum PendingTerrainConnectivity {
    PlayerHold(UAabb3),
    LoadedWorld,
}

#[derive(Clone, Copy, Debug)]
enum TerrainConnectivityRequest {
    PlayerEdit { edited: UAabb3, block: UAabb3 },
    LoadedWorld { world_dim: UVec3 },
}

#[derive(Default)]
pub(super) struct TerrainConnectivityRuntime {
    pending: Option<PendingTerrainConnectivity>,
}

impl TerrainConnectivityRuntime {
    fn observe_player_publication(
        &mut self,
        edited_voxels_inclusive: UAabb3,
        continuous_hold_active: bool,
    ) {
        if !continuous_hold_active {
            return;
        }
        self.pending = Some(match self.pending {
            Some(PendingTerrainConnectivity::PlayerHold(pending)) => {
                PendingTerrainConnectivity::PlayerHold(pending.union_with(&edited_voxels_inclusive))
            }
            Some(PendingTerrainConnectivity::LoadedWorld) => {
                PendingTerrainConnectivity::LoadedWorld
            }
            None => PendingTerrainConnectivity::PlayerHold(edited_voxels_inclusive),
        });
    }

    fn request_loaded_world_reconciliation(&mut self) {
        // A complete authoritative replacement makes any earlier player-hold bound stale.
        self.pending = Some(PendingTerrainConnectivity::LoadedWorld);
    }

    fn take_player_release(&mut self, world_dim: UVec3) -> Option<TerrainConnectivityRequest> {
        let Some(PendingTerrainConnectivity::PlayerHold(inclusive)) = self.pending else {
            return None;
        };
        self.pending = None;
        let edited_max_exclusive = inclusive.max().saturating_add(UVec3::ONE).min(world_dim);
        let edited = UAabb3::new(inclusive.min().min(world_dim), edited_max_exclusive);
        if edited.min().cmpge(edited.max()).any() {
            return None;
        }

        let halo = UVec3::splat(ANALYSIS_HALO_VOXELS);
        let block = UAabb3::new(
            edited.min().saturating_sub(halo),
            edited.max().saturating_add(halo).min(world_dim),
        );
        Some(TerrainConnectivityRequest::PlayerEdit { edited, block })
    }

    fn transact_player_release<T>(
        &mut self,
        world_dim: UVec3,
        execute: impl FnOnce() -> anyhow::Result<T>,
    ) -> anyhow::Result<Option<T>> {
        let Some(request) = self.take_player_release(world_dim) else {
            return Ok(None);
        };
        match execute() {
            Ok(value) => Ok(Some(value)),
            Err(error) => {
                self.restore(request);
                Err(error)
            }
        }
    }

    fn take_loaded_world(&mut self, world_dim: UVec3) -> Option<TerrainConnectivityRequest> {
        if !matches!(self.pending, Some(PendingTerrainConnectivity::LoadedWorld)) {
            return None;
        }
        self.pending = None;
        Some(TerrainConnectivityRequest::LoadedWorld { world_dim })
    }

    fn restore(&mut self, request: TerrainConnectivityRequest) {
        debug_assert!(self.pending.is_none());
        self.pending = Some(match request {
            TerrainConnectivityRequest::PlayerEdit { edited, .. } => {
                let inclusive_max = edited.max().saturating_sub(UVec3::ONE);
                PendingTerrainConnectivity::PlayerHold(UAabb3::new(edited.min(), inclusive_max))
            }
            TerrainConnectivityRequest::LoadedWorld { .. } => {
                PendingTerrainConnectivity::LoadedWorld
            }
        });
    }
}

fn voxel_count(bound: UAabb3) -> u64 {
    let dim = bound.dimensions();
    u64::from(dim.x) * u64::from(dim.y) * u64::from(dim.z)
}

fn select_components_for_particle_capacity(
    components: Vec<DetachedVoxelComponent>,
    particle_capacity: usize,
) -> (Vec<(UVec3, u8)>, usize) {
    let mut selected_voxels = Vec::new();
    let mut skipped_components = 0;
    for component in components {
        if component.voxels.len() <= particle_capacity.saturating_sub(selected_voxels.len()) {
            selected_voxels.extend(component.voxels);
        } else {
            skipped_components += 1;
        }
    }
    (selected_voxels, skipped_components)
}

fn select_detached_world_voxels(
    atlas_voxels: &[u8],
    world_dim: UVec3,
    voxel_type_mask: u8,
    particle_capacity: usize,
) -> anyhow::Result<(Vec<(UVec3, u8)>, usize)> {
    let expected_len =
        usize::try_from(u64::from(world_dim.x) * u64::from(world_dim.y) * u64::from(world_dim.z))?;
    anyhow::ensure!(
        atlas_voxels.len() == expected_len,
        "loaded-world connectivity atlas length {} does not match dimensions {:?} ({expected_len})",
        atlas_voxels.len(),
        world_dim,
    );

    let plane_len = usize::try_from(u64::from(world_dim.x) * u64::from(world_dim.y))?;
    let row_len = world_dim.x as usize;
    let mut classified = vec![false; expected_len];
    let mut queue = VecDeque::new();
    let mut selected_voxels = Vec::new();
    let mut skipped_components = 0usize;

    for seed in 0..expected_len {
        if classified[seed] || atlas_voxels[seed] & voxel_type_mask == 0 {
            continue;
        }

        classified[seed] = true;
        queue.push_back(seed);
        let remaining_capacity = particle_capacity.saturating_sub(selected_voxels.len());
        let mut component = Vec::new();
        let mut anchored = false;
        let mut exceeds_capacity = false;

        while let Some(index) = queue.pop_front() {
            let z = index / plane_len;
            let in_plane = index % plane_len;
            let y = in_plane / row_len;
            let x = in_plane % row_len;

            if y == 0 {
                anchored = true;
                component.clear();
            } else if !anchored && !exceeds_capacity {
                if component.len() < remaining_capacity {
                    component.push((
                        UVec3::new(x as u32, y as u32, z as u32),
                        atlas_voxels[index] & voxel_type_mask,
                    ));
                } else {
                    exceeds_capacity = true;
                    component.clear();
                }
            }

            let mut enqueue = |neighbor: usize| {
                if !classified[neighbor] && atlas_voxels[neighbor] & voxel_type_mask != 0 {
                    classified[neighbor] = true;
                    queue.push_back(neighbor);
                }
            };
            if x > 0 {
                enqueue(index - 1);
            }
            if x + 1 < row_len {
                enqueue(index + 1);
            }
            if y > 0 {
                enqueue(index - row_len);
            }
            if y + 1 < world_dim.y as usize {
                enqueue(index + row_len);
            }
            if z > 0 {
                enqueue(index - plane_len);
            }
            if z + 1 < world_dim.z as usize {
                enqueue(index + plane_len);
            }
        }

        if anchored {
            continue;
        }
        if exceeds_capacity {
            skipped_components += 1;
        } else {
            selected_voxels.extend(component);
        }
    }

    Ok((selected_voxels, skipped_components))
}

struct AtlasVoxelReader<'a> {
    plain_builder: &'a mut PlainBuilder,
    world_dim: UVec3,
    primary_bound: UAabb3,
    primary_voxels: &'a [u8],
    tiles: HashMap<UVec3, (UVec3, Vec<u8>)>,
    tile_readback_us: f64,
}

impl<'a> AtlasVoxelReader<'a> {
    fn new(
        plain_builder: &'a mut PlainBuilder,
        world_dim: UVec3,
        primary_bound: UAabb3,
        primary_voxels: &'a [u8],
    ) -> Self {
        Self {
            plain_builder,
            world_dim,
            primary_bound,
            primary_voxels,
            tiles: HashMap::new(),
            tile_readback_us: 0.0,
        }
    }

    fn voxel_at(&mut self, world_voxel: UVec3) -> anyhow::Result<u8> {
        if world_voxel.cmpge(self.primary_bound.min()).all()
            && world_voxel.cmplt(self.primary_bound.max()).all()
        {
            return Ok(voxel_at_in_region(
                self.primary_voxels,
                self.primary_bound,
                world_voxel,
            ));
        }

        let tile_origin = (world_voxel / CONNECTIVITY_TILE_DIM) * CONNECTIVITY_TILE_DIM;
        if !self.tiles.contains_key(&tile_origin) {
            let tile_dim = UVec3::splat(CONNECTIVITY_TILE_DIM).min(self.world_dim - tile_origin);
            let readback_started = Instant::now();
            let voxels = self
                .plain_builder
                .read_chunk_atlas_region(tile_origin, tile_dim)?;
            self.tile_readback_us += readback_started.elapsed().as_secs_f64() * 1_000_000.0;
            self.tiles.insert(tile_origin, (tile_dim, voxels));
        }
        let (tile_dim, voxels) = self
            .tiles
            .get(&tile_origin)
            .expect("a requested connectivity tile must be cached");
        let local = world_voxel - tile_origin;
        let index = local.x + tile_dim.x * (local.y + tile_dim.y * local.z);
        Ok(voxels[index as usize])
    }
}

fn connectivity_tile_origin(world_voxel: UVec3) -> UVec3 {
    (world_voxel / CONNECTIVITY_TILE_DIM) * CONNECTIVITY_TILE_DIM
}

fn prepare_detached_voxel_clear(
    plain_builder: &mut PlainBuilder,
    world_dim: UVec3,
    voxels: &[(UVec3, u8)],
) -> anyhow::Result<Vec<(UVec3, UVec3, Vec<u8>)>> {
    let mut by_tile = BTreeMap::<(u32, u32, u32), Vec<UVec3>>::new();
    for &(world_voxel, _) in voxels {
        let origin = connectivity_tile_origin(world_voxel);
        by_tile
            .entry((origin.x, origin.y, origin.z))
            .or_default()
            .push(world_voxel);
    }

    let mut dirty_tiles = Vec::with_capacity(by_tile.len());
    for ((x, y, z), tile_voxels) in by_tile {
        let origin = UVec3::new(x, y, z);
        let dim = UVec3::splat(CONNECTIVITY_TILE_DIM).min(world_dim - origin);
        let mut data = plain_builder.read_chunk_atlas_region(origin, dim)?;
        for world_voxel in tile_voxels {
            let local = world_voxel - origin;
            let index = local.x + dim.x * (local.y + dim.y * local.z);
            data[index as usize] = 0;
        }
        dirty_tiles.push((origin, dim, data));
    }

    Ok(dirty_tiles)
}

impl App {
    pub(super) fn observe_player_terrain_publication_for_connectivity(&mut self, bound: UAabb3) {
        self.terrain_connectivity
            .observe_player_publication(bound, self.player_tools.continuous_hold_active());
    }

    pub(super) fn finish_player_terrain_connectivity_hold(&mut self) -> anyhow::Result<()> {
        if self.try_begin_manual_connectivity_benchmark_release()? {
            return Ok(());
        }
        let world_dim = CHUNK_DIM * VOXEL_DIM_PER_CHUNK;
        let Some(request) = self.terrain_connectivity.take_player_release(world_dim) else {
            return Ok(());
        };
        let result = self.reconcile_terrain_connectivity(request);
        if result.is_err() {
            self.terrain_connectivity.restore(request);
        }
        result
    }

    pub(super) fn reconcile_loaded_terrain_publication(&mut self) -> anyhow::Result<()> {
        let world_dim = CHUNK_DIM * VOXEL_DIM_PER_CHUNK;
        self.terrain_connectivity
            .request_loaded_world_reconciliation();
        let request = self
            .terrain_connectivity
            .take_loaded_world(world_dim)
            .expect("a loaded-world reconciliation request must be immediately available");
        let result = self.reconcile_terrain_connectivity(request);
        if result.is_err() {
            self.terrain_connectivity.restore(request);
        }
        result
    }

    fn reconcile_terrain_connectivity(
        &mut self,
        request: TerrainConnectivityRequest,
    ) -> anyhow::Result<()> {
        let started = Instant::now();
        let world_dim = CHUNK_DIM * VOXEL_DIM_PER_CHUNK;
        let (mode, analysis_voxels, selected_voxels, skipped_components) = match request {
            TerrainConnectivityRequest::PlayerEdit { edited, block } => {
                let analysis_voxels = voxel_count(block);
                if analysis_voxels > MAX_ANALYSIS_VOXELS {
                    log::warn!(
                        "[TERRAIN_CONNECTIVITY] skipped oversized release analysis edited={:?}..{:?} block={:?}..{:?} voxels={} budget={}",
                        edited.min(),
                        edited.max(),
                        block.min(),
                        block.max(),
                        analysis_voxels,
                        MAX_ANALYSIS_VOXELS,
                    );
                    return Ok(());
                }

                let atlas_voxels = self
                    .plain_builder
                    .read_chunk_atlas_region(block.min(), block.dimensions())?;
                let candidate_region = UAabb3::new(
                    edited.min().saturating_sub(UVec3::ONE),
                    edited.max().saturating_add(UVec3::ONE).min(world_dim),
                );
                let available_particles = self.particle_system.available_capacity();
                let components = {
                    let mut reader = AtlasVoxelReader::new(
                        &mut self.plain_builder,
                        world_dim,
                        block,
                        &atlas_voxels,
                    );
                    detached_components_in_edit_region(
                        &atlas_voxels,
                        block,
                        candidate_region,
                        world_dim,
                        VOXEL_TYPE_MASK,
                        available_particles,
                        |world_voxel| reader.voxel_at(world_voxel),
                    )?
                };
                let (selected_voxels, skipped_components) =
                    select_components_for_particle_capacity(components, available_particles);
                (
                    "player-release",
                    analysis_voxels,
                    selected_voxels,
                    skipped_components,
                )
            }
            TerrainConnectivityRequest::LoadedWorld { world_dim } => {
                let analysis_voxels =
                    u64::from(world_dim.x) * u64::from(world_dim.y) * u64::from(world_dim.z);
                let atlas_voxels = self
                    .plain_builder
                    .read_chunk_atlas_region(UVec3::ZERO, world_dim)?;
                let (selected_voxels, skipped_components) = select_detached_world_voxels(
                    &atlas_voxels,
                    world_dim,
                    VOXEL_TYPE_MASK,
                    self.particle_system.available_capacity(),
                )?;
                (
                    "loaded-world",
                    analysis_voxels,
                    selected_voxels,
                    skipped_components,
                )
            }
        };

        if selected_voxels.is_empty() {
            let level = if skipped_components == 0 {
                log::Level::Info
            } else {
                log::Level::Warn
            };
            log::log!(
                level,
                "[TERRAIN_CONNECTIVITY] mode={} checked_voxels={} detached_voxels=0 skipped_components={} elapsed_ms={:.2}",
                mode,
                analysis_voxels,
                skipped_components,
                started.elapsed().as_secs_f64() * 1000.0,
            );
            return Ok(());
        }

        let mut detached_min = world_dim;
        let mut detached_max = UVec3::ZERO;
        for &(world_voxel, _) in &selected_voxels {
            detached_min = detached_min.min(world_voxel);
            detached_max = detached_max.max(world_voxel);
        }
        let detached_bound = UAabb3::new(
            detached_min,
            detached_max.saturating_add(UVec3::ONE).min(world_dim),
        );
        let dirty_tiles =
            prepare_detached_voxel_clear(&mut self.plain_builder, world_dim, &selected_voxels)?;
        let change =
            VisibleTerrainChange::from_build_edits(vec![BuildEdit::RebuildMeshWithoutFlora(
                detached_bound,
            )])?
            .expect("detached terrain voxels always define a visible rebuild");

        // Everything above is preparation and may fail without changing authoritative terrain.
        // The first atlas write enters a non-rollbackable commit: publication and particle count
        // failures are terminal invariants rather than retryable errors.
        for (origin, dim, data) in dirty_tiles {
            self.plain_builder
                .write_chunk_atlas_region(origin, dim, &data)
                .unwrap_or_else(|err| {
                    panic!(
                        "terrain connectivity atlas commit failed after entering non-rollbackable state: {err:#}"
                    )
                });
        }
        self.publish_visible_terrain(change).unwrap_or_else(|err| {
            panic!(
                "terrain connectivity Visible Terrain Publication failed after atlas commit: {err:#}"
            )
        });

        let spawned = self.spawn_detached_terrain_voxel_particles(&selected_voxels);
        assert_eq!(
            spawned,
            selected_voxels.len(),
            "terrain connectivity cleared {} voxels but spawned only {} particles",
            selected_voxels.len(),
            spawned,
        );
        log::info!(
            "[TERRAIN_CONNECTIVITY] mode={} checked_voxels={} detached_voxels={} spawned_particles={} skipped_components={} elapsed_ms={:.2}",
            mode,
            analysis_voxels,
            selected_voxels.len(),
            spawned,
            skipped_components,
            started.elapsed().as_secs_f64() * 1000.0,
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DetachedVoxelComponent {
    pub(super) voxels: Vec<(UVec3, u8)>,
}

#[derive(Debug)]
struct LocalVoxelComponent {
    voxels: Vec<(UVec3, u8)>,
    touches_block_boundary: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ComponentDisposition {
    Detached,
    Anchored,
    ExceedsCapacity,
}

#[derive(Debug)]
struct ComponentTrace {
    voxels: Vec<(UVec3, u8)>,
    disposition: ComponentDisposition,
}

fn voxel_at_in_region(atlas_voxels: &[u8], block: UAabb3, world_voxel: UVec3) -> u8 {
    let dim = block.dimensions();
    let local = world_voxel - block.min();
    let index = local.x + dim.x * (local.y + dim.y * local.z);
    atlas_voxels[index as usize]
}

fn local_components_in_edit_region(
    atlas_voxels: &[u8],
    block: UAabb3,
    edited: UAabb3,
    voxel_type_mask: u8,
) -> anyhow::Result<Vec<LocalVoxelComponent>> {
    let dim = block.dimensions();
    anyhow::ensure!(dim.cmpgt(UVec3::ZERO).all(), "connectivity block is empty");
    let expected_len = usize::try_from(u64::from(dim.x) * u64::from(dim.y) * u64::from(dim.z))?;
    anyhow::ensure!(
        atlas_voxels.len() == expected_len,
        "connectivity block has {} voxels, expected {} for {:?}",
        atlas_voxels.len(),
        expected_len,
        dim,
    );

    let voxel_type_at = |index: usize| atlas_voxels[index] & voxel_type_mask;
    let index_of = |position: UVec3| -> usize {
        (position.x + dim.x * (position.y + dim.y * position.z)) as usize
    };
    let position_of = |index: usize| -> UVec3 {
        let index = index as u32;
        let plane = dim.x * dim.y;
        let z = index / plane;
        let remainder = index % plane;
        UVec3::new(remainder % dim.x, remainder / dim.x, z)
    };
    let enqueue_solid_neighbors =
        |position: UVec3, visited: &mut [bool], queue: &mut VecDeque<usize>| {
            let mut enqueue = |neighbor: UVec3| {
                let index = index_of(neighbor);
                if !visited[index] && voxel_type_at(index) != 0 {
                    visited[index] = true;
                    queue.push_back(index);
                }
            };
            if position.x > 0 {
                enqueue(position - UVec3::X);
            }
            if position.x + 1 < dim.x {
                enqueue(position + UVec3::X);
            }
            if position.y > 0 {
                enqueue(position - UVec3::Y);
            }
            if position.y + 1 < dim.y {
                enqueue(position + UVec3::Y);
            }
            if position.z > 0 {
                enqueue(position - UVec3::Z);
            }
            if position.z + 1 < dim.z {
                enqueue(position + UVec3::Z);
            }
        };

    let candidate_min = edited.min().max(block.min());
    let candidate_max = edited.max().min(block.max());
    if candidate_min.cmpge(candidate_max).any() {
        return Ok(Vec::new());
    }

    let mut classified = vec![false; expected_len];
    let mut queue = VecDeque::new();
    let mut components = Vec::new();
    for world_z in candidate_min.z..candidate_max.z {
        for world_y in candidate_min.y..candidate_max.y {
            for world_x in candidate_min.x..candidate_max.x {
                let local = UVec3::new(world_x, world_y, world_z) - block.min();
                let seed_index = index_of(local);
                if voxel_type_at(seed_index) == 0 || classified[seed_index] {
                    continue;
                }

                classified[seed_index] = true;
                queue.push_back(seed_index);
                let mut voxels = Vec::new();
                let mut touches_block_boundary = false;
                while let Some(index) = queue.pop_front() {
                    let local = position_of(index);
                    touches_block_boundary |= local.x == 0
                        || local.y == 0
                        || local.z == 0
                        || local.x + 1 == dim.x
                        || local.y + 1 == dim.y
                        || local.z + 1 == dim.z;
                    voxels.push((block.min() + local, voxel_type_at(index)));
                    enqueue_solid_neighbors(local, &mut classified, &mut queue);
                }
                components.push(LocalVoxelComponent {
                    voxels,
                    touches_block_boundary,
                });
            }
        }
    }

    Ok(components)
}

fn trace_world_component<F>(
    seed: UVec3,
    world_dim: UVec3,
    voxel_type_mask: u8,
    max_component_voxels: usize,
    voxel_at: &mut F,
) -> anyhow::Result<ComponentTrace>
where
    F: FnMut(UVec3) -> anyhow::Result<u8>,
{
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    let mut voxels = Vec::new();
    visited.insert(seed);
    queue.push_back(seed);

    while let Some(position) = queue.pop_front() {
        let voxel_type = voxel_at(position)? & voxel_type_mask;
        if voxel_type == 0 {
            continue;
        }
        voxels.push((position, voxel_type));
        if position.y == 0 {
            return Ok(ComponentTrace {
                voxels,
                disposition: ComponentDisposition::Anchored,
            });
        }
        if voxels.len() > max_component_voxels {
            return Ok(ComponentTrace {
                voxels,
                disposition: ComponentDisposition::ExceedsCapacity,
            });
        }

        let mut visit_neighbor = |neighbor: UVec3| -> anyhow::Result<()> {
            if !visited.contains(&neighbor) && voxel_at(neighbor)? & voxel_type_mask != 0 {
                visited.insert(neighbor);
                queue.push_back(neighbor);
            }
            Ok(())
        };
        if position.x > 0 {
            visit_neighbor(position - UVec3::X)?;
        }
        if position.x + 1 < world_dim.x {
            visit_neighbor(position + UVec3::X)?;
        }
        if position.y > 0 {
            visit_neighbor(position - UVec3::Y)?;
        }
        if position.y + 1 < world_dim.y {
            visit_neighbor(position + UVec3::Y)?;
        }
        if position.z > 0 {
            visit_neighbor(position - UVec3::Z)?;
        }
        if position.z + 1 < world_dim.z {
            visit_neighbor(position + UVec3::Z)?;
        }
    }

    Ok(ComponentTrace {
        voxels,
        disposition: ComponentDisposition::Detached,
    })
}

pub(super) fn detached_components_in_edit_region<F>(
    atlas_voxels: &[u8],
    block: UAabb3,
    edited: UAabb3,
    world_dim: UVec3,
    voxel_type_mask: u8,
    max_component_voxels: usize,
    mut voxel_at: F,
) -> anyhow::Result<Vec<DetachedVoxelComponent>>
where
    F: FnMut(UVec3) -> anyhow::Result<u8>,
{
    let local_components =
        local_components_in_edit_region(atlas_voxels, block, edited, voxel_type_mask)?;
    let mut globally_classified = HashSet::new();
    let mut detached = Vec::new();

    for local_component in local_components {
        if local_component
            .voxels
            .iter()
            .any(|(position, _)| globally_classified.contains(position))
        {
            continue;
        }
        if !local_component.touches_block_boundary {
            detached.push(DetachedVoxelComponent {
                voxels: local_component.voxels,
            });
            continue;
        }
        if local_component.voxels.len() > max_component_voxels {
            continue;
        }

        let seed = local_component.voxels[0].0;
        let trace = trace_world_component(
            seed,
            world_dim,
            voxel_type_mask,
            max_component_voxels,
            &mut voxel_at,
        )?;
        globally_classified.extend(trace.voxels.iter().map(|(position, _)| *position));
        globally_classified.extend(local_component.voxels.iter().map(|(position, _)| *position));
        if trace.disposition == ComponentDisposition::Detached {
            detached.push(DetachedVoxelComponent {
                voxels: trace.voxels,
            });
        }
    }

    Ok(detached)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(dim: UVec3, position: UVec3) -> usize {
        (position.x + dim.x * (position.y + dim.y * position.z)) as usize
    }

    fn block_with_solids(dim: UVec3, solids: &[(UVec3, u8)]) -> Vec<u8> {
        let mut voxels = vec![0; (dim.x * dim.y * dim.z) as usize];
        for &(position, voxel_type) in solids {
            voxels[index(dim, position)] = voxel_type;
        }
        voxels
    }

    #[test]
    fn floating_component_intersecting_the_edit_region_detaches() {
        let dim = UVec3::splat(7);
        let solids = [(UVec3::new(3, 3, 3), 1), (UVec3::new(4, 3, 3), 2)];
        let voxels = block_with_solids(dim, &solids);

        let components = detached_components_in_edit_region(
            &voxels,
            UAabb3::new(UVec3::ZERO, dim),
            UAabb3::new(UVec3::new(3, 3, 3), UVec3::new(4, 4, 4)),
            dim,
            0x07,
            usize::MAX,
            |world_voxel| Ok(voxels[index(dim, world_voxel)]),
        )
        .unwrap();

        assert_eq!(components.len(), 1);
        assert_eq!(components[0].voxels, solids);
    }

    #[test]
    fn component_connected_to_world_floor_stays_terrain() {
        let dim = UVec3::splat(7);
        let solids = (0..=3)
            .map(|y| (UVec3::new(3, y, 3), 1))
            .collect::<Vec<_>>();
        let voxels = block_with_solids(dim, &solids);

        let components = detached_components_in_edit_region(
            &voxels,
            UAabb3::new(UVec3::ZERO, dim),
            UAabb3::new(UVec3::new(3, 3, 3), UVec3::new(4, 4, 4)),
            dim,
            0x07,
            usize::MAX,
            |world_voxel| Ok(voxels[index(dim, world_voxel)]),
        )
        .unwrap();

        assert!(components.is_empty());
    }

    #[test]
    fn detached_component_must_not_become_anchored_by_an_internal_analysis_edge() {
        let block_min = UVec3::new(10, 10, 10);
        let dim = UVec3::splat(7);
        let solids = (3..=6)
            .map(|x| (UVec3::new(x, 3, 3), 1))
            .collect::<Vec<_>>();
        let voxels = block_with_solids(dim, &solids);

        let components = detached_components_in_edit_region(
            &voxels,
            UAabb3::new(block_min, block_min + dim),
            UAabb3::new(
                block_min + UVec3::new(3, 3, 3),
                block_min + UVec3::new(4, 4, 4),
            ),
            UVec3::splat(32),
            0x07,
            usize::MAX,
            |world_voxel| {
                if world_voxel.cmpge(block_min).all() && world_voxel.cmplt(block_min + dim).all() {
                    Ok(voxels[index(dim, world_voxel - block_min)])
                } else {
                    Ok(0)
                }
            },
        )
        .unwrap();

        assert_eq!(components.len(), 1);
        assert_eq!(components[0].voxels.len(), solids.len());
    }

    #[test]
    fn component_leaving_the_analysis_block_stays_when_it_reaches_world_floor_elsewhere() {
        let world_dim = UVec3::splat(16);
        let block_min = UVec3::new(4, 4, 4);
        let block_dim = UVec3::splat(7);
        let block = UAabb3::new(block_min, block_min + block_dim);
        let mut world_voxels = vec![0; (world_dim.x * world_dim.y * world_dim.z) as usize];
        let world_index = |position: UVec3| index(world_dim, position);
        for x in 7..=11 {
            world_voxels[world_index(UVec3::new(x, 7, 7))] = 1;
        }
        for y in 0..=7 {
            world_voxels[world_index(UVec3::new(11, y, 7))] = 1;
        }
        let block_voxels = (0..block_dim.z)
            .flat_map(|z| {
                (0..block_dim.y)
                    .flat_map(move |y| (0..block_dim.x).map(move |x| UVec3::new(x, y, z)))
            })
            .map(|local| world_voxels[world_index(block_min + local)])
            .collect::<Vec<_>>();

        let components = detached_components_in_edit_region(
            &block_voxels,
            block,
            UAabb3::new(UVec3::new(7, 7, 7), UVec3::new(8, 8, 8)),
            world_dim,
            0x07,
            usize::MAX,
            |world_voxel| Ok(world_voxels[world_index(world_voxel)]),
        )
        .unwrap();

        assert!(components.is_empty());
    }

    #[test]
    fn floating_component_outside_the_edit_region_is_not_reclassified() {
        let dim = UVec3::splat(7);
        let voxels = block_with_solids(dim, &[(UVec3::new(5, 4, 5), 1)]);

        let components = detached_components_in_edit_region(
            &voxels,
            UAabb3::new(UVec3::ZERO, dim),
            UAabb3::new(UVec3::new(2, 2, 2), UVec3::new(4, 4, 4)),
            dim,
            0x07,
            usize::MAX,
            |world_voxel| Ok(voxels[index(dim, world_voxel)]),
        )
        .unwrap();

        assert!(components.is_empty());
    }

    #[test]
    fn player_publications_defer_until_release_and_expand_by_the_analysis_halo() {
        let mut runtime = TerrainConnectivityRuntime::default();
        runtime.observe_player_publication(
            UAabb3::new(UVec3::new(20, 30, 40), UVec3::new(24, 34, 44)),
            true,
        );

        let TerrainConnectivityRequest::PlayerEdit { edited, block } =
            runtime.take_player_release(UVec3::splat(128)).unwrap()
        else {
            panic!("player publication must create a player-edit request");
        };

        assert_eq!(edited.min(), UVec3::new(20, 30, 40));
        assert_eq!(edited.max(), UVec3::new(25, 35, 45));
        assert_eq!(block.min(), UVec3::new(0, 6, 16));
        assert_eq!(block.max(), UVec3::new(49, 59, 69));
        assert!(runtime.take_player_release(UVec3::splat(128)).is_none());
    }

    #[test]
    fn failed_player_release_transaction_restores_the_exact_request() {
        let mut runtime = TerrainConnectivityRuntime::default();
        let world_dim = UVec3::splat(128);
        runtime.observe_player_publication(UAabb3::new(UVec3::splat(32), UVec3::splat(34)), true);

        let error = runtime
            .transact_player_release(world_dim, || {
                Err::<(), _>(anyhow::anyhow!("injected snapshot failure"))
            })
            .unwrap_err();

        assert!(error.to_string().contains("injected snapshot failure"));
        assert!(runtime.take_player_release(world_dim).is_some());
    }

    #[test]
    fn terrain_detachment_commit_requires_a_prepared_single_use_capability() {
        let _: fn(PreparedTerrainDetachment, &mut App) -> CommittedTerrainDetachment =
            PreparedTerrainDetachment::commit;
        static_assertions::assert_not_impl_any!(PreparedTerrainDetachment: Clone, Copy);

        assert!(PreparedAtlasWrite::new(
            UVec3::splat(8),
            UVec3::new(7, 7, 7),
            UVec3::splat(2),
            vec![0; 8],
        )
        .is_err());
        assert!(
            PreparedAtlasWrite::new(UVec3::splat(8), UVec3::ZERO, UVec3::splat(2), vec![0; 7],)
                .is_err()
        );
    }

    #[test]
    fn inactive_player_hold_does_not_schedule_reconciliation() {
        let mut runtime = TerrainConnectivityRuntime::default();
        runtime.observe_player_publication(UAabb3::new(UVec3::splat(4), UVec3::splat(8)), false);

        assert!(runtime.take_player_release(UVec3::splat(32)).is_none());
    }

    #[test]
    fn loaded_world_reconciliation_supersedes_player_work_and_bypasses_player_budget() {
        let mut runtime = TerrainConnectivityRuntime::default();
        runtime.observe_player_publication(UAabb3::new(UVec3::splat(4), UVec3::splat(8)), true);
        runtime.request_loaded_world_reconciliation();

        let world_dim = UVec3::splat(512);
        assert!(
            u64::from(world_dim.x) * u64::from(world_dim.y) * u64::from(world_dim.z)
                > MAX_ANALYSIS_VOXELS
        );
        assert!(matches!(
            runtime.take_loaded_world(world_dim),
            Some(TerrainConnectivityRequest::LoadedWorld { world_dim: actual })
                if actual == world_dim
        ));
        assert!(runtime.take_player_release(world_dim).is_none());
    }

    #[test]
    fn loaded_world_scan_selects_only_complete_floating_components() {
        let dim = UVec3::splat(6);
        let mut voxels = vec![0; (dim.x * dim.y * dim.z) as usize];
        let set = |voxels: &mut [u8], position: UVec3| {
            voxels[index(dim, position)] = 1;
        };
        set(&mut voxels, UVec3::new(1, 0, 1));
        set(&mut voxels, UVec3::new(1, 1, 1));
        set(&mut voxels, UVec3::new(4, 3, 4));
        set(&mut voxels, UVec3::new(4, 4, 4));

        let (selected, skipped) = select_detached_world_voxels(&voxels, dim, 0x07, 2).unwrap();

        assert_eq!(skipped, 0);
        assert_eq!(
            selected,
            vec![(UVec3::new(4, 3, 4), 1), (UVec3::new(4, 4, 4), 1)]
        );
    }

    #[test]
    fn loaded_world_scan_preserves_a_component_that_does_not_fit_atomically() {
        let dim = UVec3::splat(6);
        let mut voxels = vec![0; (dim.x * dim.y * dim.z) as usize];
        for y in 2..=4 {
            voxels[index(dim, UVec3::new(3, y, 3))] = 1;
        }

        let (selected, skipped) = select_detached_world_voxels(&voxels, dim, 0x07, 2).unwrap();

        assert!(selected.is_empty());
        assert_eq!(skipped, 1);
    }

    #[test]
    fn particle_capacity_never_splits_a_detached_component() {
        let component = |start: u32, count: u32| DetachedVoxelComponent {
            voxels: (start..start + count)
                .map(|x| (UVec3::new(x, 3, 3), 1))
                .collect(),
        };
        let components = vec![component(0, 4), component(10, 3), component(20, 2)];

        let (selected, skipped) = select_components_for_particle_capacity(components, 6);

        assert_eq!(selected.len(), 6);
        assert!(selected
            .iter()
            .all(|(position, _)| position.x < 4 || position.x >= 20));
        assert_eq!(skipped, 1);
    }
}

//! Opt-in deterministic Release brush acceptance and cost replay.
//! Every timed call uses the production App edit/publication interface. Fixture/reset/readback
//! validation are outside the event timer. Run only in an isolated worktree, with --flat-terrain.
mod transactions;
use super::*;
use crate::app::world_edits::TerrainBrushEdit;
use crate::builder::{FloraInstanceResources, Instance};
use std::collections::BTreeMap;

pub(super) struct Replay {
    case: String,
    frame: u32,
    sample: u32,
    samples: u32,
    seed_step: usize,
    edit: TerrainBrushEdit,
    events: Vec<serde_json::Value>,
    baseline: RootMap,
    output: std::path::PathBuf,
    pending: Option<PendingSample>,
    asynchronous: bool,
}

struct PendingSample {
    preflight_ms: f64,
    enqueue_ms: f64,
    frame: u64,
}

impl Replay {
    pub(super) fn from_env() -> Result<Option<Self>> {
        let Ok(case) = std::env::var("RE_FLORA_BRUSH_EDIT_BENCH") else {
            return Ok(None);
        };
        anyhow::ensure!(
            std::env::args().any(|arg| arg == "--flat-terrain")
                && std::env::args().any(|arg| arg == "--hidden"),
            "brush benchmark requires --flat-terrain --hidden"
        );
        let parts: Vec<_> = case.split('-').collect();
        anyhow::ensure!(
            parts.len() == 3,
            "expected <add|remove|dig|place|noop>-<empty|sparse|dense>-<tiny|point|long|cross>"
        );
        anyhow::ensure!(
            ["add", "remove", "dig", "place", "noop"].contains(&parts[0]),
            "invalid operation"
        );
        let seed_step = match parts[1] {
            "empty" => 0,
            "sparse" => 16,
            "dense" => 2,
            _ => anyhow::bail!("invalid density"),
        };
        let (a, b, r) = match parts[2] {
            "tiny" => (
                Vec3::new(128., 127.5, 112.),
                Vec3::new(128., 127.5, 112.),
                2.,
            ),
            "point" => (
                Vec3::new(128., 127.5, 112.),
                Vec3::new(128., 127.5, 112.),
                14.,
            ),
            "long" => (
                Vec3::new(64., 127.5, 64.),
                Vec3::new(224., 127.5, 176.),
                14.,
            ),
            "cross" => (
                Vec3::new(192., 127.5, 96.),
                Vec3::new(320., 127.5, 160.),
                14.,
            ),
            _ => anyhow::bail!("invalid shape"),
        };
        let output = std::env::var("RE_FLORA_BRUSH_BENCH_REPORT")?.into();
        let samples = std::env::var("RE_FLORA_BRUSH_BENCH_SAMPLES")
            .ok()
            .map(|s| s.parse())
            .transpose()?
            .unwrap_or(30);
        anyhow::ensure!((3..=200).contains(&samples), "sample count 3..200");
        Ok(Some(Self {
            case,
            frame: 0,
            sample: 0,
            samples,
            seed_step,
            edit: TerrainBrushEdit {
                start: a / 256.,
                end: b / 256.,
                radius: r / 256.,
            },
            events: vec![],
            baseline: BTreeMap::new(),
            output,
            pending: None,
            asynchronous: std::env::var_os("RE_FLORA_BRUSH_BENCH_ASYNC").is_some(),
        }))
    }

    fn reset(&self, app: &mut App) -> Result<()> {
        app.vulkan_ctx.device().wait_idle();
        // A fixed top slab, away from the tree at z=256. No persistence/save calls.
        let origin = UVec3::new(32, 96, 32);
        let dim = UVec3::new(448, 64, 160);
        let mut bytes = vec![0u8; dim.element_product() as usize];
        for z in 0..dim.z {
            for y in 0..32 {
                for x in 0..dim.x {
                    bytes[(x + dim.x * (y + dim.y * z)) as usize] = 0x32;
                }
            }
        }
        app.plain_builder
            .write_chunk_atlas_region(origin, dim, &bytes)?;
        // Remove previous ordinary roots before publishing restored terrain.
        for (_, r) in &mut app
            .surface_builder
            .resources
            .instances
            .chunk_flora_instances
        {
            for s in 0..2 {
                r.write_species_instances(s, &[])?;
            }
        }
        let change =
            VisibleTerrainChange::from_build_edits(vec![BuildEdit::RebuildMeshWithoutFlora(
                UAabb3::new(origin, origin + dim - UVec3::ONE),
            )])?
            .unwrap();
        app.publish_visible_terrain(change)?;
        if self.seed_step > 0 {
            for (_, r) in &mut app
                .surface_builder
                .resources
                .instances
                .chunk_flora_instances
            {
                let mut roots = [Vec::new(), Vec::new()];
                for z in (33u32..480).step_by(self.seed_step) {
                    for x in (33u32..480).step_by(self.seed_step) {
                        let p = UVec3::new(x, 127, z);
                        if p / 256 != r.chunk_id {
                            continue;
                        }
                        let local = p % 256;
                        let s =
                            ((x / self.seed_step as u32 + z / self.seed_step as u32) % 2) as usize;
                        roots[s].push(Instance {
                            packed_local_pos: local.x
                                | (local.y << 8)
                                | (local.z << 16)
                                | (255 << 24),
                            spawn_start_ms: u32::MAX,
                        });
                    }
                }
                for (s, roots) in roots.iter().enumerate() {
                    r.write_species_instances(s, roots)?;
                }
            }
        }
        app.player_tools.planting.flora_index = 0;
        if self.case.starts_with("noop-") {
            app.apply_surface_flora_regeneration_at(self.edit, 7, true, 10_000)?;
        }
        app.surface_builder
            .refresh_grass_soil_cover(&mut app.plain_builder)?;
        Ok(())
    }
}

// Stable species/root keys and whole instance payload; mutable draw order is not identity.
type RootKey = (u32, u32, u32, usize, u32);
type RootMap = BTreeMap<RootKey, (u32, u32, u32)>;
fn roots(app: &App) -> Result<RootMap> {
    let mut result = BTreeMap::new();
    for (_, r) in &app
        .surface_builder
        .resources
        .instances
        .chunk_flora_instances
    {
        for s in 0..2 {
            let count = r.species_len(s);
            if count == 0 {
                continue;
            }
            let data = r.resource.instances_buf.read_back_range(
                u64::from(FloraInstanceResources::species_offset(s)) * 8,
                u64::from(count) * 8,
            )?;
            let instances: &[Instance] =
                bytemuck::try_cast_slice(&data).map_err(|e| anyhow::anyhow!("{e}"))?;
            let hydration: BTreeMap<_, _> = r.hydration[s].iter().copied().collect();
            for i in instances {
                let root = i.packed_local_pos & 0xffffff;
                let old = result.insert(
                    (r.chunk_id.x, r.chunk_id.y, r.chunk_id.z, s, root),
                    (
                        i.packed_local_pos >> 24,
                        i.spawn_start_ms,
                        hydration[&root].to_bits(),
                    ),
                );
                anyhow::ensure!(old.is_none(), "duplicate grass root");
            }
        }
    }
    Ok(result)
}
fn hash(data: impl IntoIterator<Item = u8>) -> String {
    let h = data.into_iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    });
    format!("{h:016x}")
}

impl App {
    pub(super) fn advance_brush_edit_bench(&mut self) -> Result<bool> {
        let Some(mut replay) = self.brush_edit_bench.take() else {
            return Ok(false);
        };
        let result = (|| -> Result<bool> {
            if replay.frame == 0 {
                // Diagnostic override is in-memory only; never saves GUI preferences.
                self.debug_settings.adjustables.async_grass_edits.value = replay.asynchronous;
            }
            if replay.frame == 0 && std::env::var_os("RE_FLORA_GRASS_TRANSACTION_REVIEW").is_some()
            {
                transactions::verify(self, &replay)?;
            }
            // Reset one frame, then render 8 settled frames before each measured edit.
            if replay.frame.is_multiple_of(10) {
                replay.reset(self)?;
                replay.baseline = roots(self)?;
            }
            if replay.frame % 10 == 8 {
                // Avoid a GPU readback / O(N log N) validation walk just before the timer:
                // it would consume the queue backlog we intend to measure. The expensive
                // baseline capture belongs to reset, eight rendered frames earlier.
                if replay.pending.is_some() && !self.grass_edits.is_idle() {
                    return Ok(false);
                }
                let before = &replay.baseline;
                let water: Vec<_> = self
                    .surface_builder
                    .resources
                    .instances
                    .chunk_flora_instances
                    .iter()
                    .flat_map(|(_, r)| {
                        r.hydration
                            .iter()
                            .take(2)
                            .enumerate()
                            .flat_map(move |(s, values)| {
                                values.iter().map(move |&(root, value)| {
                                    (
                                        (r.chunk_id.x, r.chunk_id.y, r.chunk_id.z, s, root),
                                        value.to_bits(),
                                    )
                                })
                            })
                    })
                    .collect();
                let (preflight_ms, edit_ms, async_frames, changed) = if let Some(PendingSample {
                    preflight_ms,
                    enqueue_ms,
                    frame,
                }) = replay.pending.take()
                {
                    let metrics = self
                        .grass_edits
                        .last
                        .expect("async brush completion metrics");
                    (
                        preflight_ms,
                        enqueue_ms + metrics.submit_ms + metrics.finish_ms,
                        self.time_info.total_frame_count() - frame,
                        0,
                    )
                } else {
                    log::info!(
                        "[BRUSH_EDIT_BENCH] begin case={} sample={} warmup={} roots={}",
                        replay.case,
                        replay.sample,
                        replay.sample < 3,
                        before.len()
                    );
                    let preflight = Instant::now();
                    if std::env::var_os("RE_FLORA_BRUSH_PREFLIGHT_DRAIN").is_some() {
                        self.vulkan_ctx.device().wait_idle();
                    }
                    let preflight_ms = preflight.elapsed().as_secs_f64() * 1000.;
                    let started = Instant::now();
                    let mut changed = 0;
                    let op = replay.case.split('-').next().unwrap();
                    match op {
                        "add" | "noop" => {
                            if replay.asynchronous {
                                self.apply_interactive_flora_regeneration_at(
                                    replay.edit,
                                    7,
                                    true,
                                    10_000,
                                )?;
                                replay.pending = Some(PendingSample {
                                    preflight_ms,
                                    enqueue_ms: started.elapsed().as_secs_f64() * 1000.,
                                    frame: self.time_info.total_frame_count(),
                                });
                                // Submission happens at the real frame-tail boundary. Render
                                // normally until the completion poll has published all chunks.
                                return Ok(false);
                            }
                            self.apply_interactive_flora_regeneration_at(
                                replay.edit,
                                7,
                                true,
                                10_000,
                            )?
                        }
                        "remove" => self.apply_surface_flora_removal(replay.edit)?,
                        "dig" => {
                            let result = self.apply_surface_terrain_removal(
                                replay.edit,
                                None,
                                Some(65_536),
                                None,
                            )?;
                            changed = result.stats.removed_counts.iter().sum();
                        }
                        "place" => {
                            let result = self.apply_surface_terrain_placement(
                                replay.edit,
                                crate::builder::VOXEL_TYPE_DIRT,
                                65_536,
                            )?;
                            changed = result.stats.added_counts.iter().sum();
                        }
                        _ => unreachable!(),
                    }
                    (
                        preflight_ms,
                        started.elapsed().as_secs_f64() * 1000.,
                        0,
                        changed,
                    )
                };
                let op = replay.case.split('-').next().unwrap();
                let after = roots(self)?;
                if op == "add" || op == "noop" || op == "remove" {
                    for (key, value) in before {
                        if let Some(next) = after.get(key) {
                            anyhow::ensure!(
                                (value.0, value.1) == (next.0, next.1),
                                "surviving grass payload changed"
                            );
                        }
                    }
                    // Across async frames, ecology may legitimately change water.
                    // transactions::verify tests latest-water preservation explicitly.
                    for (key, value) in water.into_iter().filter(|_| !replay.asynchronous) {
                        if let Some(next) = after.get(&key) {
                            anyhow::ensure!(value == next.2, "surviving grass hydration changed");
                        }
                    }
                    if op == "add" {
                        anyhow::ensure!(after.len() >= before.len(), "add deleted roots");
                    }
                    if op == "remove" {
                        anyhow::ensure!(after.len() <= before.len(), "remove added roots");
                    }
                    if op == "noop" {
                        anyhow::ensure!(
                            before.keys().eq(after.keys()),
                            "repeat grass dab is not idempotent"
                        );
                    }
                }
                let keys: Vec<u8> = after
                    .iter()
                    .flat_map(|(key, value)| {
                        [key.0, key.1, key.2, key.3 as u32, key.4, value.0, value.1]
                            .into_iter()
                            .flat_map(u32::to_le_bytes)
                    })
                    .collect();
                let root_hash = hash(keys);
                // Exact changed region, outside all timers. Ignore appearance/moisture bits.
                let origin = (replay.edit.start.min(replay.edit.end) * 256.
                    - Vec3::splat(replay.edit.radius * 256. + 2.))
                .floor()
                .as_uvec3();
                let end = (replay.edit.start.max(replay.edit.end) * 256.
                    + Vec3::splat(replay.edit.radius * 256. + 2.))
                .ceil()
                .as_uvec3();
                let atlas = self
                    .plain_builder
                    .read_chunk_atlas_region(origin, end - origin)?;
                let atlas_hash = hash(atlas.into_iter().map(|b| b & 15));
                let event = serde_json::json!({"sample":replay.sample,"warmup":replay.sample<3,"edit_ms":edit_ms,"preflight_ms":preflight_ms,"before_roots":before.len(),"after_roots":after.len(),"changed_voxels":changed,"root_hash":root_hash,"atlas_hash":atlas_hash,"async_frames":async_frames,"async_metrics":if replay.asynchronous { self.grass_edits.last } else { None }});
                log::info!(
                    "[BRUSH_EDIT_BENCH] end case={} event={}",
                    replay.case,
                    event
                );
                replay.events.push(event);
                replay.sample += 1;
                if replay.sample >= replay.samples + 3 {
                    if let Some(p) = replay.output.parent() {
                        std::fs::create_dir_all(p)?;
                    }
                    std::fs::write(
                        &replay.output,
                        serde_json::to_string_pretty(
                            &serde_json::json!({"case":replay.case,"events":replay.events,"scope":if replay.asynchronous { "CPU enqueue + all submit/finish phases; publication latency separate; not input-to-present" } else { "production edit call; excludes input pick, fixture reset, validation and future frames" }}),
                        )?,
                    )?;
                    log::info!(
                        "[BRUSH_EDIT_BENCH] complete case={} samples={}",
                        replay.case,
                        replay.samples
                    );
                    return Ok(true);
                }
            }
            replay.frame += 1;
            Ok(false)
        })();
        self.brush_edit_bench = Some(replay);
        result
    }
}

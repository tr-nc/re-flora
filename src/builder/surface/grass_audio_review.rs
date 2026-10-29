//! Opt-in hidden Release validation through real vegetation and GPU edit interfaces.
use super::*;
use crate::flora::grass_acoustics::GrassPatch;
use std::sync::Arc;

impl SurfaceBuilder {
    /// Compare the published CPU observation with the real draw buffer. Direct
    /// VRAM reads are deliberately confined to this opt-in correctness check,
    /// outside projection timing.
    pub(crate) fn review_grass_publication(&self) -> Result<()> {
        for (_, resources) in &self.resources.instances.chunk_flora_instances {
            for species in 0..2 {
                let observed = resources.read_grass_instances(species)?;
                let drawn = resources.resource.instances_buf.read_back_range(
                    u64::from(FloraInstanceResources::species_offset(species)) * 8,
                    observed.len() as u64 * 8,
                )?;
                anyhow::ensure!(
                    bytemuck::cast_slice::<Instance, u8>(&observed) == drawn,
                    "CPU grass publication differs from draw stream: chunk={:?} species={species}",
                    resources.chunk_id,
                );
            }
        }
        Ok(())
    }

    fn review_grass_patches(
        &mut self,
        now: &mut f64,
        growth: Option<f32>,
    ) -> Result<Vec<Arc<GrassPatch>>> {
        self.review_grass_publication()?;
        let mut patches = Vec::new();
        for _ in 0..=self.resources.instances.chunk_flora_instances.len() {
            *now += 0.11;
            patches = self.grass_audio_patches(Vec3::splat(0.5), 2.0, growth, *now)?;
        }
        Ok(patches)
    }

    pub(crate) fn verify_grass_audio_runtime(
        &mut self,
        plain: &mut super::super::PlainBuilder,
    ) -> Result<()> {
        self.review_grass_publication()?; // Includes startup GPU surface builds.
        let saved = self.capture_flora_snapshot(0)?;
        const INFLUENCE_ID: u64 = 0x4752_4153_535f_5256;
        anyhow::ensure!(!self
            .external_grass_growth_influences
            .contains_key(&INFLUENCE_ID));
        let roots = [
            UVec3::new(30, 32, 30),
            UVec3::new(35, 32, 35),
            UVec3::new(30, 96, 30),
        ];
        let original: Vec<_> = roots
            .iter()
            .map(|&r| plain.read_chunk_atlas_region(r, UVec3::ONE))
            .collect::<Result<_>>()?;
        let result = (|| -> Result<()> {
            for &r in &roots {
                plain.write_chunk_atlas_region(r, UVec3::ONE, &[0x32])?;
            }
            for (_, r) in &mut self.resources.instances.chunk_flora_instances {
                for s in 0..2 {
                    r.write_species_instances(s, &[])?;
                }
            }
            let mut now = 100.0;
            anyhow::ensure!(
                self.review_grass_patches(&mut now, Some(1.0))?.is_empty(),
                "bare ground emitted patches"
            );
            let index = self.get_chunk_resource_index(UVec3::ZERO)?;
            anyhow::ensure!(self.resources.instances.chunk_flora_instances[index]
                .1
                .try_append_grass(0, pack_flora_instance(roots[0], 255, u32::MAX))?);
            anyhow::ensure!(
                self.review_grass_patches(&mut now, Some(1.0))?.len() == 1,
                "CPU append did not publish audible coverage"
            );
            self.resources.instances.chunk_flora_instances[index]
                .1
                .write_species_instances(0, &[])?;
            for (s, positions) in [(0, vec![roots[0], roots[2]]), (1, vec![roots[1]])] {
                let instances: Vec<_> = positions
                    .into_iter()
                    .map(|p| pack_flora_instance(p, 255, u32::MAX))
                    .collect();
                self.resources.instances.chunk_flora_instances[index]
                    .1
                    .write_species_instances(s, &instances)?;
            }
            let patches = self.review_grass_patches(&mut now, Some(1.0))?;
            anyhow::ensure!(
                patches.len() == 2,
                "expected vertically separate grass patches: {}",
                patches.len()
            );
            let mass = |patches: &[Arc<GrassPatch>]| {
                patches
                    .iter()
                    .flat_map(|p| &p.samples)
                    .map(|s| s.mass)
                    .sum::<f32>()
            };
            let full = mass(&patches);
            anyhow::ensure!(full > 0.0, "mature grass has no mass");
            anyhow::ensure!(
                self.review_grass_patches(&mut now, Some(0.0))?.is_empty(),
                "invisible growth override emitted sound coverage"
            );
            let planted = self.capture_flora_snapshot(0)?;
            self.resources.instances.chunk_flora_instances[index]
                .1
                .grass_audio
                .pending_builds += 1;
            let pending_read = self.resources.instances.chunk_flora_instances[index]
                .1
                .read_grass_instances(0);
            let pending_patches = self.grass_audio_patches(Vec3::splat(0.5), 2.0, Some(1.0), now);
            self.resources.instances.chunk_flora_instances[index]
                .1
                .grass_audio
                .pending_builds -= 1;
            anyhow::ensure!(pending_read.is_err(), "in-flight stream was readable");
            anyhow::ensure!(
                pending_patches?.is_empty(),
                "in-flight coverage was published"
            );
            let point = (roots[0].as_vec3() + Vec3::splat(0.5)) / 256.0;
            self.trim_flora_instances_for_brush(UVec3::ZERO, point, point, 0.5 / 256.0, 0, 0, 1)?;
            let mowed = mass(&self.review_grass_patches(&mut now, Some(1.0))?);
            anyhow::ensure!(
                mowed < full && mowed > 0.0,
                "mowing did not reduce mass while preserving neighbors: {mowed}/{full}"
            );
            self.update_flora_growth_for_chunk(UVec3::ZERO, 255)?;
            let regrown = mass(&self.review_grass_patches(&mut now, Some(1.0))?);
            anyhow::ensure!(
                regrown > mowed,
                "GPU growth did not refresh grass mass: {regrown}/{mowed}"
            );
            self.restore_flora_snapshot(&planted, 0)?;
            anyhow::ensure!(
                mass(&self.review_grass_patches(&mut now, Some(1.0))?) == full,
                "snapshot restore changed mass"
            );
            let receipt =
                self.remove_flora_reversibly(UVec3::ZERO, point, point, 0.5 / 256.0, 0, 0)?;
            anyhow::ensure!(
                mass(&self.review_grass_patches(&mut now, Some(1.0))?) < full,
                "removal retained stale grass"
            );
            self.restore_removed_flora(&receipt)?;
            anyhow::ensure!(
                mass(&self.review_grass_patches(&mut now, Some(1.0))?) == full,
                "undo failed to restore grass mass"
            );
            // Runtime hydration publication is a soft refresh, not root replacement.
            for values in self.resources.instances.chunk_flora_instances[index]
                .1
                .hydration
                .iter_mut()
                .take(2)
            {
                for (_, value) in values {
                    *value = 0.0;
                }
            }
            for s in 0..2 {
                self.resources.instances.chunk_flora_instances[index]
                    .1
                    .upload_hydration(s)?;
            }
            let dry = mass(&self.review_grass_patches(&mut now, None)?);
            for values in self.resources.instances.chunk_flora_instances[index]
                .1
                .hydration
                .iter_mut()
                .take(2)
            {
                for (_, value) in values {
                    *value = 1.0;
                }
            }
            for s in 0..2 {
                self.resources.instances.chunk_flora_instances[index]
                    .1
                    .upload_hydration(s)?;
            }
            let wet = mass(&self.review_grass_patches(&mut now, None)?);
            anyhow::ensure!(wet >= dry, "hydration response reversed");
            // Exercise the real competition-field publisher and its GPU refresh,
            // not a test-only mutation of the retained CPU words.
            self.upsert_external_grass_growth_influence(
                INFLUENCE_ID,
                GrassGrowthInfluence {
                    center_world_vox: roots[0],
                    radius_voxels: 8,
                    min_level: 0,
                },
                0,
            )?;
            let suppressed = mass(&self.review_grass_patches(&mut now, None)?);
            anyhow::ensure!(
                suppressed < wet,
                "competition did not reduce audible biomass"
            );
            let r = &self.resources.instances.chunk_flora_instances[index].1;
            let linear = (roots[0].x + 256 * (roots[0].y + 256 * roots[0].z)) as usize;
            let gpu_word = r
                .grass_growth_potential_levels
                .read_back_range((linear / 8 * 4) as u64, 4)?;
            anyhow::ensure!(
                r.grass_growth_potential_cpu[linear / 8]
                    == u32::from_ne_bytes(gpu_word.try_into().unwrap())
                    && packed_grass_growth_potential_level(&r.grass_growth_potential_cpu, linear)
                        == 0,
                "CPU/GPU growth-potential publication diverged"
            );
            self.remove_external_grass_growth_influences(&[INFLUENCE_ID], 0)?;
            self.update_flora_growth_for_chunk(UVec3::ZERO, 255)?;
            anyhow::ensure!(
                mass(&self.review_grass_patches(&mut now, None)?) > suppressed,
                "removing competition did not restore audible growth"
            );
            // Support deletion goes through the production terrain/root reconciler.
            plain.write_chunk_atlas_region(roots[0], UVec3::ONE, &[0])?;
            self.reconcile_grass_terrain(UAabb3::new(roots[0], roots[0]), 0)?;
            anyhow::ensure!(
                mass(&self.review_grass_patches(&mut now, Some(1.0))?) < full,
                "terrain removal left audible grass"
            );

            // Bounded worst-chunk projection: both 40k instance streams, no window/GPU work inside timing.
            // Dense fixture changes only derived draw streams, and is restored below.
            let mut costs = Vec::new();
            for iteration in 0..12 {
                for s in 0..2 {
                    let instances: Vec<_> = (0..MAX_FLORA_INSTANCES_PER_SPECIES)
                        .map(|i| {
                            pack_flora_instance(
                                UVec3::new(i % 256, 32 + s as u32 * 64, i / 256),
                                255,
                                u32::MAX,
                            )
                        })
                        .collect();
                    self.resources.instances.chunk_flora_instances[index]
                        .1
                        .write_species_instances(s, &instances)?;
                }
                now += 0.11;
                let start = Instant::now();
                let patches = self.grass_audio_patches(Vec3::splat(0.5), 2.0, Some(1.0), now)?;
                costs.push(start.elapsed().as_secs_f64() * 1000.0);
                anyhow::ensure!(
                    patches.len() <= crate::flora::grass_acoustics::MAX_CANDIDATES,
                    "candidate budget exceeded at {iteration}"
                );
            }
            self.review_grass_publication()?;
            costs.sort_by(f64::total_cmp);
            // Loose opt-in stall regression guard, not a universal frame budget.
            // The uncached 8 MiB readback regressed to hundreds of ms on discrete GPUs.
            anyhow::ensure!(
                costs[6] < 50.0,
                "grass projection stall regression: median={:.3}ms (limit 50ms)",
                costs[6]
            );
            let start = Instant::now();
            for _ in 0..100 {
                self.grass_audio_patches(Vec3::splat(0.5), 2.0, Some(1.0), now)?;
            }
            log::info!(
                "[GRASS_AUDIO_REVIEW] cpu_draw_match=true cpu_append=true pending_excluded=true competition=true bare=true tall_short=true elevations=true override=true mowing=true gpu_growth=true remove=true undo=true load=true hydration=true terrain_remove=true max_roots=80000 projection_median_ms={:.3} projection_max_ms={:.3} idle_mean_us={:.1}",
                costs[6],
                costs[11],
                start.elapsed().as_secs_f64() * 1e4
            );
            Ok(())
        })();
        // Restore even when a behavioral assertion returns an error.
        for (&root, bytes) in roots.iter().zip(original) {
            plain.write_chunk_atlas_region(root, UVec3::ONE, &bytes)?;
        }
        self.remove_external_grass_growth_influences(&[INFLUENCE_ID], 0)?;
        self.restore_flora_snapshot(&saved, 0)?;
        self.review_grass_publication()?;
        self.refresh_grass_soil_cover(plain)?;
        result
    }
}

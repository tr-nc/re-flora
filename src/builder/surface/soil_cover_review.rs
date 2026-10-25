//! Opt-in native GPU acceptance; restores the original terrain and flora afterward.
use super::*;
use crate::builder::{PlainBuilder, VOXEL_GRASS_COVER_BIT};

impl SurfaceBuilder {
    /// Release-only opt-in measurement through real atlas/plant publications.
    pub fn benchmark_grass_soil_cover_runtime(&mut self, plain: &mut PlainBuilder) -> Result<()> {
        let saved = self.capture_flora_snapshot(0)?;
        let extent = self.chunk_bound.max() * self.voxel_dim_per_chunk;
        let origin = UVec3::new(0, 31, 0);
        let dim = UVec3::new(extent.x, 3, extent.z);
        let original = plain.read_chunk_atlas_region(origin, dim)?;
        let mut soil = vec![0; dim.element_product() as usize];
        for z in 0..dim.z {
            for x in 0..dim.x {
                soil[(x + dim.x * (1 + dim.y * z)) as usize] = 0x32;
            }
        }
        plain.write_chunk_atlas_region(origin, dim, &soil)?;
        for side in [64u32, 512u32] {
            for (_, r) in &mut self.resources.instances.chunk_flora_instances {
                for s in 0..2 {
                    r.write_species_instances(s, &[])?;
                }
            }
            let mut roots = HashMap::<UVec3, Vec<Instance>>::new();
            for z in (1..side.min(extent.z)).step_by(2) {
                for x in (1..side.min(extent.x)).step_by(2) {
                    let p = UVec3::new(x, 32, z);
                    roots.entry(p / self.voxel_dim_per_chunk).or_default().push(
                        pack_flora_instance(p % self.voxel_dim_per_chunk, 255, u32::MAX),
                    );
                }
            }
            let count: usize = roots.values().map(Vec::len).sum();
            for (&chunk, instances) in &roots {
                let i = self.get_chunk_resource_index(chunk)?;
                self.resources.instances.chunk_flora_instances[i]
                    .1
                    .write_species_instances(0, instances)?;
            }
            self.refresh_grass_soil_cover(plain)?;
            let i = self.get_chunk_resource_index(UVec3::ZERO)?;
            let instances = &roots[&UVec3::ZERO];
            let mut costs = Vec::new();
            for iteration in 0..12 {
                let n = instances.len() - usize::from(iteration % 2 == 0);
                self.resources.instances.chunk_flora_instances[i]
                    .1
                    .write_species_instances(0, &instances[..n])?;
                let start = Instant::now();
                self.refresh_grass_soil_cover(plain)?;
                costs.push(start.elapsed().as_secs_f64() * 1000.);
            }
            costs.sort_by(f64::total_cmp);
            let start = Instant::now();
            self.refresh_grass_soil_cover(plain)?;
            log::info!("[SOIL_COVER_BENCH] roots={count} edit=one_root samples={} median_ms={:.3} max_ms={:.3} idle_ms={:.3}", costs.len(), costs[costs.len()/2], costs[costs.len()-1], start.elapsed().as_secs_f64()*1000.);
        }
        plain.write_chunk_atlas_region(origin, dim, &original)?;
        self.restore_flora_snapshot(&saved, 0)?;
        self.refresh_grass_soil_cover(plain)?;
        Ok(())
    }

    pub fn verify_grass_soil_cover_runtime(&mut self, plain: &mut PlainBuilder) -> Result<()> {
        let saved = self.capture_flora_snapshot(0)?;
        let root = UVec3::new(
            self.voxel_dim_per_chunk.x - 1,
            32,
            self.voxel_dim_per_chunk.z - 1,
        );
        let other = root + UVec3::X * 2;
        let origin = root - UVec3::new(12, 1, 12);
        let dim = UVec3::new(28, 4, 28);
        let original = plain.read_chunk_atlas_region(origin, dim)?;
        let mut soil = vec![0; dim.element_product() as usize];
        for z in 0..dim.z {
            for x in 0..dim.x {
                soil[(x + dim.x * (1 + dim.y * z)) as usize] = 0x32;
            }
        }
        plain.write_chunk_atlas_region(origin, dim, &soil)?;
        // Remove ordinary grass only. Authored flowers remain live and must not
        // cause any green soil in the empty-root case.
        for (_, r) in &mut self.resources.instances.chunk_flora_instances {
            for species in 0..species::MODEL_FLOWER_FIRST_SPECIES as usize {
                r.write_species_instances(species, &[])?;
            }
        }
        self.refresh_grass_soil_cover(plain)?;
        anyhow::ensure!(
            plain.read_chunk_atlas_region(root, UVec3::ONE)? == [0x32],
            "bare soil changed without ordinary grass"
        );
        // Publish a real non-grass species through the same resource boundary.
        // It must not affect soil, even though it has hydration/root state too.
        let flower_chunk = root / self.voxel_dim_per_chunk;
        let flower_index = self.get_chunk_resource_index(flower_chunk)?;
        let flower_species = species::MODEL_FLOWER_FIRST_SPECIES as usize;
        let saved_species = self.read_flora_species(flower_chunk)?;
        self.resources.instances.chunk_flora_instances[flower_index]
            .1
            .write_species_instances(
                flower_species,
                &[pack_flora_instance(
                    root % self.voxel_dim_per_chunk,
                    255,
                    u32::MAX,
                )],
            )?;
        self.soil_cover.dirty = true;
        self.refresh_grass_soil_cover(plain)?;
        anyhow::ensure!(
            plain.read_chunk_atlas_region(root, UVec3::ONE)? == [0x32],
            "flower recolored soil"
        );
        self.resources.instances.chunk_flora_instances[flower_index]
            .1
            .write_species_instances(flower_species, &saved_species[flower_species])?;
        for (species, p) in [(0, root), (1, other)] {
            let chunk = p / self.voxel_dim_per_chunk;
            let index = self.get_chunk_resource_index(chunk)?;
            self.resources.instances.chunk_flora_instances[index]
                .1
                .write_species_instances(
                    species,
                    &[pack_flora_instance(
                        p % self.voxel_dim_per_chunk,
                        255,
                        u32::MAX,
                    )],
                )?;
        }
        self.refresh_grass_soil_cover(plain)?;
        let covered = plain.read_chunk_atlas_region(origin, dim)?;
        anyhow::ensure!(
            covered.iter().any(|b| b & VOXEL_GRASS_COVER_BIT != 0),
            "grass did not recolor soil"
        );
        for p in [root, root + UVec3::X, other] {
            anyhow::ensure!(
                plain.read_chunk_atlas_region(p, UVec3::ONE)? == [0x72],
                "cross-chunk grass footprint or moisture incorrect at {p:?}"
            );
        }
        // Unlike the former four-cell dilation, no soil outside the rooted
        // outline turns green; the cell between opposing roots is an interior gap.
        for p in [root - UVec3::X, root + UVec3::Z, root + UVec3::Z * 3] {
            anyhow::ensure!(
                plain.read_chunk_atlas_region(p, UVec3::ONE)? == [0x32],
                "green soil expanded past grass boundary at {p:?}"
            );
        }
        let planted = self.capture_flora_snapshot(0)?;
        // Production removal path, including GPU compaction and hydration-root publication.
        let center = (root.as_vec3() + Vec3::splat(0.5)) / 256.;
        self.edit_flora_instances_for_brush(
            root / self.voxel_dim_per_chunk,
            center,
            center,
            0.5 / 256.,
            0,
            0,
        )?;
        self.refresh_grass_soil_cover(plain)?;
        anyhow::ensure!(
            plain.read_chunk_atlas_region(root - UVec3::X, UVec3::ONE)? == [0x32],
            "removed grass left stale coverage"
        );
        anyhow::ensure!(
            plain.read_chunk_atlas_region(other, UVec3::ONE)? == [0x72],
            "removing one root erased surviving grass coverage"
        );
        let center = (other.as_vec3() + Vec3::splat(0.5)) / 256.;
        self.edit_flora_instances_for_brush(
            other / self.voxel_dim_per_chunk,
            center,
            center,
            0.5 / 256.,
            0,
            0,
        )?;
        self.refresh_grass_soil_cover(plain)?;
        anyhow::ensure!(
            plain
                .read_chunk_atlas_region(origin, dim)?
                .iter()
                .all(|b| b & VOXEL_GRASS_COVER_BIT == 0),
            "last root removal left green soil"
        );
        // Snapshot round-trip discards cached coverage and derives it from live roots.
        let json = serde_json::to_vec(&planted)?;
        let decoded: FloraSnapshot = serde_json::from_slice(&json)?;
        plain.restore_chunk_atlas_region(origin, dim, covered.clone())?;
        self.restore_flora_snapshot(&decoded, 0)?;
        self.refresh_grass_soil_cover(plain)?;
        anyhow::ensure!(
            plain.read_chunk_atlas_region(origin, dim)? == covered,
            "loaded grass coverage differs"
        );
        // One layer hides the green surface immediately, while grass survives
        // until the existing three-layer burial threshold.
        plain.write_chunk_atlas_region(root + UVec3::Y, UVec3::ONE, &[2])?;
        self.reconcile_grass_terrain(UAabb3::new(root + UVec3::Y, root + UVec3::Y), 0)?;
        self.refresh_grass_soil_cover(plain)?;
        anyhow::ensure!(
            plain.read_chunk_atlas_region(root, UVec3::ONE)? == [0x32],
            "covered soil remained green"
        );
        anyhow::ensure!(
            plain.read_chunk_atlas_region(root + UVec3::Y, UVec3::ONE)? == [2],
            "new soil inherited buried grass color"
        );
        plain.write_chunk_atlas_region(root + UVec3::Y, UVec3::ONE, &[0])?;
        self.reconcile_grass_terrain(UAabb3::new(root + UVec3::Y, root + UVec3::Y), 0)?;
        self.refresh_grass_soil_cover(plain)?;
        anyhow::ensure!(
            plain.read_chunk_atlas_region(root, UVec3::ONE)? == [0x72],
            "uncovering surviving grass did not restore its patch"
        );
        // Return the real garden to its original state before any visible frame.
        plain.write_chunk_atlas_region(origin, dim, &original)?;
        self.restore_flora_snapshot(&saved, 0)?;
        self.refresh_grass_soil_cover(plain)?;
        log::info!("[SOIL_COVER_REVIEW] bare=true flowers_excluded=true tall_short=true cross_chunk=true interior_gap=true tight_boundary=true remove_last=true moisture=true restore=true burial=true uncover=true");
        Ok(())
    }
}

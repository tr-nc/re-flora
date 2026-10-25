//! Per-tree simulated age. Published geometry advances in bounded stages through
//! the same transactional tree owner as manual editing; old saves stay at their age.
use super::*;

pub(in crate::app::core) const SAPLING_AGE: f32 = 0.375;
const MATURATION_SECONDS: f32 = 900.;
const AGE_STEPS: f32 = 32.;
const PUBLICATION_INTERVAL: f32 = 0.5;

fn age_at(seconds: f32) -> f32 {
    let age = SAPLING_AGE + (1. - SAPLING_AGE) * (seconds / MATURATION_SECONDS).clamp(0., 1.);
    (age * AGE_STEPS).floor() / AGE_STEPS
}

impl App {
    pub(in crate::app::core) fn plant_growing_tree(
        &mut self,
        desc: TreeDesc,
        position: Vec3,
    ) -> Result<u32> {
        anyhow::ensure!(position.is_finite(), "invalid sapling position");
        anyhow::ensure!(
            self.trees.records.len() < 512 && self.trees.placement_id(true) < u32::MAX,
            "garden tree capacity reached"
        );
        // Reject an impossible mature footprint before a tiny sapling is committed.
        // This is a world-bound check, not a reservation that excludes nearby plants.
        let mature = Tree::new(desc.clone());
        let extent = (self.world_chunk_dim * super::super::VOXEL_DIM_PER_CHUNK).as_vec3();
        anyhow::ensure!(
            mature.world_trunks(position).iter().all(|cone| {
                let a = cone.aabb();
                a.min().cmpge(Vec3::ZERO).all() && a.max().cmplt(extent).all()
            }),
            "the mature tree would extend outside the garden"
        );
        anyhow::ensure!(
            self.growing_tree_space_clear(&desc, position, SAPLING_AGE)?,
            "sapling space is occupied"
        );
        let id = self.trees.placement_id(true);
        self.apply_tree_placement_at_age(
            TreePlacementEdit {
                tree_desc: desc,
                placement: TreePlacement::World(position),
                options: TreeAddOptions::default().with_new_id(),
            },
            SAPLING_AGE,
        )?;
        self.trees
            .records
            .get_mut(&id)
            .context("planted sapling missing")?
            .growth_seconds = Some(0.);
        log::info!(
            "[TREE_GROWTH] planted={id} age={SAPLING_AGE} maturation_seconds={MATURATION_SECONDS}"
        );
        Ok(id)
    }

    pub(in crate::app::core) fn advance_tree_growth(&mut self, seconds: f32) -> Result<()> {
        if !seconds.is_finite() || seconds <= 0. {
            return Ok(());
        }
        for record in self.trees.records.values_mut() {
            if let Some(elapsed) = &mut record.growth_seconds {
                *elapsed = (*elapsed + seconds).min(MATURATION_SECONDS);
            }
        }
        self.trees.growth_publication_cooldown =
            (self.trees.growth_publication_cooldown - seconds).max(0.);
        if self.trees.growth_publication_cooldown > 0. {
            return Ok(());
        }
        // Greatest pending age gap first, stable ID as tie-breaker: bounded work
        // without permanently starving later trees in a large simultaneous planting.
        let next = self
            .trees
            .records
            .iter()
            .filter_map(|(&id, record)| {
                let elapsed = record.growth_seconds?;
                let target = age_at(elapsed);
                (target > record.age + 1e-5).then_some((id, target, elapsed, target - record.age))
            })
            .max_by(|a, b| a.3.total_cmp(&b.3).then_with(|| b.0.cmp(&a.0)));
        let Some((id, age, elapsed, _)) = next else {
            return Ok(());
        };
        let stage_start = Instant::now();
        self.trees.growth_publication_cooldown = PUBLICATION_INTERVAL;
        let mut record = self.trees.records[&id].clone();
        if !self.growing_tree_space_clear(&record.mature_desc, record.position, age)? {
            self.trees.records.get_mut(&id).unwrap().growth_seconds = None;
            log::info!(
                "[TREE_GROWTH] tree={id} paused=obstruction existing_materials_preserved=true"
            );
            return Ok(());
        }
        record.age = age;
        if age >= 1. {
            record.growth_seconds = None;
        }
        let start = Instant::now();
        if let Err(error) = self.rebuild_tree_records(vec![(id, record)]) {
            // The transactional owner retains the previous tree. Do not repeatedly
            // restamp a failed/unplaceable growth stage or undo a player's later edit.
            if let Some(record) = self.trees.records.get_mut(&id) {
                record.growth_seconds = None;
            }
            return Err(error).context("tree growth paused after failed publication");
        }
        log::info!("[TREE_GROWTH] tree={id} age={age:.5} elapsed={elapsed:.3} publish_ms={:.3} stage_ms={:.3} trunks={} leaves={}",start.elapsed().as_secs_f64()*1000.,stage_start.elapsed().as_secs_f64()*1000.,self.trees.records[&id].rest_tree.trunks().len(),self.trees.records[&id].leaf_render_positions.len());
        Ok(())
    }

    fn growing_tree_space_clear(
        &mut self,
        desc: &TreeDesc,
        position: Vec3,
        age: f32,
    ) -> Result<bool> {
        let cones = Tree::new(desc.at_age(age)).world_trunks(position);
        let extent = self.world_chunk_dim * super::super::VOXEL_DIM_PER_CHUNK;
        let mut low = Vec3::splat(f32::INFINITY);
        let mut high = Vec3::splat(f32::NEG_INFINITY);
        for cone in &cones {
            let a = cone.aabb();
            low = low.min(a.min());
            high = high.max(a.max());
        }
        if !low.is_finite() || low.cmplt(Vec3::ZERO).any() || high.cmpge(extent.as_vec3()).any() {
            return Ok(false);
        }
        let min = low.floor().as_uvec3();
        let max = high.ceil().as_uvec3().min(extent);
        let dim = max - min;
        if dim.min_element() == 0 || dim.as_dvec3().element_product() > 8. * 1024. * 1024. {
            return Ok(false);
        }
        let data = self.plain_builder.read_chunk_atlas_region(min, dim)?;
        let base_y = (position.y * 256.).floor() as u32;
        for z in 0..dim.z {
            for y in 0..dim.y {
                for x in 0..dim.x {
                    let material = data[((z * dim.y + y) * dim.x + x) as usize] & 15;
                    let p = min + UVec3::new(x, y, z);
                    if material == 0
                        || material == VOXEL_TYPE_CHERRY_WOOD as u8
                        || (material == crate::builder::VOXEL_TYPE_DIRT as u8 && p.y <= base_y)
                    {
                        continue;
                    }
                    if cones
                        .iter()
                        .any(|c| c.signed_distance(p.as_vec3() + Vec3::splat(0.5)) <= 0.)
                    {
                        return Ok(false);
                    }
                }
            }
        }
        Ok(true)
    }

    pub(in crate::app::core) fn pause_tree_growth_after_wood_edit(&mut self, bound: UAabb3) {
        for (&id, record) in &mut self.trees.records {
            if record.growth_seconds.is_some()
                && record.bound.min().cmple(bound.max()).all()
                && bound.min().cmple(record.bound.max()).all()
            {
                record.growth_seconds = None;
                log::info!(
                    "[TREE_GROWTH] tree={id} paused=hand_edited_wood age={}",
                    record.age
                );
            }
        }
    }

    pub(in crate::app::core) fn seed_sapling_review(&mut self) -> Result<()> {
        let Ok(mode) = std::env::var("RE_FLORA_SAPLING_REVIEW") else {
            return Ok(());
        };
        if mode == "load" {
            let saved = self.capture_tree_snapshot();
            self.verify_restored_tree_rendering(&saved)?;
            let growing: Vec<_> = self
                .trees
                .records
                .iter()
                .filter(|(_, r)| r.growth_seconds.is_some())
                .collect();
            anyhow::ensure!(
                growing.len() == 1 && (growing[0].1.age - 0.5).abs() < 1e-5,
                "saved middle-age sapling was reset"
            );
            log::info!(
                "[TREE_GROWTH_REVIEW] loaded=true age={} elapsed={} leaves={} verified=true",
                growing[0].1.age,
                growing[0].1.growth_seconds.unwrap(),
                growing[0].1.leaf_render_positions.len()
            );
        } else {
            let age = if mode == "cycle" || mode == "blocked" {
                SAPLING_AGE
            } else if mode == "edit" || mode == "override" {
                0.5
            } else {
                mode.parse::<f32>().context("review age must be 0.375..1")?
            };
            anyhow::ensure!(
                (SAPLING_AGE..=1.).contains(&age),
                "invalid sapling review age"
            );
            let ids = self.trees.records.keys().copied().collect::<Vec<_>>();
            for id in ids {
                self.remove_tree(id)?;
            }
            self.contree_builder.flush_cpu_chunk_cache_jobs();
            let ground = self.query_terrain_height_cpu(Vec2::new(0.82, 1.22));
            let mut desc = crate::tree_gen::TreeSpecies::AppleTree.default_desc();
            desc.branching.seed = 907;
            anyhow::ensure!(
                self.plant_growing_tree(desc.clone(), Vec3::splat(f32::NAN))
                    .is_err()
                    && self
                        .plant_growing_tree(desc.clone(), Vec3::splat(10000.))
                        .is_err()
                    && self.trees.records.is_empty(),
                "invalid planting mutated the forest"
            );
            let id = self.plant_growing_tree(desc, Vec3::new(0.82, ground, 1.22))?;
            if mode == "cycle" {
                // The next normal world-clock tick, not this fixture, must publish the next stage.
                self.trees.records.get_mut(&id).unwrap().growth_seconds =
                    Some(MATURATION_SECONDS / (AGE_STEPS * (1. - SAPLING_AGE)) - 0.1);
            } else if age > SAPLING_AGE {
                self.advance_tree_growth(
                    (age - SAPLING_AGE) / (1. - SAPLING_AGE) * MATURATION_SECONDS + 0.001,
                )?;
            }
            if mode == "edit" {
                let r = &self.trees.records[&id];
                let center =
                    (r.rest_tree.trunks()[0].center_a() + r.rest_tree.trunks()[0].center_b()) * 0.5
                        / 256.
                        + r.position;
                let removed = self.apply_surface_terrain_removal(
                    TerrainBrushEdit {
                        start: center,
                        end: center,
                        radius: 2. / 256.,
                    },
                    Some(VOXEL_TYPE_CHERRY_WOOD),
                    None,
                    None,
                )?;
                anyhow::ensure!(
                    removed.stats.removed_counts[VOXEL_TYPE_CHERRY_WOOD as usize] > 0
                        && self.trees.records[&id].growth_seconds.is_none(),
                    "wood editing did not freeze growth"
                );
                self.advance_tree_growth(900.)?;
                anyhow::ensure!(self.trees.records[&id].age == 0.5, "edited tree regrew");
                log::info!("[TREE_GROWTH_REVIEW] wood_edit_pauses=true verified=true");
            }
            if mode == "blocked" {
                let r = &self.trees.records[&id];
                let position = r.position;
                let future = Tree::new(r.mature_desc.at_age(0.5)).world_trunks(position);
                let current = r.rest_tree.world_trunks(position);
                let mut blocked = None;
                'find: for cone in &future {
                    let lo = cone.aabb().min().floor().as_uvec3();
                    let hi = cone.aabb().max().ceil().as_uvec3();
                    for z in lo.z..=hi.z {
                        for y in lo.y..=hi.y {
                            for x in lo.x..=hi.x {
                                let p = UVec3::new(x, y, z);
                                let center = p.as_vec3() + Vec3::splat(0.5);
                                if y > (position.y * 256.) as u32 + 2
                                    && cone.signed_distance(center) <= 0.
                                    && current.iter().all(|c| c.signed_distance(center) > 0.)
                                {
                                    blocked = Some(p);
                                    break 'find;
                                }
                            }
                        }
                    }
                }
                let p = blocked.context("no future trunk cell for obstruction test")?;
                self.execute_world_edit(WorldEditTransaction::terrain_change(
                    vec![VoxelEdit::RestoreAtlasRegion {
                        origin: p,
                        dim: UVec3::ONE,
                        data: vec![7].into(),
                    }],
                    UAabb3::new(p, p + UVec3::ONE),
                ))?;
                self.advance_tree_growth(
                    (0.5 - SAPLING_AGE) / (1. - SAPLING_AGE) * MATURATION_SECONDS + 0.001,
                )?;
                anyhow::ensure!(
                    self.trees.records[&id].age == SAPLING_AGE
                        && self.trees.records[&id].growth_seconds.is_none(),
                    "obstructed tree grew anyway"
                );
                anyhow::ensure!(
                    self.plain_builder.read_chunk_atlas_region(p, UVec3::ONE)?[0] & 15 == 7,
                    "growth erased construction"
                );
                log::info!("[TREE_GROWTH_REVIEW] obstruction_preserved=true verified=true");
            }
            let expected_age = if mode == "override" {
                let original = self.debug_settings.adjustables.tree_age.value;
                self.debug_settings.adjustables.tree_age.value = 0.75;
                let result = self.update_all_tree_ages_from_gui();
                self.debug_settings.adjustables.tree_age.value = original;
                result?;
                self.advance_tree_growth(900.)?;
                anyhow::ensure!(
                    self.trees.records[&id].growth_seconds.is_none(),
                    "manual age did not freeze clock"
                );
                log::info!("[TREE_GROWTH_REVIEW] manual_override_freezes=true verified=true");
                0.75
            } else {
                age
            };
            let r = &self.trees.records[&id];
            anyhow::ensure!(
                (r.age - expected_age).abs() < 1. / AGE_STEPS,
                "wrong published age"
            );
            if expected_age < 0.75 {
                anyhow::ensure!(r.fruit_specs.is_empty(), "sapling produced mature apples");
            }
            log::info!("[TREE_GROWTH_REVIEW] age={} height_voxels={:.3} branches={} leaves={} fruit={} verified=true",r.age,r.rest_tree.height_voxels(),r.rest_tree.trunks().len(),r.leaf_render_positions.len(),r.fruit_specs.len());
        }
        self.camera_control.apply_snapshot_mode(true);
        let ground = self
            .trees
            .records
            .values()
            .next()
            .context("review tree missing")?
            .position
            .y;
        let target = Vec3::new(0.82, ground + 0.07, 1.22);
        self.camera_control.set_orbit_focus(target);
        self.tracer
            .set_camera_pose_looking_at(Vec3::new(0.69, ground + 0.15, 1.49), target);
        self.reset_camera_movement_input();
        Ok(())
    }
}

pub(super) fn validate_growth(age: f32, elapsed: Option<f32>) -> Result<()> {
    anyhow::ensure!((0. ..=1.).contains(&age), "invalid individual tree age");
    if let Some(seconds) = elapsed {
        anyhow::ensure!(
            (0. ..=MATURATION_SECONDS).contains(&seconds) && age <= age_at(seconds) + 1e-5,
            "invalid sapling growth clock"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn growth_is_monotonic_bounded_staged_and_reaches_exact_maturity() {
        let ages = (0..=900).map(|s| age_at(s as f32)).collect::<Vec<_>>();
        assert_eq!(ages[0], SAPLING_AGE);
        assert_eq!(ages[900], 1.);
        assert!(ages
            .windows(2)
            .all(|w| w[1] >= w[0] && w[1] - w[0] <= 1. / AGE_STEPS));
        assert_eq!(age_at(1800.), 1.);
        assert_eq!(age_at(37.), SAPLING_AGE);
        assert!(validate_growth(0.5, Some(0.)).is_err());
        assert!(validate_growth(0.5, Some(300.)).is_ok());
    }
    #[test]
    fn real_species_geometry_changes_at_multiple_ages_without_reseeding() {
        for species in crate::tree_gen::TreeSpecies::PLANTABLE {
            let mut desc = species.default_desc();
            desc.branching.seed = 907;
            let trees = [0.25, 0.5, 0.75, 1.].map(|age| Tree::new(desc.at_age(age)));
            assert!(trees
                .windows(2)
                .all(|w| w[1].height_voxels() > w[0].height_voxels()));
            assert!(trees[3].trunks().len() > trees[0].trunks().len());
            assert!(
                trees
                    .iter()
                    .all(|t| !t.relative_leaf_placements().is_empty()),
                "{species:?}"
            );
            desc.leaf_density = 0.;
            assert!(Tree::new(desc.at_age(SAPLING_AGE))
                .relative_leaf_placements()
                .is_empty());
        }
    }
}

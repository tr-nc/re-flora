use super::*;
use serde::{Deserialize, Serialize};

mod retirement_review;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(in crate::app::core) struct TreeSnapshot {
    age: f32,
    fruit_cycle: f32,
    trees: Vec<SavedTree>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct SavedTree {
    id: u32,
    position: [f32; 3],
    mature_desc: TreeDesc,
    #[serde(default)]
    age: Option<f32>,
    #[serde(default)]
    growth_seconds: Option<f32>,
}

pub(in crate::app::core) struct PreparedTreeSnapshot {
    age: f32,
    fruit_cycle: f32,
    publications: Vec<PreparedTreePublication>,
    /// Read old baked wood first, then replace through the ordinary transactional owner.
    upgrades: Vec<(u32, TreeDesc)>,
    next_generation: u64,
}

impl TreeSnapshot {
    pub(in crate::app::core) fn ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.trees.iter().map(|t| t.id)
    }
    pub(in crate::app::core) fn contains_tree(&self, id: u32) -> bool {
        self.trees.iter().any(|t| t.id == id)
    }
    pub(in crate::app::core) fn count(&self) -> usize {
        self.trees.len()
    }

    pub(in crate::app::core) fn has_retired_trees(&self) -> bool {
        self.trees.iter().any(|tree| tree.mature_desc.is_retired())
    }

    pub(in crate::app::core) fn current_species(&self) -> Self {
        let mut current = self.clone();
        for tree in &mut current.trees {
            tree.mature_desc = tree.mature_desc.clone().into_current();
        }
        current
    }

    /// Validate allocation bounds before reconstructing any geometry or touching the garden.
    pub(in crate::app::core) fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            (0.0..=1.0).contains(&self.age) && (0.0..=1.0).contains(&self.fruit_cycle),
            "invalid tree age or fruit cycle"
        );
        anyhow::ensure!(self.trees.len() <= 512, "too many saved trees");
        let mut ids = HashSet::new();
        let mut total_budget = 0u64;
        for tree in &self.trees {
            super::growth::validate_growth(tree.age.unwrap_or(self.age), tree.growth_seconds)?;
            anyhow::ensure!(
                tree.id < u32::MAX && ids.insert(tree.id),
                "invalid or duplicate tree identity"
            );
            let position = Vec3::from_array(tree.position);
            anyhow::ensure!(
                position.is_finite()
                    && position.cmpge(Vec3::ZERO).all()
                    && position
                        .cmplt(
                            (super::super::CHUNK_DIM * super::super::VOXEL_DIM_PER_CHUNK).as_vec3()
                                / 256.0
                        )
                        .all(),
                "tree position outside garden"
            );
            let desc = &tree.mature_desc;
            let value = serde_json::to_value(desc)?;
            validate_finite_description(&value)?;
            let branch = &desc.branching;
            anyhow::ensure!(
                (1..=12).contains(&branch.iterations)
                    && branch.branch_count_min <= branch.branch_count_max
                    && branch.branch_count_max <= 8
                    && desc.subdivision_count_min <= desc.subdivision_count_max
                    && desc.subdivision_count_max <= 64
                    && desc.leaves_size_level <= 12
                    && desc.size > 0.0
                    && desc.size <= 1024.0
                    && (0.0..=1.0).contains(&desc.leaf_density),
                "saved tree description exceeds supported bounds"
            );
            let branching =
                u64::from(branch.branch_count_max + u32::from(branch.continue_main_axis)).max(1);
            let segment_budget =
                if !desc.is_retired() && desc.species.uses_axis_model(desc.axis_model) {
                    // Bounded authored axes do not inherit the recursive generator's exponential cost.
                    crate::tree_gen::MAX_AXIS_SEGMENTS as u64
                } else {
                    branching
                        .checked_pow(branch.iterations)
                        .context("saved tree complexity overflow")?
                };
            let mut budget = segment_budget
                .checked_mul(u64::from(desc.subdivision_count_max.max(1)))
                .context("saved tree complexity overflow")?;
            if desc.is_retired() {
                let replacement = desc.clone().into_current();
                budget = budget
                    .checked_add(
                        crate::tree_gen::MAX_AXIS_SEGMENTS as u64
                            * u64::from(replacement.subdivision_count_max.max(1)),
                    )
                    .context("migrated tree complexity overflow")?;
            }
            total_budget = total_budget
                .checked_add(budget)
                .context("garden tree complexity overflow")?;
            anyhow::ensure!(
                total_budget <= 2_000_000,
                "saved trees exceed reconstruction budget"
            );
        }
        Ok(())
    }

    fn prepare(&self, generation_start: u64, sample_budget: usize) -> Result<PreparedTreeSnapshot> {
        self.validate()?;
        let next_generation = generation_start
            .checked_add(self.trees.len() as u64)
            .context("saved tree canopy generation overflow")?;
        let publications = self
            .trees
            .iter()
            .enumerate()
            .map(|(index, saved)| {
                let original = saved.mature_desc.saved_geometry_desc();
                let compiled = TreePlacementService::compile(
                    original.clone(),
                    Vec3::from_array(saved.position),
                    UAabb3::default(),
                    saved.age.unwrap_or(self.age),
                    generation_start + index as u64,
                    sample_budget,
                );
                let mut publication = PreparedTreePublication::new(saved.id, original, compiled);
                publication.record.growth_seconds = saved.growth_seconds;
                publication
            })
            .collect();
        Ok(PreparedTreeSnapshot {
            age: self.age,
            fruit_cycle: self.fruit_cycle,
            publications,
            upgrades: self
                .trees
                .iter()
                .filter(|saved| saved.mature_desc.is_retired())
                .map(|saved| (saved.id, saved.mature_desc.clone().into_current()))
                .collect(),
            next_generation,
        })
    }
}

impl GardenTrees {
    /// The terrain owner has already restored the authoritative saved atlas. This
    /// owner replaces the entire observer/surface forest with one final canonical commit.
    fn restore_snapshot(
        &mut self,
        snapshot: PreparedTreeSnapshot,
        host: &mut impl TreePublicationPrimitiveHost,
    ) -> Result<()> {
        let by_id: HashMap<_, _> = snapshot
            .publications
            .iter()
            .map(|p| (p.tree_id, p))
            .collect();
        let mut ids: Vec<_> = self
            .records
            .keys()
            .copied()
            .chain(by_id.keys().copied())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        let mut executor = TreePublicationExecutor::new(host);
        executor.prepare_retained_trunks(&[])?;
        for &id in &ids {
            let previous = self.records.get(&id);
            let publication = by_id.get(&id).copied();
            let operation = if publication.is_none() {
                TreePublicationOperation::Remove
            } else if previous.is_some() {
                TreePublicationOperation::Replace
            } else {
                TreePublicationOperation::Place
            };
            executor.prepare(operation, id, publication, previous)?;
        }
        let mut forest: Vec<_> = snapshot
            .publications
            .iter()
            .map(|p| (p.tree_id, &p.record))
            .collect();
        forest.sort_unstable_by_key(|(id, _)| *id);
        let revision = self
            .canonical_revision
            .wrapping_add(self.records.len() as u64)
            .wrapping_add(snapshot.publications.len() as u64);
        let result: Result<()> = (|| {
            executor.prepare_surface(&forest, revision)?;
            // Remove every old fruit registry entry without destroying checkpointed bodies.
            for &id in self.records.keys() {
                executor.execute(TreePublicationAction::RemoveLeaves, id, None)?;
                executor.execute(TreePublicationAction::RemoveFruitLifecycle, id, None)?;
                executor.execute(TreePublicationAction::RemoveCanopyAudio, id, None)?;
            }
            executor
                .host
                .prepare_restored_fruit_cycle(snapshot.fruit_cycle)?;
            for publication in &snapshot.publications {
                for &action in TreePublicationPlan::RESTORE_ACTIONS {
                    executor.execute(action, publication.tree_id, Some(publication))?;
                }
            }
            executor.execute(TreePublicationAction::CommitPublication, 0, None)?;
            Ok(())
        })();
        if let Err(error) = result {
            if ids.is_empty() {
                return Err(error).context("preparing empty saved forest");
            }
            return match executor.compensate() {
                Ok(()) => Err(error).context("restoring complete tree forest"),
                Err(restore) => Err(anyhow::anyhow!(
                    "tree restoration failed: {error:#}; compensation failed: {restore:#}"
                )),
            };
        }
        let old_ids: Vec<_> = self.records.keys().copied().collect();
        for id in old_ids {
            self.commit_removal(id);
        }
        self.previous_bound = UAabb3::default();
        self.staged_tuned_mature_desc = None;
        self.next_tree_id = snapshot
            .publications
            .iter()
            .map(|p| p.tree_id + 1)
            .max()
            .unwrap_or(1)
            .max(1);
        self.next_canopy_acoustic_generation = snapshot.next_generation;
        for publication in snapshot.publications {
            self.commit_placement(publication.tree_id, publication.record);
        }
        debug_assert_eq!(self.canonical_revision, revision);
        Ok(())
    }
}

fn validate_finite_description(value: &serde_json::Value) -> Result<()> {
    match value {
        serde_json::Value::Null => anyhow::bail!("non-finite tree description number"),
        serde_json::Value::Number(value) if value.is_f64() => anyhow::ensure!(
            value
                .as_f64()
                .is_some_and(|number| number.is_finite() && number.abs() <= 1_000_000.0),
            "invalid tree description number"
        ),
        serde_json::Value::Object(table) => {
            for value in table.values() {
                validate_finite_description(value)?;
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                validate_finite_description(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}

impl App {
    pub(in crate::app::core) fn prepare_tree_snapshot(
        &self,
        snapshot: &TreeSnapshot,
    ) -> Result<PreparedTreeSnapshot> {
        snapshot.prepare(
            self.trees.next_canopy_acoustic_generation,
            self.debug_settings
                .adjustables
                .canopy_audio_sample_budget
                .value as usize,
        )
    }

    pub(in crate::app::core) fn verify_restored_tree_rendering(
        &self,
        snapshot: &TreeSnapshot,
    ) -> Result<()> {
        let expected = self.prepare_tree_snapshot(&snapshot.current_species())?;
        anyhow::ensure!(
            self.surface_builder
                .resources
                .instances
                .leaves_instances
                .len()
                == expected.publications.len(),
            "restored leaf resource count differs"
        );
        for publication in expected.publications {
            let rendered = self
                .surface_builder
                .resources
                .instances
                .leaves_instances
                .get(&publication.tree_id)
                .context("saved tree has no leaf draw resource")?;
            anyhow::ensure!(
                rendered.resources.instances_len as usize
                    == publication.record.leaf_render_positions.len(),
                "restored leaf draw count differs"
            );
            let actual = self
                .trees
                .records
                .get(&publication.tree_id)
                .context("saved tree has no canonical record")?;
            anyhow::ensure!(
                actual.leaf_render_positions == publication.record.leaf_render_positions
                    && actual.leaf_render_local_positions
                        == publication.record.leaf_render_local_positions,
                "restored leaf positions/anchors differ"
            );
        }
        Ok(())
    }

    pub(in crate::app::core) fn capture_tree_snapshot(&self) -> TreeSnapshot {
        let mut trees = self
            .trees
            .records
            .iter()
            .map(|(&id, record)| SavedTree {
                id,
                position: record.position.to_array(),
                mature_desc: record.mature_desc.clone(),
                age: Some(record.age),
                growth_seconds: record.growth_seconds,
            })
            .collect::<Vec<_>>();
        trees.sort_by_key(|tree| tree.id);
        TreeSnapshot {
            age: self.debug_settings.adjustables.tree_age.value,
            fruit_cycle: self.debug_settings.adjustables.fruit_cycle.value,
            trees,
        }
    }

    /// Current species restore observers without restamping hand-edited wood. Retired test trees
    /// are first bound to their saved geometry, then replaced through the same transaction used
    /// by ordinary tree editing; merely relabeling them would leave invisible old wood behind.
    pub(in crate::app::core) fn restore_tree_snapshot(
        &mut self,
        mut snapshot: PreparedTreeSnapshot,
    ) -> Result<()> {
        let age = snapshot.age;
        let fruit_cycle = snapshot.fruit_cycle;
        let upgrades = std::mem::take(&mut snapshot.upgrades);
        self.transact_garden_trees(|trees, host| trees.restore_snapshot(snapshot, host))?;
        self.debug_settings.adjustables.tree_age.value = age;
        self.debug_settings.adjustables.fruit_cycle.value = fruit_cycle;
        if !upgrades.is_empty() {
            let count = upgrades.len();
            let records = upgrades
                .into_iter()
                .map(|(id, desc)| {
                    let mut record = self
                        .trees
                        .records
                        .get(&id)
                        .context("retired tree missing after snapshot restore")?
                        .clone();
                    record.mature_desc = desc;
                    Ok((id, record))
                })
                .collect::<Result<Vec<_>>>()?;
            self.rebuild_tree_records(records)?;
            log::info!("[TREE_MIGRATION] retired_test_trees={count} replacement=AppleTree positions_ids_seeds=retained old_wood=replaced source_file=unchanged");
        }
        if let Some(record) = self.trees.tuned_record() {
            self.debug_settings.tree.desc = record.mature_desc.clone();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> TreeSnapshot {
        let mut desc = TreeDesc::default();
        desc.branching.seed = u64::MAX;
        desc.branching.iterations = 3;
        TreeSnapshot {
            age: 0.37,
            fruit_cycle: 0.6,
            trees: vec![SavedTree {
                id: 7,
                position: [0.5, 0.5, 0.5],
                mature_desc: desc,
                age: None,
                growth_seconds: None,
            }],
        }
    }

    #[test]
    fn individual_ages_and_growth_clocks_round_trip_without_resetting_legacy_trees() {
        let legacy = fixture();
        let old = legacy.prepare(1, 8).unwrap();
        assert_eq!(old.publications[0].record.age, 0.37);
        assert!(old.publications[0].record.growth_seconds.is_none());
        let mut saved = legacy;
        saved.trees[0].age = Some(0.5);
        saved.trees[0].growth_seconds = Some(300.);
        let mut adult = saved.trees[0].clone();
        adult.id = 8;
        adult.age = Some(1.);
        adult.growth_seconds = None;
        saved.trees.push(adult);
        let decoded: TreeSnapshot =
            serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
        let restored = decoded.prepare(10, 8).unwrap();
        assert_eq!(restored.publications[0].record.age, 0.5);
        assert_eq!(restored.publications[0].record.growth_seconds, Some(300.));
        assert_eq!(restored.publications[1].record.age, 1.);
        assert!(restored.publications[1].record.growth_seconds.is_none());
        saved.trees[0].growth_seconds = Some(-1.);
        assert!(saved.validate().is_err());
    }

    #[test]
    fn tree_snapshot_reconstructs_the_same_leaves_at_saved_age() {
        let snapshot = fixture();
        let decoded: TreeSnapshot =
            serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
        assert_eq!(snapshot, decoded);
        let first = snapshot
            .prepare(1, CanopyAcousticDescriptor::DEFAULT_SAMPLE_BUDGET)
            .unwrap();
        let second = decoded
            .prepare(10, CanopyAcousticDescriptor::DEFAULT_SAMPLE_BUDGET)
            .unwrap();
        assert_eq!(first.next_generation, 2);
        assert_eq!(second.next_generation, 11);
        assert_eq!(
            first.publications[0].record.leaf_render_positions,
            second.publications[0].record.leaf_render_positions
        );
        assert_eq!(
            first.publications[0].record.leaf_render_local_positions,
            second.publications[0].record.leaf_render_local_positions
        );
        assert_eq!(
            first.publications[0].record.fruit_specs,
            second.publications[0].record.fruit_specs
        );
        assert_eq!(first.publications[0].tree_id, 7);
    }

    #[test]
    fn mixed_species_snapshot_preserves_identity_seed_and_both_architectures() {
        let mut snapshot = fixture();
        for (id, axis_model) in [(8, true), (9, false)] {
            let mut desc = crate::tree_gen::TreeSpecies::Conifer.mature_desc(&TreeDesc::default());
            desc.branching.seed = 811 + id as u64;
            desc.axis_model = axis_model;
            snapshot.trees.push(SavedTree {
                id,
                position: [0.5 + (id - 7) as f32 * 0.5, 0.5, 0.5],
                mature_desc: desc,
                age: None,
                growth_seconds: None,
            });
        }
        for (index, species) in crate::tree_gen::TreeSpecies::PLANTABLE
            .into_iter()
            .enumerate()
        {
            let mut desc = species.mature_desc(&TreeDesc::default());
            desc.branching.seed = 900 + index as u64;
            snapshot.trees.push(SavedTree {
                id: 10 + index as u32,
                position: [0.5 + index as f32 * 0.5, 0.5, 1.5],
                mature_desc: desc,
                age: None,
                growth_seconds: None,
            });
        }
        let decoded: TreeSnapshot =
            serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
        assert_eq!(snapshot, decoded);
        let prepared = decoded
            .prepare(1, CanopyAcousticDescriptor::DEFAULT_SAMPLE_BUDGET)
            .unwrap();
        for (saved, published) in snapshot.trees.iter().zip(prepared.publications.iter()) {
            assert_eq!(saved.mature_desc, published.record.mature_desc);
            assert_eq!(saved.id, published.tree_id);
            let expected = Tree::new(saved.mature_desc.at_age(saved.age.unwrap_or(snapshot.age)));
            assert_eq!(expected.branches(), published.record.rest_tree.branches());
            if !saved.mature_desc.species.bears_apples() {
                assert!(published.record.fruit_specs.is_empty());
            }
            assert_eq!(
                !published.record.rest_tree.axis_segments().is_empty(),
                saved
                    .mature_desc
                    .species
                    .uses_axis_model(saved.mature_desc.axis_model)
            );
            assert_eq!(
                published.record.leaf_render_positions,
                TreePlacementService::compile(
                    saved.mature_desc.clone(),
                    Vec3::from_array(saved.position),
                    UAabb3::default(),
                    saved.age.unwrap_or(snapshot.age),
                    1,
                    8
                )
                .quantized_leaf_render_positions
            );
        }
    }

    #[test]
    fn retired_descriptions_prepare_saved_wood_then_an_in_place_apple_upgrade() {
        for explicit_species in [false, true] {
            let mut snapshot = fixture();
            let mut retired = TreeDesc::legacy_wire_defaults();
            retired.size = 17.25;
            retired.branching.seed = u64::MAX;
            retired.branching.iterations = 3;
            retired.axis_model = true; // Old Garden Trees ignored the Conifer-only toggle.
            snapshot.trees[0].mature_desc = retired.clone();
            let mut json = serde_json::to_value(&snapshot).unwrap();
            if !explicit_species {
                json["trees"][0]["mature_desc"]
                    .as_object_mut()
                    .unwrap()
                    .remove("species");
            }
            let decoded: TreeSnapshot = serde_json::from_value(json).unwrap();
            assert!(decoded.has_retired_trees());
            let prepared = decoded.prepare(10, 8).unwrap();
            let original = &prepared.publications[0];
            assert_eq!(original.tree_id, 7);
            assert_eq!(original.record.position, Vec3::splat(0.5));
            assert!(original.record.rest_tree.axis_segments().is_empty());
            assert_eq!(original.record.mature_desc.branching.seed, u64::MAX);
            let current = decoded.current_species();
            assert!(!current.has_retired_trees());
            assert_eq!(current.age, snapshot.age);
            assert_eq!(current.fruit_cycle, snapshot.fruit_cycle);
            assert_eq!(current.trees[0].id, snapshot.trees[0].id);
            assert_eq!(current.trees[0].position, snapshot.trees[0].position);
            assert_eq!(
                prepared.upgrades,
                vec![(7, current.trees[0].mature_desc.clone())]
            );
            let apple = &current.trees[0].mature_desc;
            assert_eq!(apple.species, crate::tree_gen::TreeSpecies::AppleTree);
            assert_eq!(apple.size, retired.size);
            assert_eq!(apple.branching.seed, retired.branching.seed);
            assert!(!Tree::new(apple.at_age(current.age))
                .axis_segments()
                .is_empty());
            assert!(!serde_json::to_string(&current)
                .unwrap()
                .contains("garden_tree"));
        }
    }

    #[test]
    fn retirement_leaves_all_other_saved_species_and_authored_fields_unchanged() {
        let mut snapshot = fixture();
        snapshot.trees[0].mature_desc = TreeDesc::legacy_wire_defaults();
        for (index, (species, axes)) in [
            (crate::tree_gen::TreeSpecies::Conifer, false),
            (crate::tree_gen::TreeSpecies::Conifer, true),
            (crate::tree_gen::TreeSpecies::AppleTree, true),
            (crate::tree_gen::TreeSpecies::Pine, true),
            (crate::tree_gen::TreeSpecies::WeepingWillow, true),
        ]
        .into_iter()
        .enumerate()
        {
            let mut desc = species.mature_desc(&TreeDesc::default());
            desc.axis_model = axes;
            desc.branching.seed = index as u64 + 900;
            desc.size = 17.25;
            desc.trunk_thickness = 0.25;
            snapshot.trees.push(SavedTree {
                id: index as u32 + 8,
                position: [0.75, 0.5, 0.75],
                mature_desc: desc,
                age: None,
                growth_seconds: None,
            });
        }
        let prepared = snapshot.prepare(1, 8).unwrap();
        assert_eq!(prepared.upgrades.len(), 1);
        assert_eq!(snapshot.current_species().trees[1..], snapshot.trees[1..]);
        for (saved, original) in snapshot.trees[1..].iter().zip(&prepared.publications[1..]) {
            assert_eq!(saved.id, original.tree_id);
            assert_eq!(saved.mature_desc, original.record.mature_desc);
        }
    }

    #[test]
    fn retired_snapshot_bounds_the_old_geometry_as_well_as_the_replacement() {
        let mut snapshot = fixture();
        snapshot.trees[0].mature_desc = TreeDesc::legacy_wire_defaults();
        snapshot.trees[0].mature_desc.branching.iterations = 8;
        snapshot.trees[0].mature_desc.branching.branch_count_min = 8;
        snapshot.trees[0].mature_desc.branching.branch_count_max = 8;
        assert!(snapshot.validate().is_err());
    }

    #[test]
    fn invalid_tree_data_is_rejected_before_compilation() {
        let mut snapshot = fixture();
        snapshot.age = f32::NAN;
        assert!(snapshot.validate().is_err());
        snapshot.age = 1.0;
        snapshot.trees[0].mature_desc.branching.iterations = u32::MAX;
        assert!(snapshot.validate().is_err());
        snapshot.trees[0].mature_desc.branching.iterations = 3;
        snapshot.trees[0].mature_desc.size = f32::NAN;
        assert!(snapshot.validate().is_err());
    }
}

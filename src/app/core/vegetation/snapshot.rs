use super::*;
use serde::{Deserialize, Serialize};

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
}

pub(in crate::app::core) struct PreparedTreeSnapshot {
    age: f32,
    fruit_cycle: f32,
    publications: Vec<PreparedTreePublication>,
    next_generation: u64,
}

impl TreeSnapshot {
    pub(in crate::app::core) fn count(&self) -> usize {
        self.trees.len()
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
            let budget = branching
                .checked_pow(branch.iterations)
                .and_then(|n| n.checked_mul(u64::from(desc.subdivision_count_max.max(1))))
                .context("saved tree complexity overflow")?;
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

    fn prepare(&self, generation_start: u64) -> Result<PreparedTreeSnapshot> {
        self.validate()?;
        let next_generation = generation_start
            .checked_add(self.trees.len() as u64)
            .context("saved tree canopy generation overflow")?;
        let publications = self
            .trees
            .iter()
            .enumerate()
            .map(|(index, saved)| {
                let compiled = TreePlacementService::compile(
                    saved.mature_desc.clone(),
                    Vec3::from_array(saved.position),
                    UAabb3::default(),
                    self.age,
                    generation_start + index as u64,
                );
                PreparedTreePublication::new(saved.id, saved.mature_desc.clone(), compiled)
            })
            .collect();
        Ok(PreparedTreeSnapshot {
            age: self.age,
            fruit_cycle: self.fruit_cycle,
            publications,
            next_generation,
        })
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
        snapshot.prepare(self.trees.next_canopy_acoustic_generation)
    }

    pub(in crate::app::core) fn verify_restored_tree_rendering(
        &self,
        snapshot: &TreeSnapshot,
    ) -> Result<()> {
        let expected = self.prepare_tree_snapshot(snapshot)?;
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
            })
            .collect::<Vec<_>>();
        trees.sort_by_key(|tree| tree.id);
        TreeSnapshot {
            age: self.debug_settings.adjustables.tree_age.value,
            fruit_cycle: self.debug_settings.adjustables.fruit_cycle.value,
            trees,
        }
    }

    /// The atlas already contains the saved, possibly hand-edited wood. Publish only observers
    /// and canonical tree ownership, never stamp/clear trunk geometry during restoration.
    pub(in crate::app::core) fn restore_tree_snapshot(
        &mut self,
        snapshot: PreparedTreeSnapshot,
    ) -> Result<()> {
        self.transact_garden_trees(|trees, host| {
            let old_records = trees.records.clone();
            for (&id, record) in &old_records {
                host.prepare(TreePublicationOperation::Remove, id, None, Some(record))?;
                host.remove_leaves(id)?;
                host.remove_fruit_lifecycle(id)?;
                host.remove_canopy_audio(id)?;
                trees.commit_removal(id);
            }
            host.commit_publication()?;
            host.app
                .terrain_physics
                .set_fruit_cycle(snapshot.fruit_cycle, &mut host.app.tracer)?;
            trees.previous_bound = UAabb3::default();
            trees.staged_tuned_mature_desc = None;
            trees.next_tree_id = snapshot
                .publications
                .iter()
                .map(|p| p.tree_id + 1)
                .max()
                .unwrap_or(1)
                .max(1);
            trees.next_canopy_acoustic_generation = snapshot.next_generation;
            for publication in snapshot.publications {
                let id = publication.tree_id;
                host.publish_leaves(id, &publication)?;
                host.app.terrain_physics.restore_saved_tree_fruits(
                    id,
                    publication.record.fruit_specs.clone(),
                    &mut host.app.tracer,
                )?;
                host.publish_attached_fruit(id)?;
                host.publish_canopy_audio(id, &publication)?;
                trees.commit_placement(id, publication.record);
            }
            host.app
                .tracer
                .invalidate_local_direct_sun_shadow_histories();
            Ok(())
        })?;
        self.debug_settings.adjustables.tree_age.value = snapshot.age;
        self.debug_settings.adjustables.fruit_cycle.value = snapshot.fruit_cycle;
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
            }],
        }
    }

    #[test]
    fn tree_snapshot_reconstructs_the_same_leaves_at_saved_age() {
        let snapshot = fixture();
        let decoded: TreeSnapshot =
            serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
        assert_eq!(snapshot, decoded);
        let first = snapshot.prepare(1).unwrap();
        let second = decoded.prepare(10).unwrap();
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

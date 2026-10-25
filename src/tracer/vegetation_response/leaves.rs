//! Immutable leaf input layout. CPU publication only follows stream changes;
//! wind/held poses/flutter history stay entirely in the GPU response buffers.
use super::{ResponseInput, NO_PREVIOUS};
use crate::builder::{TreeLeafInstance, TreeLeavesInstance};
use anyhow::Result;
use glam::UVec3;
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct LeafKey {
    tree: u32,
    world: UVec3,
    anchor_local: u32,
}
struct Source {
    origin: UVec3,
    instances: Arc<[TreeLeafInstance]>,
}
#[derive(Default)]
pub(super) struct LeafResponseLayout {
    sources: BTreeMap<u32, Source>,
    indices: HashMap<LeafKey, u32>,
    offsets: HashMap<u32, u32>,
    inputs: Vec<ResponseInput>,
    base: usize,
    revision: u64,
    pending_indices: Option<HashMap<LeafKey, u32>>,
}
impl LeafResponseLayout {
    /// Pointer identity is safe: retaining the immutable Arc prevents address
    /// reuse. Equal counts or a recycled Vulkan handle are not validity keys.
    pub(super) fn update(
        &mut self,
        base: usize,
        trees: &HashMap<u32, TreeLeavesInstance>,
    ) -> Result<bool> {
        if self.matches(
            base,
            trees.iter().map(|(&id, tree)| {
                (
                    id,
                    tree.chunk_world_offset,
                    &tree.resources.response_instances,
                )
            }),
        ) {
            return Ok(self.pending_indices.is_some());
        }
        let sources: BTreeMap<_, _> = trees
            .iter()
            .map(|(&id, tree)| {
                (
                    id,
                    Source {
                        origin: tree.chunk_world_offset,
                        instances: tree.resources.response_instances.clone(),
                    },
                )
            })
            .collect();
        for tree in trees.values() {
            anyhow::ensure!(
                tree.resources.response_instances.len() == tree.resources.instances_len as usize,
                "leaf response/draw stream mismatch"
            );
        }
        let mut inputs = Vec::new();
        let mut next = HashMap::new();
        let mut offsets = HashMap::new();
        for (&id, source) in &sources {
            let offset = u32::try_from(
                base.checked_add(inputs.len())
                    .ok_or_else(|| anyhow::anyhow!("leaf response index overflow"))?,
            )?;
            offsets.insert(id, offset);
            append_tree(
                &mut inputs,
                base,
                id,
                source.origin,
                &source.instances,
                &self.indices,
                &mut next,
            )?;
        }
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("leaf layout revision overflow"))?;
        self.base = base;
        self.sources = sources;
        self.pending_indices = Some(next);
        self.offsets = offsets;
        self.inputs = inputs;
        if trace_publications() {
            log::info!(
                "[VEGETATION_RESPONSE][LEAF_LAYOUT] revision={} leaves={} base={} rebuilt=true",
                self.revision,
                self.inputs.len(),
                base
            );
        }
        Ok(true)
    }
    fn matches<'a>(
        &self,
        base: usize,
        mut sources: impl ExactSizeIterator<Item = (u32, UVec3, &'a Arc<[TreeLeafInstance]>)>,
    ) -> bool {
        self.revision != 0
            && self.base == base
            && self.sources.len() == sources.len()
            && sources.all(|(id, origin, instances)| {
                self.sources.get(&id).is_some_and(|old| {
                    old.origin == origin && Arc::ptr_eq(&old.instances, instances)
                })
            })
    }
    pub(super) fn published(&mut self) {
        if let Some(indices) = self.pending_indices.take() {
            self.indices = indices;
        }
    }
    pub(super) fn inputs(&self) -> &[ResponseInput] {
        &self.inputs
    }
    pub(super) fn base(&self) -> usize {
        self.base
    }
    pub(super) fn revision(&self) -> u64 {
        self.revision
    }
    pub(super) fn offset(&self, id: u32) -> u32 {
        *self
            .offsets
            .get(&id)
            .expect("published layout must cover tree draw")
    }
}
pub(super) fn trace_publications() -> bool {
    std::env::var_os("RE_FLORA_TREE_SCALE_DIAGNOSTIC").is_some()
        || std::env::var_os("RE_FLORA_VEGETATION_RESPONSE_VALIDATE").is_some()
}
fn append_tree(
    inputs: &mut Vec<ResponseInput>,
    base: usize,
    tree: u32,
    origin: UVec3,
    leaves: &[TreeLeafInstance],
    previous: &HashMap<LeafKey, u32>,
    next: &mut HashMap<LeafKey, u32>,
) -> Result<()> {
    for leaf in leaves {
        let packed = leaf.packed_local_pos;
        let world =
            origin + UVec3::new(packed & 1023, (packed >> 10) & 1023, (packed >> 20) & 1023);
        let key = LeafKey {
            tree,
            world,
            anchor_local: leaf.packed_leaf_local_pos,
        };
        next.insert(
            key,
            u32::try_from(
                base.checked_add(inputs.len())
                    .ok_or_else(|| anyhow::anyhow!("leaf response index overflow"))?,
            )?,
        );
        let root = world.as_vec3() / 256.;
        inputs.push(ResponseInput {
            root: [root.x, root.y, root.z, 0.],
            identity: [
                previous.get(&key).copied().unwrap_or(NO_PREVIOUS),
                crate::flora::species::TREE_LEAF_RENDER_SPECIES_INDEX,
                instance_seed(world),
                1,
            ],
        });
    }
    Ok(())
}
/// Mirrors flora_types.slang::instanceSeed.
pub(in crate::tracer) fn instance_seed(world: UVec3) -> u32 {
    let mut seed = world.x ^ (world.y << 10) ^ (world.z << 20);
    seed ^= seed >> 16;
    seed ^= seed << 5;
    seed ^ (seed >> 11)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn source(x: u32) -> Arc<[TreeLeafInstance]> {
        Arc::from([TreeLeafInstance {
            packed_local_pos: x,
            packed_leaf_local_pos: 123,
        }])
    }
    #[test]
    fn unchanged_sources_only_check_stream_metadata_not_individual_leaves() {
        let instances = source(10);
        let mut layout = LeafResponseLayout {
            revision: 1,
            base: 7,
            ..Default::default()
        };
        layout.sources.insert(
            2,
            Source {
                origin: UVec3::ZERO,
                instances: instances.clone(),
            },
        );
        assert!(layout.matches(7, [(2, UVec3::ZERO, &instances)].into_iter()));
        assert!(
            !layout.matches(8, [(2, UVec3::ZERO, &instances)].into_iter()),
            "prefix change"
        );
        assert!(
            !layout.matches(7, [(2, UVec3::X, &instances)].into_iter()),
            "placement change"
        );
        assert!(
            !layout.matches(7, [(3, UVec3::ZERO, &instances)].into_iter()),
            "replacement id"
        );
        assert!(
            !layout.matches(7, [(2, UVec3::ZERO, &source(10))].into_iter()),
            "new equal-sized stream"
        );
        assert!(!layout.matches(7, std::iter::empty()), "removal");
        layout.pending_indices = Some(HashMap::new());
        layout.published();
        assert!(layout.pending_indices.is_none());
        assert!(layout.matches(7, [(2, UVec3::ZERO, &instances)].into_iter()));
    }
    #[test]
    fn publication_remaps_only_changes_and_never_confuses_replant_or_equal_count() {
        let first = source(10);
        let second = source(20);
        let mut previous = HashMap::new();
        let mut inputs = Vec::new();
        append_tree(
            &mut inputs,
            7,
            2,
            UVec3::ZERO,
            &first,
            &HashMap::new(),
            &mut previous,
        )
        .unwrap();
        append_tree(
            &mut inputs,
            7,
            5,
            UVec3::ZERO,
            &second,
            &HashMap::new(),
            &mut previous,
        )
        .unwrap();
        assert_eq!(inputs[0].identity[0], NO_PREVIOUS);
        inputs.clear();
        let mut next = HashMap::new();
        append_tree(
            &mut inputs,
            11,
            5,
            UVec3::ZERO,
            &second,
            &previous,
            &mut next,
        )
        .unwrap();
        assert_eq!(inputs[0].identity[0], 8);
        inputs.clear();
        append_tree(
            &mut inputs,
            11,
            2,
            UVec3::ZERO,
            &source(11),
            &previous,
            &mut next,
        )
        .unwrap();
        assert_eq!(inputs[0].identity[0], NO_PREVIOUS);
        inputs.clear();
        append_tree(
            &mut inputs,
            11,
            9,
            UVec3::ZERO,
            &first,
            &previous,
            &mut next,
        )
        .unwrap();
        assert_eq!(inputs[0].identity[0], NO_PREVIOUS);
    }
}

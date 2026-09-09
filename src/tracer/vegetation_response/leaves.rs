//! Leaf response ownership follows the visible voxel stream, not shadow proxies
//! or leaf-spray anchors. Repacking and draw order never define a leaf lifetime.
use super::{ResponseInput, NO_PREVIOUS};
use crate::builder::{TreeLeafInstance, TreeLeavesInstance};
use anyhow::Result;
use glam::UVec3;
use std::collections::HashMap;

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct LeafKey {
    tree: u32,
    world: UVec3,
    anchor_local: u32,
}

#[derive(Default)]
pub(super) struct LeafResponses {
    previous: HashMap<LeafKey, u32>,
    offsets: HashMap<u32, u32>,
}

impl LeafResponses {
    pub(super) fn append(
        &mut self,
        inputs: &mut Vec<ResponseInput>,
        trees: &HashMap<u32, TreeLeavesInstance>,
        reset: bool,
    ) -> Result<()> {
        if reset {
            self.previous.clear();
        }
        let mut next = HashMap::with_capacity(self.previous.len());
        self.offsets.clear();
        for (&tree_id, tree) in trees {
            let leaves = &tree.resources.response_instances;
            anyhow::ensure!(
                leaves.len() == tree.resources.instances_len as usize,
                "leaf response/draw stream mismatch for tree {tree_id}"
            );
            self.offsets.insert(tree_id, inputs.len() as u32);
            append_tree(
                inputs,
                tree_id,
                tree.chunk_world_offset,
                leaves,
                &self.previous,
                &mut next,
            );
        }
        if next.len() != self.previous.len() {
            log::info!(
                "[VEGETATION_RESPONSE][LEAVES] individual_states={} retained={} state_count={}",
                next.len(),
                next.keys()
                    .filter(|key| self.previous.contains_key(key))
                    .count(),
                inputs.len()
            );
        }
        self.previous = next;
        Ok(())
    }

    pub(super) fn offset(&self, tree_id: u32) -> u32 {
        *self
            .offsets
            .get(&tree_id)
            .expect("leaf response prepass must cover every tree draw")
    }
}

fn append_tree(
    inputs: &mut Vec<ResponseInput>,
    tree: u32,
    origin: UVec3,
    leaves: &[TreeLeafInstance],
    previous: &HashMap<LeafKey, u32>,
    next: &mut HashMap<LeafKey, u32>,
) {
    for leaf in leaves {
        let packed = leaf.packed_local_pos;
        let world =
            origin + UVec3::new(packed & 1023, (packed >> 10) & 1023, (packed >> 20) & 1023);
        let key = LeafKey {
            tree,
            world,
            anchor_local: leaf.packed_leaf_local_pos,
        };
        let root = world.as_vec3() / 256.;
        next.insert(key, inputs.len() as u32);
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
}

/// Matches flora_types.slang::instanceSeed, including wrapping 32-bit shifts.
pub(in crate::tracer) fn instance_seed(world: UVec3) -> u32 {
    let mut seed = world.x ^ (world.y << 10) ^ (world.z << 20);
    seed ^= seed >> 16;
    seed ^= seed << 5;
    seed ^ (seed >> 11)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaf_states_follow_world_identity_across_repacking_reorder_and_replant() {
        let leaf = |x| TreeLeafInstance {
            packed_local_pos: x,
            packed_leaf_local_pos: 123,
        };
        let mut inputs = vec![];
        let mut previous = HashMap::new();
        append_tree(
            &mut inputs,
            7,
            UVec3::ZERO,
            &[leaf(10), leaf(20)],
            &HashMap::new(),
            &mut previous,
        );
        assert!(inputs.iter().all(|input| input.identity[0] == NO_PREVIOUS));
        inputs.clear();
        let mut next = HashMap::new();
        append_tree(
            &mut inputs,
            7,
            UVec3::X * 5,
            &[leaf(15), leaf(25), leaf(5)],
            &previous,
            &mut next,
        );
        assert_eq!(
            inputs.iter().map(|i| i.identity[0]).collect::<Vec<_>>(),
            [1, NO_PREVIOUS, 0]
        );
        assert_ne!(inputs[0].identity[2], inputs[2].identity[2]);
        inputs.clear();
        append_tree(
            &mut inputs,
            8,
            UVec3::ZERO,
            &[leaf(10)],
            &previous,
            &mut HashMap::new(),
        );
        assert_eq!(inputs[0].identity[0], NO_PREVIOUS);
        inputs.clear();
        append_tree(
            &mut inputs,
            7,
            UVec3::ZERO,
            &[leaf(10)],
            &HashMap::new(),
            &mut HashMap::new(),
        );
        assert_eq!(inputs[0].identity[0], NO_PREVIOUS); // Removed in an intervening frame.
    }
}

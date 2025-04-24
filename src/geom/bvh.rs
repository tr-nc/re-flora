use super::Aabb;
use ordered_float::OrderedFloat;

#[derive(Debug, Clone)]
pub struct BvhNode {
    pub aabb: Aabb,
    pub left: u32,
    pub right: u32,
    /// For leaf nodes, the original index of the AABB in the input slice.
    /// For internal nodes, this is zero.
    pub data_offset: u32,
}

/// Build a BVH from a slice of AABBs.
/// Returns a `Vec<BvhNode>` where the root node is at index 0.
pub fn build_bvh(aabbs: &[Aabb]) -> Vec<BvhNode> {
    // Pair each AABB with its original index.
    let mut chunks: Vec<(Aabb, u32)> = aabbs
        .iter()
        .cloned()
        .enumerate()
        .map(|(i, aabb)| (aabb, i as u32))
        .collect();

    let mut nodes = Vec::new();
    let chunks_len = chunks.len();
    build_bvh_recursive(&mut chunks, &mut nodes, 0, chunks_len);
    nodes
}

/// Recursively builds the BVH over `chunks[start..end]`.
/// Returns the index of the newly created node in `nodes`.
fn build_bvh_recursive(
    aabb_idx_pair: &mut [(Aabb, u32)],
    nodes: &mut Vec<BvhNode>,
    start: usize,
    end: usize,
) -> usize {
    let count = end - start;

    // 1) Compute union AABB by hand (so we don't have to change `Aabb::get_union_aabb`).
    let mut union_aabb = aabb_idx_pair[start].0.clone();
    for i in (start + 1)..end {
        union_aabb = union_aabb.union(&aabb_idx_pair[i].0);
    }

    // 2) Figure out if this is leaf or internal.
    let is_leaf = count <= 1;

    // 3) Allocate a node slot.
    let node_index = nodes.len();
    let data_offset = if is_leaf {
        // For a leaf, store the original index of the single chunk
        aabb_idx_pair[start].1
    } else {
        0
    };
    nodes.push(BvhNode {
        aabb: union_aabb.clone(),
        left: 0,
        right: 0,
        data_offset,
    });

    if is_leaf {
        // Nothing more to do for a leaf.
        return node_index;
    }

    // 4) Pick the longest axis:
    let dims = union_aabb.dimensions();
    let axis = if dims.x >= dims.y && dims.x >= dims.z {
        0
    } else if dims.y >= dims.x && dims.y >= dims.z {
        1
    } else {
        2
    };

    // 5) Sort the chunk-pairs in-place by center along that axis:
    aabb_idx_pair[start..end].sort_by_key(|(aabb, _idx)| {
        let c = aabb.center();
        let key = match axis {
            0 => c.x,
            1 => c.y,
            2 => c.z,
            _ => unreachable!(),
        };
        OrderedFloat(key)
    });

    // 6) Split in half and recurse:
    let mid = start + count / 2;
    let left_child = build_bvh_recursive(aabb_idx_pair, nodes, start, mid);
    let right_child = build_bvh_recursive(aabb_idx_pair, nodes, mid, end);

    // 7) Fill in child pointers:
    nodes[node_index].left = left_child as u32;
    nodes[node_index].right = right_child as u32;

    node_index
}

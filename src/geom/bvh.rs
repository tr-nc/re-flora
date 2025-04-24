use super::Aabb;
use ordered_float::OrderedFloat;
use std::collections::VecDeque;

/// The final, flattened BVH node.
/// `data_offset != 0` ⇒ leaf, and stores the original AABB index.
/// `data_offset  == 0` ⇒ internal node, and
///   - `left` is the index of the left child,
///   - right child is implicitly `left + 1`.
#[derive(Debug, Clone)]
pub struct BvhNode {
    pub aabb: Aabb,
    /// If leaf: original AABB index.
    /// If internal: ignored.
    pub data_offset: u32,
    /// If internal: index of left child; right child is `left + 1`.
    /// If leaf: ignored.
    pub left: u32,
    pub is_leaf: bool,
}

/// An intermediate tree node used during construction.
struct TreeNode {
    aabb: Aabb,
    /// `Some(idx)` ⇒ leaf, holds `idx` from the input slice.
    /// `None` ⇒ internal node.
    data_offset: Option<u32>,
    left: Option<Box<TreeNode>>,
    right: Option<Box<TreeNode>>,
}

/// Public entry point: build the tree and then flatten breadth‐first.
pub fn build_bvh(aabbs: &[Aabb]) -> Vec<BvhNode> {
    // 1) Build the recursive TreeNode structure
    let mut chunks: Vec<(Aabb, u32)> = aabbs
        .iter()
        .cloned()
        .enumerate()
        .map(|(i, aabb)| (aabb, i as u32))
        .collect();

    let chunks_len = chunks.len();
    let tree = build_tree(&mut chunks, 0, chunks_len);

    // 2) Flatten breadth‐first
    flatten_bvh(&*tree)
}

/// Recursively build the TreeNode structure.
fn build_tree(aabb_idx_pair: &mut [(Aabb, u32)], start: usize, end: usize) -> Box<TreeNode> {
    let count = end - start;

    // 1) Compute union AABB
    let mut unioned = aabb_idx_pair[start].0.clone();
    for i in (start + 1)..end {
        unioned = unioned.union(&aabb_idx_pair[i].0);
    }

    // 2) Leaf?
    if count <= 1 {
        let idx = aabb_idx_pair[start].1;
        return Box::new(TreeNode {
            aabb: unioned,
            data_offset: Some(idx),
            left: None,
            right: None,
        });
    }

    // 3) Pick axis
    let dims = unioned.dimensions();
    let axis = if dims.x >= dims.y && dims.x >= dims.z {
        0
    } else if dims.y >= dims.x && dims.y >= dims.z {
        1
    } else {
        2
    };

    // 4) Sort by center on that axis
    aabb_idx_pair[start..end].sort_by_key(|(aabb, _idx)| {
        let c = aabb.center();
        let k = match axis {
            0 => c.x,
            1 => c.y,
            2 => c.z,
            _ => unreachable!(),
        };
        OrderedFloat(k)
    });

    // 5) Split in half and recurse
    let mid = start + count / 2;
    let left = build_tree(aabb_idx_pair, start, mid);
    let right = build_tree(aabb_idx_pair, mid, end);

    // 6) Return this internal node
    Box::new(TreeNode {
        aabb: unioned,
        data_offset: None,
        left: Some(left),
        right: Some(right),
    })
}

/// Flatten the `TreeNode` into a `Vec<BvhNode>` in breadth‐first order.
fn flatten_bvh(root: &TreeNode) -> Vec<BvhNode> {
    let mut out = Vec::new();
    let mut queue = VecDeque::new();
    queue.push_back(root);

    while let Some(node) = queue.pop_front() {
        let idx = out.len() as u32;
        let is_leaf = node.data_offset.is_some();

        // Compute `left` only for internals; for leaves we can store 0.
        let left_child_index = if !is_leaf {
            // In a level‐order (heap) layout of a full binary tree,
            // children of node `idx` live at 2*idx+1 and 2*idx+2.
            2 * idx + 1
        } else {
            0
        };

        out.push(BvhNode {
            aabb: node.aabb.clone(),
            data_offset: node.data_offset.unwrap_or(0),
            left: left_child_index,
            is_leaf,
        });

        if !is_leaf {
            // Enqueue children in order
            let l = node.left.as_ref().unwrap();
            let r = node.right.as_ref().unwrap();
            queue.push_back(l);
            queue.push_back(r);
        }
    }

    out
}

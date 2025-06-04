use super::Aabb3;
use ordered_float::OrderedFloat;

/// The final, flattened BVH node.
///
/// • `left` – index of the left-hand child in the returned vector  
/// • right-hand child is implicitly `left + 1`
#[derive(Debug, Clone)]
pub struct BvhNode {
    pub aabb: Aabb3,
    /// Leaf: index of the original AABB.  
    /// Internal: ignored.
    pub data_offset: u32,
    /// Internal: index of the left child (right = left + 1).  
    /// Leaf: ignored.
    pub left: u32,
    pub is_leaf: bool,
}

/* ------------------------------------------------------------------------- */

/// Build a BVH from a slice of AABBs.
/// The root node is always at index `0`.
pub fn build_bvh(aabbs: &[Aabb3], leaves_data: &[u32]) -> Result<Vec<BvhNode>, String> {
    if aabbs.len() != leaves_data.len() {
        return Err(format!(
            "AABBs and leaves data must have the same length. Got {} and {}.",
            aabbs.len(),
            leaves_data.len()
        ));
    }

    // An empty input ⇒ an empty BVH.
    if aabbs.is_empty() {
        return Err("Cannot build a BVH from an empty slice.".to_string());
    }

    let mut items = Vec::with_capacity(aabbs.len());
    for (i, aabb) in aabbs.iter().cloned().enumerate() {
        items.push((aabb, leaves_data[i]));
    }

    // Allocate a vector of nodes.
    // The very first element is a dummy root that will be overwritten later.
    let mut nodes = Vec::new();
    nodes.push(dummy_node(&items[0].0)); // index 0 == root

    // Fill the whole tree in-place, starting at that root.
    let len = items.len();
    build_bvh_recursive_in_place(&mut items, &mut nodes, 0, 0, len);

    return Ok(nodes);
}

/* ------------------------------------------------------------------------- */

/// Recursively builds the BVH and **writes** each node *in place*.
///
/// `node_index` – position in `nodes` that has to be filled  
/// `start..end`  – range inside `aabb_idx_pair` that this node covers
fn build_bvh_recursive_in_place(
    aabb_idx_pair: &mut [(Aabb3, u32)],
    nodes: &mut Vec<BvhNode>,
    node_index: usize,
    start: usize,
    end: usize,
) {
    let count = end - start;

    /* ------------------------------------------------- 1) union AABB ----- */

    let mut unioned = aabb_idx_pair[start].0.clone();
    for i in (start + 1)..end {
        unioned = unioned.union(&aabb_idx_pair[i].0);
    }

    /* ------------------------------------------------- leaf -------------- */

    if count == 1 {
        nodes[node_index] = BvhNode {
            aabb: unioned,
            data_offset: aabb_idx_pair[start].1,
            left: 0,
            is_leaf: true,
        };
        return;
    }

    /* ------------------------------------------------- internal ---------- */

    // 2) choose longest axis
    let dims = unioned.dimensions();
    let axis = if dims.x >= dims.y && dims.x >= dims.z {
        0
    } else if dims.y >= dims.x && dims.y >= dims.z {
        1
    } else {
        2
    };

    // 3) sort the current slice on that axis
    aabb_idx_pair[start..end].sort_by_key(|(aabb, _)| {
        let c = aabb.center();
        let k = match axis {
            0 => c.x,
            1 => c.y,
            _ => c.z,
        };
        OrderedFloat(k)
    });

    // 4) split in the middle
    let mid = start + count / 2;

    // 5) allocate *two consecutive* children
    let left_index = nodes.len();
    nodes.push(dummy_node(&unioned)); // left
    nodes.push(dummy_node(&unioned)); // right   ( => left + 1 )

    // 6) fill the current parent
    nodes[node_index] = BvhNode {
        aabb: unioned,
        data_offset: 0,
        left: left_index as u32,
        is_leaf: false,
    };

    // 7) recurse
    build_bvh_recursive_in_place(aabb_idx_pair, nodes, left_index, start, mid);
    build_bvh_recursive_in_place(aabb_idx_pair, nodes, left_index + 1, mid, end);
}

/* ------------------------------------------------------------------------- */

/// Creates a throw-away node –  the fields will be overwritten later.
#[inline(always)]
fn dummy_node(aabb: &Aabb3) -> BvhNode {
    BvhNode {
        aabb: aabb.clone(),
        data_offset: 0,
        left: 0,
        is_leaf: false,
    }
}

use super::TreeRenderInstanceData;
use anyhow::Result;
use glam::{IVec3, UVec3};
use std::collections::BTreeMap;

pub(super) const LEAF_SHADOW_PROXY_CELL_SIZE_VOXELS: i32 = 4;
pub(super) const SOURCE_LEAF_SHADOW_BILLBOARD_SIZE_VOXELS: f32 = 1.225;

// The visible leaf stream stays one instance per generated leaf voxel. The shadow stream instead
// bins leaves in stable, spray-local cells: the spray anchor is the object-space frame used by the
// leaf wind shader, so neither camera motion nor per-frame wind changes cluster membership. Four
// voxels project to roughly nine texels in the current leaf-opacity map in the benchmark scene,
// putting the producer footprint safely above its sampling bandwidth while retaining crown holes.

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct LeafShadowProxy {
    pub(super) world_pos: UVec3,
    pub(super) leaf_local_pos: IVec3,
    pub(super) billboard_size_voxels: f32,
    pub(super) opacity_layer_count: f32,
    pub(super) source_count: u32,
}

#[derive(Default)]
struct ClusterAccumulator {
    local_position_sum: [i64; 3],
    source_count: u32,
}

pub(super) fn build_leaf_shadow_proxies(
    instances: &[TreeRenderInstanceData],
) -> Result<Vec<LeafShadowProxy>> {
    let mut clusters = BTreeMap::<(i32, i32, i32, i32, i32, i32), ClusterAccumulator>::new();

    for instance in instances {
        let object_anchor = leaf_spray_anchor(instance)?;
        let local = instance.leaf_local_pos;
        let key = (
            object_anchor.x,
            object_anchor.y,
            object_anchor.z,
            local.x.div_euclid(LEAF_SHADOW_PROXY_CELL_SIZE_VOXELS),
            local.y.div_euclid(LEAF_SHADOW_PROXY_CELL_SIZE_VOXELS),
            local.z.div_euclid(LEAF_SHADOW_PROXY_CELL_SIZE_VOXELS),
        );
        let cluster = clusters.entry(key).or_default();
        cluster.local_position_sum[0] += i64::from(local.x);
        cluster.local_position_sum[1] += i64::from(local.y);
        cluster.local_position_sum[2] += i64::from(local.z);
        cluster.source_count += 1;
    }

    let proxy_size = LEAF_SHADOW_PROXY_CELL_SIZE_VOXELS as f32;
    // opacity_layer_count is an optical-area density, not a binary alpha threshold. Multiplying it
    // by the proxy area exactly recovers the summed source billboard area; the shader converts the
    // fractional layer count to transmittance with 1 - (1 - alpha)^layers.
    let source_area_ratio = (SOURCE_LEAF_SHADOW_BILLBOARD_SIZE_VOXELS / proxy_size).powi(2);
    clusters
        .into_iter()
        .map(|(key, cluster)| {
            let object_anchor = IVec3::new(key.0, key.1, key.2);
            let count = i64::from(cluster.source_count);
            let leaf_local_pos = IVec3::new(
                rounded_div(cluster.local_position_sum[0], count),
                rounded_div(cluster.local_position_sum[1], count),
                rounded_div(cluster.local_position_sum[2], count),
            );
            let world_pos_i64 = [
                i64::from(object_anchor.x) + i64::from(leaf_local_pos.x),
                i64::from(object_anchor.y) + i64::from(leaf_local_pos.y),
                i64::from(object_anchor.z) + i64::from(leaf_local_pos.z),
            ];
            anyhow::ensure!(
                world_pos_i64
                    .iter()
                    .all(|&value| (0..=i64::from(u32::MAX)).contains(&value)),
                "leaf shadow proxy world position is outside unsigned voxel space"
            );
            Ok(LeafShadowProxy {
                world_pos: UVec3::new(
                    world_pos_i64[0] as u32,
                    world_pos_i64[1] as u32,
                    world_pos_i64[2] as u32,
                ),
                leaf_local_pos,
                billboard_size_voxels: proxy_size,
                opacity_layer_count: cluster.source_count as f32 * source_area_ratio,
                source_count: cluster.source_count,
            })
        })
        .collect()
}

fn leaf_spray_anchor(instance: &TreeRenderInstanceData) -> Result<IVec3> {
    anyhow::ensure!(
        instance
            .world_pos
            .cmple(UVec3::splat(i32::MAX as u32))
            .all(),
        "tree leaf world position exceeds signed object-space range"
    );
    Ok(instance.world_pos.as_ivec3() - instance.leaf_local_pos)
}

fn rounded_div(value: i64, divisor: i64) -> i32 {
    debug_assert!(divisor > 0);
    let rounded = if value >= 0 {
        (value + divisor / 2) / divisor
    } else {
        (value - divisor / 2) / divisor
    };
    i32::try_from(rounded).expect("average leaf-local position must fit i32")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(anchor: IVec3, local: IVec3) -> TreeRenderInstanceData {
        TreeRenderInstanceData {
            world_pos: (anchor + local).as_uvec3(),
            leaf_local_pos: local,
        }
    }

    #[test]
    fn proxy_cells_are_spray_anchored_deterministic_and_signed() {
        let anchor = IVec3::new(100, 80, 120);
        let second_anchor = IVec3::new(112, 92, 136);
        let leaves = [
            leaf(anchor, IVec3::new(3, 0, 0)),
            leaf(anchor, IVec3::new(-4, -1, 0)),
            leaf(anchor, IVec3::new(0, 1, 1)),
            leaf(anchor, IVec3::new(-1, -2, 1)),
            leaf(second_anchor, IVec3::new(0, 1, 1)),
        ];
        let forward = build_leaf_shadow_proxies(&leaves).unwrap();
        let reversed =
            build_leaf_shadow_proxies(&leaves.iter().copied().rev().collect::<Vec<_>>()).unwrap();

        assert_eq!(forward, reversed);
        assert_eq!(forward.len(), 3);
        assert!(forward.iter().all(|proxy| [anchor, second_anchor]
            .contains(&(proxy.world_pos.as_ivec3() - proxy.leaf_local_pos))));
        assert!(forward
            .iter()
            .all(|proxy| proxy.billboard_size_voxels == 4.0));
    }

    #[test]
    fn proxy_opacity_layers_preserve_source_optical_area() {
        let anchor = IVec3::splat(200);
        let leaves = (0..32)
            .map(|index| {
                leaf(
                    anchor,
                    IVec3::new(index % 8 - 4, (index / 8) % 2, index / 16),
                )
            })
            .collect::<Vec<_>>();
        let proxies = build_leaf_shadow_proxies(&leaves).unwrap();
        let proxy_optical_area = proxies
            .iter()
            .map(|proxy| proxy.opacity_layer_count * proxy.billboard_size_voxels.powi(2))
            .sum::<f32>();
        let source_optical_area =
            leaves.len() as f32 * SOURCE_LEAF_SHADOW_BILLBOARD_SIZE_VOXELS.powi(2);

        assert!((proxy_optical_area - source_optical_area).abs() < 1.0e-5);
        assert_eq!(
            proxies.iter().map(|proxy| proxy.source_count).sum::<u32>(),
            leaves.len() as u32
        );
    }

    #[test]
    fn distinct_leaf_spray_anchors_never_merge() {
        let leaves = [
            leaf(IVec3::splat(100), IVec3::ZERO),
            leaf(IVec3::splat(101), IVec3::ZERO),
        ];
        let proxies = build_leaf_shadow_proxies(&leaves).unwrap();

        assert_eq!(proxies.len(), 2);
        assert!(proxies.iter().all(|proxy| proxy.source_count == 1));
    }
}

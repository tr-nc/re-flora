//! Versioned artistic initial conditions, not a botanical or live moisture simulation.
//! See docs/research/garden-spatial-ecology.md for evidence and transfer limits.
use glam::{UVec2, UVec3, Vec2, Vec3Swizzles};

pub const REVIEW_PRESETS: &[&str] = &[
    "garden-arrival",
    "garden-corridor",
    "garden-reverse",
    "garden-overview",
];
pub fn is_review_preset(name: &str) -> bool {
    REVIEW_PRESETS.contains(&name)
}
pub const VERSION: u32 = 5;
// Keep the established flower stream stable when only the tree/grass recipe changes.
const FLOWER_SEED_SALT: u32 = 1;
pub const DEFAULT_SEED: u32 = 20260519;
pub const ARRIVAL: Vec2 = Vec2::new(0.36, 0.73);
pub const POCKET: Vec2 = Vec2::new(0.62, 0.40);
pub const ROUTE: [Vec2; 4] = [
    ARRIVAL,
    Vec2::new(0.43, 0.62),
    Vec2::new(0.55, 0.53),
    POCKET,
];
pub const TREE_CENTERS: [Vec2; 4] = [
    Vec2::new(0.23, 0.32),
    Vec2::new(0.36, 0.22),
    Vec2::new(0.78, 0.57),
    Vec2::new(0.80, 0.30),
];
pub const FLOWER_KEYS: [&str; 3] = ["wild-geranium", "forget-me-not", "oxeye-daisy"];
pub const MAX_ATTEMPTS: u32 = 2304;

/// Layout owns individual variation, not a second tree recipe or size override.
/// The caller supplies the same captured planting description used by Grow.
pub fn tree_descriptor(
    seed: u32,
    index: usize,
    mut desc: crate::tree_gen::TreeDesc,
) -> crate::tree_gen::TreeDesc {
    desc.branching.seed = hash(seed ^ index as u32 ^ 0x74726565) as u64;
    desc
}
pub fn tree_radius_voxels(desc: &crate::tree_gen::TreeDesc) -> f32 {
    let tree = crate::tree_gen::Tree::new(desc.clone());
    tree.trunks()
        .iter()
        .flat_map(|c| {
            [
                c.center_a().xz().length() + c.radius_a(),
                c.center_b().xz().length() + c.radius_b(),
            ]
        })
        .chain(
            tree.relative_leaf_placements()
                .iter()
                .map(|leaf| leaf.position.xz().length() + 1.),
        )
        .fold(0., f32::max)
}
pub const MAX_FLOWERS: usize = 180;

pub fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846ca68b);
    x ^ (x >> 16)
}
pub fn unit(x: u32) -> f32 {
    (hash(x) >> 8) as f32 / 16777216.0
}

fn ellipse(p: Vec2, center: Vec2, radius: Vec2) -> f32 {
    ((p - center) / radius).length()
}
pub fn route_distance(p: Vec2) -> f32 {
    ROUTE
        .windows(2)
        .map(|s| {
            let d = s[1] - s[0];
            (p - s[0] - d * ((p - s[0]).dot(d) / d.length_squared()).clamp(0., 1.)).length()
        })
        .fold(f32::INFINITY, f32::min)
}
pub fn reserved(p: Vec2) -> bool {
    ellipse(p, ARRIVAL, Vec2::new(0.13, 0.10)) < 1.
        || ellipse(p, POCKET, Vec2::new(0.105, 0.08)) < 1.
        || route_distance(p) < 0.045
}
/// Broad unequal rises; finite differences stay well below the walking slope limit.
pub fn height_voxels(p: Vec2, seed: u32) -> u32 {
    let lobe = |c, r| (-ellipse(p, c, r).powi(2)).exp();
    let shift = (unit(seed) - 0.5) * 0.04;
    (126.
        + 17. * lobe(Vec2::new(0.23 + shift, 0.28), Vec2::new(0.30, 0.26))
        + 9. * lobe(Vec2::new(0.82, 0.43), Vec2::new(0.24, 0.34))
        - 3. * lobe(ARRIVAL, Vec2::splat(0.22)))
    .round() as u32
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Flower {
    pub column: UVec2,
    pub color: usize,
    pub seed: u32,
    pub growth: u32,
}

pub fn flowers(seed: u32, dim: UVec2, mut valid: impl FnMut(UVec2) -> bool) -> Vec<Flower> {
    // Two repeated colors plus a smaller third accent; elongated, overlapping lobes.
    let drifts = [
        (Vec2::new(0.26, 0.56), Vec2::new(0.10, 0.065), 0),
        (Vec2::new(0.31, 0.44), Vec2::new(0.075, 0.12), 0),
        (Vec2::new(0.68, 0.66), Vec2::new(0.13, 0.055), 1),
        (Vec2::new(0.71, 0.50), Vec2::new(0.065, 0.12), 1),
        (Vec2::new(0.56, 0.27), Vec2::new(0.12, 0.055), 2),
    ];
    let mut result: Vec<Flower> = Vec::new();
    for attempt in 0..MAX_ATTEMPTS {
        if result.len() == MAX_FLOWERS {
            break;
        }
        let (center, radius, color) = drifts[(attempt % 5) as usize];
        let species_key = FLOWER_KEYS[color]
            .bytes()
            .fold(0x811c9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
        let key = hash(seed ^ FLOWER_SEED_SALT ^ species_key ^ attempt.wrapping_mul(0x9e3779b9));
        let offset = Vec2::new(unit(key ^ 11), unit(key ^ 23)) * 2. - Vec2::ONE;
        let p = if attempt % 11 == 0 {
            Vec2::splat(0.12) + (offset + Vec2::ONE) * 0.38
        } else {
            center + offset * radius
        };
        if p.min_element() < 0.09
            || p.max_element() > 0.91
            || reserved(p)
            || TREE_CENTERS.iter().any(|t| p.distance(*t) < 0.10)
        {
            continue;
        }
        // Soft broad habitat preference, never a second soil-water owner.
        let suitability = 0.55 + 0.35 * (p.x * 5. + p.y * 3.).sin().abs();
        if unit(key ^ 43) > suitability {
            continue;
        }
        let column = (p * dim.as_vec2()).as_uvec2();
        if reserved(column.as_vec2() / dim.as_vec2()) {
            continue;
        }
        if result
            .iter()
            .any(|f| f.column.as_vec2().distance(column.as_vec2()) < 9.)
            || !valid(column)
        {
            continue;
        }
        result.push(Flower {
            column,
            color,
            seed: key,
            growth: 210 + (hash(key ^ 61) % 46),
        });
    }
    result
}

/// Ordinary dirt/rock atlas bytes (x fastest, then y, then z); no fixed-scene bits.
pub fn terrain_chunk(offset: UVec3, dim: UVec3, world: UVec3, seed: u32) -> Vec<u8> {
    let mut bytes = vec![0; (dim.x * dim.y * dim.z) as usize];
    for z in 0..dim.z {
        for x in 0..dim.x {
            let p = Vec2::new(
                (offset.x + x) as f32 / world.x as f32,
                (offset.z + z) as f32 / world.z as f32,
            );
            let height = height_voxels(p, seed);
            for y in 0..dim.y {
                let depth = height as i32 - (offset.y + y) as i32;
                bytes[(x + dim.x * (y + dim.y * z)) as usize] = if depth < 0 {
                    0
                } else if depth < 30 {
                    2
                } else {
                    7
                };
            }
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deterministic_bounded_spaced_and_varied() {
        let dim = UVec2::splat(512);
        let a = flowers(DEFAULT_SEED, dim, |_| true);
        assert_eq!(a, flowers(DEFAULT_SEED, dim, |_| true));
        assert_ne!(a, flowers(DEFAULT_SEED + 1, dim, |_| true));
        assert!(a.len() >= 120 && a.len() <= MAX_FLOWERS, "{}", a.len());
        assert!(flowers(1, dim, |_| false).is_empty());
        for (i, f) in a.iter().enumerate() {
            assert!(!reserved(f.column.as_vec2() / dim.as_vec2()));
            assert!(a[..i]
                .iter()
                .all(|b| b.column.as_vec2().distance(f.column.as_vec2()) >= 9.));
        }
    }
    #[test]
    fn opening_apples_fit_borders_and_clearings_at_the_original_positions() {
        assert_eq!(
            TREE_CENTERS,
            [
                Vec2::new(0.23, 0.32),
                Vec2::new(0.36, 0.22),
                Vec2::new(0.78, 0.57),
                Vec2::new(0.80, 0.30),
            ]
        );
        for seed in [DEFAULT_SEED, 0, u32::MAX, 7, 122, 907] {
            for (i, p) in TREE_CENTERS.iter().enumerate() {
                let desc = tree_descriptor(seed, i, crate::tree_gen::TreeDesc::default());
                assert_eq!(desc.species, crate::tree_gen::TreeSpecies::AppleTree);
                assert_eq!(
                    desc.branching.seed,
                    hash(seed ^ i as u32 ^ 0x74726565) as u64
                );
                assert_eq!(
                    desc.fruit_spawn_probability,
                    crate::tree_gen::TreeDesc::default().fruit_spawn_probability
                );
                assert!(desc.fruit_spawn_probability > 0.);
                assert_eq!(
                    serde_json::to_value(&desc).unwrap(),
                    serde_json::to_value(tree_descriptor(
                        seed,
                        i,
                        crate::tree_gen::TreeDesc::default()
                    ))
                    .unwrap()
                );
                let height = crate::tree_gen::Tree::new(desc.clone()).height_voxels();
                assert!(
                    (38.0..=49.0).contains(&height),
                    "seed={seed} tree={i} height={height}"
                );
                let defaults = crate::tree_gen::TreeDesc::default();
                assert_eq!(desc.leaves_size_level, defaults.leaves_size_level);
                assert_eq!(
                    desc.fruit_down_offset_voxels,
                    defaults.fruit_down_offset_voxels
                );
                let radius = tree_radius_voxels(&desc);
                assert!(radius <= 46.);
                assert!(p.min_element() * 512. > radius + 4.);
                assert!((1. - p.max_element()) * 512. > radius + 4.);
                assert!(route_distance(*p) * 512. > radius + 24.);
            }
        }
    }

    #[test]
    fn route_is_connected_clear_and_walkable() {
        for s in ROUTE.windows(2) {
            for i in 0..100 {
                let p = s[0].lerp(s[1], i as f32 / 100.);
                assert!(reserved(p));
                let h = height_voxels(p, DEFAULT_SEED) as i32;
                for d in [Vec2::X, Vec2::Y] {
                    assert!((height_voxels(p + d / 512., DEFAULT_SEED) as i32 - h).abs() <= 1);
                }
                assert!(TREE_CENTERS.iter().all(|t| t.distance(p) > 0.12));
            }
        }
    }
}

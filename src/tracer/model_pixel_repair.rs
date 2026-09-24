//! Geometry-constrained eight-neighbour bridges and conservative projected
//! coverage. Cross-runtime fixtures compare final browser and native masks.
//! Produces sparse bridge expressions and model-shaded surface seeds; no GPU image readback.
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};

const EPS: f64 = 0.0001;
pub const EXPRESSION: u32 = 1 << 31;
pub const HIDDEN: u32 = u32::MAX;
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Node {
    // destination (HIDDEN for intermediate/pruned nodes), endpoints, lerp bits.
    // links[1]==HIDDEN seeds a projected surface from links[2]'s source triangle.
    // Otherwise bit 31 selects an earlier expression or an original pixel.
    pub links: [u32; 4],
    pub color_depth: [f32; 4],
}
#[derive(serde::Deserialize, Clone)]
pub struct Group {
    pub id: u32,
    pub triangles: Vec<[[f64; 3]; 3]>,
    #[serde(default)]
    pub sources: Vec<u32>,
}
#[derive(Default)]
pub struct Plan {
    pub nodes: Vec<Node>,
    pub added: usize,
    pub before: usize,
    pub after: usize,
}

fn neighbours(pixel: usize, n: usize) -> impl Iterator<Item = usize> {
    let (x, y) = ((pixel % n) as isize, (pixel / n) as isize);
    (-1..=1).flat_map(move |dy| {
        (-1..=1).filter_map(move |dx| {
            let (xx, yy) = (x + dx, y + dy);
            ((dx != 0 || dy != 0) && xx >= 0 && yy >= 0 && xx < n as isize && yy < n as isize)
                .then_some((yy * n as isize + xx) as usize)
        })
    })
}
pub fn label(mask: &[bool], n: usize) -> (Vec<usize>, usize) {
    let mut labels = vec![0; mask.len()];
    let mut queue = Vec::with_capacity(mask.len());
    let mut count = 0;
    for i in 0..mask.len() {
        if !mask[i] || labels[i] != 0 {
            continue;
        }
        count += 1;
        labels[i] = count;
        queue.clear();
        queue.push(i);
        let mut head = 0;
        while head < queue.len() {
            let p = queue[head];
            head += 1;
            for next in neighbours(p, n) {
                if mask[next] && labels[next] == 0 {
                    labels[next] = count;
                    queue.push(next);
                }
            }
        }
    }
    (labels, count)
}
fn weights(t: &[[f64; 3]; 3], x: f64, y: f64) -> [f64; 3] {
    let cross = |a: usize, b: usize| (t[a][0] - x) * (t[b][1] - y) - (t[a][1] - y) * (t[b][0] - x);
    let area = cross(0, 1) + cross(1, 2) + cross(2, 0);
    if area.abs() > 1e-10 {
        let w = [cross(1, 2) / area, cross(2, 0) / area, cross(0, 1) / area];
        if w.iter().all(|v| *v >= 0.) {
            return w;
        }
    }
    let mut best = f64::INFINITY;
    let mut result = [0.; 3];
    for i in 0..3 {
        let j = (i + 1) % 3;
        let dx = t[j][0] - t[i][0];
        let dy = t[j][1] - t[i][1];
        let a = (((x - t[i][0]) * dx + (y - t[i][1]) * dy) / (dx * dx + dy * dy).max(1e-20))
            .clamp(0., 1.);
        let d = (t[i][0] + dx * a - x).powi(2) + (t[i][1] + dy * a - y).powi(2);
        if d < best {
            best = d;
            result = [0.; 3];
            result[i] = 1. - a;
            result[j] = a;
        }
    }
    result
}
struct Coverage {
    words: usize,
    bits: Vec<u32>,
    depth: Vec<f64>,
    source: Vec<u32>,
}
impl Coverage {
    fn new(triangles: &[[[f64; 3]; 3]], sources: &[u32], n: usize) -> Self {
        let words = triangles.len().div_ceil(32).max(1);
        let mut out = Self {
            words,
            bits: vec![0; n * n * words],
            depth: vec![f64::INFINITY; n * n],
            source: vec![0; n * n],
        };
        for (index, t) in triangles.iter().enumerate() {
            let lo = [0, 1].map(|a| t.iter().map(|p| p[a]).fold(f64::INFINITY, f64::min));
            let hi = [0, 1].map(|a| t.iter().map(|p| p[a]).fold(f64::NEG_INFINITY, f64::max));
            let winding = if (t[1][0] - t[0][0]) * (t[2][1] - t[0][1])
                - (t[1][1] - t[0][1]) * (t[2][0] - t[0][0])
                >= 0.
            {
                1.
            } else {
                -1.
            };
            let start_y = (lo[1] - 1. - EPS).ceil().max(0.) as usize;
            let end_y = (hi[1] + EPS).floor().min(n as f64 - 1.);
            if end_y < start_y as f64 {
                continue;
            }
            for y in start_y..=end_y as usize {
                let start = (lo[0] - 1. - EPS).ceil().max(0.) as usize;
                let end = (hi[0] + EPS).floor().min(n as f64 - 1.);
                if end < start as f64 {
                    continue;
                }
                for x in start..=end as usize {
                    if (0..3).all(|i| {
                        let j = (i + 1) % 3;
                        let dx = t[j][0] - t[i][0];
                        let dy = t[j][1] - t[i][1];
                        winding
                            * (dx * (y as f64 + 0.5 - t[i][1]) - dy * (x as f64 + 0.5 - t[i][0]))
                            >= -(0.5 + EPS) * (dx.abs() + dy.abs())
                    }) {
                        let p = y * n + x;
                        out.bits[p * words + index / 32] |= 1 << (index % 32);
                        let w = weights(t, x as f64 + 0.5, y as f64 + 0.5);
                        let z: f64 = (0..3).map(|i| w[i] * t[i][2]).sum();
                        if z < out.depth[p] {
                            out.depth[p] = z;
                            out.source[p] = sources.get(index).copied().unwrap_or(index as u32);
                        }
                    }
                }
            }
        }
        out
    }
    fn shares(&self, a: usize, b: usize) -> bool {
        (0..self.words).any(|w| self.bits[a * self.words + w] & self.bits[b * self.words + w] != 0)
    }
}

pub fn plan(owners: &[u32], groups: &[Group], n: usize) -> Plan {
    assert_eq!(owners.len(), n * n);
    let mut output = Plan::default();
    let mut chosen = vec![None::<usize>; n * n];
    let mut depth = vec![f64::INFINITY; n * n];
    let mut group_masks = Vec::new();
    let mut node_groups = Vec::new();
    for group in groups {
        let original: Vec<bool> = owners.iter().map(|id| *id == group.id).collect();
        let mut mask = original.clone();
        let (mut labels, mut count) = label(&mask, n);
        output.before += count;
        let coverage = Coverage::new(&group.triangles, &group.sources, n);
        let mut additions = vec![false; n * n];
        let mut references: Vec<u32> = (0..(n * n) as u32).collect();
        while count > 1 {
            let mut owner = labels.clone();
            let mut distance = vec![0; n * n];
            let mut previous = vec![usize::MAX; n * n];
            let mut queue: Vec<_> = (0..n * n).filter(|i| mask[*i]).collect();
            let mut head = 0;
            let mut best = usize::MAX;
            let mut bridge = None;
            while head < queue.len() {
                let p = queue[head];
                head += 1;
                for next in neighbours(p, n) {
                    if (owners[p] != 0 && !original[p])
                        || (owners[next] != 0 && !original[next])
                        || !coverage.shares(p, next)
                    {
                        continue;
                    }
                    if owner[next] == 0 {
                        owner[next] = owner[p];
                        distance[next] = distance[p] + 1;
                        previous[next] = p;
                        queue.push(next);
                    } else if owner[next] != owner[p] && distance[p] + distance[next] < best {
                        best = distance[p] + distance[next];
                        bridge = Some((p, next));
                    }
                }
            }
            let Some((a, b)) = bridge else { break };
            let walk = |mut p: usize| {
                let mut path = Vec::new();
                while p != usize::MAX {
                    path.push(p);
                    p = previous[p];
                }
                path
            };
            let mut path = walk(a);
            path.reverse();
            path.extend(walk(b));
            let first = references[path[0]];
            let last = references[*path.last().unwrap()];
            for (index, p) in path.iter().copied().enumerate() {
                if mask[p] {
                    continue;
                }
                references[p] = EXPRESSION | output.nodes.len() as u32;
                output.nodes.push(Node {
                    links: [
                        HIDDEN,
                        first,
                        last,
                        (index as f32 / (path.len() - 1) as f32).to_bits(),
                    ],
                    color_depth: [0., 0., 0., coverage.depth[p] as f32],
                });
                node_groups.push(group.id);
                mask[p] = true;
                additions[p] = true;
            }
            (labels, count) = label(&mask, n);
        }
        loop {
            let mut changed = false;
            for p in (0..n * n).rev() {
                if additions[p] {
                    mask[p] = false;
                    if label(&mask, n).1 == count {
                        additions[p] = false;
                        changed = true;
                    } else {
                        mask[p] = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        for p in 0..n * n {
            if additions[p] && coverage.depth[p] < depth[p] {
                if let Some(old) = chosen[p] {
                    output.nodes[old].links[0] = HIDDEN;
                }
                let index = (references[p] & !EXPRESSION) as usize;
                output.nodes[index].links[0] = p as u32;
                chosen[p] = Some(index);
                depth[p] = coverage.depth[p];
            }
        }
        // Coverage is a second pass: the minimum-bridge graph remains intact,
        // but every unoccupied projected cell receives a material ray sample.
        for p in 0..n * n {
            if owners[p] != 0 || !coverage.depth[p].is_finite() || coverage.depth[p] > depth[p] {
                continue;
            }
            if let Some(old) = chosen[p] {
                output.nodes[old].links[0] = HIDDEN;
            }
            let index = output.nodes.len();
            output.nodes.push(Node {
                links: [p as u32, HIDDEN, coverage.source[p], 0],
                color_depth: [0., 0., 0., coverage.depth[p] as f32],
            });
            node_groups.push(group.id);
            chosen[p] = Some(index);
            depth[p] = coverage.depth[p];
        }
        group_masks.push((group.id, original));
    }
    output.added = chosen.iter().filter(|i| i.is_some()).count();
    // Final visible connectivity, including depth competition between groups.
    for (id, mut mask) in group_masks {
        for (p, index) in chosen.iter().enumerate() {
            if let Some(i) = index {
                mask[p] = node_groups[*i] == id;
            }
        }
        output.after += label(&mask, n).1;
    }
    output
}

/// Same fixed camera-space cube as butterfly_mesh_types.slang. Never fit a pose.
pub fn tile_bounds(center: Vec3, size: f32, view: Mat4, projection: Mat4) -> Vec4 {
    let center = view.transform_point3(center);
    let r = size * (1.53125 * 0.5);
    let near = projection.inverse() * Vec4::new(0., 0., 0., 1.);
    let near_z = near.z / near.w;
    if center.z - r >= near_z {
        return Vec4::new(2., 2., 3., 3.);
    }
    let mut lo = glam::Vec2::splat(f32::INFINITY);
    let mut hi = glam::Vec2::splat(f32::NEG_INFINITY);
    for i in 0..8 {
        let p = Vec4::new(
            center.x + if i & 1 == 0 { -r } else { r },
            center.y + if i & 2 == 0 { -r } else { r },
            if i & 4 == 0 {
                center.z - r
            } else {
                (center.z + r).min(near_z)
            },
            1.,
        );
        let clip = projection * p;
        let ndc = glam::Vec2::new(clip.x, clip.y) / clip.w;
        lo = lo.min(ndc);
        hi = hi.max(ndc);
    }
    Vec4::new(lo.x, lo.y, hi.x, hi.y)
}

pub fn ray_triangle(origin: Vec3, direction: Vec3, a: Vec3, e1: Vec3, e2: Vec3) -> Option<f32> {
    let p = direction.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1. / det;
    let s = origin - a;
    let u = s.dot(p) * inv;
    let q = s.cross(e1);
    let v = direction.dot(q) * inv;
    let t = e2.dot(q) * inv;
    (u >= -1e-6 && v >= -1e-6 && u + v <= 1.000001 && t > 0.).then_some(t)
}

/// Rasterize center ownership with the production tile's projection/depth convention.
/// The callback returns a nearest-hit ray distance after near/far clipping.
/// Only near/far clipping: offscreen portions of a partially visible object retain
/// their fixed tile coordinates, exactly as the GPU ray sampler does.
pub fn project(
    triangles: &[(u32, [Vec3; 3])],
    vp: Mat4,
    bounds: Vec4,
    n: usize,
    mut center_distance: impl FnMut(usize, usize, usize) -> Option<f32>,
) -> (Vec<u32>, Vec<Group>) {
    let mut owners = vec![0; n * n];
    let mut depth = vec![f64::INFINITY; n * n];
    let mut groups: Vec<Group> = Vec::new();
    if bounds.x > 1. || bounds.y > 1. || bounds.z < -1. || bounds.w < -1. {
        return (owners, groups);
    }
    for (triangle_index, (id, points)) in triangles.iter().enumerate() {
        let h = points.map(|p| vp * p.extend(1.));
        let planes: [fn(Vec4) -> f32; 3] = [|p| p.w - 1e-7, |p| p.z, |p| p.w - p.z];
        let mut polygon = h.to_vec();
        for plane in planes {
            if polygon.iter().all(|p| plane(*p) >= 0.) {
                continue;
            }
            let mut next = Vec::new();
            for i in 0..polygon.len() {
                let a = polygon[i];
                let b = polygon[(i + 1) % polygon.len()];
                let da = plane(a);
                let db = plane(b);
                if da >= 0. {
                    next.push(a);
                }
                if (da >= 0.) != (db >= 0.) {
                    next.push(a.lerp(b, da / (da - db)));
                }
            }
            polygon = next;
            if polygon.len() < 3 {
                break;
            }
        }
        if polygon.len() < 3 {
            continue;
        }
        let group = if let Some(i) = groups.iter().position(|g| g.id == *id) {
            i
        } else {
            groups.push(Group {
                id: *id,
                triangles: Vec::new(),
                sources: Vec::new(),
            });
            groups.len() - 1
        };
        for i in 1..polygon.len() - 1 {
            let vertices = [polygon[0], polygon[i], polygon[i + 1]];
            let t = vertices.map(|p| {
                let q = p / p.w;
                [
                    (f64::from(q.x) - f64::from(bounds.x)) / f64::from(bounds.z - bounds.x)
                        * n as f64,
                    (f64::from(q.y) - f64::from(bounds.y)) / f64::from(bounds.w - bounds.y)
                        * n as f64,
                    // Near-plane clipping can leave a tiny negative roundoff.
                    f64::from(q.z.clamp(0., 1.)),
                ]
            });
            groups[group].triangles.push(t);
            groups[group].sources.push(triangle_index as u32);
            let lo = [0, 1].map(|a| t.iter().map(|p| p[a]).fold(f64::INFINITY, f64::min));
            let hi = [0, 1].map(|a| t.iter().map(|p| p[a]).fold(f64::NEG_INFINITY, f64::max));
            // Bound the ray queries conservatively, but classify original centers
            // in the renderer's own ray space. Projected barycentrics alone drift
            // from tiny, inverse-pose leaf rays because of float cancellation.
            let x0 = (lo[0] - 1. - EPS).ceil().max(0.) as usize;
            let y0 = (lo[1] - 1. - EPS).ceil().max(0.) as usize;
            let x1 = (hi[0] + EPS).floor().min(n as f64 - 1.);
            let y1 = (hi[1] + EPS).floor().min(n as f64 - 1.);
            if x1 < x0 as f64 || y1 < y0 as f64 {
                continue;
            }
            for y in y0..=y1 as usize {
                for x in x0..=x1 as usize {
                    let Some(z) = center_distance(triangle_index, x, y).map(f64::from) else {
                        continue;
                    };
                    let p = y * n + x;
                    if z > 0. && z < depth[p] {
                        depth[p] = z;
                        owners[p] = *id;
                    }
                }
            }
        }
    }
    (owners, groups)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn diagonal() -> Group {
        Group {
            id: 1,
            triangles: vec![[[0., 0., 0.5], [5., 5., 0.5], [4.8, 5., 0.5]]],
            sources: vec![0],
        }
    }
    #[test]
    fn projected_geometry_survives_missing_centers_without_crossing_occupied_pixels() {
        let mut owners = vec![0; 25];
        owners[0] = 1;
        owners[6] = 1;
        let partial = plan(&owners, &[diagonal()], 5);
        assert!(partial.added > 0);
        assert_eq!(partial.after, 1);
        owners[6] = 0;
        owners[12] = 1;
        let p = plan(&owners, &[diagonal()], 5);
        assert!(p.added > 0);
        assert_eq!((p.before, p.after), (2, 1));
        owners[12] = 2;
        let split = plan(
            &owners,
            &[
                diagonal(),
                Group {
                    id: 2,
                    ..diagonal()
                },
            ],
            5,
        );
        assert!(split.nodes.iter().all(|node| node.links[0] != 12));
    }
    #[test]
    fn near_clipping_preserves_sampler_ownership_and_valid_depths() {
        let triangles = [(
            3,
            [
                Vec3::new(-0.5, -0.5, -0.1),
                Vec3::new(0.5, -0.5, 0.5),
                Vec3::new(0., 0.5, 0.5),
            ],
        )];
        let (owners, groups) = project(
            &triangles,
            Mat4::IDENTITY,
            Vec4::new(-1., -1., 1., 1.),
            4,
            |i, x, y| {
                assert_eq!(i, 0);
                (x == 2 && y == 2).then_some(0.5)
            },
        );
        assert_eq!(owners.iter().filter(|id| **id != 0).count(), 1);
        assert_eq!(owners[10], 3);
        assert_eq!(groups[0].triangles.len(), 2);
        assert!(groups[0]
            .triangles
            .iter()
            .flatten()
            .all(|p| (0.0..=1.0).contains(&p[2])));
        let (owners, groups) = project(
            &triangles,
            Mat4::IDENTITY,
            Vec4::new(2., 2., 3., 3.),
            4,
            |_, _, _| panic!("offscreen ray"),
        );
        assert!(owners.iter().all(|id| *id == 0));
        assert!(groups.is_empty());
    }
    #[test]
    fn coverage_seed_tracks_source_triangle_even_without_center_hits() {
        let points = [
            Vec3::new(0.006, -0.5, 0.5),
            Vec3::new(0.012, -0.5, 0.5),
            Vec3::new(0.006, 0.4, 0.5),
        ];
        let (_, groups) = project(
            &[(1, points)],
            Mat4::IDENTITY,
            Vec4::new(-1., -1., 1., 1.),
            8,
            |_, _, _| None,
        );
        let output = plan(&[0; 64], &groups, 8);
        assert!(output.added > 0);
        for node in output.nodes.iter().filter(|n| n.links[0] != HIDDEN) {
            assert_eq!(node.links[1], HIDDEN);
            assert_eq!(node.links[2], 0);
            assert!(node.color_depth[3] < 1.);
        }
    }
    #[test]
    fn triangle_identity_does_not_wrap_at_32_and_outside_bounds_stay_empty() {
        let outside = [[-20., -20., 0.5], [-10., -20., 0.5], [-20., -10., 0.5]];
        let mut triangles = vec![outside; 156];
        triangles[0] = [[0.1, 0.1, 0.5], [0.9, 0.1, 0.5], [0.9, 0.9, 0.5]];
        triangles[32] = [[1.6, 0.1, 0.5], [2.9, 0.1, 0.5], [2.9, 0.9, 0.5]];
        let mut owners = vec![0; 9];
        owners[0] = 1;
        owners[2] = 1;
        let filled = plan(
            &owners,
            &[Group {
                id: 1,
                triangles,
                sources: vec![],
            }],
            3,
        );
        assert!(filled.added > 0);
        assert!(filled
            .nodes
            .iter()
            .all(|node| node.links[0] != 0 && node.links[0] != 2));
        assert_eq!(
            plan(
                &owners,
                &[Group {
                    id: 1,
                    triangles: vec![outside],
                    sources: vec![],
                }],
                3
            )
            .added,
            0
        );
        assert!(plan(&[0; 9], &[diagonal()], 3).added > 0);
    }
    #[test]
    fn bridge_color_expressions_only_reference_originals_or_earlier_nodes() {
        let mut owners = vec![0; 36];
        owners[0] = 1;
        owners[5] = 1;
        owners[27] = 1;
        let p = plan(
            &owners,
            &[Group {
                id: 1,
                triangles: vec![[[0., 0., 0.5], [6., 0., 0.5], [3., 6., 0.5]]],
                sources: vec![],
            }],
            6,
        );
        assert_eq!((p.before, p.after), (3, 1));
        assert!(p.added > 0);
        assert!(p
            .nodes
            .iter()
            .any(|n| n.links[1] & EXPRESSION != 0 || n.links[2] & EXPRESSION != 0));
        for (i, node) in p.nodes.iter().enumerate() {
            for r in &node.links[1..3] {
                if *r == HIDDEN {
                    assert_eq!(node.links[1], HIDDEN); // ray-sampled coverage seed
                } else if r & EXPRESSION != 0 {
                    assert!((r & !EXPRESSION) < i as u32);
                } else {
                    assert_eq!(owners[*r as usize], 1);
                }
            }
        }
    }
    #[test]
    #[ignore = "generate browser fixtures with experiments/model-preview/tests/repair-parity.cjs first"]
    fn browser_repair_plan_parity() {
        #[derive(serde::Deserialize)]
        struct Fixture {
            size: usize,
            owners: Vec<u32>,
            groups: Vec<Group>,
            additions: Vec<u32>,
        }
        let fixtures: Vec<Fixture> = serde_json::from_str(
            &std::fs::read_to_string("target/model-repair-reference.json")
                .expect("run repair-parity.cjs"),
        )
        .unwrap();
        let mut total = 0;
        for (index, f) in fixtures.iter().enumerate() {
            let p = plan(&f.owners, &f.groups, f.size);
            let mut added: Vec<_> = p
                .nodes
                .iter()
                .filter_map(|n| (n.links[0] != HIDDEN).then_some(n.links[0]))
                .collect();
            added.sort_unstable();
            assert_eq!(added, f.additions, "fixture {index}");
            total += added.len();
        }
        assert!(total > 0);
        println!(
            "{} browser repair fixtures match; {total} additions",
            fixtures.len()
        );
    }
}

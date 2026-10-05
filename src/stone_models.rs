//! Deterministic, closed low-poly solids. This triangle model is the authority;
//! voxel occupancy and renderer vertices are derived adapters, never source data.
//! Right-handed +Y up, garden world units (256 terrain voxels/unit), outward CCW.
use anyhow::{ensure, Result};
use glam::{DVec3, Vec3};
pub mod voxel;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoneKind {
    Slab,
    Rock,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlabParams {
    /// Fraction of the short footprint axis removed at corners (not top noise).
    pub edge_cut: f32,
}
impl Default for SlabParams {
    fn default() -> Self {
        Self { edge_cut: 0.18 }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RockParams {
    /// Bounded cutting-plane budget; these are real silhouette/facet changes.
    pub facets: u32,
}
impl Default for RockParams {
    fn default() -> Self {
        Self { facets: 18 }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StoneSpec {
    pub kind: StoneKind,
    pub seed: u32,
    /// X width, Y height, Z depth. Bottom pivot at local (0,0,0).
    pub size: Vec3,
    pub variation: f32,
    pub slab: SlabParams,
    pub rock: RockParams,
}
impl StoneSpec {
    pub fn new(kind: StoneKind, seed: u32) -> Self {
        Self {
            kind,
            seed,
            size: match kind {
                StoneKind::Slab => Vec3::new(0.24, 0.035, 0.22),
                StoneKind::Rock => Vec3::new(0.24, 0.20, 0.22),
            },
            variation: 0.45,
            slab: SlabParams::default(),
            rock: RockParams::default(),
        }
    }
    /// Normalize hostile/legacy inputs once. A nonfinite input uses its default;
    /// finite extremes clamp, so downstream clipping cannot produce empty solids.
    pub fn sanitized(self) -> Self {
        let defaults = Self::new(self.kind, self.seed);
        let size = Vec3::new(
            finite_clamp(self.size.x, defaults.size.x, 0.04, 0.42),
            finite_clamp(self.size.y, defaults.size.y, 0.012, 0.38),
            finite_clamp(self.size.z, defaults.size.z, 0.04, 0.42),
        );
        Self {
            size: if self.kind == StoneKind::Slab {
                // A paving slab remains a slab even with a hostile height input.
                Vec3::new(size.x, size.y.min(size.x.min(size.z) * 0.3), size.z)
            } else {
                size
            },
            variation: finite_clamp(self.variation, defaults.variation, 0., 1.),
            slab: SlabParams {
                edge_cut: finite_clamp(self.slab.edge_cut, defaults.slab.edge_cut, 0.06, 0.34),
            },
            rock: RockParams {
                facets: self.rock.facets.clamp(8, 24),
            },
            ..self
        }
    }
}
fn finite_clamp(value: f32, default: f32, min: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        default
    }
}

/// Indexed topological vertices, flat outward normal and sRGB color per triangle.
/// Attributes are face-bound so the welded topology stays closed/manifold; the
/// raster/OBJ adapters split shading corners without changing the solid.
#[derive(Clone, Debug, PartialEq)]
pub struct StoneMesh {
    pub spec: StoneSpec,
    pub positions: Vec<Vec3>,
    pub indices: Vec<u32>,
    pub normals: Vec<Vec3>,
    pub colors: Vec<Vec3>,
    pub min: Vec3,
    pub max: Vec3,
}
impl StoneMesh {
    pub fn triangles(&self) -> impl Iterator<Item = [Vec3; 3]> + '_ {
        self.indices.chunks_exact(3).map(|t| {
            [
                self.positions[t[0] as usize],
                self.positions[t[1] as usize],
                self.positions[t[2] as usize],
            ]
        })
    }
    pub fn volume(&self) -> f64 {
        self.triangles()
            .map(|[a, b, c]| a.as_dvec3().dot(b.as_dvec3().cross(c.as_dvec3())) / 6.)
            .sum()
    }
    /// Convex half-space containment, independent of the voxel adapter's winding.
    pub fn contains(&self, point: Vec3, tolerance: f32) -> bool {
        point.is_finite()
            && self
                .triangles()
                .zip(&self.normals)
                .all(|(t, n)| n.dot(point - t[0]) <= tolerance)
    }
    /// Portable OBJ: shared positions, separate flat normals, ordinary triangle
    /// faces. RGB vertex extensions are optional; importers may ignore color.
    pub fn obj(&self) -> String {
        use std::fmt::Write;
        let mut text = format!(
            "# Re: Flora {:?} seed={} +Y up; garden world units; bottom pivot\no stone\n",
            self.spec.kind, self.spec.seed
        );
        for p in &self.positions {
            writeln!(text, "v {:.9} {:.9} {:.9}", p.x, p.y, p.z).unwrap();
        }
        for n in &self.normals {
            writeln!(text, "vn {:.9} {:.9} {:.9}", n.x, n.y, n.z).unwrap();
        }
        for (i, t) in self.indices.chunks_exact(3).enumerate() {
            writeln!(
                text,
                "f {}//{} {}//{} {}//{}",
                t[0] + 1,
                i + 1,
                t[1] + 1,
                i + 1,
                t[2] + 1,
                i + 1
            )
            .unwrap();
        }
        text
    }
    /// Stable source fingerprint for native A/B evidence (not a cryptographic hash).
    pub fn fingerprint(&self) -> u64 {
        let mut h = 0xcbf29ce484222325u64;
        let mut add = |v: u32| {
            for b in v.to_le_bytes() {
                h = (h ^ u64::from(b)).wrapping_mul(0x100000001b3);
            }
        };
        for p in &self.positions {
            for v in p.to_array() {
                add(v.to_bits());
            }
        }
        for &i in &self.indices {
            add(i);
        }
        for p in self.normals.iter().chain(&self.colors) {
            for v in p.to_array() {
                add(v.to_bits());
            }
        }
        h
    }
    pub fn validate(&self) -> Result<()> {
        use std::collections::BTreeMap;
        ensure!(
            self.positions.len() >= 4 && self.indices.len().is_multiple_of(3),
            "invalid stone topology"
        );
        ensure!(
            self.normals.len() == self.indices.len() / 3 && self.colors.len() == self.normals.len(),
            "invalid face attributes"
        );
        ensure!(
            self.positions.iter().all(|p| p.is_finite())
                && self.min.is_finite()
                && self.max.is_finite(),
            "nonfinite stone"
        );
        ensure!(
            self.indices
                .iter()
                .all(|i| (*i as usize) < self.positions.len()),
            "invalid index"
        );
        let mut edges = BTreeMap::<(u32, u32), (u32, i32)>::new();
        let center = (self.min + self.max) * 0.5;
        for (face, t) in self.indices.chunks_exact(3).enumerate() {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| self.positions[i as usize]);
            let cross = (b - a).cross(c - a);
            let n = self.normals[face];
            ensure!(
                cross.length_squared() > 1e-18
                    && n.is_finite()
                    && (n.length() - 1.).abs() < 1e-5
                    && cross.dot(n) > 0.,
                "degenerate/invalid normal"
            );
            ensure!(
                n.dot(center - a) < 0. && self.positions.iter().all(|p| n.dot(*p - a) < 2e-6),
                "nonconvex or inward stone face={face} center_distance={} max_distance={} area2={}",
                n.dot(center - a),
                self.positions
                    .iter()
                    .map(|p| n.dot(*p - a))
                    .fold(f32::NEG_INFINITY, f32::max),
                cross.length()
            );
            ensure!(
                self.colors[face].is_finite()
                    && self.colors[face].cmpge(Vec3::ZERO).all()
                    && self.colors[face].cmple(Vec3::ONE).all(),
                "invalid sRGB color"
            );
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let e = edges.entry((a.min(b), a.max(b))).or_default();
                e.0 += 1;
                e.1 += if a < b { 1 } else { -1 };
            }
        }
        ensure!(
            edges
                .values()
                .all(|&(count, winding)| count == 2 && winding == 0),
            "open/nonmanifold stone"
        );
        ensure!(
            self.positions.len() as isize - edges.len() as isize + self.normals.len() as isize == 2,
            "not one sphere-topology solid"
        );
        ensure!(self.volume() > 1e-8, "nonpositive volume");
        let min = self
            .positions
            .iter()
            .copied()
            .fold(Vec3::splat(f32::INFINITY), Vec3::min);
        let max = self
            .positions
            .iter()
            .copied()
            .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
        ensure!(
            min == self.min && max == self.max && min.y == 0.,
            "incorrect local bounds/pivot"
        );
        Ok(())
    }
}

// SplitMix64 is fixed here rather than depending on a library RNG's evolving ABI.
struct Seed(u64);
impl Seed {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^= z >> 31;
        (z >> 40) as f64 / (1u64 << 24) as f64
    }
    fn signed(&mut self) -> f64 {
        self.next() * 2. - 1.
    }
}

/// Clip a convex blank; all retained planes contain a finite inner ball. No noise
/// displacement, welding of arbitrary meshes, or topology repair heuristic.
pub fn generate(spec: StoneSpec) -> Result<StoneMesh> {
    let spec = spec.sanitized();
    let mut rng = Seed(u64::from(spec.seed));
    let v = f64::from(spec.variation);
    let mut faces = box_faces();
    match spec.kind {
        StoneKind::Slab => {
            // Vertical corner cuts change only the outline of this cut stone.
            for x in [-1., 1.] {
                for z in [-1., 1.] {
                    let cut = f64::from(spec.slab.edge_cut) * (1. + rng.signed() * v * 0.5);
                    let n = DVec3::new(x, 0., z * (1. + rng.signed() * v * 0.2)).normalize();
                    clip(&mut faces, n, (n.x.abs() + n.z.abs()) - cut);
                }
            }
            // Narrow chamfers, not randomized mountain noise on the walkable top.
            for y in [-1., 1.] {
                for (x, z) in [(1., 0.), (-1., 0.), (0., 1.), (0., -1.)] {
                    let n = DVec3::new(x, y * 0.16, z).normalize();
                    let d =
                        (n.x.abs() + n.y.abs() + n.z.abs()) - 0.035 * (1. + rng.signed() * v * 0.3);
                    clip(&mut faces, n, d);
                }
            }
        }
        StoneKind::Rock => {
            // First eight cuts bound all corners, the remaining cuts break up
            // broad side faces. Asymmetric supports and small normal tilts make
            // the silhouette vary without ever inverting a face or the solid.
            let directions: Vec<_> = [-1., 1.]
                .into_iter()
                .flat_map(|x| {
                    [-1., 1.]
                        .into_iter()
                        .flat_map(move |y| [-1., 1.].into_iter().map(move |z| DVec3::new(x, y, z)))
                })
                .chain([
                    DVec3::new(1., 1., 0.),
                    DVec3::new(-1., 1., 0.),
                    DVec3::new(0., 1., 1.),
                    DVec3::new(0., 1., -1.),
                    DVec3::new(1., 0., 1.),
                    DVec3::new(-1., 0., 1.),
                    DVec3::new(1., 0., -1.),
                    DVec3::new(-1., 0., -1.),
                    DVec3::new(1., -1., 0.),
                    DVec3::new(-1., -1., 0.),
                    DVec3::new(0., -1., 1.),
                    DVec3::new(0., -1., -1.),
                ])
                .collect();
            for i in 0..spec.rock.facets as usize {
                let direction = if i < directions.len() {
                    directions[i]
                } else {
                    DVec3::new(rng.signed(), 0.6 + rng.next(), rng.signed())
                };
                let tilt = DVec3::new(rng.signed(), rng.signed(), rng.signed()) * v * 0.22;
                let n = (direction + tilt).normalize();
                let d = 1.04 + rng.signed() * v * 0.22;
                clip(&mut faces, n, d);
            }
        }
    }
    // Normalize to requested local bounds, without changing convexity/winding.
    let min = faces
        .iter()
        .flatten()
        .copied()
        .fold(DVec3::splat(f64::INFINITY), DVec3::min);
    let max = faces
        .iter()
        .flatten()
        .copied()
        .fold(DVec3::splat(f64::NEG_INFINITY), DVec3::max);
    let scale = spec.size.as_dvec3() / (max - min);
    let mut mesh = StoneMesh {
        spec,
        positions: Vec::new(),
        indices: Vec::new(),
        normals: Vec::new(),
        colors: Vec::new(),
        min: Vec3::ZERO,
        max: Vec3::ZERO,
    };
    let base_color = match spec.kind {
        StoneKind::Slab => Vec3::new(0.64, 0.61, 0.55),
        StoneKind::Rock => Vec3::new(0.49, 0.51, 0.50),
    };
    for polygon in faces {
        // Derive one flat normal from the whole f64 polygon, not from a tiny
        // f32 fan triangle (nearly collinear corners amplify rounding errors).
        let area = (1..polygon.len() - 1)
            .map(|i| {
                ((polygon[i] - polygon[0]) * scale).cross((polygon[i + 1] - polygon[0]) * scale)
            })
            .sum::<DVec3>();
        let normal = area.normalize().as_vec3();
        let mut ids = Vec::new();
        for p in polygon {
            let p = ((p - min) * scale - spec.size.as_dvec3() * DVec3::new(0.5, 0., 0.5)).as_vec3();
            let id = mesh
                .positions
                .iter()
                .position(|a| a.distance_squared(p) < 1e-16)
                .unwrap_or_else(|| {
                    mesh.positions.push(p);
                    mesh.positions.len() - 1
                });
            ids.push(id as u32);
        }
        let color = base_color * (1. + (rng.signed() * 0.045) as f32);
        for i in 1..ids.len() - 1 {
            let t = [ids[0], ids[i], ids[i + 1]];
            mesh.indices.extend(t);
            mesh.normals.push(normal);
            mesh.colors.push(color);
        }
    }
    mesh.min = mesh
        .positions
        .iter()
        .copied()
        .fold(Vec3::splat(f32::INFINITY), Vec3::min);
    mesh.max = mesh
        .positions
        .iter()
        .copied()
        .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
    mesh.validate()?;
    Ok(mesh)
}
fn box_faces() -> Vec<Vec<DVec3>> {
    let p = [
        DVec3::new(-1., -1., -1.),
        DVec3::new(1., -1., -1.),
        DVec3::new(1., 1., -1.),
        DVec3::new(-1., 1., -1.),
        DVec3::new(-1., -1., 1.),
        DVec3::new(1., -1., 1.),
        DVec3::new(1., 1., 1.),
        DVec3::new(-1., 1., 1.),
    ];
    [
        [1, 2, 6, 5],
        [4, 7, 3, 0],
        [3, 7, 6, 2],
        [4, 0, 1, 5],
        [5, 6, 7, 4],
        [0, 3, 2, 1],
    ]
    .map(|f| f.map(|i| p[i]).to_vec())
    .to_vec()
}
fn clip(faces: &mut Vec<Vec<DVec3>>, n: DVec3, d: f64) {
    let mut caps = Vec::<DVec3>::new();
    let mut retained = Vec::new();
    for polygon in faces.iter() {
        let mut next = Vec::new();
        for i in 0..polygon.len() {
            let a = polygon[i];
            let b = polygon[(i + 1) % polygon.len()];
            let da = n.dot(a) - d;
            let db = n.dot(b) - d;
            if da <= 0. {
                next.push(a);
            }
            if (da <= 0.) != (db <= 0.) {
                let p = a + (b - a) * (da / (da - db));
                next.push(p);
                if !caps.iter().any(|c| c.distance_squared(p) < 1e-20) {
                    caps.push(p);
                }
            }
        }
        if next.len() >= 3 {
            retained.push(next);
        }
    }
    if caps.len() >= 3 {
        let center = caps.iter().copied().sum::<DVec3>() / caps.len() as f64;
        let axis = if n.x.abs() < 0.8 { DVec3::X } else { DVec3::Y };
        let u = n.cross(axis).normalize();
        let v = n.cross(u);
        caps.sort_by(|a, b| {
            let a = *a - center;
            let b = *b - center;
            a.dot(v)
                .atan2(a.dot(u))
                .total_cmp(&b.dot(v).atan2(b.dot(u)))
        });
        retained.push(caps);
    }
    *faces = retained;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deterministic_many_seeds_are_closed_convex_positive_solids() {
        for kind in [StoneKind::Slab, StoneKind::Rock] {
            for seed in (0..64).chain([u32::MAX, 0x12345678]) {
                let spec = StoneSpec::new(kind, seed);
                let a = generate(spec).unwrap();
                let b = generate(spec).unwrap();
                assert_eq!(a, b);
                assert_eq!(a.fingerprint(), b.fingerprint());
                a.validate().unwrap();
                assert!(a.normals.len() <= 180);
                assert!(a.contains(Vec3::Y * spec.size.y * 0.5, 0.));
                assert!(!a.contains(Vec3::Y * (spec.size.y + 0.01), 0.));
                assert!((a.max - a.min).abs_diff_eq(spec.size, 1e-6));
            }
        }
    }
    #[test]
    fn seeds_and_type_have_real_geometry_variation() {
        for kind in [StoneKind::Slab, StoneKind::Rock] {
            let a = generate(StoneSpec::new(kind, 1)).unwrap();
            let b = generate(StoneSpec::new(kind, 2)).unwrap();
            assert_ne!(a.positions, b.positions);
        }
        let a = generate(StoneSpec::new(StoneKind::Slab, 1)).unwrap();
        let b = generate(StoneSpec::new(StoneKind::Rock, 1)).unwrap();
        assert_ne!(a.indices, b.indices);
    }
    #[test]
    fn slab_has_broad_horizontal_top_and_stable_flat_bottom() {
        for seed in 0..32 {
            let m = generate(StoneSpec::new(StoneKind::Slab, seed)).unwrap();
            let top_area: f32 = m
                .triangles()
                .zip(&m.normals)
                .filter(|(t, n)| n.y > 0.99999 && (t[0].y - m.max.y).abs() < 1e-7)
                .map(|([a, b, c], _)| (b - a).cross(c - a).length() * 0.5)
                .sum();
            assert!(top_area > m.spec.size.x * m.spec.size.z * 0.7);
            assert!(m.positions.iter().filter(|p| p.y == 0.).count() >= 4);
        }
    }
    #[test]
    fn extreme_and_nonfinite_params_normalize_before_generation() {
        for kind in [StoneKind::Slab, StoneKind::Rock] {
            for value in [
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
                -100.,
                0.,
                f32::MAX,
            ] {
                for facets in [0, 8, 24, u32::MAX] {
                    let mut spec = StoneSpec::new(kind, 3);
                    spec.size = Vec3::splat(value);
                    spec.variation = value;
                    spec.slab.edge_cut = value;
                    spec.rock.facets = facets;
                    let m = generate(spec).unwrap();
                    m.validate().unwrap();
                    assert_eq!(m.spec, m.spec.sanitized());
                }
            }
        }
    }
    #[test]
    fn variation_limits_and_facet_budgets_stay_safe() {
        for kind in [StoneKind::Slab, StoneKind::Rock] {
            for seed in 0..32 {
                for variation in [0., 1.] {
                    for facets in [8, 24] {
                        let mut s = StoneSpec::new(kind, seed);
                        s.variation = variation;
                        s.rock.facets = facets;
                        s.slab.edge_cut = 0.34;
                        generate(s)
                            .unwrap_or_else(|e| panic!("{s:?}: {e:#}"))
                            .validate()
                            .unwrap();
                    }
                }
            }
        }
    }
    #[test]
    #[ignore = "explicit offline gallery/OBJ export, not a normal test side effect"]
    fn export_stone_gallery() {
        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/research/stone-models");
        std::fs::create_dir_all(&directory).unwrap();
        let mut models = Vec::new();
        for kind in [StoneKind::Slab, StoneKind::Rock] {
            for seed in [42, 7, 122, 65535] {
                let mesh = generate(StoneSpec::new(kind, seed)).unwrap();
                let name = format!(
                    "{}-{seed}",
                    if kind == StoneKind::Slab {
                        "slab"
                    } else {
                        "rock"
                    }
                );
                std::fs::write(directory.join(format!("{name}.obj")), mesh.obj()).unwrap();
                models.push(serde_json::json!({
                    "name": name, "kind": format!("{kind:?}"), "seed": seed,
                    "fingerprint": format!("{:016x}", mesh.fingerprint()),
                    "positions": mesh.positions.iter().map(|p| p.to_array()).collect::<Vec<_>>(),
                    "indices": mesh.indices,
                    "normals": mesh.normals.iter().map(|p| p.to_array()).collect::<Vec<_>>(),
                    "colors": mesh.colors.iter().map(|p| p.to_array()).collect::<Vec<_>>(),
                    "size": mesh.spec.size.to_array(),
                }));
            }
        }
        let data = format!("// Generated by cargo test export_stone_gallery -- --ignored; do not hand-edit.\nwindow.STONE_GALLERY = {};\n", serde_json::to_string(&models).unwrap());
        std::fs::write(
            directory
                .parent()
                .unwrap()
                .join("procedural-stone-gallery-data.js"),
            data,
        )
        .unwrap();
    }

    #[test]
    fn obj_is_an_ordinary_indexed_triangle_model() {
        let mesh = generate(StoneSpec::new(StoneKind::Rock, 42)).unwrap();
        let obj = mesh.obj();
        assert_eq!(
            obj.lines().filter(|l| l.starts_with("v ")).count(),
            mesh.positions.len()
        );
        assert_eq!(
            obj.lines().filter(|l| l.starts_with("vn ")).count(),
            mesh.normals.len()
        );
        assert_eq!(
            obj.lines().filter(|l| l.starts_with("f ")).count(),
            mesh.normals.len()
        );
        assert!(!obj.contains("voxel"));
    }
}

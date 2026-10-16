//! Wall consumer of the shared support, bounded mesh and atlas-publication seam.
use super::*;
use crate::app::path_band::contains_polygon;
mod guard_review;
mod review;

pub(super) fn ground_wall_cell(
    recipe: LowWallRecipe,
    row: PathSample,
    world_size: Vec3,
    ray: impl Fn(Vec3, Vec3) -> Option<crate::builder::ContreeCpuRayHit>,
) -> Result<Option<GroundedPiece>> {
    let slices = recipe.pieces(row);
    if slices.len() != recipe.layers() {
        return Ok(None);
    }
    let raw = recipe.raw_cell(row);
    let Some(support) = ground_support(raw, &ray) else {
        return Ok(None);
    };
    let base = support.point;
    let reach = recipe.width.max(recipe.pitch) + 4. * VOXEL_SIZE;
    let mut triangles = Vec::with_capacity(slices.len() * 12);
    for slice in slices {
        let mut heights = [0.; 4];
        for (i, p) in slice.cell.footprint.iter().enumerate() {
            let expected = base + support.plane(*p);
            let Some(hit) = ray(expected + Vec3::Y * reach, Vec3::NEG_Y).filter(|h| {
                is_ground(h.voxel_type)
                    && (h.position.y - expected.y).abs()
                        <= recipe.width.min(recipe.pitch) * 0.3 + 2. * VOXEL_SIZE
            }) else {
                return Ok(None);
            };
            heights[i] = hit.position.y;
        }
        let solid = slice.solid(heights);
        ensure!(
            solid
                .triangles()
                .map(|[a, b, c]| {
                    let a = a - base;
                    let b = b - base;
                    let c = c - base;
                    a.as_dvec3().dot(b.as_dvec3().cross(c.as_dvec3())) / 6.
                })
                .sum::<f64>()
                > 1e-10,
            "wall primitive has no positive volume"
        );
        ensure!(
            solid.positions.iter().all(|p| contains_polygon(
                &raw.footprint,
                glam::Vec2::new(p.x, p.z),
                1e-6
            )),
            "wall primitive escaped the shared path band"
        );
        triangles.extend(solid.triangles().map(|p| p.map(|v| v - base)));
    }
    grounded_cell(
        PathCell {
            center: base,
            ..raw
        },
        base,
        world_size,
        triangles.into_iter(),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    fn flat(p: Vec3, d: Vec3) -> Option<crate::builder::ContreeCpuRayHit> {
        Some(crate::builder::ContreeCpuRayHit {
            position: p + d * ((0.5 - p.y) / d.y),
            voxel_type: VOXEL_TYPE_DIRT,
        })
    }
    #[test]
    fn low_wall_coverage_accepts_shared_sections_but_rejects_retrace_and_crossing() {
        for kind in [LowWallKind::Earth, LowWallKind::Stone, LowWallKind::Brick] {
            for width in [0.05, 0.06, 0.15] {
                for pitch in [0.025, 0.05, 0.1] {
                    let wall = LowWallRecipe::new(kind, width, 0.12, pitch, 42);
                    let recipe = ConstructionRecipe::from(wall);
                    let mut stroke = recipe.stroke(Vec3::ONE, Vec3::X).unwrap();
                    for i in 1..=45 {
                        let angle = std::f32::consts::FRAC_PI_2 * i as f32 / 45.;
                        assert!(stroke.push(
                            Vec3::ONE
                                + Vec3::new(
                                    width * 1.5 * angle.sin(),
                                    0.,
                                    width * 1.5 * (1. - angle.cos())
                                )
                        ));
                    }
                    assert!(stroke.finish());
                    let mut coverage = PavementCoverage::default();
                    for row in stroke.rows {
                        let cell = wall.raw_cell(row);
                        assert!(
                            recipe.accepts(&coverage, cell),
                            "shared wall section rejected"
                        );
                        coverage.record(cell);
                        assert!(!recipe.accepts(&coverage, cell), "self-retrace permitted");
                    }
                    let first = coverage.published_pieces()[0];
                    let mut crossing = first;
                    for p in &mut crossing.footprint {
                        p.x += wall.pitch * 0.25;
                    }
                    assert!(
                        !recipe.accepts(&coverage, crossing),
                        "interior overlap permitted"
                    );
                }
            }
        }
    }

    #[test]
    fn low_wall_bounds_protect_upper_obstacles_without_borrowing_paving_depth_policy() {
        let recipe = LowWallRecipe::new(LowWallKind::Brick, 0.06, 0.12, 0.05, 42);
        let row = PathSample::straight(Vec3::new(1., 0.5, 1.), Vec3::X, 0., 0, recipe.pitch);
        let mesh = ground_wall_cell(recipe, row, Vec3::splat(2.), flat)
            .unwrap()
            .unwrap();
        assert_eq!(path_protection_depth(recipe.into(), mesh.piece), 0.);
        let paving = PavementRecipe::new(PavementKind::Slabs, 0.12, 0.05, 42);
        assert!(
            (path_protection_depth(paving.into(), mesh.piece) - (0.06 + 4. * VOXEL_SIZE)).abs()
                < 1e-6
        );
        let offset = ((mesh.min - Vec3::splat(VOXEL_SIZE)) * 256.)
            .floor()
            .as_uvec3();
        let dim = ((mesh.max + Vec3::splat(VOXEL_SIZE)) * 256.)
            .ceil()
            .as_uvec3()
            - offset;
        let obstacle = UVec3::new(256, (0.59 * 256.) as u32, 256);
        let p = obstacle - offset;
        let i = ((p.z * dim.y + p.y) * dim.x + p.x) as usize;
        let mut bytes = vec![0; (dim.x * dim.y * dim.z) as usize];
        bytes[i] = VOXEL_TYPE_ROCK as u8;
        assert!(atlas_blocks_piece(
            &mesh,
            offset,
            dim,
            &bytes,
            &HashSet::new()
        ));
        assert!(!atlas_blocks_piece(
            &mesh,
            offset,
            dim,
            &bytes,
            &HashSet::from([obstacle])
        ));
    }
    #[test]
    fn low_wall_grounding_checks_entire_height_support_and_protected_materials() {
        for kind in [LowWallKind::Earth, LowWallKind::Stone, LowWallKind::Brick] {
            let recipe = LowWallRecipe::new(kind, 0.06, 0.12, 0.05, 42);
            let row = PathSample::straight(Vec3::new(1., 0.5, 1.), Vec3::X, 0., 0, recipe.pitch);
            let mesh = ground_wall_cell(recipe, row, Vec3::splat(2.), flat)
                .unwrap()
                .unwrap();
            assert!((mesh.max.y - 0.62).abs() < 1e-6);
            assert_eq!(mesh.triangles.len(), recipe.layers() * 12);
            assert!(ground_wall_cell(recipe, row, Vec3::new(2., 0.6, 2.), flat)
                .unwrap()
                .is_none());
            assert!(ground_wall_cell(recipe, row, Vec3::splat(2.), |p, d| {
                if p.x > 1.01 {
                    None
                } else {
                    flat(p, d)
                }
            })
            .unwrap()
            .is_none());
            assert!(ground_wall_cell(recipe, row, Vec3::splat(2.), |p, d| {
                flat(p, d).map(|h| crate::builder::ContreeCpuRayHit {
                    voxel_type: VOXEL_TYPE_ROCK,
                    ..h
                })
            })
            .unwrap()
            .is_none());
        }
    }
}

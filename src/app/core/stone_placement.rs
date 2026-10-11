//! Stone placement is an ordinary atlas mutation, not a separate scene object.
//! Snapshot persistence and Dig therefore operate on the exact placed voxels.
use super::{player_tools::PlayerTool, App, BuildEdit, VisibleTerrainChange};
use crate::{
    builder::ModelTriangleGpu,
    stone_models::{generate, RockParams, SlabParams, StoneKind, StoneSpec},
};
use anyhow::{ensure, Context, Result};
use glam::{Quat, Vec3};

use crate::app::gui_config::GuiAdjustables;

pub(super) fn spec_for_kind(settings: &GuiAdjustables, kind: StoneKind) -> StoneSpec {
    StoneSpec {
        kind,
        seed: settings.stone_seed.value,
        size: Vec3::new(
            settings.stone_width.value,
            if kind == StoneKind::Slab {
                settings.stone_slab_thickness.value
            } else {
                settings.stone_rock_height.value
            },
            settings.stone_depth.value,
        ),
        variation: settings.stone_variation.value,
        slab: SlabParams {
            edge_cut: settings.stone_slab_edge_cut.value,
        },
        rock: RockParams {
            facets: settings.stone_rock_facets.value,
        },
    }
    .sanitized()
}

fn next_seed(seed: u32) -> u32 {
    seed.wrapping_add(1) & 0xffff
}

const VOXEL_SIZE: f32 = 1. / 256.;

// A delayed click must use its captured answer, never the current camera ray or hover.
// Tree hits occlude terrain; they cannot authorize placing a stone behind the trunk.
fn captured_contact(pick: Option<super::terrain_picking::Resolved>) -> Option<(Vec3, Vec3)> {
    let pick = pick?;
    let hit = pick.hit?;
    (hit.tree_primitive.is_none()
        && (0.0..=super::SHOVEL_RAY_QUERY_DISTANCE).contains(&hit.distance)
        && hit.world_position.is_finite())
    .then_some((hit.world_position, pick.ray.direction))
}

/// Fit a contact plane from nearby surface-ray hits. Contree stores surface
/// voxels only; treating its occupancy as a solid volume gives sideways normals.
pub(super) fn surface_normal(
    position: Vec3,
    incoming: Vec3,
    ray: impl Fn(Vec3, Vec3) -> Option<Vec3>,
) -> Option<Vec3> {
    let direction = incoming.normalize_or_zero();
    if direction == Vec3::ZERO || !direction.is_finite() {
        return None;
    }
    let tangent = direction.any_orthonormal_vector();
    let bitangent = direction.cross(tangent);
    let radius = 4. * VOXEL_SIZE;
    let sample = |offset: Vec3| {
        ray(position + offset - direction * (4. * radius), direction)
            .filter(|point| point.is_finite() && point.distance(position) < 12. * radius)
    };
    let span = |axis: Vec3| match (sample(axis * radius), sample(-axis * radius)) {
        (Some(plus), Some(minus)) => Some(plus - minus),
        (Some(plus), None) => Some(plus - position),
        (None, Some(minus)) => Some(position - minus),
        _ => None,
    };
    let mut normal = span(tangent)?.cross(span(bitangent)?).normalize_or_zero();
    if normal == Vec3::ZERO {
        return None;
    }
    if normal.dot(-direction) < 0. {
        normal = -normal;
    }
    Some(normal)
}

struct Placement {
    triangles: Vec<ModelTriangleGpu>,
    base: Vec3,
    rotation: Quat,
    voxel_type: u32,
}
impl Placement {
    fn compile(
        spec: StoneSpec,
        hit: Vec3,
        normal: Vec3,
        yaw: f32,
        world_size: Vec3,
        scale: f32,
    ) -> Result<Self> {
        ensure!(
            hit.is_finite()
                && normal.is_finite()
                && normal.length_squared() > 0.
                && yaw.is_finite(),
            "invalid stone contact pose"
        );
        ensure!(
            scale.is_finite() && scale > 0.,
            "invalid stone placement scale"
        );
        let normal = normal.normalize();
        let rotation =
            Quat::from_rotation_arc(Vec3::Y, normal) * Quat::from_rotation_y(yaw.to_radians());
        let mesh = generate(spec)?;
        // Keep small slabs visible: join by at most a quarter of their height.
        let embedding = VOXEL_SIZE.min(mesh.spec.size.y * scale * 0.25);
        let base = hit - normal * embedding;
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        let triangles = mesh
            .triangles()
            .map(|triangle| {
                let points = triangle.map(|point| {
                    let local = rotation * (point * scale);
                    min = min.min(base + local);
                    max = max.max(base + local);
                    local.extend(0.).to_array()
                });
                ModelTriangleGpu {
                    a: points[0],
                    b: points[1],
                    c: points[2],
                }
            })
            .collect();
        // Reject rather than silently clipping a model at the unlocked world edge.
        ensure!(
            (min - Vec3::splat(VOXEL_SIZE)).cmpge(Vec3::ZERO).all()
                && (max + Vec3::splat(VOXEL_SIZE)).cmplt(world_size).all(),
            "stone would extend beyond editable terrain"
        );
        Ok(Self {
            triangles,
            base,
            rotation,
            voxel_type: crate::builder::VOXEL_TYPE_ROCK,
        })
    }
}
impl App {
    pub(super) fn try_stone_placement(&mut self) {
        if !self.terrain_edit_pointer_available()
            || self.player_tools.selected_tool() != PlayerTool::Stone
            || self.rooftop_scene.is_some()
            || !self.terrain_persistence.can_start_operation()
        {
            return;
        }
        let Some((position, direction)) = captured_contact(self.pointer_picking.active) else {
            return;
        };
        let snapshot = self.contree_builder.cpu_ray_query_snapshot();
        let Some(normal) = surface_normal(position, direction, |origin, direction| {
            snapshot
                .query_terrain_ray_cpu(origin, direction)
                .map(|hit| hit.position)
        }) else {
            log::warn!("Stone placement rejected: no reliable local surface plane");
            return;
        };
        if let Err(error) = self.place_stone_at(position, normal) {
            log::warn!("Stone placement rejected: {error:#}");
        }
    }

    /// Opt-in production-path fixture; only the explicit --terrain-save writes a snapshot.
    pub(super) fn seed_stone_placement_review(&mut self) -> Result<()> {
        if std::env::var("RE_FLORA_STONE_PLACEMENT_REVIEW").as_deref() != Ok("seed") {
            return Ok(());
        }
        let saved = (
            self.debug_settings.adjustables.stone_place_slab.value,
            self.debug_settings.adjustables.stone_width.value,
            self.debug_settings.adjustables.stone_depth.value,
            self.debug_settings.adjustables.stone_slab_thickness.value,
            self.debug_settings.adjustables.stone_rock_height.value,
            self.debug_settings.adjustables.stone_yaw.value,
            self.debug_settings.adjustables.stone_seed.value,
        );
        let result = (|| {
            for (slab, x) in [(false, 0.6), (true, 1.3)] {
                let settings = &mut self.debug_settings.adjustables;
                settings.stone_place_slab.value = slab;
                settings.stone_width.value = 0.20;
                settings.stone_depth.value = 0.18;
                settings.stone_slab_thickness.value = 0.035;
                settings.stone_rock_height.value = 0.18;
                settings.stone_yaw.value = 55.;
                if !slab {
                    settings.stone_seed.value = 42;
                }
                let used_seed = settings.stone_seed.value;
                let hit = self
                    .query_terrain_ray_cpu(Vec3::new(x, 2.5, 0.6), Vec3::NEG_Y)
                    .context("stone review requires terrain contact")?;
                let snapshot = self.contree_builder.cpu_ray_query_snapshot();
                let normal = surface_normal(hit.position, Vec3::NEG_Y, |origin, direction| {
                    snapshot
                        .query_terrain_ray_cpu(origin, direction)
                        .map(|hit| hit.position)
                })
                .context("stone review requires a reliable contact plane")?;
                self.place_stone_at(hit.position, normal)?;
                ensure!(
                    self.debug_settings.adjustables.stone_seed.value == next_seed(used_seed),
                    "successful placement must advance the next seed"
                );
                let height = (if slab { 0.035 } else { 0.18 })
                    * self.debug_settings.adjustables.stone_placement_scale.value;
                let embedding = VOXEL_SIZE.min(height * 0.25);
                let sample = (hit.position + normal * (height * 0.4 - embedding)) * 256.;
                let raw = self
                    .plain_builder
                    .read_chunk_atlas_region(sample.floor().as_uvec3(), glam::UVec3::ONE)?;
                let expected = crate::builder::VOXEL_TYPE_ROCK;
                ensure!(
                    u32::from(raw[0]) & crate::builder::VOXEL_TYPE_MASK as u32 == expected,
                    "stone stamp did not write its solid interior material"
                );
                log::info!("[STONE_PLACEMENT_REVIEW] slab={slab} interior_material={expected} atlas_write=verified");
            }
            Ok(())
        })();
        let settings = &mut self.debug_settings.adjustables;
        settings.stone_place_slab.value = saved.0;
        settings.stone_width.value = saved.1;
        settings.stone_depth.value = saved.2;
        settings.stone_slab_thickness.value = saved.3;
        settings.stone_rock_height.value = saved.4;
        settings.stone_yaw.value = saved.5;
        settings.stone_seed.value = saved.6;
        result
    }

    fn place_stone_at(&mut self, hit: Vec3, normal: Vec3) -> Result<()> {
        let settings = &self.debug_settings.adjustables;
        let kind = if settings.stone_place_slab.value {
            StoneKind::Slab
        } else {
            StoneKind::Rock
        };
        let scale = settings.stone_placement_scale.value;
        let seed = settings.stone_seed.value;
        let placement = Placement::compile(
            spec_for_kind(settings, kind),
            hit,
            normal,
            settings.stone_yaw.value,
            self.world_chunk_dim.as_vec3(),
            scale,
        )?;
        let bound = self.plain_builder.voxelize_model(
            &placement.triangles,
            placement.base,
            placement.voxel_type,
        )?;
        self.publish_visible_terrain(
            VisibleTerrainChange::from_build_edits(vec![BuildEdit::RebuildMeshWithoutFlora(
                bound,
            )])?
            .context("stone placement requires a visible terrain region")?,
        )?;
        self.observe_player_terrain_publication_for_connectivity(bound);
        self.debug_settings.adjustables.stone_seed.value = next_seed(seed);
        log::info!("[STONE_PLACEMENT] kind={kind:?} base={:?} normal={normal:?} rotation={:?} material={} scale={scale} seed={seed} next_seed={} persistence=terrain-atlas editable=true", placement.base, placement.rotation, placement.voxel_type, next_seed(seed));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{gui_config::GuiAdjustables, gui_config_loader::GuiConfigLoader};

    #[test]
    fn stone_uses_the_captured_pick_and_respects_misses_tree_occlusion_and_range() {
        use crate::tracer::{SurfacePickHit, TerrainRayQuery};
        let ray = TerrainRayQuery {
            origin: Vec3::Y,
            direction: Vec3::NEG_Y,
        };
        let hit = SurfacePickHit {
            world_position: Vec3::ZERO,
            rest_position: Vec3::ZERO,
            distance: 1.,
            voxel_type: crate::builder::VOXEL_TYPE_DIRT,
            tree_primitive: None,
        };
        let pick = super::super::terrain_picking::Resolved {
            ray,
            hit: Some(hit),
        };
        assert_eq!(
            captured_contact(Some(pick)),
            Some((Vec3::ZERO, Vec3::NEG_Y))
        );
        assert_eq!(captured_contact(None), None);
        assert_eq!(
            captured_contact(Some(super::super::terrain_picking::Resolved {
                hit: None,
                ..pick
            })),
            None
        );
        for blocked in [
            SurfacePickHit {
                tree_primitive: Some(0),
                ..hit
            },
            SurfacePickHit {
                distance: super::super::SHOVEL_RAY_QUERY_DISTANCE + 1.,
                ..hit
            },
            SurfacePickHit {
                distance: f32::NAN,
                ..hit
            },
        ] {
            assert_eq!(
                captured_contact(Some(super::super::terrain_picking::Resolved {
                    hit: Some(blocked),
                    ..pick
                })),
                None
            );
        }
    }

    #[test]
    fn every_successful_placement_gets_a_different_in_range_next_seed() {
        for seed in [0, 1, 42, 65534, 65535] {
            assert_ne!(next_seed(seed), seed);
            assert!(next_seed(seed) <= 65535);
        }
        assert_eq!(next_seed(65535), 0);
    }

    #[test]
    fn sparse_flat_surface_does_not_turn_slab_towards_the_camera() {
        let incoming = Vec3::new(0., -0.25, -1.).normalize();
        // Ray hits expose the sparse shell, regardless of missing interiors.
        let normal = surface_normal(Vec3::ZERO, incoming, |origin, direction| {
            let distance = -origin.y / direction.y;
            (distance >= 0.).then_some(origin + direction * distance)
        })
        .unwrap();
        assert!(
            normal.angle_between(Vec3::Y) < 0.03,
            "flat ground normal became {normal:?}"
        );
        let slab = Placement::compile(
            StoneSpec::new(StoneKind::Slab, 42),
            Vec3::ONE,
            normal,
            90.,
            Vec3::splat(2.),
            1.,
        )
        .unwrap();
        assert!((slab.rotation * Vec3::Y).distance(Vec3::Y) < 0.03);
    }

    #[test]
    fn surface_normal_follows_flat_sloped_and_vertical_contact_planes() {
        for expected in [Vec3::Y, Vec3::new(-0.4, 1., -0.2).normalize(), Vec3::X] {
            let actual = surface_normal(Vec3::ZERO, -expected, |origin, direction| {
                let distance = -origin.dot(expected) / direction.dot(expected);
                (distance >= 0.).then_some(origin + direction * distance)
            })
            .unwrap();
            assert!(
                actual.angle_between(expected) < 0.12,
                "expected={expected:?} actual={actual:?}"
            );
        }
    }

    #[test]
    fn both_stone_types_align_local_up_to_contact_normal_and_reject_clipping() {
        let normal = Vec3::new(-0.3, 1., 0.2).normalize();
        for kind in [StoneKind::Rock, StoneKind::Slab] {
            let spec = StoneSpec::new(kind, 42);
            let placement =
                Placement::compile(spec, Vec3::ONE, normal, 55., Vec3::splat(2.), 1.).unwrap();
            assert!((placement.rotation * Vec3::Y).distance(normal) < 1e-5);
            assert_eq!(placement.base, Vec3::ONE - normal * VOXEL_SIZE);
            assert!(!placement.triangles.is_empty());
            assert_eq!(
                placement.voxel_type,
                crate::builder::VOXEL_TYPE_ROCK,
                "Rock and Slab must both use the garden's stone material"
            );
            assert!(Placement::compile(
                spec,
                Vec3::new(0., 1., 1.),
                normal,
                0.,
                Vec3::splat(2.),
                1.
            )
            .is_err());
            assert!(
                Placement::compile(spec, Vec3::ONE, Vec3::ZERO, 0., Vec3::splat(2.), 1.).is_err()
            );
        }
    }

    #[test]
    fn shared_scale_shrinks_both_models_uniformly_without_dimension_clamping() {
        for kind in [StoneKind::Rock, StoneKind::Slab] {
            let spec = StoneSpec::new(kind, 42);
            let full =
                Placement::compile(spec, Vec3::ONE, Vec3::Y, 55., Vec3::splat(2.), 1.).unwrap();
            let small =
                Placement::compile(spec, Vec3::ONE, Vec3::Y, 55., Vec3::splat(2.), 0.2).unwrap();
            for (a, b) in full.triangles.iter().zip(&small.triangles) {
                for (full, small) in [(a.a, b.a), (a.b, b.b), (a.c, b.c)] {
                    assert!(
                        Vec3::from_slice(&small).distance(Vec3::from_slice(&full) * 0.2) < 1e-6
                    );
                }
            }
            assert_eq!(full.rotation, small.rotation);
            assert!(1. - small.base.y <= spec.size.y * 0.2 * 0.25 + 1e-6);
            assert!(Placement::compile(spec, Vec3::ONE, Vec3::Y, 0., Vec3::splat(2.), 0.).is_err());
        }
    }

    #[test]
    fn placement_kind_uses_type_specific_dimensions() {
        let mut settings = GuiAdjustables::from_config(&GuiConfigLoader::load());
        settings.stone_width.value = 0.24;
        settings.stone_depth.value = 0.18;
        settings.stone_slab_thickness.value = 0.03;
        settings.stone_rock_height.value = 0.2;
        for preview in [0, 1] {
            settings.stone_place_slab.value = preview == 0;
            let rock = spec_for_kind(&settings, StoneKind::Rock);
            let slab = spec_for_kind(&settings, StoneKind::Slab);
            assert_eq!(rock.size.y, 0.2);
            assert_eq!(slab.size.y, 0.03);
            assert_eq!(settings.stone_place_slab.value, preview == 0);
        }
    }
}

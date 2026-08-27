#![allow(dead_code)]

//! Double-precision reference for voxel material-transition queries and smooth dielectric optics.
//! Production GPU queries mirror this contract; this module remains the offline correctness oracle.

use crate::voxel_material::{material_for, VoxelMaterialMode, VoxelSurfaceClass};
use glam::{DVec3, IVec3, UVec3};

const DDA_TIE_EPSILON: f64 = 1.0e-10;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SceneRay {
    pub origin: DVec3,
    pub direction: DVec3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct QueryMaterial {
    pub voxel_type: u32,
    pub surface_class: VoxelSurfaceClass,
    pub ior: f64,
    pub sigma_a: DVec3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MediumSegment {
    pub material: QueryMaterial,
    pub start: DVec3,
    pub end: DVec3,
    pub distance_world: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct InterfaceEvent {
    pub position: DVec3,
    pub incident_face_normal: DVec3,
    pub from_cell: IVec3,
    pub to_cell: IVec3,
    pub from_material: QueryMaterial,
    pub to_material: QueryMaterial,
    pub tied_axes: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OpaqueHit {
    pub position: DVec3,
    pub incident_face_normal: DVec3,
    pub cell: IVec3,
    pub voxel_type: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QueryTermination {
    Miss,
    Opaque,
    EventBudget,
    StepBudget,
    InvalidRay,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MediaWalk {
    pub segments: Vec<MediumSegment>,
    pub interfaces: Vec<InterfaceEvent>,
    pub opaque_hit: Option<OpaqueHit>,
    pub termination: QueryTermination,
    pub dda_steps: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PrimarySurfaceQuery {
    pub glass_front: Option<InterfaceEvent>,
    pub opaque_hit: Option<OpaqueHit>,
    pub termination: QueryTermination,
    pub dda_steps: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PathBudget {
    pub max_active_paths: usize,
    pub max_interface_events: u32,
    pub max_scene_queries: u32,
    pub max_dda_steps_per_query: u32,
    pub throughput_cutoff: f64,
}

impl Default for PathBudget {
    fn default() -> Self {
        Self {
            max_active_paths: 4,
            max_interface_events: 8,
            max_scene_queries: 8,
            max_dda_steps_per_query: 2_048,
            throughput_cutoff: 1.0e-2,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct QueryDiagnostics {
    pub scene_queries: u32,
    pub dda_steps: u32,
    pub interface_events: u32,
    pub peak_active_paths: u32,
    pub tir_events: u32,
    pub throughput_cutoffs: u32,
    pub top_k_pruned: u32,
    pub query_budget_fallbacks: u32,
    pub budget_exhaustions: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RadianceResult {
    pub radiance: DVec3,
    pub diagnostics: QueryDiagnostics,
}

#[derive(Clone, Debug)]
pub(crate) struct DenseVoxelScene {
    dimensions: UVec3,
    voxel_size_world: f64,
    voxel_types: Vec<u8>,
    material_mode: VoxelMaterialMode,
}

impl DenseVoxelScene {
    pub(crate) fn new(
        dimensions: UVec3,
        voxel_size_world: f64,
        voxel_types: Vec<u8>,
        material_mode: VoxelMaterialMode,
    ) -> Self {
        assert!(voxel_size_world.is_finite() && voxel_size_world > 0.0);
        assert_eq!(
            voxel_types.len(),
            dimensions.x as usize * dimensions.y as usize * dimensions.z as usize,
        );
        Self {
            dimensions,
            voxel_size_world,
            voxel_types,
            material_mode,
        }
    }

    pub(crate) fn walk_voxel_media(
        &self,
        ray: SceneRay,
        event_budget: u32,
        step_budget: u32,
    ) -> MediaWalk {
        let mut walk = MediaWalk {
            segments: Vec::new(),
            interfaces: Vec::new(),
            opaque_hit: None,
            termination: QueryTermination::InvalidRay,
            dda_steps: 0,
        };
        let direction_length = ray.direction.length();
        if !ray.origin.is_finite()
            || !ray.direction.is_finite()
            || !direction_length.is_finite()
            || direction_length <= f64::EPSILON
        {
            return walk;
        }
        let direction = ray.direction / direction_length;
        let ray = SceneRay {
            origin: ray.origin,
            direction,
        };
        let Some((entry, exit, entry_normal)) = self.clip_ray_to_domain(ray) else {
            walk.termination = QueryTermination::Miss;
            return walk;
        };

        let sample_epsilon = self.voxel_size_world * 1.0e-8;
        let sample_t = (entry + sample_epsilon).min((entry + exit) * 0.5);
        let sample_position = ray.origin + direction * sample_t;
        let domain_max = self.dimensions.as_dvec3() * self.voxel_size_world
            - DVec3::splat(self.voxel_size_world * 1.0e-9);
        let mut cell = (sample_position.clamp(DVec3::ZERO, domain_max) / self.voxel_size_world)
            .floor()
            .as_ivec3();
        let step = direction.signum().as_ivec3();
        let delta = DVec3::new(
            axis_delta(direction.x, self.voxel_size_world),
            axis_delta(direction.y, self.voxel_size_world),
            axis_delta(direction.z, self.voxel_size_world),
        );
        let mut next = DVec3::new(
            axis_next(
                ray.origin.x,
                direction.x,
                cell.x,
                step.x,
                self.voxel_size_world,
            ),
            axis_next(
                ray.origin.y,
                direction.y,
                cell.y,
                step.y,
                self.voxel_size_world,
            ),
            axis_next(
                ray.origin.z,
                direction.z,
                cell.z,
                step.z,
                self.voxel_size_world,
            ),
        );
        let mut current_material = self.material_at(cell);
        let mut current_t = entry;

        if entry > 0.0 {
            if current_material.surface_class == VoxelSurfaceClass::Opaque {
                walk.opaque_hit = Some(OpaqueHit {
                    position: ray.origin + direction * entry,
                    incident_face_normal: entry_normal,
                    cell,
                    voxel_type: current_material.voxel_type,
                });
                walk.termination = QueryTermination::Opaque;
                return walk;
            }
            if current_material.surface_class == VoxelSurfaceClass::Dielectric {
                if event_budget == 0 {
                    walk.termination = QueryTermination::EventBudget;
                    return walk;
                }
                walk.interfaces.push(InterfaceEvent {
                    position: ray.origin + direction * entry,
                    incident_face_normal: entry_normal,
                    from_cell: cell - step,
                    to_cell: cell,
                    from_material: self.empty_material(),
                    to_material: current_material,
                    tied_axes: normal_axis_mask(entry_normal),
                });
            }
        }

        loop {
            if walk.dda_steps >= step_budget {
                walk.termination = QueryTermination::StepBudget;
                return walk;
            }
            walk.dda_steps += 1;

            let crossing = next.x.min(next.y.min(next.z)).min(exit);
            let segment_end = ray.origin + direction * crossing;
            self.push_segment(
                &mut walk.segments,
                current_material,
                ray.origin + direction * current_t,
                segment_end,
                crossing - current_t,
            );

            let tied_axes = crossing_axis_mask(next, crossing);
            let incident_face_normal = incident_normal(tied_axes, step);
            if crossing >= exit - DDA_TIE_EPSILON {
                if current_material.surface_class == VoxelSurfaceClass::Dielectric {
                    if walk.interfaces.len() as u32 >= event_budget {
                        walk.termination = QueryTermination::EventBudget;
                        return walk;
                    }
                    walk.interfaces.push(InterfaceEvent {
                        position: segment_end,
                        incident_face_normal,
                        from_cell: cell,
                        to_cell: cell + step,
                        from_material: current_material,
                        to_material: self.empty_material(),
                        tied_axes,
                    });
                }
                walk.termination = QueryTermination::Miss;
                return walk;
            }

            let previous_cell = cell;
            advance_tied_axes(&mut cell, &mut next, step, delta, tied_axes);
            let next_material = self.material_at(cell);
            if next_material.surface_class == VoxelSurfaceClass::Opaque {
                if current_material.surface_class == VoxelSurfaceClass::Dielectric {
                    if walk.interfaces.len() as u32 >= event_budget {
                        walk.termination = QueryTermination::EventBudget;
                        return walk;
                    }
                    walk.interfaces.push(InterfaceEvent {
                        position: segment_end,
                        incident_face_normal,
                        from_cell: previous_cell,
                        to_cell: cell,
                        from_material: current_material,
                        to_material: next_material,
                        tied_axes,
                    });
                }
                walk.opaque_hit = Some(OpaqueHit {
                    position: segment_end,
                    incident_face_normal,
                    cell,
                    voxel_type: next_material.voxel_type,
                });
                walk.termination = QueryTermination::Opaque;
                return walk;
            }

            if !same_medium(current_material, next_material) {
                if walk.interfaces.len() as u32 >= event_budget {
                    walk.termination = QueryTermination::EventBudget;
                    return walk;
                }
                walk.interfaces.push(InterfaceEvent {
                    position: segment_end,
                    incident_face_normal,
                    from_cell: previous_cell,
                    to_cell: cell,
                    from_material: current_material,
                    to_material: next_material,
                    tied_axes,
                });
            }
            current_material = next_material;
            current_t = crossing;
        }
    }

    pub(crate) fn trace_primary_surfaces(
        &self,
        ray: SceneRay,
        event_budget: u32,
        step_budget: u32,
    ) -> PrimarySurfaceQuery {
        let walk = self.walk_voxel_media(ray, event_budget, step_budget);
        PrimarySurfaceQuery {
            glass_front: walk.interfaces.iter().copied().find(|event| {
                event.from_material.surface_class == VoxelSurfaceClass::Dielectric
                    || event.to_material.surface_class == VoxelSurfaceClass::Dielectric
            }),
            opaque_hit: walk.opaque_hit,
            termination: walk.termination,
            dda_steps: walk.dda_steps,
        }
    }

    pub(crate) fn trace_radiance(
        &self,
        ray: SceneRay,
        budget: PathBudget,
        sky_radiance: DVec3,
        opaque_radiance: DVec3,
    ) -> RadianceResult {
        let mut result = RadianceResult {
            radiance: DVec3::ZERO,
            diagnostics: QueryDiagnostics::default(),
        };
        let Some(direction) = ray.direction.try_normalize() else {
            return result;
        };
        if budget.max_active_paths == 0
            || budget.max_scene_queries == 0
            || budget.max_interface_events == 0
            || budget.max_dda_steps_per_query == 0
            || !budget.throughput_cutoff.is_finite()
            || budget.throughput_cutoff < 0.0
        {
            result.diagnostics.budget_exhaustions = 1;
            return result;
        }

        let mut next_serial = 1_u64;
        let mut active = vec![ActivePath {
            ray: SceneRay {
                origin: ray.origin,
                direction,
            },
            throughput: DVec3::ONE,
            serial: 0,
        }];
        result.diagnostics.peak_active_paths = 1;

        while !active.is_empty() {
            sort_active_paths(&mut active);
            let path = active.remove(0);
            if throughput_luminance(path.throughput) < budget.throughput_cutoff {
                result.diagnostics.throughput_cutoffs += 1;
                continue;
            }
            if result.diagnostics.scene_queries >= budget.max_scene_queries {
                result.radiance += path.throughput * sky_radiance;
                result.diagnostics.query_budget_fallbacks += 1;
                continue;
            }
            result.diagnostics.scene_queries += 1;
            let walk = self.walk_voxel_media(
                path.ray,
                budget.max_interface_events,
                budget.max_dda_steps_per_query,
            );
            result.diagnostics.dda_steps += walk.dda_steps;

            let first_interface = walk.interfaces.first().copied();
            let mut throughput = path.throughput
                * attenuation_before(
                    &walk.segments,
                    path.ray,
                    first_interface.map(|event| event.position),
                );
            if throughput_luminance(throughput) < budget.throughput_cutoff {
                result.diagnostics.throughput_cutoffs += 1;
                continue;
            }

            let Some(interface) = first_interface else {
                match walk.termination {
                    QueryTermination::Opaque => {
                        result.radiance += throughput * opaque_radiance;
                    }
                    QueryTermination::Miss => {
                        result.radiance += throughput * sky_radiance;
                    }
                    QueryTermination::EventBudget | QueryTermination::StepBudget => {
                        result.diagnostics.budget_exhaustions += 1;
                    }
                    QueryTermination::InvalidRay => {}
                }
                continue;
            };

            if result.diagnostics.interface_events >= budget.max_interface_events {
                result.diagnostics.budget_exhaustions += 1;
                continue;
            }
            result.diagnostics.interface_events += 1;
            if interface.to_material.surface_class == VoxelSurfaceClass::Opaque {
                result.radiance += throughput * opaque_radiance;
                continue;
            }

            let eta_incident = interface.from_material.ior;
            let eta_transmitted = interface.to_material.ior;
            let fresnel = dielectric_fresnel_unpolarized(
                path.ray.direction,
                interface.incident_face_normal,
                eta_incident,
                eta_transmitted,
            );
            let reflected_direction = (path.ray.direction
                - 2.0
                    * path.ray.direction.dot(interface.incident_face_normal)
                    * interface.incident_face_normal)
                .normalize();
            let reflected_throughput = throughput * fresnel;
            let origin_epsilon = self.voxel_size_world * 1.0e-6;

            let mut candidates = Vec::with_capacity(2);
            if let Some(transmitted_direction) = refract_dielectric(
                path.ray.direction,
                interface.incident_face_normal,
                eta_incident,
                eta_transmitted,
            ) {
                candidates.push(ActivePath {
                    ray: SceneRay {
                        origin: interface.position + transmitted_direction * origin_epsilon,
                        direction: transmitted_direction,
                    },
                    throughput: throughput * (1.0 - fresnel),
                    serial: next_serial,
                });
                next_serial += 1;
            } else {
                result.diagnostics.tir_events += 1;
                throughput = reflected_throughput;
            }
            candidates.push(ActivePath {
                ray: SceneRay {
                    origin: interface.position + reflected_direction * origin_epsilon,
                    direction: reflected_direction,
                },
                throughput: if fresnel >= 1.0 {
                    throughput
                } else {
                    reflected_throughput
                },
                serial: next_serial,
            });
            next_serial += 1;

            for candidate in candidates {
                if throughput_luminance(candidate.throughput) < budget.throughput_cutoff {
                    result.diagnostics.throughput_cutoffs += 1;
                } else {
                    active.push(candidate);
                }
            }
            sort_active_paths(&mut active);
            if active.len() > budget.max_active_paths {
                result.diagnostics.top_k_pruned += (active.len() - budget.max_active_paths) as u32;
                active.truncate(budget.max_active_paths);
            }
            result.diagnostics.peak_active_paths = result
                .diagnostics
                .peak_active_paths
                .max(active.len() as u32);
        }
        result
    }

    fn clip_ray_to_domain(&self, ray: SceneRay) -> Option<(f64, f64, DVec3)> {
        let maximum = self.dimensions.as_dvec3() * self.voxel_size_world;
        let mut entry = f64::NEG_INFINITY;
        let mut exit = f64::INFINITY;
        let mut entry_normal = DVec3::ZERO;
        for axis in 0..3 {
            let origin = ray.origin[axis];
            let direction = ray.direction[axis];
            if direction.abs() <= 1.0e-20 {
                if origin < 0.0 || origin >= maximum[axis] {
                    return None;
                }
                continue;
            }
            let first = -origin / direction;
            let second = (maximum[axis] - origin) / direction;
            let near = first.min(second);
            let far = first.max(second);
            if near > entry {
                entry = near;
                entry_normal = axis_vector(axis) * -direction.signum();
            }
            exit = exit.min(far);
            if entry > exit {
                return None;
            }
        }
        entry = entry.max(0.0);
        (exit >= entry).then_some((entry, exit, entry_normal))
    }

    fn material_at(&self, cell: IVec3) -> QueryMaterial {
        if cell.cmplt(IVec3::ZERO).any() || cell.cmpge(self.dimensions.as_ivec3()).any() {
            return self.empty_material();
        }
        let index = cell.x as usize
            + self.dimensions.x as usize
                * (cell.y as usize + self.dimensions.y as usize * cell.z as usize);
        self.query_material(u32::from(
            self.voxel_types[index] & crate::builder::VOXEL_TYPE_MASK,
        ))
    }

    fn empty_material(&self) -> QueryMaterial {
        self.query_material(crate::builder::VOXEL_TYPE_EMPTY)
    }

    fn query_material(&self, voxel_type: u32) -> QueryMaterial {
        let material = material_for(voxel_type, self.material_mode);
        let (ior, sigma_a) = material.optical.map_or((1.0, DVec3::ZERO), |optical| {
            let attenuation_color = DVec3::from_array(optical.attenuation_color.map(f64::from));
            (
                f64::from(optical.ior),
                -attenuation_color.ln() / f64::from(optical.attenuation_distance_world),
            )
        });
        QueryMaterial {
            voxel_type,
            surface_class: material.surface_class,
            ior,
            sigma_a,
        }
    }

    fn push_segment(
        &self,
        segments: &mut Vec<MediumSegment>,
        material: QueryMaterial,
        start: DVec3,
        end: DVec3,
        distance_world: f64,
    ) {
        if distance_world <= DDA_TIE_EPSILON {
            return;
        }
        if let Some(previous) = segments.last_mut().filter(|segment| {
            same_medium(segment.material, material)
                && segment.end.abs_diff_eq(start, DDA_TIE_EPSILON)
        }) {
            previous.end = end;
            previous.distance_world += distance_world;
            return;
        }
        segments.push(MediumSegment {
            material,
            start,
            end,
            distance_world,
        });
    }
}

pub(crate) fn dielectric_fresnel_unpolarized(
    incident_direction: DVec3,
    incident_face_normal: DVec3,
    eta_incident: f64,
    eta_transmitted: f64,
) -> f64 {
    let Some((_, _, cosine_incident)) =
        normalized_incident_frame(incident_direction, incident_face_normal)
    else {
        return 1.0;
    };
    if eta_incident <= 0.0 || eta_transmitted <= 0.0 {
        return 1.0;
    }
    let eta = eta_incident / eta_transmitted;
    let sine_transmitted_squared = eta * eta * (1.0 - cosine_incident * cosine_incident);
    if sine_transmitted_squared >= 1.0 {
        return 1.0;
    }
    let cosine_transmitted = (1.0 - sine_transmitted_squared).sqrt();
    let parallel = (eta_transmitted * cosine_incident - eta_incident * cosine_transmitted)
        / (eta_transmitted * cosine_incident + eta_incident * cosine_transmitted);
    let perpendicular = (eta_incident * cosine_incident - eta_transmitted * cosine_transmitted)
        / (eta_incident * cosine_incident + eta_transmitted * cosine_transmitted);
    let fresnel = 0.5 * (parallel * parallel + perpendicular * perpendicular);
    fresnel.clamp(0.0, 1.0)
}

pub(crate) fn refract_dielectric(
    incident_direction: DVec3,
    incident_face_normal: DVec3,
    eta_incident: f64,
    eta_transmitted: f64,
) -> Option<DVec3> {
    let (incident, normal, cosine_incident) =
        normalized_incident_frame(incident_direction, incident_face_normal)?;
    if eta_incident <= 0.0 || eta_transmitted <= 0.0 {
        return None;
    }
    let eta = eta_incident / eta_transmitted;
    let discriminant = 1.0 - eta * eta * (1.0 - cosine_incident * cosine_incident);
    if discriminant < 0.0 {
        return None;
    }
    Some((eta * incident + (eta * cosine_incident - discriminant.sqrt()) * normal).normalize())
}

pub(crate) fn beer_lambert(sigma_a: DVec3, distance_world: f64) -> DVec3 {
    if !sigma_a.is_finite() || !distance_world.is_finite() || distance_world < 0.0 {
        return DVec3::ZERO;
    }
    (-sigma_a.max(DVec3::ZERO) * distance_world).exp()
}

#[derive(Clone, Copy, Debug)]
struct ActivePath {
    ray: SceneRay,
    throughput: DVec3,
    serial: u64,
}

fn throughput_luminance(throughput: DVec3) -> f64 {
    throughput.dot(DVec3::new(0.2126, 0.7152, 0.0722))
}

fn sort_active_paths(paths: &mut [ActivePath]) {
    paths.sort_by(|left, right| {
        throughput_luminance(right.throughput)
            .total_cmp(&throughput_luminance(left.throughput))
            .then_with(|| left.serial.cmp(&right.serial))
    });
}

fn attenuation_before(
    segments: &[MediumSegment],
    ray: SceneRay,
    limit_position: Option<DVec3>,
) -> DVec3 {
    let direction = ray.direction.normalize();
    let limit = limit_position
        .map(|position| (position - ray.origin).dot(direction))
        .unwrap_or(f64::INFINITY);
    segments
        .iter()
        .filter(|segment| segment.material.surface_class == VoxelSurfaceClass::Dielectric)
        .fold(DVec3::ONE, |attenuation, segment| {
            let start = (segment.start - ray.origin).dot(direction).max(0.0);
            let end = (segment.end - ray.origin).dot(direction).min(limit);
            let distance = (end - start).max(0.0);
            attenuation * beer_lambert(segment.material.sigma_a, distance)
        })
}

fn same_medium(left: QueryMaterial, right: QueryMaterial) -> bool {
    match (left.surface_class, right.surface_class) {
        (VoxelSurfaceClass::Empty, VoxelSurfaceClass::Empty) => true,
        (VoxelSurfaceClass::Dielectric, VoxelSurfaceClass::Dielectric) => {
            left.voxel_type == right.voxel_type
        }
        _ => false,
    }
}

fn axis_delta(direction: f64, voxel_size_world: f64) -> f64 {
    if direction.abs() <= 1.0e-20 {
        f64::INFINITY
    } else {
        voxel_size_world / direction.abs()
    }
}

fn axis_next(origin: f64, direction: f64, cell: i32, step: i32, voxel_size_world: f64) -> f64 {
    if step == 0 {
        return f64::INFINITY;
    }
    let boundary = if step > 0 { cell + 1 } else { cell };
    (f64::from(boundary) * voxel_size_world - origin) / direction
}

fn crossing_axis_mask(next: DVec3, crossing: f64) -> u8 {
    let tolerance = DDA_TIE_EPSILON * crossing.abs().max(1.0);
    u8::from((next.x - crossing).abs() <= tolerance)
        | (u8::from((next.y - crossing).abs() <= tolerance) << 1)
        | (u8::from((next.z - crossing).abs() <= tolerance) << 2)
}

fn normal_axis_mask(normal: DVec3) -> u8 {
    u8::from(normal.x != 0.0) | (u8::from(normal.y != 0.0) << 1) | (u8::from(normal.z != 0.0) << 2)
}

fn incident_normal(tied_axes: u8, step: IVec3) -> DVec3 {
    for axis in 0..3 {
        if tied_axes & (1 << axis) != 0 {
            return axis_vector(axis) * -f64::from(step[axis]);
        }
    }
    DVec3::ZERO
}

fn axis_vector(axis: usize) -> DVec3 {
    match axis {
        0 => DVec3::X,
        1 => DVec3::Y,
        2 => DVec3::Z,
        _ => unreachable!(),
    }
}

fn advance_tied_axes(cell: &mut IVec3, next: &mut DVec3, step: IVec3, delta: DVec3, tied_axes: u8) {
    for axis in 0..3 {
        if tied_axes & (1 << axis) != 0 {
            cell[axis] += step[axis];
            next[axis] += delta[axis];
        }
    }
}

fn normalized_incident_frame(
    incident_direction: DVec3,
    incident_face_normal: DVec3,
) -> Option<(DVec3, DVec3, f64)> {
    if !incident_direction.is_finite() || !incident_face_normal.is_finite() {
        return None;
    }
    let incident = incident_direction.try_normalize()?;
    let mut normal = incident_face_normal.try_normalize()?;
    if incident.dot(normal) > 0.0 {
        normal = -normal;
    }
    let cosine_incident = (-incident.dot(normal)).clamp(0.0, 1.0);
    Some((incident, normal, cosine_incident))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::{VOXEL_TYPE_EMPTY, VOXEL_TYPE_ROCK, VOXEL_TYPE_SAND};

    fn scene_x(voxels: &[u8]) -> DenseVoxelScene {
        DenseVoxelScene::new(
            UVec3::new(voxels.len() as u32, 1, 1),
            1.0,
            voxels.to_vec(),
            VoxelMaterialMode::GlassExperiment,
        )
    }

    fn ray_x(origin_x: f64) -> SceneRay {
        SceneRay {
            origin: DVec3::new(origin_x, 0.5, 0.5),
            direction: DVec3::X,
        }
    }

    #[test]
    fn connected_glass_cells_have_no_internal_interface_and_camera_inside_uses_current_medium() {
        let scene = scene_x(&[
            VOXEL_TYPE_EMPTY as u8,
            VOXEL_TYPE_SAND as u8,
            VOXEL_TYPE_SAND as u8,
            VOXEL_TYPE_EMPTY as u8,
        ]);
        let walk = scene.walk_voxel_media(ray_x(1.25), 8, 32);
        assert_eq!(walk.termination, QueryTermination::Miss);
        assert_eq!(walk.interfaces.len(), 1);
        assert!((walk.interfaces[0].position.x - 3.0).abs() <= 1.0e-12);
        assert!((walk.segments[0].distance_world - 1.75).abs() <= 1.0e-12);
        assert_eq!(
            scene
                .trace_primary_surfaces(ray_x(1.25), 8, 32)
                .glass_front
                .unwrap()
                .position
                .x,
            3.0,
        );
    }

    #[test]
    fn one_cell_air_gap_creates_four_real_interfaces() {
        let scene = scene_x(&[
            VOXEL_TYPE_SAND as u8,
            VOXEL_TYPE_EMPTY as u8,
            VOXEL_TYPE_SAND as u8,
            VOXEL_TYPE_EMPTY as u8,
        ]);
        let walk = scene.walk_voxel_media(ray_x(-0.5), 8, 32);
        assert_eq!(walk.termination, QueryTermination::Miss);
        assert_eq!(
            walk.interfaces
                .iter()
                .map(|event| event.position.x)
                .collect::<Vec<_>>(),
            vec![0.0, 1.0, 2.0, 3.0],
        );
    }

    #[test]
    fn primary_query_keeps_nearest_glass_front_and_first_opaque_behind_it() {
        let scene = scene_x(&[
            VOXEL_TYPE_SAND as u8,
            VOXEL_TYPE_SAND as u8,
            VOXEL_TYPE_EMPTY as u8,
            VOXEL_TYPE_ROCK as u8,
        ]);
        let query = scene.trace_primary_surfaces(ray_x(-0.5), 8, 32);
        assert_eq!(query.glass_front.unwrap().position.x, 0.0);
        assert_eq!(query.opaque_hit.unwrap().position.x, 3.0);
        assert_eq!(query.termination, QueryTermination::Opaque);
    }

    #[test]
    fn connected_glass_crosses_a_chunk_seam_without_a_false_interface() {
        let mut voxels = vec![VOXEL_TYPE_EMPTY as u8; 512];
        voxels[255] = VOXEL_TYPE_SAND as u8;
        voxels[256] = VOXEL_TYPE_SAND as u8;
        let scene = DenseVoxelScene::new(
            UVec3::new(512, 1, 1),
            1.0 / 256.0,
            voxels,
            VoxelMaterialMode::GlassExperiment,
        );
        let walk = scene.walk_voxel_media(
            SceneRay {
                origin: DVec3::new(254.5 / 256.0, 0.5 / 256.0, 0.5 / 256.0),
                direction: DVec3::X,
            },
            8,
            512,
        );

        assert_eq!(walk.interfaces.len(), 2);
        assert!(walk
            .interfaces
            .iter()
            .all(|event| (event.position.x - 1.0).abs() > 1.0e-12));
    }

    #[test]
    fn glass_to_opaque_contact_records_the_interface_and_terminal_hit() {
        let scene = scene_x(&[VOXEL_TYPE_SAND as u8, VOXEL_TYPE_ROCK as u8]);
        let walk = scene.walk_voxel_media(ray_x(-0.5), 8, 32);

        assert_eq!(walk.termination, QueryTermination::Opaque);
        assert_eq!(walk.interfaces.len(), 2);
        assert_eq!(
            walk.interfaces[1].to_material.surface_class,
            VoxelSurfaceClass::Opaque,
        );
        assert_eq!(walk.opaque_hit.unwrap().position.x, 1.0);
    }

    #[test]
    fn media_walk_reports_event_and_step_budget_exhaustion_explicitly() {
        let scene = scene_x(&[
            VOXEL_TYPE_SAND as u8,
            VOXEL_TYPE_EMPTY as u8,
            VOXEL_TYPE_SAND as u8,
            VOXEL_TYPE_EMPTY as u8,
        ]);

        let event_limited = scene.walk_voxel_media(ray_x(-0.5), 2, 32);
        let step_limited = scene.walk_voxel_media(ray_x(0.5), 8, 1);

        assert_eq!(event_limited.interfaces.len(), 2);
        assert_eq!(event_limited.termination, QueryTermination::EventBudget);
        assert_eq!(step_limited.termination, QueryTermination::StepBudget);
    }

    #[test]
    fn corner_tie_advances_all_axes_but_chooses_stable_axis_aligned_normal() {
        let mut voxels = vec![VOXEL_TYPE_EMPTY as u8; 4];
        voxels[3] = VOXEL_TYPE_SAND as u8;
        let scene = DenseVoxelScene::new(
            UVec3::new(2, 2, 1),
            1.0,
            voxels,
            VoxelMaterialMode::GlassExperiment,
        );
        let ray = SceneRay {
            origin: DVec3::new(0.5, 0.5, 0.5),
            direction: DVec3::new(1.0, 1.0, 0.0),
        };
        let walk = scene.walk_voxel_media(ray, 8, 32);
        assert_eq!(walk.interfaces[0].to_cell, IVec3::new(1, 1, 0));
        assert_eq!(walk.interfaces[0].tied_axes, 0b011);
        assert_eq!(walk.interfaces[0].incident_face_normal, -DVec3::X);
    }

    #[test]
    fn exact_fresnel_snell_and_tir_match_reference_values() {
        let normal_incidence = dielectric_fresnel_unpolarized(DVec3::X, -DVec3::X, 1.0, 1.5);
        assert!((normal_incidence - 0.04).abs() <= 1.0e-12);
        let refracted = refract_dielectric(
            DVec3::new(0.5, -(3.0_f64).sqrt() * 0.5, 0.0),
            DVec3::Y,
            1.0,
            1.5,
        )
        .unwrap();
        assert!((refracted.x - 1.0 / 3.0).abs() <= 1.0e-12);
        let below_critical = DVec3::new(0.65, (1.0_f64 - 0.65 * 0.65).sqrt(), 0.0);
        let above_critical = DVec3::new(0.68, (1.0_f64 - 0.68 * 0.68).sqrt(), 0.0);
        assert!(refract_dielectric(below_critical, -DVec3::Y, 1.5, 1.0).is_some());
        assert!(refract_dielectric(above_critical, -DVec3::Y, 1.5, 1.0).is_none());
        assert_eq!(
            dielectric_fresnel_unpolarized(above_critical, -DVec3::Y, 1.5, 1.0),
            1.0,
        );
    }

    #[test]
    fn beer_attenuation_is_exact_at_author_distance_and_monotonic() {
        let attenuation_color = DVec3::new(0.82, 0.94, 0.98);
        let distance = 0.25;
        let sigma_a = -attenuation_color.ln() / distance;
        let at_distance = beer_lambert(sigma_a, distance);
        let farther = beer_lambert(sigma_a, distance * 2.0);
        assert!(at_distance.abs_diff_eq(attenuation_color, 1.0e-12));
        assert!(farther.cmple(at_distance).all());
        assert!(farther.cmpge(DVec3::ZERO).all());
    }

    #[test]
    fn bounded_path_trace_is_deterministic_and_default_slab_does_not_exhaust() {
        let scene = scene_x(&[
            VOXEL_TYPE_EMPTY as u8,
            VOXEL_TYPE_SAND as u8,
            VOXEL_TYPE_EMPTY as u8,
        ]);
        let expected = scene.trace_radiance(
            ray_x(0.5),
            PathBudget::default(),
            DVec3::ONE,
            DVec3::splat(0.25),
        );
        let repeated = scene.trace_radiance(
            ray_x(0.5),
            PathBudget::default(),
            DVec3::ONE,
            DVec3::splat(0.25),
        );

        assert_eq!(expected, repeated);
        assert_eq!(expected.diagnostics.budget_exhaustions, 0);
        assert!(expected.diagnostics.scene_queries <= 8);
        assert!(expected.diagnostics.peak_active_paths <= 4);
        assert!(expected.radiance.cmpgt(DVec3::ZERO).all());
        assert!(expected.radiance.cmple(DVec3::ONE).all());
    }

    #[test]
    fn scene_query_cap_uses_declared_sky_fallback_without_exhaustion() {
        let scene = scene_x(&[
            VOXEL_TYPE_EMPTY as u8,
            VOXEL_TYPE_SAND as u8,
            VOXEL_TYPE_EMPTY as u8,
        ]);
        let result = scene.trace_radiance(
            ray_x(0.5),
            PathBudget {
                max_scene_queries: 1,
                ..PathBudget::default()
            },
            DVec3::ONE,
            DVec3::ZERO,
        );

        assert!(result.diagnostics.query_budget_fallbacks >= 1);
        assert_eq!(result.diagnostics.budget_exhaustions, 0);
        assert!(result.radiance.cmpgt(DVec3::ZERO).all());
    }

    #[test]
    fn bounded_path_trace_counts_total_internal_reflection() {
        let scene = DenseVoxelScene::new(
            UVec3::new(2, 1, 1),
            1.0,
            vec![VOXEL_TYPE_SAND as u8; 2],
            VoxelMaterialMode::GlassExperiment,
        );
        let result = scene.trace_radiance(
            SceneRay {
                origin: DVec3::new(0.5, 0.5, 0.5),
                direction: DVec3::new(0.8, 0.6, 0.0),
            },
            PathBudget {
                max_interface_events: 16,
                max_scene_queries: 16,
                ..PathBudget::default()
            },
            DVec3::ONE,
            DVec3::ZERO,
        );

        assert!(result.diagnostics.tir_events >= 1);
        assert_eq!(result.diagnostics.budget_exhaustions, 0);
    }
}

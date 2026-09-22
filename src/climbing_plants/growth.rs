//! Bounded rotating exploration and one authoritative surface-contact description.
use super::{clear_segment, segment_material, Contact, Node, Plant, Restart, Terrain, Tip};
use glam::{IVec3, Vec3};

const STEPS_PER_TURN: u8 = 24;
const STEP_LENGTH: f32 = 2.0;
const CONTACT_CLEARANCE: f32 = 0.18;

pub(super) struct Step {
    pub position: Vec3,
    pub normal: Vec3,
    pub contact: Option<Contact>,
    pub backing: Option<(Vec3, Vec3)>,
}

pub(super) fn initial_phase(random: f32) -> u8 {
    (random * f32::from(STEPS_PER_TURN)) as u8
}

pub(super) fn probe(plant: &Plant, tip: &Tip) -> Vec3 {
    let node = &plant.nodes[tip.node];
    let mut axis = Vec3::Y - node.normal * node.normal.y;
    if axis.length_squared() < 0.01 {
        axis = if node.normal.y < -0.5 && tip.under_ceiling {
            tip.exterior
        } else {
            Vec3::Y
        };
    }
    axis = axis.normalize();
    let radial = if axis.dot(node.normal).abs() > 0.9 {
        tip.exterior
    } else {
        node.normal
    };
    let side = axis.cross(radial).normalize();
    let handedness = if tip.clockwise { 1.0 } else { -1.0 };
    let angle =
        std::f32::consts::TAU * f32::from(tip.phase + 1) / f32::from(STEPS_PER_TURN) * handedness;
    // Support bias prevents collision projection from ratcheting circles away from the wall.
    let mut direction = axis * 0.9
        + radial * (0.85 * angle.cos() - 0.45)
        + side * (0.85 * angle.sin() + 0.1 * tip.lateral * handedness);
    if node.normal.y < -0.5 && tip.under_ceiling {
        // Follow a ceiling tangentially to its edge instead of making down-up loops.
        direction -= node.normal * direction.dot(node.normal);
    }
    node.position + direction.normalize() * STEP_LENGTH
}

/// None = unavailable; Some(None) = a ready attempt without extension.
pub(super) fn advance(
    plant: &Plant,
    tip: &mut Tip,
    terrain: &impl Terrain,
    spacing: f32,
) -> Option<Option<Step>> {
    let start = &plant.nodes[tip.node];
    if let Some(restart) = &tip.restart {
        if !restart_valid(plant, start.position, restart, terrain)? {
            return Some(None);
        }
        return Some(Some(Step {
            position: restart.position,
            normal: restart.normal,
            contact: restart.contact.clone(),
            backing: restart.backing,
        }));
    }
    // Sagging can slightly separate a tip from the ceiling without taking it past
    // the ledge. Guide it by actual overhead occupancy, not a stale contact flag.
    tip.under_ceiling = start.normal.y < -0.5
        && !clear_segment(
            terrain,
            start.position,
            start.position + Vec3::Y * STEP_LENGTH,
            plant.radius,
        )?;
    let end = probe(plant, tip);
    tip.phase = (tip.phase + 1) % STEPS_PER_TURN;
    if let Some((position, contact)) = touch_surface(plant, start, end, terrain)? {
        let backing = backing_for(plant, start, position, Some(&contact), terrain)?;
        return Some(Some(Step {
            position,
            normal: contact.normal,
            contact: Some(contact),
            backing,
        }));
    }
    let limit = (spacing * 3.0).clamp(12.0, 64.0);
    if tip.arc + STEP_LENGTH > limit
        || end.y < start.position.y - 0.0001
        || !clear_segment(terrain, start.position, end, plant.radius)?
    {
        return Some(None);
    }
    Some(Some(Step {
        position: end,
        normal: start.normal,
        contact: None,
        backing: None,
    }))
}

pub(super) fn restart_valid(
    plant: &Plant,
    start: Vec3,
    restart: &Restart,
    terrain: &impl Terrain,
) -> Option<bool> {
    let contact_ok = match &restart.contact {
        Some(contact) => terrain.voxel(contact.cell)? == plant.anchors[0].material,
        None => true,
    };
    let backing_ok = match restart.backing {
        Some((a, b)) => segment_material(terrain, a, b, 0.0, plant.anchors[0].material)?,
        None => true,
    };
    let clear = clear_segment(terrain, start, restart.position, plant.radius)?;
    Some(contact_ok && backing_ok && clear)
}

pub(super) fn backing_for(
    plant: &Plant,
    start: &Node,
    end: Vec3,
    contact: Option<&Contact>,
    terrain: &impl Terrain,
) -> Option<Option<(Vec3, Vec3)>> {
    let Some(contact) = contact else {
        return Some(None);
    };
    if !start
        .contact
        .as_ref()
        .is_some_and(|c| c.normal == contact.normal)
    {
        return Some(None);
    }
    let offset = contact.normal * (plant.radius + CONTACT_CLEARANCE + 0.1);
    let (a, b) = (start.position - offset, end - offset);
    Some(segment_material(terrain, a, b, 0.0, plant.anchors[0].material)?.then_some((a, b)))
}

/// Refresh provisional support after deformation; never keep a contact at the old pose.
pub(super) fn contact_at(
    plant: &Plant,
    node: &Node,
    terrain: &impl Terrain,
) -> Option<Option<Contact>> {
    Some(
        nearby_contacts(plant, node.position, terrain)?
            .into_iter()
            .min_by(|(pa, a), (pb, b)| {
                contact_score(*pa, a, node.position, node.normal).total_cmp(&contact_score(
                    *pb,
                    b,
                    node.position,
                    node.normal,
                ))
            })
            .map(|(_, contact)| contact),
    )
}

fn contact_score(position: Vec3, contact: &Contact, target: Vec3, normal: Vec3) -> f32 {
    position.distance_squared(target) + 0.02 * (1.0 - contact.normal.dot(normal))
}
fn touch_surface(
    plant: &Plant,
    start: &Node,
    end: Vec3,
    terrain: &impl Terrain,
) -> Option<Option<(Vec3, Contact)>> {
    let mut best: Option<(f32, Vec3, Contact)> = None;
    let travel = end - start.position;
    for (position, contact) in nearby_contacts(plant, end, terrain)? {
        let delta = position - start.position;
        if delta.y < -0.0001
            || delta.length_squared() > 2.5 * 2.5
            || delta.length_squared() < 0.1 * 0.1
            || delta.dot(travel) < 0.01
        {
            continue;
        }
        if !clear_segment(terrain, start.position, position, plant.radius)? {
            continue;
        }
        let score = contact_score(position, &contact, end, start.normal);
        if best.as_ref().is_none_or(|(old, _, _)| score < *old) {
            best = Some((score, position, contact));
        }
    }
    Some(best.map(|(_, position, contact)| (position, contact)))
}

fn nearby_contacts(
    plant: &Plant,
    end: Vec3,
    terrain: &impl Terrain,
) -> Option<Vec<(Vec3, Contact)>> {
    let min = (end - Vec3::splat(1.5)).floor().as_ivec3();
    let max = (end + Vec3::splat(1.5)).floor().as_ivec3();
    let mut contacts = Vec::new();
    for z in min.z..=max.z {
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                let cell = IVec3::new(x, y, z);
                if terrain.voxel(cell)? != plant.anchors[0].material {
                    continue;
                }
                for normal in [
                    IVec3::X,
                    IVec3::NEG_X,
                    IVec3::Y,
                    IVec3::NEG_Y,
                    IVec3::Z,
                    IVec3::NEG_Z,
                ] {
                    if terrain.voxel(cell + normal)? != 0 {
                        continue;
                    }
                    let n = normal.as_vec3();
                    let mut surface = end.clamp(cell.as_vec3(), cell.as_vec3() + Vec3::ONE);
                    let axis = if normal.x != 0 {
                        0
                    } else if normal.y != 0 {
                        1
                    } else {
                        2
                    };
                    surface[axis] = cell[axis] as f32 + if normal[axis] > 0 { 1.0 } else { 0.0 };
                    // Local distance to an actual face, never attraction to an infinite plane.
                    if end.distance_squared(surface)
                        > (plant.radius + CONTACT_CLEARANCE + 0.0001).powi(2)
                    {
                        continue;
                    }
                    contacts.push((
                        surface + n * (plant.radius + CONTACT_CLEARANCE),
                        Contact { cell, normal: n },
                    ));
                }
            }
        }
    }
    Some(contacts)
}

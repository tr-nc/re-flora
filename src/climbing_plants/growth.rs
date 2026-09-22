//! Bounded circumnutation: an upward/tangential axis plus a rotating radial search.
//! Contact changes the local climbing frame; mature tips attach only to reachable,
//! exposed faces of the seed material. No snap is allowed through an obstacle.
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

pub(super) fn reverse_search(tip: &mut Tip) {
    tip.clockwise = !tip.clockwise;
    tip.phase = (tip.phase + STEPS_PER_TURN / 2) % STEPS_PER_TURN;
}

pub(super) fn probe(plant: &Plant, tip: &Tip) -> Vec3 {
    let node = &plant.nodes[tip.node];
    let mut axis = Vec3::Y - node.normal * node.normal.y;
    if axis.length_squared() < 0.01 {
        axis = if node.normal.y < -0.5 && node.contact.is_some() {
            // Follow the underside outward; once clear of it, resume upward search.
            tip.exterior
        } else {
            Vec3::Y
        }; // a ground/top seed grows up rather than along the floor
    }
    axis = axis.normalize();
    let radial = if axis.dot(node.normal).abs() > 0.9 {
        tip.exterior
    } else {
        node.normal
    };
    let side = axis.cross(radial).normalize();
    let angle = std::f32::consts::TAU * f32::from(tip.phase + 1) / f32::from(STEPS_PER_TURN)
        * if tip.clockwise { 1.0 } else { -1.0 };
    // A small support-seeking bias prevents collision projection from ratcheting
    // successive circles farther away from the wall. The rotating component still
    // opens a finite searching arc around gaps and changes of surface direction.
    let mut direction = axis * 0.9
        + radial * (0.85 * angle.cos() - 0.45)
        + side * (0.85 * angle.sin() + 0.1 * tip.lateral);
    if node.normal.y < -0.5 && node.contact.is_some() {
        // A ceiling blocks upward extension. Keep the rotating lateral sweep but
        // advance tangentially toward its edge, rather than laying down/down-up loops.
        direction -= node.normal * direction.dot(node.normal);
    }
    node.position + direction.normalize() * STEP_LENGTH
}

/// None = unavailable; Some(None) = a ready search attempt without extension.
/// Only the angular search state advances on a blocked ordinary tip, never RNG.
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
    let end = probe(plant, tip);
    tip.phase = (tip.phase + 1) % STEPS_PER_TURN;
    if let Some((position, contact)) = touch_surface(plant, start, end, terrain)? {
        let backing = if start
            .contact
            .as_ref()
            .is_some_and(|c| c.normal == contact.normal)
        {
            let offset = contact.normal * (plant.radius + CONTACT_CLEARANCE + 0.1);
            let a = start.position - offset;
            let b = position - offset;
            if segment_material(terrain, a, b, 0.0, plant.anchors[0].material)? {
                Some((a, b))
            } else {
                None
            }
        } else {
            None
        };
        return Some(Some(Step {
            position,
            normal: contact.normal,
            contact: Some(contact),
            backing,
        }));
    }
    // A searching shoot may bridge a small hole but cannot grow an infinite air vine.
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

fn touch_surface(
    plant: &Plant,
    start: &Node,
    end: Vec3,
    terrain: &impl Terrain,
) -> Option<Option<(Vec3, Contact)>> {
    let min = (end - Vec3::splat(1.5)).floor().as_ivec3();
    let max = (end + Vec3::splat(1.5)).floor().as_ivec3();
    let mut best: Option<(f32, Vec3, Contact)> = None;
    let travel = end - start.position;
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
                    // Contact is local to the tip; this is not a long-range attraction ray.
                    if end.distance_squared(surface)
                        > (plant.radius + CONTACT_CLEARANCE + 0.0001).powi(2)
                    {
                        continue;
                    }
                    let position = surface + n * (plant.radius + CONTACT_CLEARANCE);
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
                    let score = position.distance_squared(end) + 0.02 * (1.0 - n.dot(start.normal));
                    if best.as_ref().is_none_or(|(old, _, _)| score < *old) {
                        best = Some((score, position, Contact { cell, normal: n }));
                    }
                }
            }
        }
    }
    Some(best.map(|(_, position, contact)| (position, contact)))
}

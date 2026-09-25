//! Continuous stem. Material memory, a moving growth zone and local
//! compliant attachments are independent: an attachment never ends a bend stencil.
//! All geometry, phase, material and contact changes commit against one snapshot.
use super::{clear_segment, growth, sweep::clear_sweep, Anchor, Contact, Plant, Terrain, Tip};
use glam::{Quat, Vec3};
use std::collections::BTreeMap;

mod collision;
#[cfg(test)]
mod tests;
mod weight;

const ZONE_LENGTH: f32 = 28.0;
const MATURATION: f32 = 3.0;
const CONTACT_TIME: f32 = 0.35;
// Reserve bending room ahead of a new attachment, in arc length rather than
// node count (contact-corrected segments can be much shorter than two voxels).
const FREE_APEX_LENGTH: f32 = 6.0;
const MAX_MOTION: f32 = 0.6;
const MAX_ANCHOR_DRIFT: f32 = 0.3;
const LENGTH_TOLERANCE: f32 = 0.002;
pub(super) const AIR_BUDGET: f32 = 64.0;
// Short rootlets bridge rough voxel stair faces while the main stem remains smooth.
// This is extra reach beyond the stem's radius + normal growth clearance.
pub(super) const ATTACHMENT_EXTENSION: f32 = 2.0;

#[derive(Clone, Debug, PartialEq)]
struct Material {
    age: f32,
    direction: Vec3,
}
#[derive(Clone, Debug, PartialEq)]
struct Pending {
    id: u64,
    contact: Contact,
    position: Vec3,
    time: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Rod {
    phase: f32,
    period: f32,
    amplitude: f32,
    heading: Vec3,
    side: Vec3,
    outward: Vec3,
    material: BTreeMap<u64, Material>,
    pending: Option<Pending>,
    // Keep the base through the last surviving attachment stable after a cut;
    // the free span to the stump remains flexible.
    pub locked_through: u64,
}
impl Rod {
    pub fn new(tip: &Tip) -> Self {
        Self {
            phase: f32::from(tip.phase) * std::f32::consts::TAU / 24.0,
            period: 5.5 + tip.lateral * 2.0,
            amplitude: 0.0,
            heading: Vec3::Y,
            side: Vec3::Y.cross(tip.exterior).normalize(),
            outward: tip.exterior,
            material: BTreeMap::new(),
            pending: None,
            locked_through: 0,
        }
    }

    pub fn reorient_at(&mut self, nodes: &[super::Node], stump: usize) {
        self.heading = nodes[stump].parent.map_or(Vec3::Y, |parent| {
            (nodes[stump].position - nodes[parent].position).normalize()
        });
    }
}

pub(super) fn probe(plant: &Plant, tip: &Tip) -> Vec3 {
    let node = &plant.nodes[tip.node];
    let rod = &plant.rod;
    let incoming = node.parent.map_or(rod.heading, |p| {
        (node.position - plant.nodes[p].position).normalize()
    });
    // No phase here: new material continues the existing tangent. A small
    // tropic correction permits escape from a stalled tip without sharp births.
    node.position + incoming.lerp(rod.heading, 0.12).normalize() * 2.0
}

pub(super) fn step(
    plant: &mut Plant,
    terrain: &impl Terrain,
    dt: f32,
    flexibility: f32,
    spacing: f32,
    exploring: bool,
) -> Option<usize> {
    if !dt.is_finite()
        || dt <= 0.0
        || !flexibility.is_finite()
        || flexibility < 0.0
        || !spacing.is_finite()
        || spacing < 4.0
        || !plant.root_connected
        || plant.tips.len() != 1
        || plant
            .nodes
            .iter()
            .enumerate()
            .skip(1)
            .any(|(i, n)| n.parent != Some(i - 1))
    {
        return Some(0);
    }
    if !terrain.current() {
        return None;
    }
    if !plant.root_supported(terrain)? {
        return Some(0);
    }
    let dt = dt.min(0.05);
    let mut next = plant.clone();
    let mut rod = next.rod.clone();
    let tip = &next.tips[0];
    let end = next.nodes[tip.node].position;
    let overhead = !clear_segment(terrain, end, end + Vec3::Y * 4.0, next.radius)?;
    let support_bias = if next.nodes[tip.node].contact.is_some() {
        0.08
    } else {
        0.8
    };
    // Choose the most upward locally clear guide, rather than treating every
    // stair voxel as a horizontal ceiling. Smooth transport below carries the
    // existing frame into this guide; geometry still inherits its own tangent.
    // Keep the established side through provisional stair/hole contacts. Only a
    // confirmed attachment can change it; normal noise must not flip the guide.
    let mut desired_heading = rod.outward;
    for outward in [-support_bias, 0.0, 0.4, 0.8, 1.5, 4.0] {
        let candidate = (Vec3::Y + rod.outward * outward).normalize();
        if clear_segment(terrain, end, end + candidate * 4.0, next.radius)? {
            desired_heading = candidate;
            break;
        }
    }
    if desired_heading.y < 0.5 {
        // At a ceiling, the established face normal alone can point along the
        // inside of a hole forever. Look around both horizontal axes for a clear
        // way out, preferring candidates with actual headroom beyond the lip.
        let side = Vec3::Y.cross(rod.outward).normalize();
        let mut score = desired_heading.y;
        for horizontal in [rod.outward, -rod.outward, side, -side] {
            for tilt in [0.8, 1.5, 4.0] {
                let candidate = (Vec3::Y + horizontal * tilt).normalize();
                let destination = end + candidate * 4.0;
                if !clear_segment(terrain, end, destination, next.radius)? {
                    continue;
                }
                let headroom = clear_segment(
                    terrain,
                    destination,
                    destination + Vec3::Y * 4.0,
                    next.radius,
                )?;
                let candidate_score = candidate.y + if headroom { 0.5 } else { 0.0 };
                if candidate_score > score {
                    score = candidate_score;
                    desired_heading = candidate;
                }
            }
        }
    }
    let heading = rod
        .heading
        .lerp(desired_heading, 1.0 - (-3.0 * dt).exp())
        .normalize();
    rod.side = (Quat::from_rotation_arc(rod.heading, heading) * rod.side).normalize();
    rod.heading = heading;
    if exploring {
        let winding = if tip.clockwise { 1.0 } else { -1.0 };
        rod.phase = (rod.phase
            + winding * std::f32::consts::TAU * dt * next.search_rate / rod.period)
            .rem_euclid(std::f32::consts::TAU);
    }
    let count = next.nodes.len();
    let old: Vec<_> = next.nodes.iter().map(|n| n.position).collect();
    let mut distance = vec![0.0; count];
    for i in (1..count).rev() {
        distance[i - 1] = distance[i] + next.nodes[i].rest_length;
    }
    rod.material
        .retain(|id, _| next.nodes.iter().any(|n| n.id == *id));
    for (i, node) in next.nodes.iter().enumerate() {
        let direction = node.parent.map_or(Vec3::Y, |p| {
            (node.position - next.nodes[p].position).normalize()
        });
        let m = rod.material.entry(node.id).or_insert(Material {
            age: 0.0,
            direction,
        });
        // Young tissue gradually acquires its intrinsic shape, not an instant
        // freeze at a support. Older material retains finite elastic resistance.
        let plasticity = (1.0 - m.age / MATURATION).clamp(0.0, 1.0);
        m.direction = m.direction.lerp(direction, dt * plasticity).normalize();
        // Material matures as it leaves the apical growing zone. A tip stalled
        // at a ceiling must not age out of all shape adaptation while searching.
        // Age never goes backwards, including after a cut.
        m.age += dt * (distance[i] / ZONE_LENGTH).clamp(0.0, 1.0);
    }
    let mobility: Vec<_> = next
        .nodes
        .iter()
        .map(|n| if n.id <= rod.locked_through { 0.0 } else { 1.0 })
        .collect();
    let base = distance.iter().position(|&d| d < ZONE_LENGTH).unwrap_or(0);
    let base_tangent = if base > 0 {
        (old[base] - old[base - 1]).normalize()
    } else {
        Vec3::Y
    };
    let radial = rod.side.cross(heading).normalize();
    let envelope = if overhead { 0.12 } else { 0.28 } * flexibility.min(2.0) * next.search_turn;
    rod.amplitude += (envelope - rod.amplitude) * (1.0 - (-2.0 * dt).exp());
    // Smooth periodic modulation must agree at the phase wrap, unlike fractional
    // harmonics of a wrapped angle. Contact changes fade amplitude, never reset it.
    let angle = rod.phase + 0.12 * (2.0 * rod.phase).sin();
    let sweep = rod.side * angle.sin() + radial * (0.45 * angle.cos() - 0.22);
    let target = (heading + sweep * rod.amplitude).normalize();
    let tangents: Vec<_> = distance
        .iter()
        .map(|&d| {
            let u = (1.0 - d / ZONE_LENGTH).clamp(0.0, 1.0);
            base_tangent
                .lerp(target, u * u * (3.0 - 2.0 * u))
                .normalize()
        })
        .collect();
    let mut contacts = collision::gather(&next, &old, terrain)?;
    let mut solved = weight::propose(
        &next,
        &rod,
        &old,
        &tangents,
        &distance,
        terrain,
        dt,
        flexibility,
    )?;
    for i in 1..count {
        if mobility[i] > 0.0 {
            solved[i].y -= 0.018 * dt * flexibility.min(2.0);
        }
    }
    for iteration in 0..16 {
        // C = incoming unit-length edge - outgoing edge - preferred bend.
        // Its three-node gradient couples BOTH sides even when the middle is attached.
        for i in 1..count.saturating_sub(1) {
            let a = next.nodes[i].rest_length;
            let b = next.nodes[i + 1].rest_length;
            let incoming = (solved[i] - solved[i - 1]).normalize();
            let remembered =
                Quat::from_rotation_arc(rod.material[&next.nodes[i].id].direction, incoming)
                    * rod.material[&next.nodes[i + 1].id].direction;
            let young = (1.0 - distance[i] / ZONE_LENGTH).clamp(0.0, 1.0)
                * (1.0 - rod.material[&next.nodes[i].id].age / MATURATION).clamp(0.0, 1.0);
            let preferred = (incoming - remembered).lerp(tangents[i] - tangents[i + 1], young);
            let maturity = (rod.material[&next.nodes[i].id].age / MATURATION).clamp(0.0, 1.0);
            let bend_stiffness = 0.35 + 0.3 * maturity;
            let c = (solved[i] - solved[i - 1]) / a - (solved[i + 1] - solved[i]) / b - preferred;
            let gradients = [-1.0 / a, 1.0 / a + 1.0 / b, -1.0 / b];
            let denom: f32 = (0..3)
                .map(|j| mobility[i + j - 1] * gradients[j].powi(2))
                .sum();
            if denom > 0.0 {
                for j in 0..3 {
                    solved[i + j - 1] -=
                        c * (bend_stiffness * mobility[i + j - 1] * gradients[j] / denom);
                }
            }
        }
        // A distributed growth-zone torque aligns the curved zone with its guide.
        for i in 1..count {
            let young = (1.0 - distance[i] / ZONE_LENGTH).clamp(0.0, 1.0);
            let error = solved[i] - solved[i - 1] - tangents[i] * next.nodes[i].rest_length;
            let w = mobility[i] + mobility[i - 1];
            if w > 0.0 {
                let correction = error * (0.06 * young / w);
                solved[i] -= correction * mobility[i];
                solved[i - 1] += correction * mobility[i - 1];
            }
        }
        for anchor in next.anchors.iter().skip(1) {
            let i = anchor.node;
            solved[i] = solved[i].lerp(anchor.position, 0.6 * mobility[i]);
        }
        if let Some(pending) = &rod.pending {
            if let Some(i) = next.nodes.iter().position(|n| n.id == pending.id) {
                solved[i] = solved[i].lerp(
                    pending.position,
                    0.2 * pending.time / CONTACT_TIME * mobility[i],
                );
            }
        }
        project_lengths(&next, &mobility, &mut solved);
        project_supports(&next, &mobility, &mut solved);
        if iteration % 4 == 0 {
            contacts = collision::gather(&next, &solved, terrain)?;
        }
        collision::project(&contacts, &mobility, &mut solved);
    }
    // Collision-safe line search followed by length projection: interpolation alone
    // would shorten bent segments. Never publish an unconverged or swept-through pose.
    let mut accepted = None;
    for trial in 0..10 {
        let alpha = (1.0 - (-6.0 * dt).exp()) * 0.5f32.powi(trial);
        let mut positions: Vec<_> = old
            .iter()
            .zip(&solved)
            .enumerate()
            .map(|(i, (a, b))| {
                if mobility[i] == 0.0 {
                    *a
                } else {
                    a.lerp(*b, alpha)
                }
            })
            .collect();
        for iteration in 0..128 {
            project_lengths(&next, &mobility, &mut positions);
            project_supports(&next, &mobility, &mut positions);
            if iteration % 8 == 0 {
                contacts = collision::gather(&next, &positions, terrain)?;
            }
            collision::project(&contacts, &mobility, &mut positions);
            if lengths_valid(&next, &positions) {
                let mut clear = true;
                for i in 1..count {
                    if !clear_segment(terrain, positions[i - 1], positions[i], next.radius)? {
                        clear = false;
                        break;
                    }
                }
                if clear {
                    break;
                }
            }
        }
        if !lengths_valid(&next, &positions)
            || positions
                .iter()
                .zip(&old)
                .any(|(a, b)| !a.is_finite() || a.distance(*b) > MAX_MOTION)
            || next
                .anchors
                .iter()
                .any(|a| positions[a.node].distance(a.position) > MAX_ANCHOR_DRIFT)
        {
            continue;
        }
        let mut clear = true;
        for i in 1..count {
            if !clear_segment(terrain, positions[i - 1], positions[i], next.radius)?
                || !clear_sweep(
                    terrain,
                    [old[i - 1], old[i], positions[i - 1], positions[i]],
                    next.radius,
                )?
            {
                clear = false;
                break;
            }
        }
        if clear {
            for anchor in next.anchors.iter().skip(1) {
                if !clear_segment(
                    terrain,
                    positions[anchor.node],
                    anchor.surface_position(),
                    0.0,
                )? {
                    clear = false;
                    break;
                }
            }
        }
        if clear {
            accepted = Some(positions);
            break;
        }
    }
    let positions = accepted.unwrap_or_else(|| old.clone());
    let mut moved = 0;
    for i in 1..count {
        moved += usize::from(positions[i].distance_squared(old[i]) > 1e-10);
        next.nodes[i].position = positions[i];
        // Preserve the rooted cut stump and its support records while new tissue grows.
        if mobility[i] == 0.0 {
            continue;
        }
        let contact = growth::contact_at(&next, &next.nodes[i], terrain)?;
        let backing = growth::backing_for(
            &next,
            &next.nodes[i - 1],
            positions[i],
            contact.as_ref(),
            terrain,
        )?;
        if let Some(c) = &contact {
            next.nodes[i].normal = c.normal;
        }
        next.nodes[i].contact = contact;
        next.nodes[i].backing = backing;
    }
    establish_contact(&mut next, &mut rod, dt, spacing);
    next.rod = rod;
    if !terrain.current() {
        return None;
    }
    *plant = next;
    Some(moved)
}

// Bound local attachment compliance. Provisional rootlet contacts are NOT body
// half-spaces: a short root can reach a face around a corner while the stem is
// outside that face's finite voxel. Body obstacles belong to collision::gather.
fn project_supports(plant: &Plant, mobility: &[f32], positions: &mut [Vec3]) {
    for anchor in plant.anchors.iter().skip(1) {
        if mobility[anchor.node] == 0.0 {
            continue;
        }
        let delta = positions[anchor.node] - anchor.position;
        positions[anchor.node] = anchor.position + delta.clamp_length_max(MAX_ANCHOR_DRIFT * 0.98);
    }
}

fn project_lengths(plant: &Plant, mobility: &[f32], positions: &mut [Vec3]) {
    for i in 1..positions.len() {
        let delta = positions[i] - positions[i - 1];
        let length = delta.length();
        let w = mobility[i] + mobility[i - 1];
        if w > 0.0 && length > 1e-6 {
            let correction = delta * ((length - plant.nodes[i].rest_length) / (length * w));
            positions[i] -= correction * mobility[i];
            positions[i - 1] += correction * mobility[i - 1];
        }
    }
}
fn lengths_valid(plant: &Plant, positions: &[Vec3]) -> bool {
    (1..positions.len()).all(|i| {
        (positions[i].distance(positions[i - 1]) - plant.nodes[i].rest_length).abs()
            <= LENGTH_TOLERANCE
    })
}

fn establish_contact(plant: &mut Plant, rod: &mut Rod, dt: f32, spacing: f32) {
    let last_anchor = plant.anchors.last().unwrap();
    let last = last_anchor.node;
    let total_arc: f32 = plant.nodes[last + 1..].iter().map(|n| n.rest_length).sum();
    let mut arc = 0.0;
    let candidate = (last + 1..plant.nodes.len()).find(|&i| {
        arc += plant.nodes[i].rest_length;
        if total_arc - arc < FREE_APEX_LENGTH {
            return false;
        }
        let Some(contact) = &plant.nodes[i].contact else {
            return false;
        };
        // Spacing is a target on one face, not a veto on the first reachable
        // support around a corner. New faces still require finite separation,
        // a clear rootlet and the same persistence/strengthening interval.
        let turning_over_lip = contact.normal.dot(last_anchor.normal) < 0.5
            && (contact.normal.y > 0.5 || last_anchor.normal.y > 0.5);
        let interval = if turning_over_lip {
            plant.tips[0].spacing.min(2.0)
        } else {
            plant.tips[0]
                .spacing
                .min(plant.search_reach - FREE_APEX_LENGTH)
        };
        arc >= interval
    });
    let Some(i) = candidate else {
        rod.pending = None;
        return;
    };
    let node = &plant.nodes[i];
    let contact = node.contact.clone().unwrap();
    if !rod.pending.as_ref().is_some_and(|p| {
        p.id == node.id
            && p.contact.normal == contact.normal
            && (p.contact.cell - contact.cell).abs().element_sum() <= 1
    }) {
        rod.pending = Some(Pending {
            id: node.id,
            contact: contact.clone(),
            position: node.position,
            time: 0.0,
        });
    }
    let pending = rod.pending.as_mut().unwrap();
    pending.contact = contact.clone();
    pending.time += dt;
    if pending.time < CONTACT_TIME {
        return;
    }
    // This wall-clinging phenotype keeps its wall axis through small sideways
    // hole/stair faces. A confirmed opposite-facing attachment means it really
    // reached the far side; reverse the support bias without resetting phase.
    if contact.normal.dot(rod.outward) < -0.5 {
        rod.outward = contact.normal;
    }
    plant.anchors.push(Anchor {
        node: i,
        cell: contact.cell,
        material: plant.anchors[0].material,
        position: node.position,
        attached: true,
        normal: contact.normal,
    });
    // Established-history coloring is not a mechanical lock.
    plant.freeze_path(i);
    plant.tips[0].arc = plant.nodes[i + 1..].iter().map(|n| n.rest_length).sum();
    plant.tips[0].spacing = spacing * (0.85 + 0.3 * super::random(&mut plant.tips[0].rng));
    rod.pending = None;
}

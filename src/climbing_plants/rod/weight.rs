//! Overdamped rotational relaxation of the free cantilever. Rotating a joint's
//! entire distal chain resolves long-wave bending without stretching segments.
//! The torque includes distributed weight, elastic strain and a finite growing-
//! zone response. Terrain rejects inward motion, not all motion of a contacted node.
use super::{collision, Rod, CONTACT_TIME, MATURATION, ZONE_LENGTH};
use crate::climbing_plants::{clear_segment, sweep::clear_sweep, Plant, Terrain};
use glam::{Quat, Vec3};

const RIGIDITY: f32 = 12000.0;
const GUIDE_DENSITY: f32 = 4.0 * RIGIDITY / (ZONE_LENGTH * ZONE_LENGTH);
const PROPOSAL_MOTION: f32 = 1.8;

pub(super) fn propose(
    plant: &Plant,
    rod: &mut Rod,
    old: &[Vec3],
    tangents: &[Vec3],
    distance: &[f32],
    terrain: &impl Terrain,
    dt: f32,
    flexibility: f32,
) -> Option<Vec<Vec3>> {
    let base = plant.anchors.last().unwrap().node.max(
        plant
            .nodes
            .iter()
            .rposition(|n| n.id <= rod.locked_through)
            .unwrap_or(0),
    );
    let joints = old.len() - base - 1;
    if joints == 0 {
        return Some(old.to_vec());
    }
    let visits = joints.min(4);
    let mut positions = old.to_vec();
    let pending = rod.pending.as_ref().and_then(|p| {
        plant
            .nodes
            .iter()
            .position(|n| n.id == p.id)
            .map(|i| (i, p))
    });
    for offset in 0..visits {
        let j = base + (rod.weight_cursor + offset) % joints;
        let pivot = positions[j];
        let incoming = if j > 0 {
            (pivot - positions[j - 1]).normalize()
        } else {
            Vec3::Y
        };
        let outgoing = (positions[j + 1] - pivot).normalize();
        let material = &rod.material[&plant.nodes[j].id];
        let remembered = Quat::from_rotation_arc(material.direction, incoming)
            * rod.material[&plant.nodes[j + 1].id].direction;
        let maturity = (material.age / MATURATION).clamp(0.0, 1.0);
        let young = (1.0 - distance[j] / ZONE_LENGTH).clamp(0.0, 1.0) * (1.0 - maturity);
        let preferred = (incoming - remembered).lerp(tangents[j] - tangents[j + 1], young);
        let rest = (incoming - preferred).normalize();
        let stiffness = RIGIDITY * (1.0 + 2.0 * maturity) / plant.nodes[j + 1].rest_length;
        let mut torque = outgoing.cross(rest) * stiffness;
        let mut guide_weight = 0.0;
        let mut lever = 0.0f32;
        for k in j + 1..old.len() {
            let length = plant.nodes[k].rest_length;
            let midpoint = (positions[k] + positions[k - 1]) * 0.5;
            torque += (midpoint - pivot).cross(Vec3::NEG_Y) * (length * flexibility.min(2.0));
            let direction = (positions[k] - positions[k - 1]).normalize();
            let weight = GUIDE_DENSITY * length * (1.0 - distance[k] / ZONE_LENGTH).clamp(0.0, 1.0);
            torque += direction.cross(tangents[k]) * weight;
            guide_weight += weight;
            lever = lever.max(positions[k].distance(pivot));
        }
        if let Some((k, p)) = pending.filter(|(k, _)| *k > j) {
            torque += (positions[k] - pivot).cross(p.position - positions[k])
                * (1000.0 * p.time / CONTACT_TIME);
        }
        let turn = (torque * (4.0 * dt / (stiffness + guide_weight)))
            .clamp_length_max((PROPOSAL_MOTION / visits as f32) / lever.max(1.0));
        let contacts = collision::gather(plant, &positions, terrain)?;
        let turn = collision::project_turn(&contacts, &positions, j, turn);
        if turn.length_squared() < 1e-12 {
            continue;
        }
        for trial in 0..6 {
            let rotation = Quat::from_scaled_axis(turn * 0.5f32.powi(trial));
            let mut candidate = positions.clone();
            for k in j + 1..old.len() {
                candidate[k] = pivot + rotation * (positions[k] - pivot);
            }
            if candidate
                .iter()
                .zip(old)
                .any(|(a, b)| a.distance(*b) > PROPOSAL_MOTION)
            {
                continue;
            }
            let mut clear = true;
            for k in j + 1..old.len() {
                if !clear_segment(terrain, candidate[k - 1], candidate[k], plant.radius)?
                    || !clear_sweep(
                        terrain,
                        [
                            positions[k - 1],
                            positions[k],
                            candidate[k - 1],
                            candidate[k],
                        ],
                        plant.radius,
                    )?
                {
                    clear = false;
                    break;
                }
            }
            if clear {
                positions = candidate;
                break;
            }
        }
    }
    rod.weight_cursor = (rod.weight_cursor + visits) % joints;
    Some(positions)
}

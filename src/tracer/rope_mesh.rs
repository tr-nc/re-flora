//! Dynamic strand triangles use the existing opaque, lit scene pipeline.
//! Per-acquired-slot buffers are never rewritten while another frame reads them.
use super::static_scene::{StaticSceneMesh, StaticSceneVertex};
use crate::rope::Material;
use anyhow::{ensure, Result};
use glam::{Quat, Vec3, Vec4};
use re_flora_vkn::{vk, Allocator, Buffer, BufferUsage, Device, MemoryLocation};

pub(crate) struct RopeRenderStrand {
    pub points: Vec<Vec3>,
    pub radius: f32,
    pub material: Material,
    pub pins: [bool; 2],
}
struct Slot {
    vertices: Buffer,
    indices: Buffer,
    vertex_capacity: usize,
    index_capacity: usize,
}
pub(super) struct RopeResources {
    device: Device,
    allocator: Allocator,
    slots: Vec<Slot>,
    slot: usize,
    pending: StaticSceneMesh,
    pub index_count: u32,
}
impl RopeResources {
    pub fn new(device: Device, allocator: Allocator) -> Self {
        Self {
            device,
            allocator,
            slots: Vec::new(),
            slot: 0,
            pending: StaticSceneMesh::default(),
            index_count: 0,
        }
    }
    pub fn stage(
        &mut self,
        strands: &[RopeRenderStrand],
        pending_anchor: Option<Vec3>,
    ) -> Result<()> {
        let mut mesh = StaticSceneMesh::default();
        for strand in strands {
            ensure!(
                strand.points.len() >= 2
                    && strand.points.iter().all(|p| p.is_finite())
                    && strand.radius.is_finite()
                    && strand.radius > 0.,
                "invalid strand render snapshot"
            );
            let color = match strand.material {
                Material::Hemp => Vec3::new(0.62, 0.41, 0.19),
                Material::Hose => Vec3::new(0.24, 0.48, 0.31),
                Material::Kite => Vec3::new(0.78, 0.72, 0.53),
            };
            append_tube(
                &mut mesh,
                &strand.points,
                strand.radius,
                color,
                strand.material == Material::Hemp,
            );
            for (i, pinned) in [
                (0, strand.pins[0]),
                (strand.points.len() - 1, strand.pins[1]),
            ] {
                if pinned {
                    let p = strand.points[i];
                    let r = strand.radius * 1.7;
                    mesh.append_box(
                        p - Vec3::splat(r),
                        p + Vec3::splat(r),
                        Vec4::new(0.7, 0.53, 0.28, 1.),
                    );
                }
            }
            if strand.material == Material::Hose {
                let p = *strand.points.last().unwrap();
                let r = strand.radius * 1.4;
                mesh.append_box(
                    p - Vec3::splat(r),
                    p + Vec3::splat(r),
                    Vec4::new(0.70, 0.52, 0.22, 1.),
                );
            }
            if strand.material == Material::Kite {
                append_kite(&mut mesh, *strand.points.last().unwrap());
            }
        }
        if let Some(p) = pending_anchor {
            mesh.append_box(
                p - Vec3::splat(0.008),
                p + Vec3::splat(0.008),
                Vec4::new(0.95, 0.62, 0.18, 1.),
            );
        }
        self.pending = mesh;
        self.index_count = self.pending.indices.len() as u32;
        Ok(())
    }
    fn buffer(&self, usage: vk::BufferUsageFlags, bytes: usize) -> Buffer {
        Buffer::new_sized(
            self.device.clone(),
            self.allocator.clone(),
            BufferUsage::from_flags(usage),
            MemoryLocation::CpuToGpu,
            bytes as u64,
        )
    }
    pub fn prepare_frame(&mut self, slot: usize) -> Result<()> {
        self.slot = slot;
        if self.index_count == 0 {
            return Ok(());
        }
        while self.slots.len() <= slot {
            self.slots.push(Slot {
                vertices: self.buffer(vk::BufferUsageFlags::VERTEX_BUFFER, 40),
                indices: self.buffer(vk::BufferUsageFlags::INDEX_BUFFER, 4),
                vertex_capacity: 1,
                index_capacity: 1,
            });
        }
        let nv = self.pending.vertices.len();
        let ni = self.pending.indices.len();
        if self.slots[slot].vertex_capacity < nv {
            let capacity = nv.next_power_of_two();
            self.slots[slot].vertices = self.buffer(
                vk::BufferUsageFlags::VERTEX_BUFFER,
                capacity * std::mem::size_of::<StaticSceneVertex>(),
            );
            self.slots[slot].vertex_capacity = capacity;
        }
        if self.slots[slot].index_capacity < ni {
            let capacity = ni.next_power_of_two();
            self.slots[slot].indices =
                self.buffer(vk::BufferUsageFlags::INDEX_BUFFER, capacity * 4);
            self.slots[slot].index_capacity = capacity;
        }
        self.slots[slot].vertices.fill(&self.pending.vertices)?;
        self.slots[slot].indices.fill(&self.pending.indices)?;
        Ok(())
    }
    pub fn buffers(&self) -> Option<(&Buffer, &Buffer)> {
        (self.index_count > 0).then(|| {
            (
                &self.slots[self.slot].vertices,
                &self.slots[self.slot].indices,
            )
        })
    }
}

fn append_tube(
    mesh: &mut StaticSceneMesh,
    points: &[Vec3],
    radius: f32,
    color: Vec3,
    braided: bool,
) {
    const SIDES: usize = 8;
    let base = mesh.vertices.len() as u32;
    let mut last_tangent = (points[1] - points[0]).try_normalize().unwrap_or(Vec3::Y);
    let mut normal = last_tangent.any_orthonormal_vector();
    let mut length = 0.;
    for i in 0..points.len() {
        let delta = points[(i + 1).min(points.len() - 1)] - points[i.saturating_sub(1)];
        let tangent = delta.try_normalize().unwrap_or(last_tangent);
        if tangent.dot(last_tangent) > -0.9999 {
            normal = Quat::from_rotation_arc(last_tangent, tangent) * normal;
        }
        normal = (normal - tangent * normal.dot(tangent))
            .try_normalize()
            .unwrap_or_else(|| tangent.any_orthonormal_vector());
        let binormal = tangent.cross(normal);
        if i > 0 {
            length += points[i].distance(points[i - 1]);
        }
        for side in 0..SIDES {
            let angle = side as f32 * std::f32::consts::TAU / SIDES as f32;
            let n = normal * angle.cos() + binormal * angle.sin();
            let shade = if braided {
                0.87 + 0.13 * (angle * 3. - length * 480.).cos()
            } else {
                1.
            };
            mesh.vertices.push(StaticSceneVertex {
                position: (points[i] + n * radius).to_array(),
                normal: n.to_array(),
                color: (color * shade).extend(1.).to_array(),
            });
        }
        last_tangent = tangent;
    }
    for i in 0..points.len() - 1 {
        for s in 0..SIDES {
            let a = base + (i * SIDES + s) as u32;
            let b = base + (i * SIDES + (s + 1) % SIDES) as u32;
            let c = b + SIDES as u32;
            let d = a + SIDES as u32;
            mesh.indices.extend([a, b, c, a, c, d]);
        }
    }
    for end in [0, points.len() - 1] {
        let normal = if end == 0 {
            (points[0] - points[1])
                .try_normalize()
                .unwrap_or(-last_tangent)
        } else {
            (points[end] - points[end - 1])
                .try_normalize()
                .unwrap_or(last_tangent)
        };
        let c = mesh.vertices.len() as u32;
        mesh.vertices.push(StaticSceneVertex {
            position: points[end].to_array(),
            normal: normal.to_array(),
            color: color.extend(1.).to_array(),
        });
        for s in 0..SIDES {
            let a = base + (end * SIDES + s) as u32;
            let b = base + (end * SIDES + (s + 1) % SIDES) as u32;
            if end == 0 {
                mesh.indices.extend([c, b, a]);
            } else {
                mesh.indices.extend([c, a, b]);
            }
        }
    }
}
fn append_kite(mesh: &mut StaticSceneMesh, center: Vec3) {
    let corners = [
        Vec3::new(0., 0.12, 0.),
        Vec3::new(0.075, 0.02, 0.),
        Vec3::new(0., -0.09, 0.),
        Vec3::new(-0.075, 0.02, 0.),
    ];
    for (tri, color) in [
        ([0, 2, 1], Vec4::new(0.76, 0.28, 0.13, 1.)),
        ([0, 3, 2], Vec4::new(0.95, 0.64, 0.30, 1.)),
    ] {
        for back in [false, true] {
            let base = mesh.vertices.len() as u32;
            let indices = if back { [tri[2], tri[1], tri[0]] } else { tri };
            for i in indices {
                mesh.vertices.push(StaticSceneVertex {
                    position: (center + corners[i]).to_array(),
                    normal: (if back { Vec3::NEG_Z } else { Vec3::Z }).to_array(),
                    color: color.to_array(),
                });
            }
            mesh.indices.extend([base, base + 1, base + 2]);
        }
    }
    let tail = [
        center + Vec3::new(0., -0.09, 0.),
        center + Vec3::new(0.015, -0.15, 0.),
        center + Vec3::new(-0.01, -0.21, 0.),
    ];
    append_tube(mesh, &tail, 0.0015, Vec3::new(0.75, 0.48, 0.24), false);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tube_is_finite_outward_wound_and_handles_straight_and_reversing_segments() {
        for points in [
            vec![Vec3::ZERO, Vec3::Y, Vec3::Y * 2.],
            vec![Vec3::ZERO, Vec3::Y, Vec3::ZERO],
            vec![Vec3::ZERO, Vec3::ZERO, Vec3::Y],
        ] {
            let mut mesh = StaticSceneMesh::default();
            append_tube(&mut mesh, &points, 0.02, Vec3::ONE, true);
            assert!(mesh
                .vertices
                .iter()
                .all(|v| Vec3::from_array(v.position).is_finite()
                    && Vec3::from_array(v.normal).is_finite()));
            assert!(mesh
                .indices
                .iter()
                .all(|i| (*i as usize) < mesh.vertices.len()));
            if points[2] == Vec3::Y * 2. {
                for tri in mesh.indices.chunks_exact(3).take(32) {
                    let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| mesh.vertices[i as usize]);
                    assert!(
                        (Vec3::from_array(b.position) - Vec3::from_array(a.position))
                            .cross(Vec3::from_array(c.position) - Vec3::from_array(a.position))
                            .dot(Vec3::from_array(a.normal))
                            > 0.
                    );
                }
            }
        }
    }
}

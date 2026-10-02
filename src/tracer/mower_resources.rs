//! Fixed conventional model + a tiny transform instance; participates in raster/post-processing.
use super::static_scene::{StaticSceneMesh, StaticSceneVertex};
use crate::resource::Resource;
use anyhow::{ensure, Result};
use bytemuck::{Pod, Zeroable};
use glam::{Quat, Vec3, Vec4};
use re_flora_vkn::{vk, Allocator, Buffer, BufferUsage, Device, MemoryLocation};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MowerInstance {
    position: [f32; 3],
    rotation: [f32; 4],
}

pub(super) struct MowerRendererResources {
    pub vertices: Resource<Buffer>,
    pub indices: Resource<Buffer>,
    pub index_count: u32,
    pub visible: bool,
    device: Device,
    allocator: Allocator,
    frame_instances: Vec<Resource<Buffer>>,
    frame_poses: Vec<Option<(Vec3, Quat)>>,
    frame_slot: usize,
    pending_pose: Option<(Vec3, Quat)>,
}

impl MowerRendererResources {
    pub fn new(device: Device, allocator: Allocator) -> Self {
        let mesh = mower_mesh();
        let buffer = |usage, size| {
            Buffer::new_sized(
                device.clone(),
                allocator.clone(),
                BufferUsage::from_flags(usage),
                MemoryLocation::CpuToGpu,
                size,
            )
        };
        let vertices = buffer(
            vk::BufferUsageFlags::VERTEX_BUFFER,
            std::mem::size_of_val(mesh.vertices.as_slice()) as u64,
        );
        let indices = buffer(
            vk::BufferUsageFlags::INDEX_BUFFER,
            std::mem::size_of_val(mesh.indices.as_slice()) as u64,
        );
        vertices.fill(&mesh.vertices).expect("mower vertex upload");
        indices.fill(&mesh.indices).expect("mower index upload");
        Self {
            vertices: Resource::new(vertices),
            indices: Resource::new(indices),
            index_count: mesh.indices.len() as u32,
            visible: false,
            device,
            allocator,
            frame_instances: Vec::new(),
            frame_poses: Vec::new(),
            frame_slot: 0,
            pending_pose: None,
        }
    }

    pub fn show(&mut self, position: Option<Vec3>, rotation: Quat) -> Result<()> {
        let Some(position) = position else {
            self.visible = false;
            self.pending_pose = None;
            return Ok(());
        };
        ensure!(
            position.is_finite() && rotation.is_finite() && rotation.is_normalized(),
            "nonfinite mower pose"
        );
        self.pending_pose = Some((position, rotation));
        self.visible = true;
        Ok(())
    }

    /// Called only after acquiring the frame slot (its prior fence has completed).
    /// Never overwrite a transform buffer still read by another in-flight frame.
    pub fn prepare_frame(&mut self, slot: usize) -> Result<()> {
        self.frame_slot = slot;
        let Some((position, rotation)) = self.pending_pose else {
            return Ok(());
        };
        while self.frame_instances.len() <= slot {
            self.frame_instances.push(Resource::new(Buffer::new_sized(
                self.device.clone(),
                self.allocator.clone(),
                BufferUsage::from_flags(vk::BufferUsageFlags::VERTEX_BUFFER),
                MemoryLocation::CpuToGpu,
                std::mem::size_of::<MowerInstance>() as u64,
            )));
            self.frame_poses.push(None);
        }
        if self.frame_poses[slot] != self.pending_pose {
            self.frame_instances[slot].fill(&[MowerInstance {
                position: position.to_array(),
                rotation: rotation.to_array(),
            }])?;
            self.frame_poses[slot] = self.pending_pose;
        }
        Ok(())
    }

    pub fn instances(&self) -> &Buffer {
        &self.frame_instances[self.frame_slot]
    }
}

fn mower_mesh() -> StaticSceneMesh {
    let mut mesh = StaticSceneMesh::default();
    let mut add = |min: [f32; 3], max: [f32; 3], rgb: [f32; 3]| {
        mesh.append_box(
            Vec3::from_array(min) / 256.,
            Vec3::from_array(max) / 256.,
            Vec3::from_array(rgb).extend(1.),
        );
    };
    // 1950s petrol mower: sage pressed-metal deck, cream engine, red badge and chrome pushbar.
    add([-9., 3., -11.], [9., 6., 11.], [0.28, 0.49, 0.37]);
    add([-10., 4., -8.], [10., 7., 8.], [0.36, 0.58, 0.43]);
    add([-5., 7., -4.], [5., 12., 5.], [0.86, 0.81, 0.64]);
    add([-4., 12., -3.], [4., 13., 4.], [0.71, 0.26, 0.19]);
    add([5., 8., -1.], [8., 11., 3.], [0.30, 0.32, 0.30]);
    for x in [-3., -1., 1., 3.] {
        add([x, 8., 5.], [x + 0.6, 11., 5.5], [0.35, 0.36, 0.31]);
    }
    add([-6., 7., -9.], [6., 8., -5.], [0.75, 0.73, 0.63]);
    // Segmented sloping handle forms a recognizable push mower silhouette.
    for x in [-7., 6.] {
        for i in 0..10 {
            let y = 6. + i as f32 * 1.9;
            let z = -8. - i as f32 * 1.4;
            add([x, y, z - 1.5], [x + 1., y + 2.5, z], [0.70, 0.73, 0.68]);
        }
    }
    add([-8., 25., -22.5], [8., 27., -20.5], [0.20, 0.26, 0.23]);
    // Four low-poly tyres with cream hubs; the wheel bottoms are exactly at terrain height.
    for x in [-10., 10.] {
        for z in [-8., 8.] {
            append_wheel(&mut mesh, Vec3::new(x, 3., z) / 256.);
        }
    }
    mesh
}

fn append_wheel(mesh: &mut StaticSceneMesh, center: Vec3) {
    const SIDES: usize = 12;
    let radius = 3. / 256.;
    let half_width = 1.5 / 256.;
    for segment in 0..SIDES {
        let angle_a = segment as f32 * std::f32::consts::TAU / SIDES as f32;
        let angle_b = (segment + 1) as f32 * std::f32::consts::TAU / SIDES as f32;
        let a = Vec3::new(0., angle_a.cos(), angle_a.sin());
        let b = Vec3::new(0., angle_b.cos(), angle_b.sin());
        let left = center - Vec3::X * half_width;
        let right = center + Vec3::X * half_width;
        let dark = Vec4::new(0.18, 0.22, 0.20, 1.);
        let cream = Vec4::new(0.76, 0.73, 0.61, 1.);
        append_triangle(
            mesh,
            [left, left + b * radius, left + a * radius],
            Vec3::NEG_X,
            cream,
        );
        append_triangle(
            mesh,
            [right, right + a * radius, right + b * radius],
            Vec3::X,
            cream,
        );
        let normal = (a + b).normalize();
        append_triangle(
            mesh,
            [left + a * radius, left + b * radius, right + b * radius],
            normal,
            dark,
        );
        append_triangle(
            mesh,
            [left + a * radius, right + b * radius, right + a * radius],
            normal,
            dark,
        );
    }
}

fn append_triangle(mesh: &mut StaticSceneMesh, points: [Vec3; 3], normal: Vec3, color: Vec4) {
    let first = mesh.vertices.len() as u32;
    mesh.vertices.extend(points.map(|p| StaticSceneVertex {
        position: p.to_array(),
        normal: normal.to_array(),
        color: color.to_array(),
    }));
    mesh.indices.extend([first, first + 1, first + 2]);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_is_grounded_finite_and_outward_wound() {
        let mesh = mower_mesh();
        assert_eq!(std::mem::size_of::<MowerInstance>(), 28);
        let min_y = mesh
            .vertices
            .iter()
            .map(|v| v.position[1])
            .fold(f32::INFINITY, f32::min);
        assert!(min_y.abs() < 1e-6);
        for triangle in mesh.indices.chunks_exact(3) {
            let [a, b, c] =
                [triangle[0], triangle[1], triangle[2]].map(|i| mesh.vertices[i as usize]);
            let pa = Vec3::from_array(a.position);
            let pb = Vec3::from_array(b.position);
            let pc = Vec3::from_array(c.position);
            assert!(pa.is_finite() && pb.is_finite() && pc.is_finite());
            assert!((pb - pa).cross(pc - pa).dot(Vec3::from_array(a.normal)) > 0.);
        }
    }
}

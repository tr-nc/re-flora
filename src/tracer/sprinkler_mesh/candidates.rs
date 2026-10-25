//! Silhouette studies, not product replicas or an alternate irrigation model.
use super::*;
use crate::sprinkler::{Design, RotorPose};

const STEEL: Vec3 = Vec3::new(0.65, 0.70, 0.65);
const RUBBER: Vec3 = Vec3::new(0.12, 0.17, 0.16);
const ROTOR: Vec3 = Vec3::Y;

fn bar(mesh: &mut Mesh, start: Vec3, end: Vec3, width: f32, depth: f32, color: Vec3, motion: Vec3) {
    let axis = (end - start).normalize();
    let reference = if axis.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
    let u = (reference - axis * reference.dot(axis)).normalize() * (width * 0.5);
    let v = axis.cross(u.normalize()) * (depth * 0.5);
    let p = [
        start - u - v,
        start + u - v,
        start + u + v,
        start - u + v,
        end - u - v,
        end + u - v,
        end + u + v,
        end - u + v,
    ];
    for [a, b, c, d] in [
        [0, 3, 2, 1],
        [4, 5, 6, 7],
        [0, 1, 5, 4],
        [1, 2, 6, 5],
        [2, 3, 7, 6],
        [3, 0, 4, 7],
    ] {
        mesh.triangle([p[a], p[b], p[c]], color, motion);
        mesh.triangle([p[a], p[c], p[d]], color, motion);
    }
}

fn tube(mesh: &mut Mesh, a: Vec3, b: Vec3, radius: f32, color: Vec3, motion: Vec3) {
    mesh.lathe(
        a,
        (b - a).normalize(),
        &[
            (0., 0., color),
            (0., radius, color),
            ((b - a).length(), radius, color),
            ((b - a).length(), 0., color),
        ],
        8,
        motion,
    );
}

fn foot(mesh: &mut Mesh, radius: f32, color: Vec3) {
    mesh.lathe(
        Vec3::ZERO,
        Vec3::Y,
        &[
            (0., 0., DARK),
            (0., radius * 0.92, DARK),
            (0.2, radius, color),
            (0.5, radius, color),
            (0.72, radius * 0.78, SAGE_LIGHT),
            (0.85, 0.75, color),
            (0.85, 0., color),
        ],
        12,
        Vec3::ZERO,
    );
}

fn inlet(mesh: &mut Mesh) {
    mesh.lathe(
        Vec3::new(0., 0.7, 0.),
        -Vec3::Z,
        &[
            (0.5, 0., SAGE_DARK),
            (0.5, 0.45, SAGE_DARK),
            (2.1, 0.45, SAGE),
            (2.2, 0.62, BRASS),
            (2.65, 0.62, BRASS),
            (2.65, 0.32, BRASS),
            (2.5, 0.32, RUBBER),
            (2.5, 0., RUBBER),
        ],
        10,
        Vec3::ZERO,
    );
}

fn bearing(mesh: &mut Mesh, color: Vec3) {
    mesh.lathe(
        Vec3::ZERO,
        Vec3::Y,
        &[
            (0.6, 0., color),
            (0.6, 0.73, color),
            (1., 0.73, color),
            (1.2, 0.49, color),
            (2.7, 0.49, color),
            (2.8, 0.72, STEEL),
            (3.15, 0.72, STEEL),
            (3.25, 0.53, COPPER),
            (3.8, 0.53, COPPER),
            (3.8, 0., COPPER),
        ],
        12,
        Vec3::ZERO,
    );
    // Readable sector collars; these stay on the fixed water inlet.
    for y in [2.45, 2.64] {
        mesh.lathe(
            Vec3::Y * y,
            Vec3::Y,
            &[
                (0., 0.48, STEEL),
                (0., 0.82, STEEL),
                (0.10, 0.82, STEEL),
                (0.10, 0.48, STEEL),
            ],
            12,
            Vec3::ZERO,
        );
    }
}

fn mouth(mesh: &mut Mesh, length: f32, body: Vec3) {
    let pose = RotorPose { angle: 0. };
    let outlet = pose.outlet() / WORLD_SCALE;
    // Full-reach launch elevation; the baffle still disperses weaker water.
    let axis = (pose.direction() * 0.65 + Vec3::Y * 0.32).normalize();
    mesh.lathe(
        outlet - axis * length,
        axis,
        &[
            (0., 0., body),
            (0., 0.42, body),
            (length - 0.20, 0.42, body),
            (length - 0.12, 0.38, BRASS),
            (length, 0.34, BRASS),
            (length, 0.18, BRASS),
            (length - 0.11, 0.18, RUBBER),
            (length - 0.11, 0., RUBBER),
        ],
        12,
        ROTOR,
    );
}

fn brass_impact(mesh: &mut Mesh) {
    foot(mesh, 2.2, SAGE_DARK);
    inlet(mesh);
    bearing(mesh, BRASS);
    mesh.lathe(
        Vec3::ZERO,
        Vec3::Y,
        &[
            (3.25, 0., COPPER),
            (3.25, 0.9, COPPER),
            (3.55, 1.05, BRASS),
            (4.45, 0.86, BRASS),
            (4.6, 0.61, COPPER),
            (4.6, 0., COPPER),
        ],
        10,
        ROTOR,
    );
    let pose = RotorPose { angle: 0.0 };
    let axis = (pose.direction() * 0.65 + Vec3::Y * 0.32).normalize();
    // Cast elbow overlaps both the bearing body and the angled barrel.
    tube(
        mesh,
        Vec3::Y * 3.5,
        pose.outlet() / WORLD_SCALE - axis * 2.45,
        0.55,
        BRASS,
        ROTOR,
    );
    mouth(mesh, 2.55, BRASS);
    let side = Vec3::new(0.605, 0., 0.796).normalize();
    // Open arch and a central, visible return spring are the defining silhouette.
    for sign in [-1., 1.] {
        bar(
            mesh,
            side * sign * 1.2 + Vec3::Y * 4.1,
            side * sign * 1.1 + Vec3::Y * 6.15,
            0.32,
            0.32,
            BRASS,
            ROTOR,
        );
    }
    bar(
        mesh,
        -side * 1.22 + Vec3::Y * 6.15,
        side * 1.22 + Vec3::Y * 6.15,
        0.3,
        0.36,
        BRASS,
        ROTOR,
    );
    tube(mesh, Vec3::Y * 4.45, Vec3::Y * 6.35, 0.16, STEEL, ROTOR);
    for turn in 0..5 {
        let y = 4.7 + turn as f32 * 0.24;
        mesh.lathe(
            Vec3::Y * y,
            Vec3::Y,
            &[
                (0., 0.24, DARK),
                (0., 0.42, STEEL),
                (0.12, 0.42, STEEL),
                (0.12, 0.24, STEEL),
            ],
            12,
            ROTOR,
        );
    }
    let d = RotorPose { angle: 0. }.direction();
    let tip = RotorPose { angle: 0. }.outlet() / WORLD_SCALE + Vec3::Y * 0.65;
    let arm = Vec3::Y * 4.55 - side * 0.8;
    bar(mesh, arm, tip - side * 0.35, 0.24, 0.50, BRASS, ROTOR);
    bar(
        mesh,
        tip - side * 0.55,
        tip + side * 0.25,
        0.48,
        0.38,
        STEEL,
        ROTOR,
    );
    bar(mesh, arm, arm - d * 2.0, 0.30, 0.60, COPPER, ROTOR);
    bar(
        mesh,
        arm - d * 1.7 - side * 0.25,
        arm - d * 1.7 + side * 0.55,
        0.55,
        0.48,
        BRASS,
        ROTOR,
    );
}

fn garden_pulse(mesh: &mut Mesh) {
    // A low portable sled, rather than another scaled version of the round foot.
    for x in [-2.05, 2.05] {
        bar(
            mesh,
            Vec3::new(x, 0.25, -2.7),
            Vec3::new(x, 0.25, 2.7),
            0.50,
            0.78,
            SAGE_DARK,
            Vec3::ZERO,
        );
        bar(
            mesh,
            Vec3::new(x, 0.48, -2.3),
            Vec3::new(0., 0.85, -0.25),
            0.36,
            0.55,
            SAGE,
            Vec3::ZERO,
        );
        bar(
            mesh,
            Vec3::new(x, 0.48, 2.3),
            Vec3::new(0., 0.85, 0.25),
            0.36,
            0.55,
            SAGE,
            Vec3::ZERO,
        );
    }
    inlet(mesh);
    bearing(mesh, SAGE_DARK);
    mesh.lathe(
        Vec3::ZERO,
        Vec3::Y,
        &[
            (3.1, 0., DARK),
            (3.1, 1.25, DARK),
            (3.45, 1.55, SAGE_DARK),
            (4.4, 1.35, SAGE),
            (4.75, 0.95, SAGE_LIGHT),
            (4.85, 0., SAGE_LIGHT),
        ],
        12,
        ROTOR,
    );
    mouth(mesh, 1.9, SAGE_DARK);
    let d = RotorPose { angle: 0. }.direction();
    let side = Vec3::new(-d.z, 0., d.x);
    let tip = RotorPose { angle: 0. }.outlet() / WORLD_SCALE + Vec3::Y * 0.70;
    let hinge = Vec3::Y * 4.55 - side * 1.0;
    bar(mesh, hinge, tip - side * 0.40, 0.25, 0.72, STEEL, ROTOR);
    bar(
        mesh,
        tip - side * 0.6,
        tip + side * 0.3,
        0.45,
        0.50,
        STEEL,
        ROTOR,
    );
    bar(mesh, hinge, hinge - d * 1.45, 0.44, 1.35, SAGE_DARK, ROTOR);
    // A generous adjustment dial survives low-resolution model sampling.
    mesh.lathe(
        Vec3::Y * 4.7,
        Vec3::Y,
        &[
            (0., 0., COPPER),
            (0., 0.74, COPPER),
            (0.35, 0.74, COPPER),
            (0.43, 0.57, BRASS),
            (0.43, 0., BRASS),
        ],
        10,
        ROTOR,
    );
    bar(
        mesh,
        -d * 0.47 + Vec3::Y * 5.15,
        d * 0.47 + Vec3::Y * 5.15,
        0.14,
        0.19,
        CREAM,
        ROTOR,
    );
}

fn enclosed_rotor(mesh: &mut Mesh) {
    mesh.lathe(
        Vec3::ZERO,
        Vec3::Y,
        &[
            (0., 0., RUBBER),
            (0., 2.55, RUBBER),
            (0.25, 2.9, DARK),
            (0.65, 2.9, DARK),
            (0.82, 2.35, SAGE_DARK),
            (2.95, 2.35, SAGE_DARK),
            (3.1, 2.65, CREAM),
            (3.3, 2.65, CREAM),
            (3.3, 0., CREAM),
        ],
        16,
        Vec3::ZERO,
    );
    mesh.lathe(
        Vec3::ZERO,
        Vec3::Y,
        &[
            (3.2, 0., RUBBER),
            (3.2, 2.4, RUBBER),
            (4.65, 2.4, DARK),
            (4.85, 2.65, SAGE),
            (5.12, 2.65, SAGE),
            (5.25, 2.35, SAGE_LIGHT),
            (5.25, 0., SAGE_LIGHT),
        ],
        16,
        ROTOR,
    );
    mouth(mesh, 0.85, DARK);
    let d = RotorPose { angle: 0. }.direction();
    bar(
        mesh,
        Vec3::Y * 5.28 - d * 0.25,
        Vec3::Y * 5.28 + d * 1.65,
        0.10,
        0.34,
        CREAM,
        ROTOR,
    );
    mesh.lathe(
        Vec3::Y * 5.22,
        Vec3::Y,
        &[
            (0., 0., DARK),
            (0., 0.33, DARK),
            (0.12, 0.33, CREAM),
            (0.12, 0., CREAM),
        ],
        8,
        ROTOR,
    );
}

fn tall(mesh: &mut Mesh) {
    // A complete elevated impact head on a braced stand. The moving head and
    // water outlet receive exactly the same lift; the ground foot stays fixed.
    brass_impact(mesh);
    for vertex in &mut mesh.vertices {
        vertex.position[1] += crate::sprinkler::TALL_LIFT_VOXELS * WORLD_SCALE;
    }
    foot(mesh, 3.6, SAGE_DARK);
    inlet(mesh);
    tube(
        mesh,
        Vec3::Y * 0.7,
        Vec3::Y * (crate::sprinkler::TALL_LIFT_VOXELS + 0.8),
        0.58,
        BRASS,
        Vec3::ZERO,
    );
    for n in 0..3 {
        let angle = n as f32 * std::f32::consts::TAU / 3.0;
        let foot = Vec3::new(angle.cos() * 3.4, 0.4, angle.sin() * 3.4);
        tube(mesh, foot, Vec3::Y * 8.0, 0.20, SAGE, Vec3::ZERO);
    }
}

pub(super) fn build(design: Design) -> (Vec<SprinklerVertex>, Vec<u32>) {
    let mut mesh = Mesh::default();
    match design {
        Design::BrassImpact => brass_impact(&mut mesh),
        Design::GardenPulse => garden_pulse(&mut mesh),
        Design::EnclosedRotor => enclosed_rotor(&mut mesh),
        Design::Tall => tall(&mut mesh),
        Design::Current => unreachable!("the reference mesh has its own authoring function"),
    }
    (mesh.vertices, mesh.indices)
}

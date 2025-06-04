use glam::Vec3;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

// Assuming crate::geom::{Cuboid, RoundCone} are defined elsewhere
// For example:
// mod geom {
//     use glam::Vec3;
//     #[derive(Debug)]
//     pub struct Cuboid { pos: Vec3, half_size: Vec3 }
//     impl Cuboid { pub fn new(pos: Vec3, half_size: Vec3) -> Self { Self { pos, half_size } } }
//     #[derive(Debug)]
//     pub struct RoundCone { r1: f32, p1: Vec3, r2: f32, p2: Vec3 }
//     impl RoundCone { pub fn new(r1: f32, p1: Vec3, r2: f32, p2: Vec3) -> Self { Self { r1, p1, r2, p2 } } }
// }
use crate::geom::{Cuboid, RoundCone};

#[derive(Debug, Clone)]
pub struct TreeDesc {
    pub size: f32,
    pub trunk_thickness: f32,
    pub trunk_thickness_min: f32,
    pub spread: f32,
    pub twisted: f32,
    /// The actual leaves size is 2^leaves_size_level
    pub leaves_size_level: u32,
    pub gravity: f32,
    pub iterations: u32,
    pub wide: f32,
    pub seed: u64,
}

impl Default for TreeDesc {
    fn default() -> Self {
        TreeDesc {
            size: 3.0,
            trunk_thickness: 0.6,
            trunk_thickness_min: 1.05,
            spread: 0.47,
            twisted: 0.08,
            leaves_size_level: 5,
            gravity: 0.0,
            iterations: 12,
            wide: 0.5,
            seed: 30,
        }
    }
}

impl TreeDesc {
    pub fn edit_by_gui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;

        changed |= ui
            .add(
                egui::Slider::new(&mut self.size, 0.1..=50.0)
                    .text("Tree Size")
                    .logarithmic(true),
            )
            .changed();

        changed |= ui
            .add(egui::Slider::new(&mut self.trunk_thickness, 0.01..=5.0).text("Trunk Thickness"))
            .changed();

        changed |= ui
            .add(
                egui::Slider::new(&mut self.trunk_thickness_min, 0.001..=2.0)
                    .text("Min Trunk Thickness"),
            )
            .changed();

        changed |= ui
            .add(egui::Slider::new(&mut self.spread, 0.0..=1.0).text("Spread"))
            .changed();

        changed |= ui
            .add(egui::Slider::new(&mut self.twisted, 0.0..=1.0).text("Twisted"))
            .changed();

        changed |= ui
            .add(
                egui::Slider::new(&mut self.leaves_size_level, 0..=8)
                    .text("Leaves Size Level (2^level)"),
            )
            .changed();

        changed |= ui
            .add(egui::Slider::new(&mut self.gravity, -2.0..=2.0).text("Gravity"))
            .changed();

        changed |= ui
            .add(egui::Slider::new(&mut self.iterations, 1..=12).text("Iterations"))
            .changed();

        changed |= ui
            .add(egui::Slider::new(&mut self.wide, 0.0..=5.0).text("Wide"))
            .changed();

        changed |= ui
            .add(
                egui::DragValue::new(&mut self.seed)
                    .speed(1.0)
                    .prefix("Seed: "),
            )
            .changed();

        return changed;
    }
}

#[derive(Debug)]
struct BuiltObjects {
    trunks: Vec<RoundCone>,
    leaves: Vec<Cuboid>,
}

#[derive(Debug)]
pub struct Tree {
    desc: TreeDesc,
    built_objects: BuiltObjects,
}

impl Tree {
    pub fn new(desc: TreeDesc) -> Self {
        let built_objects = Self::build(&desc);
        Tree {
            desc,
            built_objects,
        }
    }

    pub fn desc(&self) -> &TreeDesc {
        &self.desc
    }

    pub fn trunks(&self) -> &[RoundCone] {
        &self.built_objects.trunks
    }

    pub fn leaves(&self) -> &[Cuboid] {
        &self.built_objects.leaves
    }

    fn build(desc: &TreeDesc) -> BuiltObjects {
        let mut rng = StdRng::seed_from_u64(desc.seed);
        let mut trunks = Vec::new();
        let mut leaves_positions = Vec::new();

        let size_factor = 150.0 * desc.size / (desc.iterations.max(1) as f32); // Ensure iterations is not 0 for division
        let branch_len_start = size_factor * (1.0 - desc.wide.clamp(0.0, 1.0));
        let branch_len_end = size_factor * desc.wide.clamp(0.0, 1.0);
        let trunk_thickness_base = desc.trunk_thickness * desc.size * 6.0;

        recurse(
            Vec3::ZERO,
            Vec3::Y,
            0,
            desc,
            branch_len_start,
            branch_len_end,
            trunk_thickness_base,
            &mut trunks,
            &mut leaves_positions,
            &mut rng,
        );

        let leaves = make_leaves(&leaves_positions, desc.leaves_size_level);

        return BuiltObjects { trunks, leaves };

        fn recurse(
            pos: Vec3,
            dir: Vec3,
            i: u32,
            desc: &TreeDesc,
            branch_len_start: f32,
            branch_len_end: f32,
            trunk_thickness_base: f32,
            trunks: &mut Vec<RoundCone>,
            leaves: &mut Vec<Vec3>,
            rng: &mut StdRng,
        ) {
            let iter_f = desc.iterations.max(1) as f32; // Ensure iterations is not 0 for division
            let t = ((i as f32) / iter_f).sqrt();
            let branch_len = branch_len_start + t * (branch_len_end - branch_len_start);

            let t_next = (((i + 1) as f32) / iter_f).sqrt();
            let mut thickness_start_val = (1.0 - t) * trunk_thickness_base;
            let mut thickness_end_val = (1.0 - t_next) * trunk_thickness_base;

            thickness_start_val = thickness_start_val.max(desc.trunk_thickness_min);
            thickness_end_val = thickness_end_val.max(desc.trunk_thickness_min);

            let end = pos + dir * branch_len;

            // --- MODIFICATION START: Only add trunk segment if i > 0 ---
            // This skips drawing the segment for i=0, effectively removing the root sphere/cone.
            // The tree structure will start from the children of the conceptual i=0 segment.
            if i > 0 {
                trunks.push(RoundCone::new(
                    thickness_start_val,
                    pos,
                    thickness_end_val,
                    end,
                ));
            }
            // --- MODIFICATION END ---

            if i < desc.iterations - 1 {
                let mut b = 1;
                let mut var = (i as f32) * 0.2 * desc.twisted;
                if rng.gen::<f32>() < t {
                    b = 2;
                    var = 2.0 * desc.spread * t;
                }

                for _ in 0..b {
                    let rand_dx = rng.gen_range(-var..=var);
                    let rand_dy = rng.gen_range(-var..=var);
                    let rand_dz = rng.gen_range(-var..=var);

                    let next_dir_x_unnormalized = dir.x + rand_dx;
                    let mut next_dir_y_unnormalized = dir.y + rand_dy;
                    let next_dir_z_unnormalized = dir.z + rand_dz;

                    let downward_pull = desc.gravity * t.powi(2);
                    next_dir_y_unnormalized -= downward_pull;

                    let new_dir = Vec3::new(
                        next_dir_x_unnormalized,
                        next_dir_y_unnormalized,
                        next_dir_z_unnormalized,
                    )
                    .normalize_or_zero();

                    if new_dir != Vec3::ZERO {
                        recurse(
                            end, // Children start from the 'end' of the current (possibly conceptual) segment
                            new_dir,
                            i + 1, // Increment iteration count for children
                            desc,
                            branch_len_start,
                            branch_len_end,
                            trunk_thickness_base,
                            trunks,
                            leaves,
                            rng,
                        );
                    }
                }
            } else {
                // Leaf spawn point at the midpoint of the last segment (even if conceptual for i=0)
                leaves.push((pos + end) * 0.5);
            }
        }

        fn make_leaves(leaves_positions: &[Vec3], leaves_size_level: u32) -> Vec<Cuboid> {
            let mut leaves = Vec::new();
            let leaf_actual_size = 2_u32.pow(leaves_size_level) as f32;
            for pos in leaves_positions {
                leaves.push(Cuboid::new(*pos, Vec3::splat(leaf_actual_size * 0.5)));
            }
            leaves
        }
    }
}

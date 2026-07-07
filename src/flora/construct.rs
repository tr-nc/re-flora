use crate::branch_skeleton::{generate_branch_skeleton, BranchingDesc};
use crate::tracer::voxel_encoding::{
    append_indexed_cube_data, append_indexed_cube_data_with_info, FloraMeshData, FloraVoxelInfo,
    FLORA_VOXEL_MATERIAL_TOMATO_FRUIT,
};
use anyhow::Result;
use glam::{IVec3, Vec3};
use std::{collections::HashMap, f32::consts::PI};

fn gen_grass_column(voxel_count: u32, is_lod_used: bool) -> Result<FloraMeshData> {
    const ORIGIN: IVec3 = IVec3::new(0, 0, 0);
    let max_length = voxel_count - 1;

    let mut mesh = FloraMeshData::new(max_length);

    for i in 0..voxel_count {
        let vertex_offset = mesh.vertices.len() as u32;
        let base_pos = IVec3::new(0, i as i32, 0);

        append_indexed_cube_data(
            &mut mesh.vertices,
            &mut mesh.indices,
            &mut mesh.voxel_infos,
            base_pos,
            vertex_offset,
            ORIGIN,
            max_length,
            is_lod_used,
        )?;
    }

    Ok(mesh)
}

pub fn gen_tall_grass(is_lod_used: bool) -> Result<FloraMeshData> {
    gen_grass_column(8, is_lod_used)
}

pub fn gen_short_grass(is_lod_used: bool) -> Result<FloraMeshData> {
    gen_grass_column(4, is_lod_used)
}

pub fn gen_carrot(is_lod_used: bool) -> Result<FloraMeshData> {
    const BURIED_TIP_Y: i32 = -4;
    const ORIGIN: IVec3 = IVec3::new(0, BURIED_TIP_Y, 0);
    const LEAF_BASE_Y: i32 = 2;
    const LEAF_HEIGHT: i32 = 4;
    const MAX_LENGTH: u32 = 9;

    let mut mesh = FloraMeshData::new(MAX_LENGTH);

    // Ten voxels tall from buried tip y=-4 to leaf tip y=5. Most orange root voxels sit below
    // the soil; y=0..1 leaves a small carrot shoulder visible above ground.
    const ROOT_LAYERS: &[(i32, i32)] = &[(-4, 0), (-3, 0), (-2, 1), (-1, 1), (0, 1), (1, 0)];
    for &(y, radius) in ROOT_LAYERS {
        for x in -radius..=radius {
            for z in -radius..=radius {
                if x.abs() + z.abs() > radius + 1 {
                    continue;
                }
                let vertex_offset = mesh.vertices.len() as u32;
                append_indexed_cube_data(
                    &mut mesh.vertices,
                    &mut mesh.indices,
                    &mut mesh.voxel_infos,
                    IVec3::new(x, y, z),
                    vertex_offset,
                    ORIGIN,
                    MAX_LENGTH,
                    is_lod_used,
                )?;
            }
        }
    }

    // Leafy tufts start above the exposed orange shoulder and reach up to y=5.
    const LEAF_TUFTS: &[(i32, i32)] = &[(0, 0), (-1, 0), (1, 1), (0, -1)];
    for &(base_x, base_z) in LEAF_TUFTS {
        for y in 0..LEAF_HEIGHT {
            let lean_x = base_x * y / 3;
            let lean_z = base_z * y / 3;
            let vertex_offset = mesh.vertices.len() as u32;
            append_indexed_cube_data(
                &mut mesh.vertices,
                &mut mesh.indices,
                &mut mesh.voxel_infos,
                IVec3::new(base_x + lean_x, LEAF_BASE_Y + y, base_z + lean_z),
                vertex_offset,
                ORIGIN,
                MAX_LENGTH,
                is_lod_used,
            )?;
        }
    }

    Ok(mesh)
}

type TomatoVoxelKey = (i32, i32, i32);
type TomatoVoxelMap = HashMap<TomatoVoxelKey, TomatoMaterial>;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum TomatoMaterial {
    Stem,
    Fruit,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct TomatoVoxel {
    pos: IVec3,
    material: TomatoMaterial,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TomatoVineDesc {
    pub branching: BranchingDesc,
    pub overall_scale: f32,
    pub base_diameter_voxels: u32,
    pub thickness_taper_power: f32,
    pub fruit_count: u32,
    pub fruit_radius: f32,
    pub fruit_min_height_fraction: f32,
}

fn tomato_voxel_key(pos: IVec3) -> TomatoVoxelKey {
    (pos.x, pos.y, pos.z)
}

fn insert_tomato_voxel(voxels: &mut TomatoVoxelMap, pos: IVec3, material: TomatoMaterial) {
    if pos.y < 0 {
        return;
    }

    let key = tomato_voxel_key(pos);
    match (voxels.get(&key).copied(), material) {
        (Some(TomatoMaterial::Fruit), TomatoMaterial::Stem) => {}
        _ => {
            voxels.insert(key, material);
        }
    }
}

fn tomato_cross_section_offsets(diameter: u32) -> Vec<IVec3> {
    let diameter = diameter.max(1) as i32;
    if diameter == 1 {
        return vec![IVec3::ZERO];
    }

    let center = (diameter - 1) as f32 * 0.5;
    let radius = diameter as f32 * 0.5;
    let origin = diameter / 2;
    let mut offsets = Vec::new();
    for x in 0..diameter {
        for z in 0..diameter {
            let dx = x as f32 - center;
            let dz = z as f32 - center;
            if dx * dx + dz * dz <= radius * radius + 0.001 {
                offsets.push(IVec3::new(x - origin, 0, z - origin));
            }
        }
    }
    offsets
}

fn tomato_branch_diameter(desc: &TomatoVineDesc, level: u32, segment_t: f32) -> u32 {
    let base = desc.base_diameter_voxels.max(1) as f32;
    if base <= 1.0 {
        return 1;
    }

    let max_progress = desc.branching.iterations.saturating_sub(1).max(1) as f32;
    let progress = ((level as f32 + segment_t) / max_progress).clamp(0.0, 1.0);
    let taper = progress.powf(desc.thickness_taper_power.max(0.05));
    (1.0 + (base - 1.0) * (1.0 - taper)).round().max(1.0) as u32
}

fn push_tomato_branch_segment(
    voxels: &mut TomatoVoxelMap,
    start: Vec3,
    end: Vec3,
    level: u32,
    desc: &TomatoVineDesc,
) {
    let delta = end - start;
    let steps = delta
        .x
        .abs()
        .max(delta.y.abs())
        .max(delta.z.abs())
        .ceil()
        .max(1.0) as i32;

    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let center = round_vec3_to_ivec3(start.lerp(end, t));
        let diameter = tomato_branch_diameter(desc, level, t);
        for offset in tomato_cross_section_offsets(diameter) {
            insert_tomato_voxel(voxels, center + offset, TomatoMaterial::Stem);
        }
    }
}

fn insert_tomato_fruit_sphere(voxels: &mut TomatoVoxelMap, center: IVec3, radius: f32) {
    let radius = radius.max(0.5);
    let search_radius = radius.ceil() as i32;
    for x in -search_radius..=search_radius {
        for y in -search_radius..=search_radius {
            for z in -search_radius..=search_radius {
                let offset = IVec3::new(x, y, z);
                let sample = offset.as_vec3();
                if sample.length() <= radius + 0.001 {
                    insert_tomato_voxel(voxels, center + offset, TomatoMaterial::Fruit);
                }
            }
        }
    }
}

fn add_tomato_fruits(
    voxels: &mut TomatoVoxelMap,
    skeleton: &crate::branch_skeleton::BranchSkeleton,
    desc: &TomatoVineDesc,
) {
    let fruit_count = desc.fruit_count as usize;
    if fruit_count == 0 || desc.fruit_radius <= 0.0 {
        return;
    }

    let scale = desc.overall_scale.max(0.1);
    let max_y = skeleton
        .segments
        .iter()
        .map(|segment| (segment.end * scale).y)
        .fold(0.0_f32, f32::max);
    let min_y = max_y * desc.fruit_min_height_fraction.clamp(0.0, 1.0);

    let mut candidates = skeleton
        .segments
        .iter()
        .filter_map(|segment| {
            let start = segment.start * scale;
            let end = segment.end * scale;
            if segment.level == 0 || end.y < min_y {
                return None;
            }
            Some((start, end, segment.level))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| {
        b.1.y
            .total_cmp(&a.1.y)
            .then_with(|| b.2.cmp(&a.2))
            .then_with(|| a.1.x.total_cmp(&b.1.x))
            .then_with(|| a.1.z.total_cmp(&b.1.z))
    });
    if candidates.is_empty() {
        return;
    }

    for fruit_index in 0..fruit_count {
        let candidate_index = fruit_index * candidates.len() / fruit_count;
        let (start, end, _) = candidates[candidate_index.min(candidates.len() - 1)];
        let dir = (end - start).normalize_or_zero();
        let mut side = Vec3::new(-dir.z, 0.0, dir.x).normalize_or_zero();
        if side == Vec3::ZERO {
            side = if fruit_index % 2 == 0 {
                Vec3::X
            } else {
                Vec3::Z
            };
        }
        if fruit_index % 2 == 1 {
            side = -side;
        }
        let hang = Vec3::new(0.0, -0.35 * desc.fruit_radius, 0.0);
        let center = round_vec3_to_ivec3(end + side * (desc.fruit_radius + 1.0) + hang);
        insert_tomato_fruit_sphere(voxels, center, desc.fruit_radius);
    }
}

fn round_vec3_to_ivec3(pos: Vec3) -> IVec3 {
    IVec3::new(
        pos.x.round() as i32,
        pos.y.round() as i32,
        pos.z.round() as i32,
    )
}

pub const TOMATO_BRANCHING_SEED_HIGH_BITS: u64 = 0x746f_0000_0000;
pub const TOMATO_BRANCHING_SEED_LOW_BITS: u32 = 0x6d61_746f;

pub fn default_tomato_branching_desc() -> BranchingDesc {
    BranchingDesc {
        // Fixed species-level seed: the authored tomato has procedural shape, but every placed
        // instance still shares this exact mesh until we intentionally add per-instance variants.
        seed: TOMATO_BRANCHING_SEED_HIGH_BITS | TOMATO_BRANCHING_SEED_LOW_BITS as u64,
        iterations: 5,
        branch_start_fraction: 0.0,
        branch_end_fraction: 1.0,
        initial_length: 4.0,
        length_dropoff: 0.76,
        spread: 0.08,
        randomness: 0.18,
        vertical_tendency: 0.48,
        branch_angle_min: 30.0_f32.to_radians(),
        branch_angle_max: 58.0_f32.to_radians(),
        branch_probability: 0.92,
        branch_count_min: 2,
        branch_count_max: 3,
        segment_length_variation: 0.10,
        continue_main_axis: true,
    }
}

pub fn default_tomato_vine_desc() -> TomatoVineDesc {
    TomatoVineDesc {
        branching: default_tomato_branching_desc(),
        overall_scale: 1.0,
        base_diameter_voxels: 1,
        thickness_taper_power: 1.15,
        fruit_count: 0,
        fruit_radius: 1.25,
        fruit_min_height_fraction: 0.35,
    }
}

fn tomato_vine_voxels(desc: &TomatoVineDesc) -> Vec<TomatoVoxel> {
    let skeleton = generate_branch_skeleton(&desc.branching);
    let scale = desc.overall_scale.max(0.1);
    let mut voxel_map = TomatoVoxelMap::new();

    for segment in &skeleton.segments {
        push_tomato_branch_segment(
            &mut voxel_map,
            segment.start * scale,
            segment.end * scale,
            segment.level,
            desc,
        );
    }
    add_tomato_fruits(&mut voxel_map, &skeleton, desc);

    let mut voxels = voxel_map
        .into_iter()
        .map(|((x, y, z), material)| TomatoVoxel {
            pos: IVec3::new(x, y, z),
            material,
        })
        .collect::<Vec<_>>();
    voxels.sort_by_key(|voxel| (voxel.pos.y, voxel.pos.x, voxel.pos.z, voxel.material as u8));
    voxels
}

fn tomato_max_length(voxels: &[TomatoVoxel], origin: IVec3) -> u32 {
    voxels
        .iter()
        .map(|voxel| (voxel.pos - origin).as_vec3().length().ceil() as u32)
        .max()
        .unwrap_or(1)
        .max(1)
}

fn tomato_voxel_growth_gradient(pos: IVec3, origin: IVec3, max_length: u32) -> f32 {
    ((pos - origin).as_vec3().length() / max_length.max(1) as f32).clamp(0.0, 1.0)
}

pub fn gen_tomato_with_desc(desc: &TomatoVineDesc, is_lod_used: bool) -> Result<FloraMeshData> {
    const ORIGIN: IVec3 = IVec3::new(0, 0, 0);

    let voxels = tomato_vine_voxels(desc);
    let max_length = tomato_max_length(&voxels, ORIGIN);
    let mut mesh = FloraMeshData::new(max_length);

    for voxel in voxels {
        let vertex_offset = mesh.vertices.len() as u32;
        match voxel.material {
            TomatoMaterial::Stem => append_indexed_cube_data(
                &mut mesh.vertices,
                &mut mesh.indices,
                &mut mesh.voxel_infos,
                voxel.pos,
                vertex_offset,
                ORIGIN,
                max_length,
                is_lod_used,
            )?,
            TomatoMaterial::Fruit => append_indexed_cube_data_with_info(
                &mut mesh.vertices,
                &mut mesh.indices,
                &mut mesh.voxel_infos,
                voxel.pos,
                vertex_offset,
                FloraVoxelInfo::new(
                    1.0,
                    0.75,
                    tomato_voxel_growth_gradient(voxel.pos, ORIGIN, max_length),
                    FLORA_VOXEL_MATERIAL_TOMATO_FRUIT,
                ),
                is_lod_used,
            )?,
        }
    }

    Ok(mesh)
}

pub fn gen_tomato(is_lod_used: bool) -> Result<FloraMeshData> {
    gen_tomato_with_desc(&default_tomato_vine_desc(), is_lod_used)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tomato_vine_keeps_single_voxel_stems() {
        let desc = default_tomato_vine_desc();
        let voxels = tomato_vine_voxels(&desc);

        assert!(voxels.iter().any(|voxel| voxel.pos == IVec3::ZERO));
        assert!(voxels.iter().all(|voxel| voxel.pos.y >= 0));
        assert!(voxels
            .iter()
            .all(|voxel| voxel.material == TomatoMaterial::Stem));
    }

    #[test]
    fn tomato_vine_stays_in_half_height_scale() {
        let desc = default_tomato_vine_desc();
        let voxels = tomato_vine_voxels(&desc);
        let max_y = voxels.iter().map(|voxel| voxel.pos.y).max().unwrap_or(0);

        assert!((7..=15).contains(&max_y), "max_y was {max_y}");
    }

    #[test]
    fn tomato_vine_can_emit_fruit_voxels_when_configured() {
        let desc = TomatoVineDesc {
            fruit_count: 1,
            ..default_tomato_vine_desc()
        };
        let voxels = tomato_vine_voxels(&desc);

        assert!(voxels
            .iter()
            .any(|voxel| voxel.material == TomatoMaterial::Fruit));
    }
}

pub fn gen_lavender(is_lod_used: bool) -> Result<FloraMeshData> {
    const STEM_VOXEL_COUNT: u32 = 6;
    const LEAF_BALL_RADIUS: f32 = 1.5;
    const LEAF_BALL_BOUNDARY: i32 = LEAF_BALL_RADIUS as i32;
    const ORIGIN: IVec3 = IVec3::new(0, 0, 0);

    let max_vertical = (STEM_VOXEL_COUNT + LEAF_BALL_BOUNDARY as u32) as f32;
    let max_horizontal = LEAF_BALL_BOUNDARY as f32;
    let max_length = ((max_vertical * max_vertical + 2.0 * max_horizontal * max_horizontal).sqrt())
        .ceil()
        .max(1.0) as u32;

    let mut mesh = FloraMeshData::new(max_length);

    // draw the stem
    let total_stem_voxel_count = STEM_VOXEL_COUNT - LEAF_BALL_BOUNDARY as u32;
    for i in 0..total_stem_voxel_count {
        let vertex_offset = mesh.vertices.len() as u32;
        let base_pos = IVec3::new(0, i as i32, 0);

        append_indexed_cube_data(
            &mut mesh.vertices,
            &mut mesh.indices,
            &mut mesh.voxel_infos,
            base_pos,
            vertex_offset,
            ORIGIN,
            max_length,
            is_lod_used,
        )?;
    }

    // draw the leaf ball at the top of the stem
    for i in -LEAF_BALL_BOUNDARY..=LEAF_BALL_BOUNDARY {
        for j in -LEAF_BALL_BOUNDARY..=LEAF_BALL_BOUNDARY {
            for k in -LEAF_BALL_BOUNDARY..=LEAF_BALL_BOUNDARY {
                if i * i + j * j + k * k > LEAF_BALL_BOUNDARY * LEAF_BALL_BOUNDARY {
                    continue;
                }

                let vertex_offset = mesh.vertices.len() as u32;
                let base_pos = IVec3::new(i, j, k) + IVec3::new(0, STEM_VOXEL_COUNT as i32, 0);

                append_indexed_cube_data(
                    &mut mesh.vertices,
                    &mut mesh.indices,
                    &mut mesh.voxel_infos,
                    base_pos,
                    vertex_offset,
                    ORIGIN,
                    max_length,
                    is_lod_used,
                )?;
            }
        }
    }

    Ok(mesh)
}

pub fn gen_ember_bloom(is_lod_used: bool) -> Result<FloraMeshData> {
    const HEIGHT: i32 = 12;
    // Width Configuration: How wide the plant swells
    const MAX_RADIUS: f32 = 2.0;
    const ORIGIN: IVec3 = IVec3::new(0, 0, 0);

    let max_vertical = (HEIGHT - 1) as f32;
    let max_horizontal = (MAX_RADIUS + 2.0).ceil(); // includes search padding
    let max_length = ((max_vertical * max_vertical + 2.0 * max_horizontal * max_horizontal).sqrt())
        .ceil()
        .max(1.0) as u32;

    let mut mesh = FloraMeshData::new(max_length);

    for y in 0..HEIGHT {
        // Normalized height (0.0 at bottom, 1.0 at top)
        let t = y as f32 / HEIGHT as f32;

        // Vertical Profile:
        // Uses a sine wave to create a soft bulb shape.
        // It starts small, swells wide in the middle, and tapers at the top.
        // We add 0.5 base radius so it doesn't disappear completely at the very bottom.
        let vertical_swell = (t * PI).sin();
        let base_radius = 0.5 + (vertical_swell * MAX_RADIUS);

        // Define search area for this layer
        let search_radius = (base_radius + 1.5).ceil() as i32;

        for x in -search_radius..=search_radius {
            for z in -search_radius..=search_radius {
                // Calculate distance from center (0,0)
                let dist_sq = (x * x + z * z) as f32;
                let dist = dist_sq.sqrt();

                // Calculate Angle for the "Wavy" texture
                let angle = (z as f32).atan2(x as f32);

                // Wavy/Leafy Logic:
                // We use cos(angle * 6.0) to create 6 gentle lobes (leaves) wrapping around.
                // 'lobe_depth' controls how deep the ridges are.
                let lobe_depth = 0.6;
                let wave_modifier = (angle * 6.0).cos() * lobe_depth;

                // The effective radius limit at this specific angle
                let radius_limit = base_radius + wave_modifier;

                // Solid Fill Logic:
                // We fill everything inside the calculated radius.
                // This guarantees the shape is symmetrical and has no holes.
                if dist <= radius_limit {
                    let vertex_offset = mesh.vertices.len() as u32;

                    // No stem sway, just straight up for symmetry
                    let pos = IVec3::new(x, y, z);

                    append_indexed_cube_data(
                        &mut mesh.vertices,
                        &mut mesh.indices,
                        &mut mesh.voxel_infos,
                        pos,
                        vertex_offset,
                        ORIGIN,
                        max_length,
                        is_lod_used,
                    )?;
                }
            }
        }
    }

    Ok(mesh)
}

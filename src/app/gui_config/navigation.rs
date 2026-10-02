//! Presentation taxonomy shared by browsing and search; storage ownership stays unchanged.
pub(super) const CATEGORIES: &[&str] = &[
    "Rendering & Lighting",
    "World & Simulation",
    "Plants & Wildlife",
    "Camera & Audio",
    "Other Settings",
];

pub(super) fn category(path: &str) -> &'static str {
    let root = path.split(" / ").next().unwrap_or(path);
    match root {
        "Pixel Sampling — Flower Stems"
        | "Pixel Models — Global"
        | "Visibility & Detail"
        | "Lighting Diagnostics"
        | "DDGI Experiments"
        | "Atmos"
        | "Sky"
        | "Glass"
        | "Shadow"
        | "Starlight"
        | "GodRay"
        | "Post Processing" => CATEGORIES[0],
        "Wind"
        | "Terrain"
        | "Voxel"
        | "Terrain Material"
        | "WaterSimulation"
        | "Water Simulation"
        | "World Timing"
        | "Terrain Harvest Particles" => CATEGORIES[1],
        "Growth & Fruiting" | "Flora" | "Butterflies" | "Falling Leaves" | "Climbing Plants" => {
            CATEGORIES[2]
        }
        "Camera" | "HeadBob" | "Audio" => CATEGORIES[3],
        _ => CATEGORIES[4],
    }
}

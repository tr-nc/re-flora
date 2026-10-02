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

/// Query-side synonyms: narrow concepts, not a second index of individual controls.
pub(super) fn aliases(term: &str) -> &[&str] {
    match term {
        "光照" | "照明" => &["lighting", "light", "luminance"],
        "阴影" => &["shadow"],
        "风" | "风力" => &["wind"],
        "树" | "树木" => &["tree"],
        "草" | "草地" => &["grass"],
        "花" | "花朵" => &["flower"],
        "蝴蝶" => &["butterfly", "butterflies"],
        "水" | "水流" => &["water"],
        "地形" => &["terrain", "voxel"],
        "颜色" | "色彩" | "colour" => &["color"],
        "声音" | "音量" | "sound" => &["audio", "sound", "volume"],
        "相机" | "镜头" => &["camera", "headbob"],
        "生长" => &["growth", "age"],
        "像素" => &["pixel"],
        "分辨率" => &["resolution"],
        "密度" => &["density"],
        "速度" => &["speed", "rate"],
        "重力" => &["gravity"],
        "树叶" | "叶子" => &["leaf", "leaves"],
        _ => &[],
    }
}

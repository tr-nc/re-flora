//! One GLB authority for browser and game. Renderer adapters supply world placement
//! and lighting; neither adapter reauthors geometry or local animation.
use anyhow::{bail, ensure, Context, Result};
use glam::{Mat4, Quat, Vec2, Vec3, Vec4};
use std::sync::OnceLock;

pub const BUTTERFLY_BYTES: &[u8] = include_bytes!("../assets/models/butterfly.glb");

#[derive(Clone, Debug)]
pub struct Triangle {
    pub node: usize,
    pub positions: [Vec3; 3],
    pub normals: [Vec3; 3],
    pub uvs: [Vec2; 3],
}
#[derive(Clone, Copy)]
struct Pose {
    translation: Vec3,
    rotation: Quat,
    scale: Vec3,
}
struct Node {
    name: String,
    parent: Option<usize>,
    pose: Pose,
}
struct Channel {
    node: usize,
    property: gltf::animation::Property,
    step: bool,
    times: Vec<f32>,
    values: Vec<Vec4>,
}
struct Clip {
    duration: f32,
    channels: Vec<Channel>,
}
pub struct Model {
    nodes: Vec<Node>,
    pub triangles: Vec<Triangle>,
    clips: Vec<Clip>,
}

pub fn butterfly() -> &'static Model {
    static MODEL: OnceLock<Model> = OnceLock::new();
    MODEL.get_or_init(|| Model::load(BUTTERFLY_BYTES).expect("validated shared butterfly GLB"))
}

impl Model {
    pub fn load(bytes: &[u8]) -> Result<Self> {
        let gltf = gltf::Gltf::from_slice(bytes)?;
        let blob = gltf
            .blob
            .as_deref()
            .context("embedded GLB buffer required")?;
        ensure!(
            gltf.buffers()
                .all(|b| matches!(b.source(), gltf::buffer::Source::Bin)),
            "external buffers are not supported"
        );
        ensure!(
            gltf.skins().next().is_none(),
            "skinned models need a skinning adapter"
        );
        let mut nodes: Vec<_> = gltf
            .nodes()
            .map(|node| {
                let (t, r, s) = node.transform().decomposed();
                Node {
                    name: node.name().unwrap_or("").into(),
                    parent: None,
                    pose: Pose {
                        translation: t.into(),
                        rotation: Quat::from_array(r),
                        scale: s.into(),
                    },
                }
            })
            .collect();
        for node in gltf.nodes() {
            for child in node.children() {
                ensure!(
                    nodes[child.index()].parent.replace(node.index()).is_none(),
                    "multiple node parents"
                );
            }
        }
        // Only the default scene is visible in both consumers.
        let scene = gltf
            .default_scene()
            .or_else(|| gltf.scenes().next())
            .context("missing scene")?;
        fn visit(node: gltf::Node<'_>, output: &mut Vec<usize>) {
            output.push(node.index());
            for child in node.children() {
                visit(child, output);
            }
        }
        let mut visible = Vec::new();
        for root in scene.nodes() {
            visit(root, &mut visible);
        }
        let mut triangles = Vec::new();
        for index in visible {
            let node = gltf.nodes().nth(index).unwrap();
            let Some(mesh) = node.mesh() else { continue };
            for primitive in mesh.primitives() {
                ensure!(
                    primitive.mode() == gltf::mesh::Mode::Triangles,
                    "triangle meshes required"
                );
                ensure!(
                    primitive.morph_targets().next().is_none(),
                    "morph targets require an explicit adapter"
                );
                let reader = primitive.reader(|_| Some(blob));
                let positions: Vec<Vec3> = reader
                    .read_positions()
                    .context("missing positions")?
                    .map(Vec3::from)
                    .collect();
                let normals: Vec<Vec3> = reader
                    .read_normals()
                    .context("missing normals")?
                    .map(Vec3::from)
                    .collect();
                let uvs: Vec<Vec2> = reader
                    .read_tex_coords(0)
                    .map(|v| v.into_f32().map(Vec2::from).collect())
                    .unwrap_or_else(|| vec![Vec2::ZERO; positions.len()]);
                let indices: Vec<u32> = reader
                    .read_indices()
                    .map(|v| v.into_u32().collect())
                    .unwrap_or_else(|| (0..positions.len() as u32).collect());
                ensure!(
                    indices.len() % 3 == 0
                        && normals.len() == positions.len()
                        && uvs.len() == positions.len(),
                    "invalid vertex attributes"
                );
                for indices in indices.chunks_exact(3) {
                    ensure!(
                        indices.iter().all(|i| (*i as usize) < positions.len()),
                        "out of range index"
                    );
                    let indices = [
                        indices[0] as usize,
                        indices[1] as usize,
                        indices[2] as usize,
                    ];
                    triangles.push(Triangle {
                        node: index,
                        positions: indices.map(|i| positions[i]),
                        normals: indices.map(|i| normals[i]),
                        uvs: indices.map(|i| uvs[i]),
                    });
                }
            }
        }
        let mut clips = Vec::new();
        for animation in gltf.animations() {
            let mut channels = Vec::new();
            let mut duration = 0.0_f32;
            for channel in animation.channels() {
                let interpolation = channel.sampler().interpolation();
                ensure!(
                    interpolation != gltf::animation::Interpolation::CubicSpline,
                    "cubic animation requires a sampler extension"
                );
                let reader = channel.reader(|_| Some(blob));
                let times: Vec<_> = reader
                    .read_inputs()
                    .context("missing animation times")?
                    .collect();
                let values = match reader.read_outputs().context("missing animation values")? {
                    gltf::animation::util::ReadOutputs::Translations(v)
                    | gltf::animation::util::ReadOutputs::Scales(v) => {
                        v.map(|p| Vec3::from(p).extend(0.)).collect::<Vec<_>>()
                    }
                    gltf::animation::util::ReadOutputs::Rotations(v) => {
                        v.into_f32().map(Vec4::from).collect()
                    }
                    _ => bail!("unsupported animation target"),
                };
                ensure!(
                    !times.is_empty()
                        && times.len() == values.len()
                        && times.windows(2).all(|w| w[1] > w[0]),
                    "invalid animation keys"
                );
                ensure!(
                    times.iter().all(|t| t.is_finite() && *t >= 0.)
                        && values.iter().all(|v| v.is_finite()),
                    "non-finite animation"
                );
                duration = duration.max(*times.last().unwrap());
                channels.push(Channel {
                    node: channel.target().node().index(),
                    property: channel.target().property(),
                    step: interpolation == gltf::animation::Interpolation::Step,
                    times,
                    values,
                });
            }
            clips.push(Clip { duration, channels });
        }
        Ok(Self {
            nodes,
            triangles,
            clips,
        })
    }

    pub fn node(&self, name: &str) -> usize {
        self.nodes
            .iter()
            .position(|n| n.name == name)
            .unwrap_or_else(|| panic!("missing authored node {name}"))
    }
    pub fn duration(&self, clip: usize) -> f32 {
        self.clips.get(clip).map_or(0., |c| c.duration)
    }

    fn pose(&self, time: f32, clip: usize) -> Vec<Pose> {
        let mut pose: Vec<_> = self.nodes.iter().map(|n| n.pose).collect();
        if let Some(clip) = self.clips.get(clip) {
            let time = if clip.duration > 0. {
                time.rem_euclid(clip.duration)
            } else {
                0.
            };
            for channel in &clip.channels {
                let end = channel
                    .times
                    .partition_point(|t| *t <= time)
                    .min(channel.times.len() - 1);
                let start = if time >= channel.times[end] {
                    end
                } else {
                    end.saturating_sub(1)
                };
                let fraction = if start == end || channel.step {
                    0.
                } else {
                    ((time - channel.times[start]) / (channel.times[end] - channel.times[start]))
                        .clamp(0., 1.)
                };
                let a = channel.values[start];
                let b = channel.values[end];
                let target = &mut pose[channel.node];
                match channel.property {
                    gltf::animation::Property::Translation => {
                        target.translation = a.truncate().lerp(b.truncate(), fraction)
                    }
                    gltf::animation::Property::Scale => {
                        target.scale = a.truncate().lerp(b.truncate(), fraction)
                    }
                    gltf::animation::Property::Rotation => {
                        target.rotation = Quat::from_array(a.to_array())
                            .slerp(Quat::from_array(b.to_array()), fraction)
                            .normalize()
                    }
                    _ => unreachable!(),
                }
            }
        }
        pose
    }

    pub fn transforms(&self, time: f32, clip: usize) -> Vec<Mat4> {
        let pose = self.pose(time, clip);
        let local: Vec<_> = pose
            .iter()
            .map(|p| Mat4::from_scale_rotation_translation(p.scale, p.rotation, p.translation))
            .collect();
        (0..local.len())
            .map(|index| {
                let mut matrix = local[index];
                let mut parent = self.nodes[index].parent;
                while let Some(index) = parent {
                    matrix = local[index] * matrix;
                    parent = self.nodes[index].parent;
                }
                matrix
            })
            .collect()
    }

    /// Adapter data for flight/wingbeat coupling, sampled from the SAME GLB clip.
    pub fn local_rotation(&self, node: usize, time: f32) -> Quat {
        self.pose(time, 0)[node].rotation
    }
    pub fn local_translation(&self, node: usize, time: f32) -> Vec3 {
        self.pose(time, 0)[node].translation
    }
    pub fn rotation_keys(&self, node: usize) -> (Vec<f32>, Vec<Quat>) {
        let channel = self.clips[0]
            .channels
            .iter()
            .find(|c| c.node == node && c.property == gltf::animation::Property::Rotation)
            .expect("authored wing rotation channel");
        (
            channel.times.clone(),
            channel
                .values
                .iter()
                .map(|v| Quat::from_array(v.to_array()))
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "first export target/model-pose-reference.json with tests/asset-parity.cjs"]
    fn browser_model_pose_parity() {
        #[derive(serde::Deserialize)]
        struct Frame {
            time: f32,
            positions: Vec<[f32; 3]>,
        }
        let json = std::fs::read_to_string("target/model-pose-reference.json")
            .expect("run asset-parity.cjs first");
        let reference: std::collections::HashMap<String, Vec<Frame>> =
            serde_json::from_str(&json).unwrap();
        for (name, model) in [("butterfly", butterfly())] {
            for frame in &reference[name] {
                let transforms = model.transforms(frame.time, 0);
                let actual: Vec<_> = model
                    .triangles
                    .iter()
                    .flat_map(|t| t.positions.map(|p| transforms[t.node].transform_point3(p)))
                    .collect();
                assert_eq!(actual.len(), frame.positions.len());
                for (i, (a, b)) in actual.iter().zip(&frame.positions).enumerate() {
                    assert!(
                        a.distance(Vec3::from(*b)) < 2e-6,
                        "{name} time={} vertex={i}: {a:?} != {b:?}",
                        frame.time
                    );
                }
            }
            println!(
                "{name}: {} Three.js poses match Rust (<2e-6 source units)",
                reference[name].len()
            );
        }
    }

    #[test]
    fn shared_butterfly_has_the_approved_geometry_and_loop() {
        let model = butterfly();
        assert_eq!(model.triangles.len(), 156);
        assert_eq!(model.duration(0), 1.);
        assert_eq!(model.transforms(0., 0), model.transforms(1., 0));
        assert!(model
            .nodes
            .iter()
            .all(|n| !["head", "thorax", "abdomen", "body"]
                .iter()
                .any(|s| n.name.to_lowercase().contains(s))));
        for phase in [0., 0.1, 0.237, 0.5, 0.75, 0.999] {
            assert!(model.transforms(phase, 0).iter().all(|m| m.is_finite()));
        }
    }
}

use anyhow::{anyhow, bail, Context, Result};
use glam::{Mat3, Mat4, Vec3};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct LoadedModel {
    pub path: PathBuf,
    pub meshes: Vec<ModelMesh>,
}

#[derive(Debug, Clone)]
pub struct ModelMesh {
    pub name: Option<String>,
    pub primitives: Vec<ModelPrimitive>,
}

#[derive(Debug, Clone)]
pub struct ModelPrimitive {
    pub vertices: Vec<ModelVertex>,
    pub indices: Vec<u32>,
    pub material: ModelMaterial,
}

#[derive(Debug, Clone, Copy)]
pub struct ModelVertex {
    pub position: [f32; 3],
    pub normal: Option<[f32; 3]>,
    pub tex_coord: Option<[f32; 2]>,
}

#[derive(Debug, Clone, Copy)]
pub struct ModelMaterial {
    pub base_color_factor: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ModelTriangleGpu {
    pub a: [f32; 4],
    pub b: [f32; 4],
    pub c: [f32; 4],
}

pub fn load_model(path: impl AsRef<Path>) -> Result<LoadedModel> {
    let path = path.as_ref();
    let gltf = gltf::Gltf::open(path)
        .with_context(|| format!("failed to open glTF model '{}'", path.display()))?;
    let base = path.parent().unwrap_or_else(|| Path::new("./"));
    let buffers = gltf::import_buffers(&gltf.document, Some(base), gltf.blob)
        .with_context(|| format!("failed to import glTF buffers '{}'", path.display()))?;
    let document = gltf.document;

    let mut meshes = Vec::new();
    if let Some(scene) = document
        .default_scene()
        .or_else(|| document.scenes().next())
    {
        for node in scene.nodes() {
            load_node(&node, Mat4::IDENTITY, &buffers, &mut meshes)?;
        }
    } else {
        for mesh in document.meshes() {
            meshes.push(load_mesh(&mesh, None, Mat4::IDENTITY, &buffers)?);
        }
    }

    Ok(LoadedModel {
        path: path.to_path_buf(),
        meshes,
    })
}

fn load_node(
    node: &gltf::Node<'_>,
    parent_transform: Mat4,
    buffers: &[gltf::buffer::Data],
    meshes: &mut Vec<ModelMesh>,
) -> Result<()> {
    let transform = parent_transform * Mat4::from_cols_array_2d(&node.transform().matrix());
    if let Some(mesh) = node.mesh() {
        meshes.push(load_mesh(&mesh, node.name(), transform, buffers)?);
    }

    for child in node.children() {
        load_node(&child, transform, buffers, meshes)?;
    }

    Ok(())
}

fn load_mesh(
    mesh: &gltf::Mesh<'_>,
    node_name: Option<&str>,
    transform: Mat4,
    buffers: &[gltf::buffer::Data],
) -> Result<ModelMesh> {
    let normal_transform = normal_transform(transform);
    let mut primitives = Vec::new();
    for primitive in mesh.primitives() {
        if primitive.mode() != gltf::mesh::Mode::Triangles {
            bail!(
                "unsupported glTF primitive mode {:?}; only triangle primitives are supported",
                primitive.mode()
            );
        }

        let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(|b| b.0.as_slice()));

        let positions = reader
            .read_positions()
            .ok_or_else(|| anyhow!("model primitive is missing POSITION data"))?
            .collect::<Vec<_>>();
        let normals = reader.read_normals().map(|n| n.collect::<Vec<_>>());
        let tex_coords = reader
            .read_tex_coords(0)
            .map(|coords| coords.into_f32().collect::<Vec<_>>());

        let vertices = positions
            .iter()
            .enumerate()
            .map(|(index, &position)| {
                let position = transform.transform_point3(Vec3::from(position)).to_array();
                let normal = normals.as_ref().and_then(|normals| {
                    normals.get(index).map(|&normal| {
                        (normal_transform * Vec3::from(normal))
                            .normalize_or_zero()
                            .to_array()
                    })
                });
                ModelVertex {
                    position,
                    normal,
                    tex_coord: tex_coords.as_ref().and_then(|t| t.get(index).copied()),
                }
            })
            .collect::<Vec<_>>();

        let indices = reader
            .read_indices()
            .map(|indices| indices.into_u32().collect::<Vec<_>>())
            .unwrap_or_else(|| (0..vertices.len() as u32).collect());

        primitives.push(ModelPrimitive {
            vertices,
            indices,
            material: ModelMaterial {
                base_color_factor: primitive
                    .material()
                    .pbr_metallic_roughness()
                    .base_color_factor(),
            },
        });
    }

    Ok(ModelMesh {
        name: node_name.or_else(|| mesh.name()).map(str::to_owned),
        primitives,
    })
}

fn normal_transform(transform: Mat4) -> Mat3 {
    let mat = Mat3::from_mat4(transform);
    if mat.determinant().abs() > f32::EPSILON {
        mat.inverse().transpose()
    } else {
        mat
    }
}

impl LoadedModel {
    pub fn scale_to_longest_edge(&mut self, longest_edge: f32) -> Result<f32> {
        if longest_edge <= 0.0 {
            bail!("model longest edge must be positive, got {longest_edge}");
        }

        let (min, max) = self
            .bounds()
            .ok_or_else(|| anyhow!("cannot scale an empty model"))?;
        let current_longest_edge = (max - min).max_element();
        if current_longest_edge <= f32::EPSILON {
            bail!("cannot scale a model with zero-sized bounds");
        }

        let scale = longest_edge / current_longest_edge;
        self.scale_uniform(scale)?;
        Ok(scale)
    }

    pub fn scale_uniform(&mut self, scale: f32) -> Result<()> {
        if scale <= 0.0 {
            bail!("model scale must be positive, got {scale}");
        }

        for vertex in self.vertices_mut() {
            vertex.position = (Vec3::from(vertex.position) * scale).to_array();
        }

        Ok(())
    }

    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        let mut found = false;

        for vertex in self.vertices() {
            let position = Vec3::from(vertex.position);
            min = min.min(position);
            max = max.max(position);
            found = true;
        }

        found.then_some((min, max))
    }

    pub fn triangles(&self) -> Result<Vec<ModelTriangleGpu>> {
        let mut triangles = Vec::new();
        for mesh in &self.meshes {
            for primitive in &mesh.primitives {
                if primitive.indices.len() % 3 != 0 {
                    bail!(
                        "model primitive has {} indices, which is not divisible by 3",
                        primitive.indices.len()
                    );
                }

                for index_triplet in primitive.indices.chunks_exact(3) {
                    let a = primitive
                        .vertices
                        .get(index_triplet[0] as usize)
                        .ok_or_else(|| {
                            anyhow!("model index {} is out of bounds", index_triplet[0])
                        })?
                        .position;
                    let b = primitive
                        .vertices
                        .get(index_triplet[1] as usize)
                        .ok_or_else(|| {
                            anyhow!("model index {} is out of bounds", index_triplet[1])
                        })?
                        .position;
                    let c = primitive
                        .vertices
                        .get(index_triplet[2] as usize)
                        .ok_or_else(|| {
                            anyhow!("model index {} is out of bounds", index_triplet[2])
                        })?
                        .position;
                    triangles.push(ModelTriangleGpu {
                        a: [a[0], a[1], a[2], 0.0],
                        b: [b[0], b[1], b[2], 0.0],
                        c: [c[0], c[1], c[2], 0.0],
                    });
                }
            }
        }

        Ok(triangles)
    }

    fn vertices(&self) -> impl Iterator<Item = &ModelVertex> {
        self.meshes
            .iter()
            .flat_map(|mesh| mesh.primitives.iter())
            .flat_map(|primitive| primitive.vertices.iter())
    }

    fn vertices_mut(&mut self) -> impl Iterator<Item = &mut ModelVertex> {
        self.meshes
            .iter_mut()
            .flat_map(|mesh| mesh.primitives.iter_mut())
            .flat_map(|primitive| primitive.vertices.iter_mut())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_stylized_rock_glb() {
        let model =
            load_model("assets/models/free_pack_rocks_stylized/glb/SM_Rocks_01.glb").unwrap();
        assert!(!model.meshes.is_empty());
        assert!(model.meshes.iter().any(|mesh| !mesh.primitives.is_empty()));
    }

    #[test]
    fn extracts_stylized_rock_bounds_and_triangles() {
        let model =
            load_model("assets/models/free_pack_rocks_stylized/glb/SM_Rocks_01.glb").unwrap();
        let (min, max) = model.bounds().unwrap();
        let span = (max - min).max_element();
        assert!(span > 0.0);
        assert!(!model.triangles().unwrap().is_empty());
    }

    #[test]
    fn scales_stylized_rock_to_target_longest_edge() {
        let mut model =
            load_model("assets/models/free_pack_rocks_stylized/glb/SM_Rocks_01.glb").unwrap();
        let scale = model.scale_to_longest_edge(0.5).unwrap();
        let (min, max) = model.bounds().unwrap();
        let span = (max - min).max_element();

        assert!(scale > 0.0);
        assert!((span - 0.5).abs() < 0.0001);
    }
}

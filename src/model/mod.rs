use anyhow::{anyhow, Context, Result};
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

pub fn load_model(path: impl AsRef<Path>) -> Result<LoadedModel> {
    let path = path.as_ref();
    let (document, buffers, _images) = gltf::import(path)
        .with_context(|| format!("failed to import glTF model '{}'", path.display()))?;

    let mut meshes = Vec::new();
    for mesh in document.meshes() {
        let mut primitives = Vec::new();
        for primitive in mesh.primitives() {
            let reader =
                primitive.reader(|buffer| buffers.get(buffer.index()).map(|b| b.0.as_slice()));

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
                .map(|(index, &position)| ModelVertex {
                    position,
                    normal: normals.as_ref().and_then(|n| n.get(index).copied()),
                    tex_coord: tex_coords.as_ref().and_then(|t| t.get(index).copied()),
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

        meshes.push(ModelMesh {
            name: mesh.name().map(str::to_owned),
            primitives,
        });
    }

    Ok(LoadedModel {
        path: path.to_path_buf(),
        meshes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_big_stone_glb() {
        let model = load_model("assets/models/big_stone.glb").unwrap();
        assert!(!model.meshes.is_empty());
        assert!(model.meshes.iter().any(|mesh| !mesh.primitives.is_empty()));
    }
}

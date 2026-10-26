//! Derived, bounded per-tree compilation facts. Canonical trees stay in GardenTrees.
use super::*;
use anyhow::Context;
const CACHE_BUDGET: usize = 256 * 1024 * 1024;

pub(super) struct TreeSurfaceFragment {
    tree: Arc<Tree>,
    tree_origin: Vec3,
    origin: UVec3,
    dim: UVec3,
    cones: Vec<[[u32; 4]; 2]>,
    materials: Vec<u8>,
    mesh: RasterTreeMesh,
}
fn cone_bits(cones: &[RoundCone]) -> Vec<[[u32; 4]; 2]> {
    cones
        .iter()
        .map(|cone| {
            [
                cone.center_a()
                    .extend(cone.radius_a())
                    .to_array()
                    .map(f32::to_bits),
                cone.center_b()
                    .extend(cone.radius_b())
                    .to_array()
                    .map(f32::to_bits),
            ]
        })
        .collect()
}
impl TreeSurfaceFragment {
    /// Conservative logical accounting includes ample tree-node overhead. The
    /// optimization is optional: over-budget fragments assemble but are not retained.
    fn cache_bytes(&self) -> usize {
        self.materials
            .capacity()
            .saturating_add(self.cones.capacity() * 32)
            .saturating_add(self.mesh.vertices.capacity() * std::mem::size_of::<RasterTreeVertex>())
            .saturating_add(self.mesh.indices.capacity() * 4)
            .saturating_add(
                self.mesh.bindings.capacity() * std::mem::size_of::<Option<(u32, SkinBinding)>>(),
            )
            .saturating_add(self.mesh.cell_vertex_indices.capacity() * 4)
            .saturating_add(self.mesh.cells.len() * 256)
            .saturating_add(self.mesh.solid_cells.len() * 64)
            .saturating_add(self.mesh.trunk_collision.cells().len() * 64)
            .saturating_add(
                self.mesh
                    .binding_cache
                    .values()
                    .map(|c| c.corners.len() * 128)
                    .sum::<usize>(),
            )
    }
}

/// Authored identity and exact atlas inputs for one tree's surface compilation.
pub(crate) struct TreeSurfaceRegion<'a> {
    pub id: u32,
    pub tree: &'a Arc<Tree>,
    pub tree_origin: Vec3,
    pub origin: UVec3,
    pub dim: UVec3,
    pub bytes: &'a [u8],
    pub cones: &'a [RoundCone],
}

impl RasterTreeMesh {
    /// Compare the actual radius-two material input, not a terrain revision/hash.
    /// Broad terrain publication can dirty trees whose observable inputs did not change.
    pub(crate) fn append_cached_tree_region(
        &mut self,
        region: TreeSurfaceRegion<'_>,
        previous: &Self,
    ) -> Result<bool> {
        let TreeSurfaceRegion {
            id,
            tree,
            tree_origin,
            origin,
            dim,
            bytes,
            cones,
        } = region;
        ensure!(
            bytes.len() == dim.as_u64vec3().element_product() as usize,
            "tree atlas region size mismatch"
        );
        let signature = cone_bits(cones);
        let previous_fragment = previous.fragments.get(&id).filter(|cached| {
            Arc::ptr_eq(&cached.tree, tree)
                && cached.tree_origin.to_array().map(f32::to_bits)
                    == tree_origin.to_array().map(f32::to_bits)
                && cached.origin == origin
                && cached.dim == dim
                && cached.cones == signature
                && cached.materials.len() == bytes.len()
                && cached
                    .materials
                    .iter()
                    .zip(bytes)
                    .all(|(old, new)| *old == super::surface_material(*new))
        });
        let hit = previous_fragment.is_some();
        let fragment = match previous_fragment {
            Some(fragment) => fragment.clone(),
            None => {
                let mut mesh = RasterTreeMesh::default();
                mesh.append_region(origin, dim, bytes, cones)?;
                mesh.finish()?;
                mesh.bind_tree(id, tree_origin, tree, Some(previous))?;
                ensure!(
                    mesh.bindings.iter().all(Option::is_some),
                    "unbound GPU tree vertex"
                );
                Arc::new(TreeSurfaceFragment {
                    tree: tree.clone(),
                    tree_origin,
                    origin,
                    dim,
                    cones: signature,
                    materials: bytes.iter().map(|&b| super::surface_material(b)).collect(),
                    mesh,
                })
            }
        };
        // Match full extraction's normal/face overwrite order. Ownership is a
        // separate fact: stable lowest ID, never the latest normal writer.
        self.cells
            .extend(fragment.mesh.cells.iter().map(|(&p, &facts)| (p, facts)));
        self.solid_cells
            .extend(fragment.mesh.solid_cells.iter().copied());
        self.trunk_collision.append(&fragment.mesh.trunk_collision);
        self.bark_styles.insert(id, tree.bark_style());
        self.dependencies.include_region(origin, dim, cones);
        for (cell_index, &cell) in fragment.mesh.cells.keys().enumerate() {
            if self
                .fragment_bindings
                .get(&cell)
                .is_some_and(|(owner, _)| *owner <= id)
            {
                continue;
            }
            let bindings = fragment.mesh.bindings[cell_index * 8..cell_index * 8 + 8]
                .iter()
                .map(|b| b.context("unbound GPU tree vertex").map(|(_, skin)| skin))
                .collect::<Result<Vec<_>>>()?
                .try_into()
                .expect("eight voxel corners");
            self.fragment_bindings.insert(cell, (id, bindings));
        }
        // Keep just the current forest's bounded cache. No removed-tree or failed-publication history.
        let cost = fragment.cache_bytes();
        if self.fragment_cache_bytes.saturating_add(cost) <= CACHE_BUDGET {
            self.fragment_cache_bytes += cost;
            self.fragments.insert(id, fragment);
        }
        Ok(hit)
    }

    pub(crate) fn bind_compiled_fragments(&mut self) -> Result<()> {
        ensure!(
            self.fragment_bindings.len() == self.cells.len(),
            "incomplete forest cell binding plan"
        );
        for (index, &cell) in self.cells.keys().enumerate() {
            let (id, bindings) = self
                .fragment_bindings
                .get(&cell)
                .context("missing forest cell binding")?;
            for (corner, &binding) in bindings.iter().enumerate() {
                self.bindings[index * 8 + corner] = Some((*id, binding));
            }
        }
        // Preserve only currently owned/used corners for the next compilation.
        for (&id, fragment) in &self.fragments {
            let corners = self
                .vertices
                .iter()
                .zip(&self.bindings)
                .filter_map(|(vertex, binding)| {
                    binding
                        .filter(|(owner, _)| *owner == id)
                        .map(|(_, skin)| (vertex.position.map(f32::to_bits), skin))
                })
                .collect();
            self.binding_cache.insert(
                id,
                TreeBindingCache {
                    tree: fragment.tree.clone(),
                    origin: fragment.tree_origin,
                    corners,
                },
            );
        }
        self.fragment_bindings.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Arc<Tree>, Vec<u8>) {
        let mut desc = crate::tree_gen::TreeDesc::default();
        desc.branching.iterations = 3;
        let mut bytes = vec![0; 512];
        for (x, y, z) in [(3, 3, 3), (4, 3, 3), (3, 4, 3)] {
            bytes[x + 8 * (y + 8 * z)] = 5;
        }
        (Arc::new(Tree::new(desc)), bytes)
    }
    fn cached(tree: &Arc<Tree>, bytes: &[u8], previous: &RasterTreeMesh) -> (RasterTreeMesh, bool) {
        let mut mesh = RasterTreeMesh::default();
        let hit = mesh
            .append_cached_tree_region(
                TreeSurfaceRegion {
                    id: 2,
                    tree,
                    tree_origin: Vec3::ZERO,
                    origin: UVec3::ZERO,
                    dim: UVec3::splat(8),
                    bytes,
                    cones: tree.trunks(),
                },
                previous,
            )
            .unwrap();
        mesh.finish().unwrap();
        mesh.bind_compiled_fragments().unwrap();
        (mesh, hit)
    }
    #[test]
    fn bark_appearance_survives_uncached_fragments_without_recoloring_other_trees() {
        let (_, bytes) = fixture();
        for species in [
            crate::tree_gen::TreeSpecies::AppleTree,
            crate::tree_gen::TreeSpecies::LondonPlane,
        ] {
            let mut desc = species.default_desc();
            desc.size = 20.;
            desc.trunk_thickness = 1.; // fixture cells must actually lie inside both trunks
            let tree = Arc::new(Tree::new(desc));
            let mut mesh = RasterTreeMesh {
                fragment_cache_bytes: CACHE_BUDGET,
                ..Default::default()
            };
            mesh.append_cached_tree_region(
                TreeSurfaceRegion {
                    id: 2,
                    tree: &tree,
                    tree_origin: Vec3::ZERO,
                    origin: UVec3::ZERO,
                    dim: UVec3::splat(8),
                    bytes: &bytes,
                    cones: tree.trunks(),
                },
                &RasterTreeMesh::default(),
            )
            .unwrap();
            mesh.finish().unwrap();
            mesh.bind_compiled_fragments().unwrap();
            assert!(
                mesh.fragments.is_empty(),
                "exercise cache exhaustion, not a cache hit"
            );
            let skin = mesh.gpu_skin().unwrap();
            assert!(!skin.rest.is_empty());
            assert!(skin
                .rest
                .chunks_exact(3)
                .all(|v| v[1][3] == 1. + tree.bark_style() as f32));
        }
    }

    fn reference(tree: &Arc<Tree>, bytes: &[u8]) -> RasterTreeMesh {
        let mut mesh = RasterTreeMesh::default();
        mesh.append_region(UVec3::ZERO, UVec3::splat(8), bytes, tree.trunks())
            .unwrap();
        mesh.finish().unwrap();
        mesh.bind_tree(2, Vec3::ZERO, tree, None).unwrap();
        mesh
    }
    #[test]
    fn exact_material_inputs_reuse_without_losing_cuts_or_normal_only_edits() {
        let (tree, mut bytes) = fixture();
        let (before, hit) = cached(&tree, &bytes, &RasterTreeMesh::default());
        assert!(!hit);
        assert!(before.same_surface(&reference(&tree, &bytes)));
        let (same, hit) = cached(&tree, &bytes, &before);
        assert!(hit);
        assert!(same.same_surface(&before));
        for b in &mut bytes {
            *b |= 0x30;
        } // Soil state does not enter the material sampler.
        let (same, hit) = cached(&tree, &bytes, &before);
        assert!(hit);
        assert!(same.same_surface(&reference(&tree, &bytes)));
        bytes[3 + 8 * (3 + 8 * 3)] = 0; // Hand-carved wood must rebuild, not resurrect.
        let (cut, hit) = cached(&tree, &bytes, &before);
        assert!(!hit);
        assert!(cut.same_surface(&reference(&tree, &bytes)));
        bytes[2 + 8 * (3 + 8 * 3)] = 2; // Neighbor dirt/stone changes faces/normals with no removed wood.
        let (normal, hit) = cached(&tree, &bytes, &cut);
        assert!(!hit);
        assert!(normal.same_surface(&reference(&tree, &bytes)));
        let replacement = fixture().0;
        let (_, hit) = cached(&replacement, &bytes, &normal);
        assert!(!hit);
    }
    #[test]
    fn cache_budget_is_optional_and_cannot_drop_or_reject_valid_geometry() {
        let (tree, bytes) = fixture();
        let mut mesh = RasterTreeMesh::default();
        mesh.fragment_cache_bytes = CACHE_BUDGET;
        mesh.append_cached_tree_region(
            TreeSurfaceRegion {
                id: 2,
                tree: &tree,
                tree_origin: Vec3::ZERO,
                origin: UVec3::ZERO,
                dim: UVec3::splat(8),
                bytes: &bytes,
                cones: tree.trunks(),
            },
            &RasterTreeMesh::default(),
        )
        .unwrap();
        assert!(mesh.fragments.is_empty());
        mesh.finish().unwrap();
        mesh.bind_compiled_fragments().unwrap();
        assert!(mesh.same_surface(&reference(&tree, &bytes)));
    }
    #[test]
    fn overlapping_fragment_ownership_matches_complete_forest_compilation() {
        let (tree, bytes) = fixture();
        let trees = [(2, Vec3::ZERO), (5, Vec3::X / 256.)];
        let mut full = RasterTreeMesh::default();
        let mut merged = RasterTreeMesh::default();
        for (id, position) in trees {
            let cones = tree.world_trunks(position);
            full.append_region(UVec3::ZERO, UVec3::splat(8), &bytes, &cones)
                .unwrap();
            merged
                .append_cached_tree_region(
                    TreeSurfaceRegion {
                        id,
                        tree: &tree,
                        tree_origin: position,
                        origin: UVec3::ZERO,
                        dim: UVec3::splat(8),
                        bytes: &bytes,
                        cones: &cones,
                    },
                    &RasterTreeMesh::default(),
                )
                .unwrap();
        }
        full.finish().unwrap();
        for (id, position) in trees {
            full.bind_tree(id, position, &tree, None).unwrap();
        }
        merged.finish().unwrap();
        merged.bind_compiled_fragments().unwrap();
        assert!(merged.same_surface(&full));
        assert_eq!(
            merged.gpu_skin().unwrap().bindings,
            full.gpu_skin().unwrap().bindings
        );
    }
}

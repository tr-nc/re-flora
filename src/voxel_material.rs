mod generated {
    include!(concat!(env!("OUT_DIR"), "/voxel_material_config.rs"));
}

pub(crate) const GLASS_EXPERIMENT_MATERIAL_REVISION: u32 =
    generated::GLASS_EXPERIMENT_MATERIAL_REVISION;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VoxelMaterialMode {
    Standard,
    GlassExperiment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VoxelSurfaceClass {
    Empty,
    Opaque,
    Dielectric,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DirectShadowPolicy {
    Opaque,
    Skip,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalShadowPolicy {
    Opaque,
    OpticalTransmittance,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct VoxelOpticalProperties {
    pub(crate) ior: f32,
    pub(crate) attenuation_color: [f32; 3],
    pub(crate) attenuation_distance_world: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct VoxelMaterial {
    pub(crate) surface_class: VoxelSurfaceClass,
    pub(crate) collision_solid: bool,
    pub(crate) water_solid: bool,
    pub(crate) terrain_support: bool,
    pub(crate) probe_relocation_solid: bool,
    pub(crate) blocks_ddgi_visibility: bool,
    pub(crate) direct_shadow: DirectShadowPolicy,
    pub(crate) local_shadow: LocalShadowPolicy,
    pub(crate) soil_state_allowed: bool,
    pub(crate) optical: Option<VoxelOpticalProperties>,
}

fn has_flag(flags: u32, flag: u32) -> bool {
    flags & flag != 0
}

fn surface_class(value: u32) -> VoxelSurfaceClass {
    match value {
        generated::VOXEL_SURFACE_CLASS_EMPTY => VoxelSurfaceClass::Empty,
        generated::VOXEL_SURFACE_CLASS_OPAQUE => VoxelSurfaceClass::Opaque,
        generated::VOXEL_SURFACE_CLASS_DIELECTRIC => VoxelSurfaceClass::Dielectric,
        _ => unreachable!("generated voxel surface class is invalid"),
    }
}

fn direct_shadow_policy(value: u32) -> DirectShadowPolicy {
    match value {
        generated::VOXEL_DIRECT_SHADOW_OPAQUE => DirectShadowPolicy::Opaque,
        generated::VOXEL_DIRECT_SHADOW_SKIP => DirectShadowPolicy::Skip,
        _ => unreachable!("generated direct-shadow policy is invalid"),
    }
}

fn local_shadow_policy(value: u32) -> LocalShadowPolicy {
    match value {
        generated::VOXEL_LOCAL_SHADOW_OPAQUE => LocalShadowPolicy::Opaque,
        generated::VOXEL_LOCAL_SHADOW_OPTICAL_TRANSMITTANCE => {
            LocalShadowPolicy::OpticalTransmittance
        }
        _ => unreachable!("generated local-shadow policy is invalid"),
    }
}

pub(crate) fn material_for(voxel_type: u32, mode: VoxelMaterialMode) -> VoxelMaterial {
    if voxel_type == crate::builder::VOXEL_TYPE_EMPTY {
        return VoxelMaterial {
            surface_class: surface_class(generated::VOXEL_SURFACE_CLASS_EMPTY),
            collision_solid: false,
            water_solid: false,
            terrain_support: false,
            probe_relocation_solid: false,
            blocks_ddgi_visibility: false,
            direct_shadow: direct_shadow_policy(generated::VOXEL_DIRECT_SHADOW_SKIP),
            local_shadow: local_shadow_policy(generated::VOXEL_LOCAL_SHADOW_OPTICAL_TRANSMITTANCE),
            soil_state_allowed: false,
            optical: None,
        };
    }

    let glass_experiment = mode == VoxelMaterialMode::GlassExperiment
        && voxel_type == generated::GLASS_EXPERIMENT_VOXEL_TYPE;
    let flags = if glass_experiment {
        generated::GLASS_EXPERIMENT_MATERIAL_FLAGS
    } else {
        let soil = ((1u32 << voxel_type) & generated::STANDARD_SOIL_VOXEL_TYPE_MASK) != 0;
        generated::STANDARD_SOLID_MATERIAL_FLAGS
            | if soil {
                generated::VOXEL_MATERIAL_FLAG_SOIL_STATE_ALLOWED
            } else {
                0
            }
    };

    VoxelMaterial {
        surface_class: surface_class(if glass_experiment {
            generated::VOXEL_SURFACE_CLASS_DIELECTRIC
        } else {
            generated::VOXEL_SURFACE_CLASS_OPAQUE
        }),
        collision_solid: has_flag(flags, generated::VOXEL_MATERIAL_FLAG_COLLISION_SOLID),
        water_solid: has_flag(flags, generated::VOXEL_MATERIAL_FLAG_WATER_SOLID),
        terrain_support: has_flag(flags, generated::VOXEL_MATERIAL_FLAG_TERRAIN_SUPPORT),
        probe_relocation_solid: has_flag(
            flags,
            generated::VOXEL_MATERIAL_FLAG_PROBE_RELOCATION_SOLID,
        ),
        blocks_ddgi_visibility: has_flag(
            flags,
            generated::VOXEL_MATERIAL_FLAG_BLOCKS_DDGI_VISIBILITY,
        ),
        direct_shadow: direct_shadow_policy(if glass_experiment {
            generated::VOXEL_DIRECT_SHADOW_SKIP
        } else {
            generated::VOXEL_DIRECT_SHADOW_OPAQUE
        }),
        local_shadow: local_shadow_policy(if glass_experiment {
            generated::VOXEL_LOCAL_SHADOW_OPTICAL_TRANSMITTANCE
        } else {
            generated::VOXEL_LOCAL_SHADOW_OPAQUE
        }),
        soil_state_allowed: has_flag(flags, generated::VOXEL_MATERIAL_FLAG_SOIL_STATE_ALLOWED),
        optical: glass_experiment.then_some(VoxelOpticalProperties {
            ior: generated::GLASS_EXPERIMENT_IOR,
            attenuation_color: generated::GLASS_EXPERIMENT_ATTENUATION_COLOR,
            attenuation_distance_world: generated::GLASS_EXPERIMENT_ATTENUATION_DISTANCE_WORLD,
        }),
    }
}

pub(crate) fn canonicalize_atlas_data(voxel_data: u8, mode: VoxelMaterialMode) -> u8 {
    let voxel_type = u32::from(voxel_data & crate::builder::VOXEL_TYPE_MASK);
    if material_for(voxel_type, mode).surface_class == VoxelSurfaceClass::Dielectric {
        voxel_data & crate::builder::VOXEL_TYPE_MASK
    } else {
        voxel_data
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::{VOXEL_TYPE_EMISSIVE, VOXEL_TYPE_ROCK, VOXEL_TYPE_SAND};

    #[test]
    fn glass_experiment_reinterprets_only_sand_as_a_dielectric() {
        assert_eq!(generated::GLASS_EXPERIMENT_VOXEL_TYPE, VOXEL_TYPE_SAND);
        let standard_sand = material_for(VOXEL_TYPE_SAND, VoxelMaterialMode::Standard);
        assert_eq!(standard_sand.surface_class, VoxelSurfaceClass::Opaque);
        assert!(standard_sand.soil_state_allowed);
        assert!(standard_sand.blocks_ddgi_visibility);

        let glass = material_for(VOXEL_TYPE_SAND, VoxelMaterialMode::GlassExperiment);
        assert_eq!(glass.surface_class, VoxelSurfaceClass::Dielectric);
        assert!(glass.collision_solid);
        assert!(glass.water_solid);
        assert!(glass.terrain_support);
        assert!(glass.probe_relocation_solid);
        assert!(!glass.blocks_ddgi_visibility);
        assert_eq!(glass.direct_shadow, DirectShadowPolicy::Skip);
        assert_eq!(glass.local_shadow, LocalShadowPolicy::OpticalTransmittance);
        assert!(!glass.soil_state_allowed);
        assert_eq!(
            glass.optical.expect("Glass must have optical data").ior,
            1.5
        );

        for voxel_type in [VOXEL_TYPE_ROCK, VOXEL_TYPE_EMISSIVE] {
            assert_eq!(
                material_for(voxel_type, VoxelMaterialMode::GlassExperiment),
                material_for(voxel_type, VoxelMaterialMode::Standard),
                "the experiment must not reinterpret voxel type {voxel_type}",
            );
        }
    }

    #[test]
    fn glass_experiment_canonicalizes_sand_soil_state_without_touching_standard_data() {
        let sand_with_soil_state = 0xf0 | VOXEL_TYPE_SAND as u8;
        assert_eq!(
            canonicalize_atlas_data(sand_with_soil_state, VoxelMaterialMode::Standard),
            sand_with_soil_state,
        );
        assert_eq!(
            canonicalize_atlas_data(sand_with_soil_state, VoxelMaterialMode::GlassExperiment,),
            VOXEL_TYPE_SAND as u8,
        );

        let rock_with_state = 0xf0 | VOXEL_TYPE_ROCK as u8;
        assert_eq!(
            canonicalize_atlas_data(rock_with_state, VoxelMaterialMode::GlassExperiment),
            rock_with_state,
        );
    }
}

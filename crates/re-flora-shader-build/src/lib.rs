use libloading::Library;
use std::collections::BTreeSet;
use std::env;
use std::ffi::{c_char, c_void, CStr, CString};
use std::path::{Path, PathBuf};

const ENTRY_POINT: &str = "main";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderStage {
    Compute,
    Vertex,
    Fragment,
}

impl ShaderStage {
    fn slang_api_value(self) -> u32 {
        match self {
            Self::Compute => 6,
            Self::Vertex => 1,
            Self::Fragment => 5,
        }
    }

    pub fn file_extension(self) -> &'static str {
        match self {
            Self::Compute => "comp",
            Self::Vertex => "vert",
            Self::Fragment => "frag",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ShaderConfig {
    pub logical_path: &'static str,
    pub source_path: &'static str,
    pub module_path: &'static str,
    pub stage: ShaderStage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizationLevel {
    Zero,
    Performance,
}

pub struct CompilerOutput {
    pub spirv: Vec<u8>,
    pub dependencies: BTreeSet<PathBuf>,
}

pub const NATIVE_SHADERS: &[ShaderConfig] = &[
    ShaderConfig {
        logical_path: "shader/builder/chunk_writer/buffer_setup.comp",
        source_path: "shader/slang/chunk_writer_buffer_setup.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/chunk_writer/chunk_heightmap.comp",
        source_path: "shader/slang/chunk_heightmap.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/chunk_writer/chunk_init.comp",
        source_path: "shader/slang/chunk_init.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/chunk_writer/model_voxelize.comp",
        source_path: "shader/slang/model_voxelize.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/chunk_writer/chunk_modify.comp",
        source_path: "shader/slang/chunk_modify.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/chunk_writer/chunk_modify_sample.comp",
        source_path: "shader/slang/chunk_modify_sample.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/chunk_writer/chunk_solid_sample.comp",
        source_path: "shader/slang/chunk_solid_sample.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/chunk_writer/terrain_fertility_brush.comp",
        source_path: "shader/slang/terrain_fertility_brush.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/chunk_writer/terrain_moisture_brush.comp",
        source_path: "shader/slang/terrain_moisture_brush.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/chunk_writer/terrain_moisture_dry.comp",
        source_path: "shader/slang/terrain_moisture_dry.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/chunk_writer/terrain_moisture_spread.comp",
        source_path: "shader/slang/terrain_moisture_spread.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/chunk_writer/terrain_soil_mix.comp",
        source_path: "shader/slang/terrain_soil_mix.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/chunk_writer/terrain_smooth_apply.comp",
        source_path: "shader/slang/terrain_smooth_apply.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/chunk_writer/terrain_smooth_heights.comp",
        source_path: "shader/slang/terrain_smooth_heights.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/chunk_writer/terrain_smooth_target.comp",
        source_path: "shader/slang/terrain_smooth_target.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/chunk_writer/terrain_smooth_mbo_apply.comp",
        source_path: "shader/slang/terrain_smooth_mbo_apply.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/chunk_writer/terrain_smooth_mbo_diffuse_ab.comp",
        source_path: "shader/slang/terrain_smooth_mbo_diffuse_ab.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/chunk_writer/terrain_smooth_mbo_diffuse_ba.comp",
        source_path: "shader/slang/terrain_smooth_mbo_diffuse_ba.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/chunk_writer/terrain_smooth_mbo_init.comp",
        source_path: "shader/slang/terrain_smooth_mbo_init.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/chunk_writer/terrain_smooth_mbo_score.comp",
        source_path: "shader/slang/terrain_smooth_mbo_score.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/chunk_writer/voxel_property_sample.comp",
        source_path: "shader/slang/voxel_property_sample.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/tracer/composition.comp",
        source_path: "shader/slang/composition.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/tracer/terrain_depth_prefill.vert",
        source_path: "shader/slang/terrain_depth_prefill.vert.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Vertex,
    },
    ShaderConfig {
        logical_path: "shader/tracer/terrain_depth_prefill.frag",
        source_path: "shader/slang/terrain_depth_prefill.frag.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Fragment,
    },
    ShaderConfig {
        logical_path: "shader/builder/contree/buffer_setup.comp",
        source_path: "shader/slang/contree_buffer_setup.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/contree/buffer_update.comp",
        source_path: "shader/slang/contree_buffer_update.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/contree/concat.comp",
        source_path: "shader/slang/contree_concat.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/contree/leaf_write.comp",
        source_path: "shader/slang/contree_leaf_write.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/contree/last_buffer_update.comp",
        source_path: "shader/slang/contree_last_buffer_update.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/contree/tree_write.comp",
        source_path: "shader/slang/contree_tree_write.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/egui/egui.vert",
        source_path: "shader/slang/egui.vert.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Vertex,
    },
    ShaderConfig {
        logical_path: "shader/egui/egui.frag",
        source_path: "shader/slang/egui.frag.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Fragment,
    },
    ShaderConfig {
        logical_path: "shader/foliage/flora.vert",
        source_path: "shader/slang/flora.vert.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Vertex,
    },
    ShaderConfig {
        logical_path: "shader/foliage/flora_lod.vert",
        source_path: "shader/slang/flora_lod.vert.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Vertex,
    },
    ShaderConfig {
        logical_path: "shader/foliage/leaves_lod.vert",
        source_path: "shader/slang/leaves_lod.vert.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Vertex,
    },
    ShaderConfig {
        logical_path: "shader/foliage/leaves_shadow.frag",
        source_path: "shader/slang/leaves_shadow.frag.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Fragment,
    },
    ShaderConfig {
        logical_path: "shader/foliage/leaves_shadow.vert",
        source_path: "shader/slang/leaves_shadow.vert.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Vertex,
    },
    ShaderConfig {
        logical_path: "shader/foliage/leaves.vert",
        source_path: "shader/slang/leaves.vert.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Vertex,
    },
    ShaderConfig {
        logical_path: "shader/foliage/flora.frag",
        source_path: "shader/slang/flora.frag.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Fragment,
    },
    ShaderConfig {
        logical_path: "shader/props/dynamic_fruit.vert",
        source_path: "shader/slang/dynamic_fruit.vert.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Vertex,
    },
    ShaderConfig {
        logical_path: "shader/props/dynamic_fruit_shadow.vert",
        source_path: "shader/slang/dynamic_fruit_shadow.vert.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Vertex,
    },
    ShaderConfig {
        logical_path: "shader/props/dynamic_fruit_shadow.frag",
        source_path: "shader/slang/dynamic_fruit_shadow.frag.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Fragment,
    },
    ShaderConfig {
        logical_path: "shader/particles/particle_lod_textured.frag",
        source_path: "shader/slang/particle_lod_textured.frag.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Fragment,
    },
    ShaderConfig {
        logical_path: "shader/particles/particle_lod_textured.vert",
        source_path: "shader/slang/particle_lod_textured.vert.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Vertex,
    },
    ShaderConfig {
        logical_path: "shader/particles/water_droplet.frag",
        source_path: "shader/slang/water_droplet.frag.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Fragment,
    },
    ShaderConfig {
        logical_path: "shader/tracer/player_collider.comp",
        source_path: "shader/slang/player_collider.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/tracer/post_processing.comp",
        source_path: "shader/slang/post_processing.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/scene_accel/update_scene_tex.comp",
        source_path: "shader/slang/update_scene_tex.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/props/sprinkler.vert",
        source_path: "shader/slang/sprinkler.vert.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Vertex,
    },
    ShaderConfig {
        logical_path: "shader/preview/geometry_preview.frag",
        source_path: "shader/slang/geometry_preview.frag.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Fragment,
    },
    ShaderConfig {
        logical_path: "shader/preview/geometry_preview.vert",
        source_path: "shader/slang/geometry_preview.vert.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Vertex,
    },
    ShaderConfig {
        logical_path: "shader/preview/environment_probe_visualization.vert",
        source_path: "shader/slang/environment_probe_visualization.vert.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Vertex,
    },
    ShaderConfig {
        logical_path: "shader/tracer/environment_probe_global_copy.comp",
        source_path: "shader/slang/environment_probe_global_copy.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/tracer/environment_probe_classify.comp",
        source_path: "shader/slang/environment_probe_classify.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/tracer/environment_probe_update.comp",
        source_path: "shader/slang/environment_probe_update.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/tracer/environment_probe_stats.comp",
        source_path: "shader/slang/environment_probe_stats.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/surface/active_surface_to_flora_instances.comp",
        source_path: "shader/slang/active_surface_to_flora_instances.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/surface/clear_occupancy.comp",
        source_path: "shader/slang/clear_occupancy.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/surface/edit_occupancy_capsule.comp",
        source_path: "shader/slang/edit_occupancy_capsule.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/surface/instances_to_occupancy.comp",
        source_path: "shader/slang/instances_to_occupancy.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/surface/make_surface.comp",
        source_path: "shader/slang/make_surface.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/surface/make_surface_sparse.comp",
        source_path: "shader/slang/make_surface_sparse.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/surface/occupancy_to_flora_instances.comp",
        source_path: "shader/slang/occupancy_to_flora_instances.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/surface/prepare_active_surface_flora_dispatch.comp",
        source_path: "shader/slang/prepare_active_surface_flora_dispatch.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/surface/prepare_sparse_surface_dispatch.comp",
        source_path: "shader/slang/prepare_sparse_surface_dispatch.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/builder/surface/update_flora_growth.comp",
        source_path: "shader/slang/update_flora_growth.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/tracer/cloud.comp",
        source_path: "shader/slang/cloud.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/tracer/cloud_shadow.comp",
        source_path: "shader/slang/cloud_shadow.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/tracer/cloud_shadow_temporal.comp",
        source_path: "shader/slang/cloud_shadow_temporal.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/tracer/cloud_temporal.comp",
        source_path: "shader/slang/cloud_temporal.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/tracer/god_ray.comp",
        source_path: "shader/slang/god_ray.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/tracer/leaf_shadow_mask.comp",
        source_path: "shader/slang/leaf_shadow_mask.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/tracer/leaf_shadow_temporal.comp",
        source_path: "shader/slang/leaf_shadow_temporal.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/tracer/lens_flare_downsample.comp",
        source_path: "shader/slang/lens_flare_downsample.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/tracer/lens_flare.comp",
        source_path: "shader/slang/lens_flare.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/tracer/lens_flare_sun_visible.comp",
        source_path: "shader/slang/lens_flare_sun_visible.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/tracer/shadow_depth_copy.comp",
        source_path: "shader/slang/shadow_depth_copy.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/tracer/terrain_query.comp",
        source_path: "shader/slang/terrain_query.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/tracer/vsm_blur_h.comp",
        source_path: "shader/slang/vsm_blur_h.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/tracer/vsm_blur_v.comp",
        source_path: "shader/slang/vsm_blur_v.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/tracer/vsm_creation.comp",
        source_path: "shader/slang/vsm_creation.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/tracer/wind_volume.comp",
        source_path: "shader/slang/wind_volume.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/terrarium/glass.frag",
        source_path: "shader/slang/terrarium_glass.frag.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Fragment,
    },
    ShaderConfig {
        logical_path: "shader/terrarium/glass.vert",
        source_path: "shader/slang/terrarium_glass.vert.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Vertex,
    },
    ShaderConfig {
        logical_path: "shader/tracer/tracer.comp",
        source_path: "shader/slang/tracer.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
    ShaderConfig {
        logical_path: "shader/tracer/tracer_shadow.comp",
        source_path: "shader/slang/tracer_shadow.slang",
        module_path: "shader/slang",
        stage: ShaderStage::Compute,
    },
];

#[repr(C)]
struct SlangGlobalSessionDesc {
    structure_size: u32,
    api_version: u32,
    min_language_version: u32,
    enable_glsl: bool,
    reserved: [u32; 16],
}

#[repr(C)]
struct SlangBlob {
    vtable: *const SlangBlobVTable,
}

#[repr(C)]
struct SlangBlobVTable {
    query_interface: *const c_void,
    add_ref: *const c_void,
    release: unsafe extern "C" fn(*mut SlangBlob) -> u32,
    get_buffer_pointer: unsafe extern "C" fn(*mut SlangBlob) -> *const c_void,
    get_buffer_size: unsafe extern "C" fn(*mut SlangBlob) -> usize,
}

type SlangCreateGlobalSession =
    unsafe extern "C" fn(*const SlangGlobalSessionDesc, *mut *mut c_void) -> i32;
type SlangDestroySession = unsafe extern "C" fn(*mut c_void);
type SlangGetBuildTagString = unsafe extern "C" fn() -> *const c_char;
type SlangCreateCompileRequest = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
type SlangDestroyCompileRequest = unsafe extern "C" fn(*mut c_void);
type SlangSetCodeGenTarget = unsafe extern "C" fn(*mut c_void, i32);
type SlangFindProfile = unsafe extern "C" fn(*mut c_void, *const c_char) -> u32;
type SlangSetTargetProfile = unsafe extern "C" fn(*mut c_void, i32, u32);
type SlangSetMatrixLayoutMode = unsafe extern "C" fn(*mut c_void, u32);
type SlangSetOptimizationLevel = unsafe extern "C" fn(*mut c_void, u32);
type SlangAddSearchPath = unsafe extern "C" fn(*mut c_void, *const c_char);
type SlangProcessCommandLineArguments =
    unsafe extern "C" fn(*mut c_void, *const *const c_char, i32) -> i32;
type SlangAddTranslationUnit = unsafe extern "C" fn(*mut c_void, i32, *const c_char) -> i32;
type SlangAddTranslationUnitSourceFile = unsafe extern "C" fn(*mut c_void, i32, *const c_char);
type SlangAddEntryPoint = unsafe extern "C" fn(*mut c_void, i32, *const c_char, u32) -> i32;
type SlangCompile = unsafe extern "C" fn(*mut c_void) -> i32;
type SlangGetDiagnosticOutput = unsafe extern "C" fn(*mut c_void) -> *const c_char;
type SlangGetDependencyFileCount = unsafe extern "C" fn(*mut c_void) -> i32;
type SlangGetDependencyFilePath = unsafe extern "C" fn(*mut c_void, i32) -> *const c_char;
type SlangGetEntryPointCodeBlob =
    unsafe extern "C" fn(*mut c_void, i32, i32, *mut *mut SlangBlob) -> i32;

struct SlangApi {
    create_global_session: SlangCreateGlobalSession,
    destroy_session: SlangDestroySession,
    get_build_tag_string: SlangGetBuildTagString,
    create_compile_request: SlangCreateCompileRequest,
    destroy_compile_request: SlangDestroyCompileRequest,
    set_code_gen_target: SlangSetCodeGenTarget,
    find_profile: SlangFindProfile,
    set_target_profile: SlangSetTargetProfile,
    set_matrix_layout_mode: SlangSetMatrixLayoutMode,
    set_optimization_level: SlangSetOptimizationLevel,
    add_search_path: SlangAddSearchPath,
    process_command_line_arguments: SlangProcessCommandLineArguments,
    add_translation_unit: SlangAddTranslationUnit,
    add_translation_unit_source_file: SlangAddTranslationUnitSourceFile,
    add_entry_point: SlangAddEntryPoint,
    compile: SlangCompile,
    get_diagnostic_output: SlangGetDiagnosticOutput,
    get_dependency_file_count: SlangGetDependencyFileCount,
    get_dependency_file_path: SlangGetDependencyFilePath,
    get_entry_point_code_blob: SlangGetEntryPointCodeBlob,
    _library: Library,
}

impl SlangApi {
    unsafe fn load(library_path: &Path) -> Self {
        let library = Library::new(library_path).unwrap_or_else(|error| {
            panic!(
                "load Slang compiler library {}: {error}. Install Slang from the Vulkan SDK, set SLANG_LIB, or set SLANGC to a compiler beside the shared library",
                library_path.display(),
            )
        });
        Self {
            create_global_session: load_slang_symbol(&library, b"slang_createGlobalSession2\0"),
            destroy_session: load_slang_symbol(&library, b"spDestroySession\0"),
            get_build_tag_string: load_slang_symbol(&library, b"spGetBuildTagString\0"),
            create_compile_request: load_slang_symbol(&library, b"spCreateCompileRequest\0"),
            destroy_compile_request: load_slang_symbol(&library, b"spDestroyCompileRequest\0"),
            set_code_gen_target: load_slang_symbol(&library, b"spSetCodeGenTarget\0"),
            find_profile: load_slang_symbol(&library, b"spFindProfile\0"),
            set_target_profile: load_slang_symbol(&library, b"spSetTargetProfile\0"),
            set_matrix_layout_mode: load_slang_symbol(&library, b"spSetMatrixLayoutMode\0"),
            set_optimization_level: load_slang_symbol(&library, b"spSetOptimizationLevel\0"),
            add_search_path: load_slang_symbol(&library, b"spAddSearchPath\0"),
            process_command_line_arguments: load_slang_symbol(
                &library,
                b"spProcessCommandLineArguments\0",
            ),
            add_translation_unit: load_slang_symbol(&library, b"spAddTranslationUnit\0"),
            add_translation_unit_source_file: load_slang_symbol(
                &library,
                b"spAddTranslationUnitSourceFile\0",
            ),
            add_entry_point: load_slang_symbol(&library, b"spAddEntryPoint\0"),
            compile: load_slang_symbol(&library, b"spCompile\0"),
            get_diagnostic_output: load_slang_symbol(&library, b"spGetDiagnosticOutput\0"),
            get_dependency_file_count: load_slang_symbol(
                &library,
                b"spGetDependencyFileCount\0",
            ),
            get_dependency_file_path: load_slang_symbol(
                &library,
                b"spGetDependencyFilePath\0",
            ),
            get_entry_point_code_blob: load_slang_symbol(&library, b"spGetEntryPointCodeBlob\0"),
            _library: library,
        }
    }
}

unsafe fn load_slang_symbol<T: Copy>(library: &Library, symbol: &[u8]) -> T {
    *library.get::<T>(symbol).unwrap_or_else(|error| {
        let symbol = String::from_utf8_lossy(symbol);
        panic!(
            "load Slang compiler API symbol {}: {error}",
            symbol.trim_end_matches('\0'),
        )
    })
}

pub struct NativeSlangCompiler {
    api: SlangApi,
    session: *mut c_void,
    build_tag: String,
}

impl NativeSlangCompiler {
    pub fn new() -> Self {
        let library_path = find_slang_library();
        let api = unsafe { SlangApi::load(&library_path) };
        let desc = SlangGlobalSessionDesc {
            structure_size: std::mem::size_of::<SlangGlobalSessionDesc>() as u32,
            api_version: 0,
            min_language_version: 2025,
            enable_glsl: false,
            reserved: [0; 16],
        };
        let mut session = std::ptr::null_mut();
        let result = unsafe { (api.create_global_session)(&desc, &mut session) };
        assert!(
            result >= 0 && !session.is_null(),
            "create Slang global compiler session: result {result}",
        );

        let build_tag = unsafe { (api.get_build_tag_string)() };
        let build_tag = if build_tag.is_null() {
            "unknown".into()
        } else {
            unsafe { CStr::from_ptr(build_tag) }
                .to_string_lossy()
                .into_owned()
        };
        println!(
            "cargo:warning=using Slang compiler API {build_tag} from {}",
            library_path.display(),
        );

        Self {
            api,
            session,
            build_tag,
        }
    }

    pub fn build_tag(&self) -> &str {
        &self.build_tag
    }

    pub fn compile_shader(
        &self,
        config: &ShaderConfig,
        project_root: &Path,
        optimization_level: OptimizationLevel,
    ) -> CompilerOutput {
        const SLANG_SPIRV: i32 = 6;
        const SLANG_SOURCE_LANGUAGE_SLANG: i32 = 1;
        const SLANG_MATRIX_LAYOUT_COLUMN_MAJOR: u32 = 2;
        const SLANG_OPTIMIZATION_LEVEL_NONE: u32 = 0;
        const SLANG_OPTIMIZATION_LEVEL_MAXIMAL: u32 = 3;

        let request = SlangCompileRequest::new(&self.api, self.session);
        let request_ptr = request.raw;
        let profile_name = CString::new("spirv_1_6").expect("valid Slang profile name");
        let profile = unsafe { (self.api.find_profile)(self.session, profile_name.as_ptr()) };
        assert_ne!(
            profile, 0,
            "Slang compiler does not support profile spirv_1_6"
        );

        let (optimization, optimization_arg) = match optimization_level {
            OptimizationLevel::Zero => (SLANG_OPTIMIZATION_LEVEL_NONE, "-O0"),
            OptimizationLevel::Performance => (SLANG_OPTIMIZATION_LEVEL_MAXIMAL, "-O3"),
        };
        let source_language = SLANG_SOURCE_LANGUAGE_SLANG;
        let matrix_layout = SLANG_MATRIX_LAYOUT_COLUMN_MAJOR;
        let compiler_options = &["-fvk-use-gl-layout", "-std", "2025"][..];

        unsafe {
            (self.api.set_code_gen_target)(request_ptr, SLANG_SPIRV);
            (self.api.set_target_profile)(request_ptr, 0, profile);
            (self.api.set_matrix_layout_mode)(request_ptr, matrix_layout);
            (self.api.set_optimization_level)(request_ptr, optimization);
        }

        let module_path = path_to_c_string(&project_root.join(config.module_path));
        unsafe { (self.api.add_search_path)(request_ptr, module_path.as_ptr()) };
        let mut options = compiler_options.to_vec();
        if optimization_level == OptimizationLevel::Zero {
            options.push("-preserve-params");
        }
        let options = options
            .into_iter()
            .map(|option| CString::new(option).expect("valid Slang compiler option"))
            .collect::<Vec<_>>();
        let option_pointers = options
            .iter()
            .map(|option| option.as_ptr())
            .collect::<Vec<_>>();
        let options_result = unsafe {
            (self.api.process_command_line_arguments)(
                request_ptr,
                option_pointers.as_ptr(),
                option_pointers.len() as i32,
            )
        };
        if options_result < 0 {
            panic!(
                "configure Slang compiler for {} {optimization_arg}:\n{}",
                config.source_path,
                request.diagnostics(),
            );
        }

        let translation_unit_name =
            CString::new(config.logical_path).expect("logical shader path must not contain null");
        let translation_unit = unsafe {
            (self.api.add_translation_unit)(
                request_ptr,
                source_language,
                translation_unit_name.as_ptr(),
            )
        };
        assert!(
            translation_unit >= 0,
            "add Slang translation unit for {}",
            config.source_path,
        );
        let shader_path = project_root.join(config.source_path);
        let shader_path_c = path_to_c_string(&shader_path);
        unsafe {
            (self.api.add_translation_unit_source_file)(
                request_ptr,
                translation_unit,
                shader_path_c.as_ptr(),
            );
        }
        let entry_point_name = CString::new(ENTRY_POINT).expect("valid Slang entry point name");
        let entry_point = unsafe {
            (self.api.add_entry_point)(
                request_ptr,
                translation_unit,
                entry_point_name.as_ptr(),
                config.stage.slang_api_value(),
            )
        };
        assert!(
            entry_point >= 0,
            "add Slang entry point for {}",
            config.source_path,
        );

        let compile_result = unsafe { (self.api.compile)(request_ptr) };
        if compile_result < 0 {
            panic!(
                "compile {} with Slang {optimization_arg}:\n{}",
                shader_path.display(),
                request.diagnostics(),
            );
        }

        let mut blob = std::ptr::null_mut();
        let code_result =
            unsafe { (self.api.get_entry_point_code_blob)(request_ptr, entry_point, 0, &mut blob) };
        if code_result < 0 || blob.is_null() {
            panic!(
                "emit {} with Slang {optimization_arg}: result {code_result}\n{}",
                shader_path.display(),
                request.diagnostics(),
            );
        }
        // Ask Slang for the resolved transitive module graph rather than
        // duplicating its import syntax or search-path rules in the build.
        let mut dependencies = BTreeSet::new();
        dependencies.insert(canonical_dependency(&shader_path));
        let dependency_count = unsafe { (self.api.get_dependency_file_count)(request_ptr) };
        assert!(
            dependency_count >= 0,
            "get Slang dependency count for {}: {dependency_count}",
            shader_path.display(),
        );
        for dependency_index in 0..dependency_count {
            let dependency = unsafe {
                (self.api.get_dependency_file_path)(request_ptr, dependency_index)
            };
            assert!(
                !dependency.is_null(),
                "get Slang dependency {dependency_index} for {}",
                shader_path.display(),
            );
            let dependency = unsafe { CStr::from_ptr(dependency) }.to_string_lossy();
            let dependency = PathBuf::from(dependency.as_ref());
            let dependency = if dependency.is_absolute() {
                dependency
            } else {
                project_root.join(dependency)
            };
            dependencies.insert(canonical_dependency(&dependency));
        }

        CompilerOutput {
            spirv: SlangOwnedBlob(blob).to_vec(),
            dependencies,
        }
    }
}

impl Drop for NativeSlangCompiler {
    fn drop(&mut self) {
        unsafe { (self.api.destroy_session)(self.session) };
    }
}

struct SlangCompileRequest<'a> {
    api: &'a SlangApi,
    raw: *mut c_void,
}

impl<'a> SlangCompileRequest<'a> {
    fn new(api: &'a SlangApi, session: *mut c_void) -> Self {
        let raw = unsafe { (api.create_compile_request)(session) };
        assert!(!raw.is_null(), "create Slang compile request");
        Self { api, raw }
    }

    fn diagnostics(&self) -> String {
        let diagnostics = unsafe { (self.api.get_diagnostic_output)(self.raw) };
        if diagnostics.is_null() {
            String::new()
        } else {
            unsafe { CStr::from_ptr(diagnostics) }
                .to_string_lossy()
                .into_owned()
        }
    }
}

impl Drop for SlangCompileRequest<'_> {
    fn drop(&mut self) {
        unsafe { (self.api.destroy_compile_request)(self.raw) };
    }
}

struct SlangOwnedBlob(*mut SlangBlob);

impl SlangOwnedBlob {
    fn to_vec(&self) -> Vec<u8> {
        let vtable = unsafe { &*(*self.0).vtable };
        let pointer = unsafe { (vtable.get_buffer_pointer)(self.0) };
        let size = unsafe { (vtable.get_buffer_size)(self.0) };
        assert!(
            !pointer.is_null() || size == 0,
            "Slang returned a null code blob"
        );
        unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), size) }.to_vec()
    }
}

impl Drop for SlangOwnedBlob {
    fn drop(&mut self) {
        let vtable = unsafe { &*(*self.0).vtable };
        unsafe { (vtable.release)(self.0) };
    }
}

fn path_to_c_string(path: &Path) -> CString {
    CString::new(path.to_string_lossy().as_bytes())
        .unwrap_or_else(|_| panic!("path contains a null byte: {}", path.display()))
}

fn find_slang_library() -> PathBuf {
    if let Some(path) = env::var_os("SLANG_LIB") {
        return PathBuf::from(path);
    }

    let library_name = format!(
        "{}slang{}",
        env::consts::DLL_PREFIX,
        env::consts::DLL_SUFFIX,
    );
    let mut candidates = Vec::new();
    if let Some(slangc) = env::var_os("SLANGC").and_then(resolve_executable) {
        add_slangc_library_candidates(&mut candidates, &slangc, &library_name);
    }
    if let Some(vulkan_sdk) = env::var_os("VULKAN_SDK") {
        let vulkan_sdk = PathBuf::from(vulkan_sdk);
        for directory in ["lib", "Lib", "bin", "Bin"] {
            candidates.push(vulkan_sdk.join(directory).join(&library_name));
        }
    }
    let default_slangc = PathBuf::from(format!("slangc{}", env::consts::EXE_SUFFIX));
    if let Some(slangc) = resolve_executable(default_slangc) {
        add_slangc_library_candidates(&mut candidates, &slangc, &library_name);
    }

    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| PathBuf::from(library_name))
}

fn resolve_executable(path: impl Into<PathBuf>) -> Option<PathBuf> {
    let path = path.into();
    if path.is_file() {
        return Some(path);
    }
    if path.components().count() > 1 {
        return None;
    }
    env::var_os("PATH").and_then(|search_path| {
        env::split_paths(&search_path)
            .map(|directory| directory.join(&path))
            .find(|candidate| candidate.is_file())
    })
}

fn add_slangc_library_candidates(candidates: &mut Vec<PathBuf>, slangc: &Path, library_name: &str) {
    let Some(bin_directory) = slangc.parent() else {
        return;
    };
    candidates.push(bin_directory.join(library_name));
    if let Some(prefix) = bin_directory.parent() {
        for directory in ["lib", "Lib", "bin", "Bin"] {
            candidates.push(prefix.join(directory).join(library_name));
        }
    }
}

fn canonical_dependency(path: &Path) -> PathBuf {
    std::fs::canonicalize(path)
        .unwrap_or_else(|error| panic!("resolve shader dependency {}: {error}", path.display()))
}

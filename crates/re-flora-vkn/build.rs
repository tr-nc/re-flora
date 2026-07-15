use shaderc::{CompileOptions, Compiler, EnvVersion, OptimizationLevel, ShaderKind, SpirvVersion};
use spirv_reflect::{
    types::{ReflectDecorationFlags, ReflectDescriptorType, ReflectDimension, ReflectImageFormat},
    ShaderModule as ReflectShaderModule,
};
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const ENTRY_POINT: &str = "main";

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShaderStage {
    Compute,
    Vertex,
    Fragment,
}

impl ShaderStage {
    fn slang_arg(self) -> &'static str {
        match self {
            Self::Compute => "compute",
            Self::Vertex => "vertex",
            Self::Fragment => "fragment",
        }
    }

    fn file_extension(self) -> &'static str {
        match self {
            Self::Compute => "comp",
            Self::Vertex => "vert",
            Self::Fragment => "frag",
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShaderFrontend {
    NativeSlang2025,
    GlslViaSlang,
}

#[derive(Debug, Clone, Copy)]
struct ShaderOverride {
    logical_path: &'static str,
    source_path: &'static str,
    include_path: &'static str,
    stage: ShaderStage,
    frontend: ShaderFrontend,
    defines: &'static [&'static str],
}

// A logical path is the stable runtime identity. Without a matching override it
// compiles from GLSL through shaderc. A feature can replace only that path with
// native Slang, or with GLSL through Slang for an isolated backend comparison.
const SHADER_OVERRIDES: &[ShaderOverride] = &[
    #[cfg(feature = "slang-composition-backend")]
    ShaderOverride {
        logical_path: "shader/tracer/composition.comp",
        source_path: "shader/tracer/composition.comp",
        include_path: "shader",
        stage: ShaderStage::Compute,
        frontend: ShaderFrontend::GlslViaSlang,
        defines: &["COMPOSITION_EXPLICIT_LOD"],
    },
    #[cfg(feature = "slang-contree-leaf")]
    ShaderOverride {
        logical_path: "shader/builder/contree/leaf_write.comp",
        source_path: "shader/experiments/slang/contree_leaf_write.slang",
        include_path: "shader/experiments/slang",
        stage: ShaderStage::Compute,
        frontend: ShaderFrontend::NativeSlang2025,
        defines: &[],
    },
    #[cfg(feature = "slang-egui")]
    ShaderOverride {
        logical_path: "shader/egui/egui.vert",
        source_path: "shader/experiments/slang/egui.vert.slang",
        include_path: "shader/experiments/slang",
        stage: ShaderStage::Vertex,
        frontend: ShaderFrontend::NativeSlang2025,
        defines: &[],
    },
    #[cfg(feature = "slang-egui")]
    ShaderOverride {
        logical_path: "shader/egui/egui.frag",
        source_path: "shader/experiments/slang/egui.frag.slang",
        include_path: "shader/experiments/slang",
        stage: ShaderStage::Fragment,
        frontend: ShaderFrontend::NativeSlang2025,
        defines: &[],
    },
    #[cfg(feature = "slang-post-processing")]
    ShaderOverride {
        logical_path: "shader/tracer/post_processing.comp",
        source_path: "shader/experiments/slang/post_processing.slang",
        include_path: "shader/experiments/slang",
        stage: ShaderStage::Compute,
        frontend: ShaderFrontend::NativeSlang2025,
        defines: &[],
    },
    #[cfg(feature = "slang-surface")]
    ShaderOverride {
        logical_path: "shader/builder/surface/make_surface_sparse.comp",
        source_path: "shader/experiments/slang/make_surface_sparse.slang",
        include_path: "shader/experiments/slang",
        stage: ShaderStage::Compute,
        frontend: ShaderFrontend::NativeSlang2025,
        defines: &[],
    },
    #[cfg(feature = "slang-tracer-backend")]
    ShaderOverride {
        logical_path: "shader/tracer/tracer.comp",
        source_path: "shader/tracer/tracer.comp",
        include_path: "shader",
        stage: ShaderStage::Compute,
        frontend: ShaderFrontend::GlslViaSlang,
        defines: &["DIRECT_SUN_SHADOW_EXPLICIT_LOD"],
    },
    #[cfg(feature = "slang-tracer-shadow")]
    ShaderOverride {
        logical_path: "shader/tracer/tracer_shadow.comp",
        source_path: "shader/experiments/slang/tracer_shadow.slang",
        include_path: "shader/experiments/slang",
        stage: ShaderStage::Compute,
        frontend: ShaderFrontend::NativeSlang2025,
        defines: &[],
    },
];

fn main() {
    let crate_root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let project_root = crate_root
        .parent()
        .and_then(Path::parent)
        .expect("re-flora-vkn must live under <project>/crates/re-flora-vkn");
    let shader_root = project_root.join("shader");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));

    println!("cargo:rerun-if-changed={}", shader_root.display());
    println!("cargo:rerun-if-env-changed=SLANGC");
    println!("cargo:rerun-if-env-changed=VULKAN_SDK");

    let mut shader_paths = Vec::new();
    collect_shader_paths(&shader_root, &mut shader_paths);
    shader_paths.sort();
    validate_shader_overrides(project_root, &shader_paths);

    let compiler = Compiler::new().expect("create shader compiler");
    let artifact_root = out_dir.join("precompiled-shaders");
    let mut generated = String::from(
        "// Generated by crates/re-flora-vkn/build.rs. Do not edit.\n\
         pub(crate) struct PrecompiledShader {\n\
         \tpub(crate) reflection_spirv: &'static [u8],\n\
         \tpub(crate) optimized_spirv: &'static [u8],\n\
         }\n\n\
         pub(crate) fn find_precompiled_shader(file_path: &str) -> Option<PrecompiledShader> {\n\
         \tmatch file_path {\n",
    );

    let mut native_slang_shader_count = 0;
    let mut slang_glsl_shader_count = 0;
    for shader_path in &shader_paths {
        let relative_path = shader_path
            .strip_prefix(project_root)
            .expect("shader path must be under project root");
        let logical_path = path_with_forward_slashes(relative_path);
        let source = fs::read_to_string(shader_path)
            .unwrap_or_else(|error| panic!("read {}: {error}", shader_path.display()));
        let shader_kind = shader_kind(shader_path);
        let shader_override = SHADER_OVERRIDES
            .iter()
            .find(|shader_override| shader_override.logical_path == logical_path);

        let (reflection_spirv, optimized_spirv) = if let Some(shader_override) = shader_override {
            match shader_override.frontend {
                ShaderFrontend::NativeSlang2025 => native_slang_shader_count += 1,
                ShaderFrontend::GlslViaSlang => slang_glsl_shader_count += 1,
            }

            let glsl_reference_spirv = compile_shader(
                &compiler,
                &source,
                shader_kind,
                shader_path,
                OptimizationLevel::Zero,
            );
            let replacement_reflection_spirv = compile_slang_shader(
                shader_override,
                project_root,
                &out_dir,
                OptimizationLevel::Zero,
            );
            validate_shader_abi(
                &logical_path,
                shader_override.stage,
                &glsl_reference_spirv,
                &replacement_reflection_spirv,
            );

            (
                replacement_reflection_spirv,
                compile_slang_shader(
                    shader_override,
                    project_root,
                    &out_dir,
                    OptimizationLevel::Performance,
                ),
            )
        } else {
            (
                compile_shader(
                    &compiler,
                    &source,
                    shader_kind,
                    shader_path,
                    OptimizationLevel::Zero,
                ),
                compile_shader(
                    &compiler,
                    &source,
                    shader_kind,
                    shader_path,
                    OptimizationLevel::Performance,
                ),
            )
        };

        let artifact_path = artifact_root.join(relative_path);
        let reflection_path = append_extension(&artifact_path, "reflection.spv");
        let optimized_path = append_extension(&artifact_path, "optimized.spv");
        fs::create_dir_all(
            reflection_path
                .parent()
                .expect("shader artifact must have parent"),
        )
        .expect("create shader artifact directory");
        fs::write(&reflection_path, reflection_spirv)
            .unwrap_or_else(|error| panic!("write {}: {error}", reflection_path.display()));
        fs::write(&optimized_path, optimized_spirv)
            .unwrap_or_else(|error| panic!("write {}: {error}", optimized_path.display()));

        let relative_reflection = path_with_forward_slashes(
            reflection_path
                .strip_prefix(&out_dir)
                .expect("artifact must be under OUT_DIR"),
        );
        let relative_optimized = path_with_forward_slashes(
            optimized_path
                .strip_prefix(&out_dir)
                .expect("artifact must be under OUT_DIR"),
        );
        generated.push_str(&format!(
            "\t\t{logical_path:?} => Some(PrecompiledShader {{\n\
             \t\t\treflection_spirv: include_bytes!(concat!(env!(\"OUT_DIR\"), {reflection:?})),\n\
             \t\t\toptimized_spirv: include_bytes!(concat!(env!(\"OUT_DIR\"), {optimized:?})),\n\
             \t\t}}),\n",
            reflection = format!("/{relative_reflection}"),
            optimized = format!("/{relative_optimized}"),
        ));
    }

    generated.push_str("\t\t_ => None,\n\t}\n}\n");
    fs::write(out_dir.join("precompiled_shaders.rs"), generated)
        .expect("write generated precompiled shader registry");

    println!(
        "cargo:warning=precompiled {} shaderc GLSL, {} Slang GLSL, and {} native Slang shaders into SPIR-V artifacts",
        shader_paths.len() - native_slang_shader_count - slang_glsl_shader_count,
        slang_glsl_shader_count,
        native_slang_shader_count,
    );
}

fn validate_shader_overrides(project_root: &Path, shader_paths: &[PathBuf]) {
    let logical_paths: BTreeSet<_> = shader_paths
        .iter()
        .map(|shader_path| {
            path_with_forward_slashes(
                shader_path
                    .strip_prefix(project_root)
                    .expect("shader path must be under project root"),
            )
        })
        .collect();
    let mut configured_paths = BTreeSet::new();

    for shader_override in SHADER_OVERRIDES {
        assert!(
            configured_paths.insert(shader_override.logical_path),
            "duplicate shader override for {}",
            shader_override.logical_path,
        );
        assert!(
            logical_paths.contains(shader_override.logical_path),
            "shader override logical path does not exist: {}",
            shader_override.logical_path,
        );

        let logical_extension = Path::new(shader_override.logical_path)
            .extension()
            .and_then(|extension| extension.to_str());
        assert_eq!(
            logical_extension,
            Some(shader_override.stage.file_extension()),
            "shader override stage does not match logical path: {}",
            shader_override.logical_path,
        );

        let source_path = project_root.join(shader_override.source_path);
        assert!(
            source_path.is_file(),
            "shader override source does not exist: {}",
            source_path.display(),
        );
        if shader_override.frontend == ShaderFrontend::NativeSlang2025 {
            assert_eq!(
                source_path
                    .extension()
                    .and_then(|extension| extension.to_str()),
                Some("slang"),
                "native Slang override must use a .slang source: {}",
                source_path.display(),
            );
        }

        let include_path = project_root.join(shader_override.include_path);
        assert!(
            include_path.is_dir(),
            "shader override include path does not exist: {}",
            include_path.display(),
        );
    }
}

#[derive(Debug, PartialEq)]
struct ShaderAbi {
    stage_bits: u32,
    workgroup_size: Option<[u32; 3]>,
    descriptors: Vec<DescriptorAbi>,
    push_constants: Vec<PushConstantAbi>,
    inputs: Vec<InterfaceAbi>,
    outputs: Vec<InterfaceAbi>,
}

#[derive(Debug, PartialEq)]
struct DescriptorAbi {
    set: u32,
    binding: u32,
    descriptor_type: ReflectDescriptorType,
    count: u32,
    array_dims: Vec<u32>,
    image: ImageAbi,
    block_size: u32,
    block_padded_size: u32,
    block_members: Vec<BlockMemberAbi>,
}

// SPIR-V's image Depth operand is intentionally excluded: Slang emits Unknown
// where shaderc emits NotDepth for storage images, but it does not affect the
// Vulkan descriptor or pipeline ABI. Dimension, array shape, sampling mode, and
// format are stable and are checked below.
#[derive(Debug, PartialEq)]
struct ImageAbi {
    dimension: ReflectDimension,
    arrayed: u32,
    multisampled: u32,
    sampled: u32,
    format: ReflectImageFormat,
}

#[derive(Debug, PartialEq, Eq)]
struct BlockMemberAbi {
    offset: u32,
    size: u32,
    padded_size: u32,
    array_dims: Vec<u32>,
    array_stride: u32,
}

#[derive(Debug, PartialEq, Eq)]
struct PushConstantAbi {
    offset: u32,
    size: u32,
    padded_size: u32,
    members: Vec<BlockMemberAbi>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct InterfaceAbi {
    location: u32,
    format: String,
    array_dims: Vec<u32>,
    no_perspective: bool,
    flat: bool,
    patch: bool,
    per_vertex: bool,
}

fn validate_shader_abi(
    logical_path: &str,
    stage: ShaderStage,
    glsl_spirv: &[u8],
    replacement_spirv: &[u8],
) {
    let glsl_abi = reflect_shader_abi(logical_path, stage, "GLSL", glsl_spirv);
    let replacement_abi = reflect_shader_abi(logical_path, stage, "replacement", replacement_spirv);
    assert_eq!(
        glsl_abi, replacement_abi,
        "shader frontend ABI mismatch for {logical_path}",
    );
}

fn reflect_shader_abi(
    logical_path: &str,
    stage: ShaderStage,
    frontend: &str,
    spirv: &[u8],
) -> ShaderAbi {
    let module = ReflectShaderModule::load_u8_data(spirv)
        .unwrap_or_else(|error| panic!("reflect {frontend} SPIR-V for {logical_path}: {error}"));

    let mut descriptors = module
        .enumerate_descriptor_bindings(Some(ENTRY_POINT))
        .unwrap_or_else(|error| {
            panic!("enumerate {frontend} descriptors for {logical_path}: {error}")
        })
        .into_iter()
        .map(|binding| DescriptorAbi {
            set: binding.set,
            binding: binding.binding,
            descriptor_type: binding.descriptor_type,
            count: binding.count,
            array_dims: binding.array.dims,
            image: ImageAbi {
                dimension: binding.image.dim,
                arrayed: binding.image.arrayed,
                multisampled: binding.image.ms,
                sampled: binding.image.sampled,
                format: binding.image.image_format,
            },
            block_size: binding.block.size,
            block_padded_size: binding.block.padded_size,
            block_members: reflect_block_members(&binding.block.members),
        })
        .collect::<Vec<_>>();
    descriptors.sort_by_key(|binding| (binding.set, binding.binding));

    let mut push_constants = module
        .enumerate_push_constant_blocks(Some(ENTRY_POINT))
        .unwrap_or_else(|error| {
            panic!("enumerate {frontend} push constants for {logical_path}: {error}")
        })
        .into_iter()
        .map(|block| PushConstantAbi {
            offset: block.offset,
            size: block.size,
            padded_size: block.padded_size,
            members: reflect_block_members(&block.members),
        })
        .collect::<Vec<_>>();
    push_constants.sort_by_key(|block| block.offset);

    let inputs = reflect_interfaces(
        logical_path,
        frontend,
        "inputs",
        module.enumerate_input_variables(Some(ENTRY_POINT)),
    );
    let outputs = reflect_interfaces(
        logical_path,
        frontend,
        "outputs",
        module.enumerate_output_variables(Some(ENTRY_POINT)),
    );

    let workgroup_size = (stage == ShaderStage::Compute).then(|| {
        let entry_points = module.enumerate_entry_points().unwrap_or_else(|error| {
            panic!("enumerate {frontend} entry points for {logical_path}: {error}")
        });
        let entry_point = entry_points
            .iter()
            .find(|entry_point| entry_point.name == ENTRY_POINT)
            .unwrap_or_else(|| panic!("missing {frontend} entry point for {logical_path}"));
        [
            entry_point.local_size.x,
            entry_point.local_size.y,
            entry_point.local_size.z,
        ]
    });

    ShaderAbi {
        stage_bits: module.get_shader_stage().bits(),
        workgroup_size,
        descriptors,
        push_constants,
        inputs,
        outputs,
    }
}

fn reflect_block_members(
    members: &[spirv_reflect::types::ReflectBlockVariable],
) -> Vec<BlockMemberAbi> {
    members
        .iter()
        .map(|member| BlockMemberAbi {
            offset: member.offset,
            size: member.size,
            padded_size: member.padded_size,
            array_dims: member.array.dims.clone(),
            array_stride: member.array.stride,
        })
        .collect()
}

fn reflect_interfaces(
    logical_path: &str,
    frontend: &str,
    interface_kind: &str,
    reflected: Result<Vec<spirv_reflect::types::ReflectInterfaceVariable>, &'static str>,
) -> Vec<InterfaceAbi> {
    let mut interfaces = reflected
        .unwrap_or_else(|error| {
            panic!("enumerate {frontend} {interface_kind} for {logical_path}: {error}")
        })
        .into_iter()
        .filter(|variable| {
            !variable
                .decoration_flags
                .contains(ReflectDecorationFlags::BUILT_IN)
        })
        .map(|variable| InterfaceAbi {
            location: variable.location,
            format: format!("{:?}", variable.format),
            array_dims: variable.array.dims,
            no_perspective: variable
                .decoration_flags
                .contains(ReflectDecorationFlags::NO_PERSPECTIVE),
            flat: variable
                .decoration_flags
                .contains(ReflectDecorationFlags::FLAT),
            patch: variable
                .decoration_flags
                .contains(ReflectDecorationFlags::PATCH),
            per_vertex: variable
                .decoration_flags
                .contains(ReflectDecorationFlags::PER_VERTEX),
        })
        .collect::<Vec<_>>();
    interfaces.sort();
    interfaces
}

fn compile_slang_shader(
    config: &ShaderOverride,
    project_root: &Path,
    out_dir: &Path,
    optimization_level: OptimizationLevel,
) -> Vec<u8> {
    let (optimization_arg, artifact_suffix) = match optimization_level {
        OptimizationLevel::Zero => ("-O0", "reflection"),
        OptimizationLevel::Performance => ("-O3", "optimized"),
        other => panic!("unsupported Slang optimization level: {other:?}"),
    };
    let artifact_stem: String = config
        .logical_path
        .chars()
        .map(|character| match character {
            '/' | '\\' | '.' => '-',
            other => other,
        })
        .collect();
    let output_path = out_dir
        .join("slang")
        .join(format!("{artifact_stem}-{artifact_suffix}.spv"));
    fs::create_dir_all(
        output_path
            .parent()
            .expect("Slang artifact must have parent"),
    )
    .expect("create Slang artifact directory");

    let shader_path = project_root.join(config.source_path);
    let include_path = project_root.join(config.include_path);
    let slangc = find_slangc();
    // Slang's GLSL frontend lowers column-major GLSL matrices through row-major
    // storage wrappers. Selecting row-major here preserves GLSL's std140 byte
    // interpretation; native Slang sources use the project's column-major mode.
    let matrix_layout_arg = match config.frontend {
        ShaderFrontend::NativeSlang2025 => "-matrix-layout-column-major",
        ShaderFrontend::GlslViaSlang => "-matrix-layout-row-major",
    };

    let mut command = Command::new(&slangc);
    command.args([
        shader_path.as_os_str(),
        "-I".as_ref(),
        include_path.as_os_str(),
        "-target".as_ref(),
        "spirv".as_ref(),
        "-profile".as_ref(),
        "spirv_1_6".as_ref(),
        "-entry".as_ref(),
        ENTRY_POINT.as_ref(),
        "-stage".as_ref(),
        config.stage.slang_arg().as_ref(),
        matrix_layout_arg.as_ref(),
        "-fvk-use-gl-layout".as_ref(),
        optimization_arg.as_ref(),
    ]);
    match config.frontend {
        ShaderFrontend::NativeSlang2025 => {
            command.args(["-std", "2025"]);
        }
        ShaderFrontend::GlslViaSlang => {
            command.arg("-allow-glsl");
        }
    }
    for define in config.defines {
        command.arg(format!("-D{define}"));
    }
    if optimization_level == OptimizationLevel::Zero {
        command.arg("-preserve-params");
    }
    command.arg("-o").arg(&output_path);

    let output = command.output().unwrap_or_else(|error| {
        panic!(
            "run Slang compiler {} for {}: {error}. Install slangc from the Vulkan SDK or set SLANGC",
            slangc.display(),
            shader_path.display(),
        )
    });
    if !output.status.success() {
        panic!(
            "compile {} with Slang {optimization_arg}:\n{}{}",
            shader_path.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    fs::read(&output_path).unwrap_or_else(|error| panic!("read {}: {error}", output_path.display()))
}

fn find_slangc() -> PathBuf {
    if let Some(path) = env::var_os("SLANGC") {
        return PathBuf::from(path);
    }
    if let Some(vulkan_sdk) = env::var_os("VULKAN_SDK") {
        let candidate = PathBuf::from(vulkan_sdk)
            .join("bin")
            .join(format!("slangc{}", env::consts::EXE_SUFFIX));
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from(format!("slangc{}", env::consts::EXE_SUFFIX))
}

fn collect_shader_paths(directory: &Path, shader_paths: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read shader directory {}: {error}", directory.display()));
    for entry in entries {
        let path = entry.expect("read shader directory entry").path();
        if path.is_dir() {
            collect_shader_paths(&path, shader_paths);
        } else if matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("comp" | "vert" | "frag")
        ) {
            shader_paths.push(path);
        }
    }
}

fn shader_kind(path: &Path) -> ShaderKind {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("comp") => ShaderKind::Compute,
        Some("vert") => ShaderKind::Vertex,
        Some("frag") => ShaderKind::Fragment,
        extension => panic!(
            "unsupported shader extension {extension:?}: {}",
            path.display()
        ),
    }
}

fn compile_shader(
    compiler: &Compiler,
    source: &str,
    shader_kind: ShaderKind,
    shader_path: &Path,
    optimization_level: OptimizationLevel,
) -> Vec<u8> {
    let mut options = CompileOptions::new().expect("create shader compile options");
    options.set_target_env(shaderc::TargetEnv::Vulkan, EnvVersion::Vulkan1_3 as u32);
    options.set_target_spirv(SpirvVersion::V1_6);
    options.set_source_language(shaderc::SourceLanguage::GLSL);
    options.set_optimization_level(optimization_level);
    options.set_include_callback(resolve_include);

    compiler
        .compile_into_spirv(
            source,
            shader_kind,
            shader_path.to_string_lossy().as_ref(),
            ENTRY_POINT,
            Some(&options),
        )
        .unwrap_or_else(|error| {
            panic!(
                "compile {} with {optimization_level:?}: {error}",
                shader_path.display()
            )
        })
        .as_binary_u8()
        .to_vec()
}

fn resolve_include(
    requested_source: &str,
    include_type: shaderc::IncludeType,
    requesting_source: &str,
    _include_depth: usize,
) -> Result<shaderc::ResolvedInclude, String> {
    let base_dir = match include_type {
        shaderc::IncludeType::Relative => Path::new(requesting_source)
            .parent()
            .ok_or_else(|| format!("{requesting_source} has no parent directory"))?,
        shaderc::IncludeType::Standard => {
            return Err("standard shader includes are not supported".to_owned())
        }
    };
    let full_path = base_dir
        .join(requested_source)
        .canonicalize()
        .map_err(|error| format!("resolve {requested_source}: {error}"))?;
    let content = fs::read_to_string(&full_path)
        .map_err(|error| format!("read {}: {error}", full_path.display()))?;

    Ok(shaderc::ResolvedInclude {
        resolved_name: full_path.to_string_lossy().into_owned(),
        content,
    })
}

fn append_extension(path: &Path, extension: &str) -> PathBuf {
    let mut result = path.as_os_str().to_owned();
    result.push(".");
    result.push(extension);
    result.into()
}

fn path_with_forward_slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

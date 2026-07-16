use libloading::Library;
use shaderc::{CompileOptions, Compiler, EnvVersion, OptimizationLevel, ShaderKind, SpirvVersion};
use spirv_reflect::{
    types::{ReflectDecorationFlags, ReflectDescriptorType, ReflectDimension, ReflectImageFormat},
    ShaderModule as ReflectShaderModule,
};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::env;
use std::ffi::{c_char, c_void, CStr, CString};
use std::fs;
use std::path::{Path, PathBuf};

const ENTRY_POINT: &str = "main";
const ARTIFACT_CACHE_VERSION: &str = "shader-artifact-cache-v1";

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShaderStage {
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

struct CompiledShader {
    spirv: Vec<u8>,
    dependencies: BTreeSet<PathBuf>,
}

// A logical path is the stable runtime identity. Without a matching override it
// compiles from GLSL through shaderc. A feature can replace only that path with
// native Slang, or with GLSL through Slang for an isolated backend comparison.
const SHADER_OVERRIDES: &[ShaderOverride] = &[
    #[cfg(feature = "slang-composition")]
    ShaderOverride {
        logical_path: "shader/tracer/composition.comp",
        source_path: "shader/slang/composition.slang",
        include_path: "shader/slang",
        stage: ShaderStage::Compute,
        frontend: ShaderFrontend::NativeSlang2025,
        defines: &[],
    },
    #[cfg(all(
        feature = "slang-composition-backend",
        not(feature = "slang-composition")
    ))]
    ShaderOverride {
        logical_path: "shader/tracer/composition.comp",
        source_path: "shader/tracer/composition.comp",
        include_path: "shader",
        stage: ShaderStage::Compute,
        frontend: ShaderFrontend::GlslViaSlang,
        defines: &["COMPOSITION_EXPLICIT_LOD"],
    },
    #[cfg(feature = "slang-contree-buffer-setup")]
    ShaderOverride {
        logical_path: "shader/builder/contree/buffer_setup.comp",
        source_path: "shader/slang/contree_buffer_setup.slang",
        include_path: "shader/slang",
        stage: ShaderStage::Compute,
        frontend: ShaderFrontend::NativeSlang2025,
        defines: &[],
    },
    #[cfg(feature = "slang-contree-buffer-update")]
    ShaderOverride {
        logical_path: "shader/builder/contree/buffer_update.comp",
        source_path: "shader/slang/contree_buffer_update.slang",
        include_path: "shader/slang",
        stage: ShaderStage::Compute,
        frontend: ShaderFrontend::NativeSlang2025,
        defines: &[],
    },
    #[cfg(feature = "slang-contree-concat")]
    ShaderOverride {
        logical_path: "shader/builder/contree/concat.comp",
        source_path: "shader/slang/contree_concat.slang",
        include_path: "shader/slang",
        stage: ShaderStage::Compute,
        frontend: ShaderFrontend::NativeSlang2025,
        defines: &[],
    },
    #[cfg(feature = "slang-contree-leaf")]
    ShaderOverride {
        logical_path: "shader/builder/contree/leaf_write.comp",
        source_path: "shader/slang/contree_leaf_write.slang",
        include_path: "shader/slang",
        stage: ShaderStage::Compute,
        frontend: ShaderFrontend::NativeSlang2025,
        defines: &[],
    },
    #[cfg(feature = "slang-contree-last-buffer-update")]
    ShaderOverride {
        logical_path: "shader/builder/contree/last_buffer_update.comp",
        source_path: "shader/slang/contree_last_buffer_update.slang",
        include_path: "shader/slang",
        stage: ShaderStage::Compute,
        frontend: ShaderFrontend::NativeSlang2025,
        defines: &[],
    },
    #[cfg(feature = "slang-contree-tree-write")]
    ShaderOverride {
        logical_path: "shader/builder/contree/tree_write.comp",
        source_path: "shader/slang/contree_tree_write.slang",
        include_path: "shader/slang",
        stage: ShaderStage::Compute,
        frontend: ShaderFrontend::NativeSlang2025,
        defines: &[],
    },
    #[cfg(feature = "slang-egui")]
    ShaderOverride {
        logical_path: "shader/egui/egui.vert",
        source_path: "shader/slang/egui.vert.slang",
        include_path: "shader/slang",
        stage: ShaderStage::Vertex,
        frontend: ShaderFrontend::NativeSlang2025,
        defines: &[],
    },
    #[cfg(feature = "slang-egui")]
    ShaderOverride {
        logical_path: "shader/egui/egui.frag",
        source_path: "shader/slang/egui.frag.slang",
        include_path: "shader/slang",
        stage: ShaderStage::Fragment,
        frontend: ShaderFrontend::NativeSlang2025,
        defines: &[],
    },
    #[cfg(feature = "slang-flora")]
    ShaderOverride {
        logical_path: "shader/foliage/flora.vert",
        source_path: "shader/slang/flora.vert.slang",
        include_path: "shader/slang",
        stage: ShaderStage::Vertex,
        frontend: ShaderFrontend::NativeSlang2025,
        defines: &[],
    },
    #[cfg(feature = "slang-flora")]
    ShaderOverride {
        logical_path: "shader/foliage/flora.frag",
        source_path: "shader/slang/flora.frag.slang",
        include_path: "shader/slang",
        stage: ShaderStage::Fragment,
        frontend: ShaderFrontend::NativeSlang2025,
        defines: &[],
    },
    #[cfg(feature = "slang-player-collider")]
    ShaderOverride {
        logical_path: "shader/tracer/player_collider.comp",
        source_path: "shader/slang/player_collider.slang",
        include_path: "shader/slang",
        stage: ShaderStage::Compute,
        frontend: ShaderFrontend::NativeSlang2025,
        defines: &[],
    },
    #[cfg(feature = "slang-post-processing")]
    ShaderOverride {
        logical_path: "shader/tracer/post_processing.comp",
        source_path: "shader/slang/post_processing.slang",
        include_path: "shader/slang",
        stage: ShaderStage::Compute,
        frontend: ShaderFrontend::NativeSlang2025,
        defines: &[],
    },
    #[cfg(feature = "slang-surface-clear-occupancy")]
    ShaderOverride {
        logical_path: "shader/builder/surface/clear_occupancy.comp",
        source_path: "shader/slang/clear_occupancy.slang",
        include_path: "shader/slang",
        stage: ShaderStage::Compute,
        frontend: ShaderFrontend::NativeSlang2025,
        defines: &[],
    },
    #[cfg(feature = "slang-surface-make")]
    ShaderOverride {
        logical_path: "shader/builder/surface/make_surface.comp",
        source_path: "shader/slang/make_surface.slang",
        include_path: "shader/slang",
        stage: ShaderStage::Compute,
        frontend: ShaderFrontend::NativeSlang2025,
        defines: &[],
    },
    #[cfg(feature = "slang-surface-make-sparse")]
    ShaderOverride {
        logical_path: "shader/builder/surface/make_surface_sparse.comp",
        source_path: "shader/slang/make_surface_sparse.slang",
        include_path: "shader/slang",
        stage: ShaderStage::Compute,
        frontend: ShaderFrontend::NativeSlang2025,
        defines: &[],
    },
    #[cfg(feature = "slang-surface-prepare-active-flora-dispatch")]
    ShaderOverride {
        logical_path: "shader/builder/surface/prepare_active_surface_flora_dispatch.comp",
        source_path: "shader/slang/prepare_active_surface_flora_dispatch.slang",
        include_path: "shader/slang",
        stage: ShaderStage::Compute,
        frontend: ShaderFrontend::NativeSlang2025,
        defines: &[],
    },
    #[cfg(feature = "slang-surface-prepare-sparse-dispatch")]
    ShaderOverride {
        logical_path: "shader/builder/surface/prepare_sparse_surface_dispatch.comp",
        source_path: "shader/slang/prepare_sparse_surface_dispatch.slang",
        include_path: "shader/slang",
        stage: ShaderStage::Compute,
        frontend: ShaderFrontend::NativeSlang2025,
        defines: &[],
    },
    #[cfg(feature = "slang-tracer")]
    ShaderOverride {
        logical_path: "shader/tracer/tracer.comp",
        source_path: "shader/slang/tracer.slang",
        include_path: "shader/slang",
        stage: ShaderStage::Compute,
        frontend: ShaderFrontend::NativeSlang2025,
        defines: &[],
    },
    #[cfg(all(feature = "slang-tracer-backend", not(feature = "slang-tracer")))]
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
        source_path: "shader/slang/tracer_shadow.slang",
        include_path: "shader/slang",
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
    println!("cargo:rerun-if-env-changed=SLANG_LIB");
    println!("cargo:rerun-if-env-changed=VULKAN_SDK");

    let mut shader_paths = Vec::new();
    collect_shader_paths(&shader_root, &mut shader_paths);
    shader_paths.sort();
    validate_shader_overrides(project_root, &shader_paths);

    let compiler = Compiler::new().expect("create shader compiler");
    let slang_compiler = (!SHADER_OVERRIDES.is_empty()).then(SlangCompiler::new);
    let common_cache_dependencies = [
        crate_root.join("build.rs"),
        crate_root.join("Cargo.toml"),
        project_root.join("Cargo.lock"),
    ]
    .into_iter()
    .map(|path| canonical_dependency(&path))
    .collect::<BTreeSet<_>>();
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
    let mut compiled_shader_count = 0;
    let mut reused_shader_count = 0;
    for shader_path in &shader_paths {
        let relative_path = shader_path
            .strip_prefix(project_root)
            .expect("shader path must be under project root");
        let logical_path = path_with_forward_slashes(relative_path);
        let shader_kind = shader_kind(shader_path);
        let shader_override = SHADER_OVERRIDES
            .iter()
            .find(|shader_override| shader_override.logical_path == logical_path);

        if let Some(shader_override) = shader_override {
            match shader_override.frontend {
                ShaderFrontend::NativeSlang2025 => native_slang_shader_count += 1,
                ShaderFrontend::GlslViaSlang => slang_glsl_shader_count += 1,
            }
        }

        let artifact_path = artifact_root.join(relative_path);
        let reflection_path = append_extension(&artifact_path, "reflection.spv");
        let optimized_path = append_extension(&artifact_path, "optimized.spv");
        let cache_path = append_extension(&artifact_path, "cache");
        fs::create_dir_all(
            reflection_path
                .parent()
                .expect("shader artifact must have parent"),
        )
        .expect("create shader artifact directory");

        let cache_context = shader_cache_context(
            &logical_path,
            shader_kind,
            shader_override,
            shader_override.map(|_| {
                slang_compiler
                    .as_ref()
                    .expect("Slang compiler must exist when overrides are enabled")
                    .build_tag()
            }),
        );
        if artifact_cache_is_current(
            &cache_path,
            &reflection_path,
            &optimized_path,
            &cache_context,
        ) {
            reused_shader_count += 1;
        } else {
            remove_cache_marker(&cache_path);
            let source = fs::read_to_string(shader_path)
                .unwrap_or_else(|error| panic!("read {}: {error}", shader_path.display()));
            let (reflection_spirv, optimized_spirv, mut dependencies) =
                if let Some(shader_override) = shader_override {
                    let glsl_reference = compile_shader(
                        &compiler,
                        &source,
                        shader_kind,
                        shader_path,
                        OptimizationLevel::Zero,
                    );
                    let slang_compiler = slang_compiler
                        .as_ref()
                        .expect("Slang compiler must exist when overrides are enabled");
                    let replacement_reflection = slang_compiler.compile_shader(
                        shader_override,
                        project_root,
                        OptimizationLevel::Zero,
                    );
                    validate_shader_abi(
                        &logical_path,
                        shader_override.stage,
                        &glsl_reference.spirv,
                        &replacement_reflection.spirv,
                    );
                    let replacement_optimized = slang_compiler.compile_shader(
                        shader_override,
                        project_root,
                        OptimizationLevel::Performance,
                    );

                    let mut dependencies = glsl_reference.dependencies;
                    dependencies.extend(replacement_reflection.dependencies);
                    dependencies.extend(replacement_optimized.dependencies);
                    (
                        replacement_reflection.spirv,
                        replacement_optimized.spirv,
                        dependencies,
                    )
                } else {
                    let reflection = compile_shader(
                        &compiler,
                        &source,
                        shader_kind,
                        shader_path,
                        OptimizationLevel::Zero,
                    );
                    let optimized = compile_shader(
                        &compiler,
                        &source,
                        shader_kind,
                        shader_path,
                        OptimizationLevel::Performance,
                    );
                    let mut dependencies = reflection.dependencies;
                    dependencies.extend(optimized.dependencies);
                    (reflection.spirv, optimized.spirv, dependencies)
                };
            dependencies.extend(common_cache_dependencies.iter().cloned());

            fs::write(&reflection_path, reflection_spirv)
                .unwrap_or_else(|error| panic!("write {}: {error}", reflection_path.display()));
            fs::write(&optimized_path, optimized_spirv)
                .unwrap_or_else(|error| panic!("write {}: {error}", optimized_path.display()));
            write_artifact_cache(
                &cache_path,
                &reflection_path,
                &optimized_path,
                &cache_context,
                &dependencies,
            );
            compiled_shader_count += 1;
        }

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
    write_if_changed(&out_dir.join("precompiled_shaders.rs"), generated.as_bytes());

    println!(
        "cargo:warning=precompiled {} shaderc GLSL, {} Slang GLSL, and {} native Slang shaders into SPIR-V artifacts (compiled {}, reused {})",
        shader_paths.len() - native_slang_shader_count - slang_glsl_shader_count,
        slang_glsl_shader_count,
        native_slang_shader_count,
        compiled_shader_count,
        reused_shader_count,
    );
}

// Cache entries use dependency paths reported by shaderc and Slang after a
// successful compile. On later build-script runs, content and artifact digests
// let unrelated shader changes reuse both reflection and optimized SPIR-V.
fn shader_cache_context(
    logical_path: &str,
    shader_kind: ShaderKind,
    shader_override: Option<&ShaderOverride>,
    slang_build_tag: Option<&str>,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hash_field(&mut hasher, ARTIFACT_CACHE_VERSION.as_bytes());
    hash_field(&mut hasher, logical_path.as_bytes());
    hash_field(
        &mut hasher,
        match shader_kind {
            ShaderKind::Compute => b"compute",
            ShaderKind::Vertex => b"vertex",
            ShaderKind::Fragment => b"fragment",
            other => panic!("unsupported shader kind in artifact cache: {other:?}"),
        },
    );
    hash_field(
        &mut hasher,
        env::var("TARGET")
            .expect("TARGET must be available to build scripts")
            .as_bytes(),
    );

    if let Some(shader_override) = shader_override {
        hash_field(&mut hasher, b"override");
        hash_field(&mut hasher, shader_override.source_path.as_bytes());
        hash_field(&mut hasher, shader_override.include_path.as_bytes());
        hash_field(
            &mut hasher,
            match shader_override.stage {
                ShaderStage::Compute => b"compute",
                ShaderStage::Vertex => b"vertex",
                ShaderStage::Fragment => b"fragment",
            },
        );
        hash_field(
            &mut hasher,
            match shader_override.frontend {
                ShaderFrontend::NativeSlang2025 => b"native-slang-2025",
                ShaderFrontend::GlslViaSlang => b"glsl-via-slang",
            },
        );
        for define in shader_override.defines {
            hash_field(&mut hasher, define.as_bytes());
        }
        hash_field(
            &mut hasher,
            slang_build_tag
                .expect("Slang overrides require a compiler build tag")
                .as_bytes(),
        );
    } else {
        hash_field(&mut hasher, b"shaderc-glsl");
    }

    hasher.finalize().to_hex().to_string()
}

fn artifact_cache_is_current(
    cache_path: &Path,
    reflection_path: &Path,
    optimized_path: &Path,
    cache_context: &str,
) -> bool {
    let Ok(manifest) = fs::read_to_string(cache_path) else {
        return false;
    };
    let mut lines = manifest.lines();
    if lines.next() != Some(ARTIFACT_CACHE_VERSION) {
        return false;
    }
    if lines
        .next()
        .and_then(|line| line.strip_prefix("context="))
        != Some(cache_context)
    {
        return false;
    }
    let Some(expected_dependency_digest) = lines
        .next()
        .and_then(|line| line.strip_prefix("dependencies="))
    else {
        return false;
    };
    let Some(expected_reflection_digest) = lines
        .next()
        .and_then(|line| line.strip_prefix("reflection="))
    else {
        return false;
    };
    let Some(expected_optimized_digest) = lines
        .next()
        .and_then(|line| line.strip_prefix("optimized="))
    else {
        return false;
    };
    let mut dependencies = BTreeSet::new();
    for line in lines {
        let Some(path) = line.strip_prefix("dependency=") else {
            return false;
        };
        dependencies.insert(PathBuf::from(path));
    }
    if dependencies.is_empty() {
        return false;
    }

    dependency_digest(&dependencies).as_deref() == Some(expected_dependency_digest)
        && file_digest(reflection_path).as_deref() == Some(expected_reflection_digest)
        && file_digest(optimized_path).as_deref() == Some(expected_optimized_digest)
}

fn write_artifact_cache(
    cache_path: &Path,
    reflection_path: &Path,
    optimized_path: &Path,
    cache_context: &str,
    dependencies: &BTreeSet<PathBuf>,
) {
    let dependency_digest = dependency_digest(dependencies)
        .unwrap_or_else(|| panic!("hash dependencies for {}", cache_path.display()));
    let reflection_digest = file_digest(reflection_path)
        .unwrap_or_else(|| panic!("hash {}", reflection_path.display()));
    let optimized_digest = file_digest(optimized_path)
        .unwrap_or_else(|| panic!("hash {}", optimized_path.display()));
    let mut manifest = format!(
        "{ARTIFACT_CACHE_VERSION}\ncontext={cache_context}\ndependencies={dependency_digest}\nreflection={reflection_digest}\noptimized={optimized_digest}\n"
    );
    for dependency in dependencies {
        let dependency = path_with_forward_slashes(dependency);
        assert!(
            !dependency.contains(['\n', '\r']),
            "shader dependency path contains a newline: {dependency:?}",
        );
        manifest.push_str("dependency=");
        manifest.push_str(&dependency);
        manifest.push('\n');
    }
    write_if_changed(cache_path, manifest.as_bytes());
}

fn dependency_digest(dependencies: &BTreeSet<PathBuf>) -> Option<String> {
    let mut hasher = blake3::Hasher::new();
    for dependency in dependencies {
        let dependency = fs::canonicalize(dependency).ok()?;
        let contents = fs::read(&dependency).ok()?;
        hash_field(
            &mut hasher,
            path_with_forward_slashes(&dependency).as_bytes(),
        );
        hash_field(&mut hasher, &contents);
    }
    Some(hasher.finalize().to_hex().to_string())
}

fn file_digest(path: &Path) -> Option<String> {
    fs::read(path)
        .ok()
        .map(|contents| blake3::hash(&contents).to_hex().to_string())
}

fn hash_field(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value);
}

fn canonical_dependency(path: &Path) -> PathBuf {
    fs::canonicalize(path)
        .unwrap_or_else(|error| panic!("resolve shader dependency {}: {error}", path.display()))
}

fn remove_cache_marker(path: &Path) {
    if let Err(error) = fs::remove_file(path) {
        assert_eq!(
            error.kind(),
            std::io::ErrorKind::NotFound,
            "remove stale shader cache marker {}: {error}",
            path.display(),
        );
    }
}

fn write_if_changed(path: &Path, contents: &[u8]) {
    if fs::read(path).is_ok_and(|existing| existing == contents) {
        return;
    }
    fs::write(path, contents).unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
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
        .map(|member| {
            // Slang represents imported fixed arrays through a one-member
            // `_Array_*` wrapper struct. SPIRV-Reflect therefore leaves the
            // outer block variable's array traits empty even though its nested
            // `data` member retains the real dimensions and stride. Normalize
            // that frontend-specific shape without treating runtime arrays as
            // fixed or ignoring their byte layout.
            let is_slang_array_wrapper = member
                .type_description
                .as_ref()
                .is_some_and(|description| description.type_name.starts_with("_Array_"));
            let nested_array = match member.members.as_slice() {
                [nested]
                    if is_slang_array_wrapper
                        && nested.offset == 0
                        && nested.size == member.size
                        && !nested.array.dims.is_empty()
                        && nested.array.dims.iter().all(|dimension| *dimension != 0) =>
                {
                    Some(&nested.array)
                }
                _ => None,
            };
            let reflected_array = if member.array.dims.is_empty() {
                nested_array
            } else {
                Some(&member.array)
            };
            let (array_dims, array_stride) = reflected_array
                .map(|array| (array.dims.clone(), array.stride))
                .unwrap_or_else(|| (Vec::new(), 0));

            BlockMemberAbi {
                offset: member.offset,
                size: member.size,
                padded_size: member.padded_size,
                array_dims,
                array_stride,
            }
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

// Load Slang dynamically so the default GLSL build neither links nor locates it.
// The flat C request API keeps all compile requests under one global session;
// keep its manually declared ABI centralized here until the compiler version is pinned.
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
type SlangAddPreprocessorDefine = unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char);
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
    add_preprocessor_define: SlangAddPreprocessorDefine,
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
            add_preprocessor_define: load_slang_symbol(&library, b"spAddPreprocessorDefine\0"),
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

struct SlangCompiler {
    api: SlangApi,
    session: *mut c_void,
    build_tag: String,
}

impl SlangCompiler {
    fn new() -> Self {
        let library_path = find_slang_library();
        let api = unsafe { SlangApi::load(&library_path) };
        let desc = SlangGlobalSessionDesc {
            structure_size: std::mem::size_of::<SlangGlobalSessionDesc>() as u32,
            api_version: 0,
            min_language_version: 2025,
            enable_glsl: true,
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

    fn build_tag(&self) -> &str {
        &self.build_tag
    }

    fn compile_shader(
        &self,
        config: &ShaderOverride,
        project_root: &Path,
        optimization_level: OptimizationLevel,
    ) -> CompiledShader {
        const SLANG_SPIRV: i32 = 6;
        const SLANG_SOURCE_LANGUAGE_SLANG: i32 = 1;
        const SLANG_SOURCE_LANGUAGE_GLSL: i32 = 3;
        const SLANG_MATRIX_LAYOUT_ROW_MAJOR: u32 = 1;
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
            other => panic!("unsupported Slang optimization level: {other:?}"),
        };
        let (source_language, matrix_layout, frontend_options) = match config.frontend {
            ShaderFrontend::NativeSlang2025 => (
                SLANG_SOURCE_LANGUAGE_SLANG,
                SLANG_MATRIX_LAYOUT_COLUMN_MAJOR,
                &["-fvk-use-gl-layout", "-std", "2025"][..],
            ),
            ShaderFrontend::GlslViaSlang => (
                SLANG_SOURCE_LANGUAGE_GLSL,
                SLANG_MATRIX_LAYOUT_ROW_MAJOR,
                &["-fvk-use-gl-layout", "-allow-glsl"][..],
            ),
        };

        unsafe {
            (self.api.set_code_gen_target)(request_ptr, SLANG_SPIRV);
            (self.api.set_target_profile)(request_ptr, 0, profile);
            (self.api.set_matrix_layout_mode)(request_ptr, matrix_layout);
            (self.api.set_optimization_level)(request_ptr, optimization);
        }

        let include_path = path_to_c_string(&project_root.join(config.include_path));
        unsafe { (self.api.add_search_path)(request_ptr, include_path.as_ptr()) };
        let empty_define_value = CString::new("").expect("valid empty Slang define value");
        for define in config.defines {
            let define = CString::new(*define).expect("Slang define must not contain a null byte");
            unsafe {
                (self.api.add_preprocessor_define)(
                    request_ptr,
                    define.as_ptr(),
                    empty_define_value.as_ptr(),
                );
            }
        }

        let mut options = frontend_options.to_vec();
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

        CompiledShader {
            spirv: SlangOwnedBlob(blob).to_vec(),
            dependencies,
        }
    }
}

impl Drop for SlangCompiler {
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
) -> CompiledShader {
    let dependencies = RefCell::new(BTreeSet::new());
    let mut options = CompileOptions::new().expect("create shader compile options");
    options.set_target_env(shaderc::TargetEnv::Vulkan, EnvVersion::Vulkan1_3 as u32);
    options.set_target_spirv(SpirvVersion::V1_6);
    options.set_source_language(shaderc::SourceLanguage::GLSL);
    options.set_optimization_level(optimization_level);
    options.set_include_callback(|requested_source, include_type, requesting_source, include_depth| {
        let resolved = resolve_include(
            requested_source,
            include_type,
            requesting_source,
            include_depth,
        );
        if let Ok(resolved) = &resolved {
            dependencies
                .borrow_mut()
                .insert(canonical_dependency(Path::new(&resolved.resolved_name)));
        }
        resolved
    });

    let spirv = compiler
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
        .to_vec();
    drop(options);
    let mut dependencies = dependencies.into_inner();
    dependencies.insert(canonical_dependency(shader_path));

    CompiledShader {
        spirv,
        dependencies,
    }
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

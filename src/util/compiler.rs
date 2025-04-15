use crate::util::get_full_path_to_dir;
use shaderc::{CompileOptions, Compiler, OptimizationLevel};
use std::sync::Mutex;

/// Global cache to store the requesting source when include_depth is 1.
static REQUESTING_SOURCE_CACHE: Mutex<Option<String>> = Mutex::new(None);

#[allow(unused)]
pub struct ShaderCompiler<'a> {
    compiler: Compiler,
    default_options: CompileOptions<'a>,
}

fn custom_include_callback(
    requested_source: &str,
    _include_type: shaderc::IncludeType,
    requesting_source: &str,
    include_depth: usize,
) -> Result<shaderc::ResolvedInclude, String> {
    // when include_depth is 1, cache the requesting source
    if include_depth == 1 {
        let mut cache = REQUESTING_SOURCE_CACHE
            .lock()
            .map_err(|_| "Mutex poisoned".to_string())?;
        *cache = Some(requesting_source.to_string());
    }
    // when include_depth > 1, try to override the requesting source using the cached value
    else if include_depth > 1 {
        let cache = REQUESTING_SOURCE_CACHE
            .lock()
            .map_err(|_| "Mutex poisoned".to_string())?;
        if let Some(ref cached_source) = *cache {
            let full_path_to_dir = get_full_path_to_dir(cached_source);
            let concated_path = full_path_to_dir.to_string() + requested_source;
            let source = std::fs::read_to_string(&concated_path).map_err(|e| e.to_string())?;
            return Ok(shaderc::ResolvedInclude {
                resolved_name: requested_source.to_string(),
                content: source,
            });
        } else {
            log::error!(
                "No cached requesting source available for depth > 1, using current requesting source."
            );
        }
    }

    // fallback: use the provided requesting_source if no cached source is available
    let full_path_to_dir = get_full_path_to_dir(requesting_source);
    let concated_path = full_path_to_dir.to_string() + requested_source;
    let source = std::fs::read_to_string(&concated_path).map_err(|e| e.to_string())?;
    Ok(shaderc::ResolvedInclude {
        resolved_name: requested_source.to_string(),
        content: source,
    })
}

#[allow(unused)]
impl<'a> ShaderCompiler<'a> {
    pub fn new() -> Result<Self, String> {
        let compiler = Compiler::new().ok_or("Failed to create shader compiler")?;
        let mut default_options =
            CompileOptions::new().ok_or("Failed to create compile options")?;
        default_options.set_target_env(
            shaderc::TargetEnv::Vulkan,
            shaderc::EnvVersion::Vulkan1_3 as u32,
        );
        default_options.set_target_spirv(shaderc::SpirvVersion::V1_6);
        default_options.set_source_language(shaderc::SourceLanguage::GLSL);
        default_options.set_include_callback(custom_include_callback);

        Ok(Self {
            compiler,
            default_options,
        })
    }

    pub fn compile_to_bytecode(
        &self,
        code: &str,
        shader_kind: shaderc::ShaderKind,
        entry_point_name: &str,
        full_path_to_shader_file: &str,
        optimization_level: OptimizationLevel,
    ) -> Result<Vec<u8>, String> {
        let mut compile_options = self.default_options.clone().unwrap();
        compile_options.set_optimization_level(optimization_level);

        let compilation_artifact = self
            .compiler
            .compile_into_spirv(
                code,
                shader_kind,
                full_path_to_shader_file,
                entry_point_name,
                Some(&compile_options),
            )
            .map_err(|e| e.to_string())?;
        Ok(compilation_artifact.as_binary_u8().into())
    }
}

use shaderc::{CompileOptions, Compiler, OptimizationLevel};
use std::path::{Path, PathBuf};

#[allow(unused)]
pub struct ShaderCompiler<'a> {
    compiler: Compiler,
    default_options: CompileOptions<'a>,
}

fn custom_include_callback(
    requested_source: &str,
    include_type: shaderc::IncludeType,
    requesting_source: &str,
    _include_depth: usize,
) -> Result<shaderc::ResolvedInclude, String> {
    let base_dir = get_base_dir(include_type, requesting_source)?;

    // create absolute path and normalise "..", ".", symlinks, …
    let full_path = base_dir
        .join(requested_source)
        .canonicalize() // -> absolute, OS-native separators
        .map_err(|e| format!("{}: {}", requested_source, e))?;

    let content = std::fs::read_to_string(&full_path)
        .map_err(|e| format!("{}: {}", full_path.display(), e))?;

    return Ok(shaderc::ResolvedInclude {
        resolved_name: full_path.to_string_lossy().into_owned(),
        content,
    });

    fn get_base_dir(
        include_type: shaderc::IncludeType,
        requesting_source: &str,
    ) -> Result<PathBuf, String> {
        match include_type {
            shaderc::IncludeType::Relative => Ok(Path::new(requesting_source)
                .parent()
                .ok_or_else(|| format!("`{requesting_source}` has no parent directory"))?
                .to_owned()),
            shaderc::IncludeType::Standard => {
                Err("Standard include not supported for now".to_string())
            }
        }
    }
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

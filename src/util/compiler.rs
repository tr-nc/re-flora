use shaderc::{CompileOptions, Compiler, OptimizationLevel};


#[allow(unused)]
pub struct ShaderCompiler<'a> {
    compiler: Compiler,
    default_options: CompileOptions<'a>,
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
        default_options.set_source_language(shaderc::SourceLanguage::GLSL);

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
        file_name: &str,
        optimization_level: OptimizationLevel,
    ) -> Result<Vec<u8>, String> {
        let mut compile_options = self.default_options.clone().unwrap();
        compile_options.set_optimization_level(optimization_level);

        let compilation_artifact = self
            .compiler
            .compile_into_spirv(
                code,
                shader_kind,
                file_name,
                entry_point_name,
                Some(&compile_options),
            )
            .map_err(|e| e.to_string())?;
        Ok(compilation_artifact.as_binary_u8().into())
    }
}

use shaderc::{CompileOptions, Compiler, OptimizationLevel};

pub struct ShaderCompilerDesc {
    pub optimization_level: OptimizationLevel,
}

impl Default for ShaderCompilerDesc {
    fn default() -> Self {
        Self {
            optimization_level: OptimizationLevel::Performance,
        }
    }
}

#[allow(unused)]
pub struct ShaderCompiler {
    compiler: Compiler,
    compile_options: CompileOptions<'static>,
}

#[allow(unused)]
impl ShaderCompiler {
    pub fn new(create_info: ShaderCompilerDesc) -> Result<Self, String> {
        let compiler = Compiler::new().ok_or("Failed to create shader compiler")?;
        let mut compile_options =
            CompileOptions::new().ok_or("Failed to create compile options")?;
        compile_options.set_optimization_level(create_info.optimization_level);

        Ok(Self {
            compiler,
            compile_options,
        })
    }

    pub fn code_to_bytecode(
        &self,
        code: &str,
        shader_kind: shaderc::ShaderKind,
        file_name: &str,
    ) -> Result<Vec<u8>, String> {
        let compilation_artifact = self
            .compiler
            .compile_into_spirv(
                code,
                shader_kind,
                file_name,
                "main",
                Some(&self.compile_options),
            )
            .map_err(|e| e.to_string())?;
        Ok(compilation_artifact.as_binary_u8().into())
    }
}

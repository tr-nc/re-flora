use ash::vk::{ShaderModule, ShaderModuleCreateInfo};
use shaderc::{CompileOptions, Compiler, OptimizationLevel};
use spirv_reflect::ShaderModule as ReflectShaderModule;

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

pub struct ShaderCompiler {
    compiler: Compiler,
    compile_options: CompileOptions<'static>,
}

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

    /// Compiles a shader from a file.
    ///
    /// # Arguments
    /// * `device` - The Vulkan device.
    /// * `shader_path` - The path to the shader file.
    /// * `shader_kind` - The kind of shader.
    pub fn compile_from_path(
        &self,
        device: &ash::Device,
        shader_path: &str,
        shader_kind: shaderc::ShaderKind,
    ) -> Result<ShaderModule, String> {
        let code = std::fs::read_to_string(shader_path).map_err(|e| e.to_string())?;
        self.compile_from_code(device, &code, shader_kind, shader_path)
    }

    /// Compiles a shader from source code.
    ///
    /// # Arguments
    /// * `device` - The Vulkan device.
    /// * `code` - The source code of the shader.
    /// * `shader_kind` - The kind of shader.
    /// * `file_name` - The name of the file, only used for messages.
    pub fn compile_from_code(
        &self,
        device: &ash::Device,
        code: &str,
        shader_kind: shaderc::ShaderKind,
        file_name: &str,
    ) -> Result<ShaderModule, String> {
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

        let shader_module_create_info =
            ShaderModuleCreateInfo::default().code(&compilation_artifact.as_binary());

        let shader_module = unsafe {
            device
                .create_shader_module(&shader_module_create_info, None)
                .map_err(|e| e.to_string())?
        };

        Ok(shader_module)
    }
}

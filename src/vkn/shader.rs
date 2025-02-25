use crate::util::compiler::ShaderCompiler;

use super::Device;
use ash::vk::ShaderModuleCreateInfo;
use spirv_reflect::ShaderModule as ReflectShaderModule;

pub struct ShaderModule {
    shader_module: ash::vk::ShaderModule,
    reflect_shader_module: ReflectShaderModule,

    device: Device,
}

impl Drop for ShaderModule {
    fn drop(&mut self) {
        unsafe {
            self.device
                .as_raw()
                .destroy_shader_module(self.shader_module, None);
        }
    }
}

impl ShaderModule {
    pub fn get_shader_module(&self) -> ash::vk::ShaderModule {
        self.shader_module
    }

    #[allow(unused)]
    pub fn from_glsl(
        device: &Device,
        file_path: &str,
        compiler: &ShaderCompiler,
    ) -> Result<Self, String> {
        let code = std::fs::read_to_string(format!("{}{}", env!("PROJECT_ROOT"), file_path))
            .map_err(|e| e.to_string())?;

        let shader_kind = predict_shader_kind(file_path).map_err(|e| e.to_string())?;

        let code = get_code_from_path(file_path)?;

        let shader_byte_code_u8 = compiler
            .compile_to_bytecode(&code, shader_kind, file_path)
            .map_err(|e| e.to_string())?;

        let reflect_shader_module =
            ReflectShaderModule::load_u8_data(&shader_byte_code_u8).map_err(|e| e.to_string())?;

        let shader_module = bytecode_to_shader_module(device.as_raw(), &shader_byte_code_u8)?;

        Ok(Self {
            shader_module,
            reflect_shader_module,
            device: device.clone(),
        })
    }

    #[allow(unused)]
    pub fn from_spv(device: &Device, byte_code: &[u8]) -> Result<Self, String> {
        // Reflect the shader module
        let reflect_shader_module =
            ReflectShaderModule::load_u8_data(byte_code).map_err(|e| e.to_string())?;

        // Convert bytecode to shader module
        let shader_module = bytecode_to_shader_module(device.as_raw(), byte_code)?;

        Ok(Self {
            shader_module,
            reflect_shader_module,
            device: device.clone(),
        })
    }
}

fn predict_shader_kind(file_path: &str) -> Result<shaderc::ShaderKind, String> {
    match file_path.split('.').last() {
        Some("vert") => Ok(shaderc::ShaderKind::Vertex),
        Some("frag") => Ok(shaderc::ShaderKind::Fragment),
        Some("comp") => Ok(shaderc::ShaderKind::Compute),
        _ => Err(format!("Unknown shader extension: {}", file_path)),
    }
}

fn bytecode_to_shader_module(
    device: &ash::Device,
    shader_byte_code: &[u8],
) -> Result<ash::vk::ShaderModule, String> {
    let shader_byte_code_u32 = u8_to_u32(shader_byte_code);
    let shader_module_create_info = ShaderModuleCreateInfo::default().code(&shader_byte_code_u32);
    unsafe {
        device
            .create_shader_module(&shader_module_create_info, None)
            .map_err(|e| e.to_string())
    }
}

fn u8_to_u32(byte_code: &[u8]) -> Vec<u32> {
    byte_code
        .chunks_exact(4)
        .map(|chunk| {
            let mut bytes = [0u8; 4];
            bytes.copy_from_slice(chunk);
            u32::from_ne_bytes(bytes)
        })
        .collect()
}

fn load_reflect_shader_module(shader_path: &str) -> Result<ReflectShaderModule, String> {
    let out_dir = env!("OUT_DIR");

    // load from out_dir/shader.vert.spv
    let res = ReflectShaderModule::load_u8_data(
        &std::fs::read(format!("{}/{}", out_dir, shader_path)).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    Ok(res)
}

fn get_code_from_path(shader_path: &str) -> Result<String, String> {
    std::fs::read_to_string(shader_path).map_err(|e| e.to_string())
}

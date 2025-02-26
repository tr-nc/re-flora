use std::ffi::CString;

use crate::util::compiler::ShaderCompiler;

use super::Device;
use ash::vk::{ShaderModuleCreateInfo, ShaderStageFlags};
use shaderc::ShaderKind;
use spirv_reflect::ShaderModule as ReflectShaderModule;

pub struct ShaderModule {
    device: ash::Device,
    entry_point_name: CString,
    shader_module: ash::vk::ShaderModule,
    reflect_shader_module: ReflectShaderModule,
}

impl Drop for ShaderModule {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_shader_module(self.shader_module, None);
        }
    }
}

impl ShaderModule {
    /// Create a new shader module from GLSL code
    ///
    /// # Arguments
    ///
    /// * `device` - The device to create the shader module on
    /// * `file_path` - The path to the GLSL file, from the project root
    /// * `compiler` - The shader compiler to use
    pub fn from_glsl(
        device: &Device,
        file_path: &str,
        entry_point_name: &str,
        compiler: &ShaderCompiler,
    ) -> Result<Self, String> {
        let full_path = format!("{}{}", env!("PROJECT_ROOT"), file_path);
        let code = read_code_from_path(&full_path)?;
        let shader_kind = predict_shader_kind(file_path)
            .map_err(|e| e.to_string())
            .unwrap();

        Self::from_glsl_code(
            &device.as_raw(),
            &code,
            &Self::get_file_name_from_path(file_path),
            entry_point_name,
            compiler,
            shader_kind,
        )
    }

    fn get_file_name_from_path(file_path: &str) -> String {
        file_path.split('/').last().unwrap().to_string()
    }

    /// Core code for creating a shader module from GLSL code
    fn from_glsl_code(
        device: &ash::Device,
        code: &str,
        file_name: &str,
        entry_point_name: &str,
        compiler: &ShaderCompiler,
        shader_kind: ShaderKind,
    ) -> Result<Self, String> {
        let shader_byte_code_u8 = compiler
            .compile_to_bytecode(&code, shader_kind, entry_point_name, file_name)
            .map_err(|e| e.to_string())?;

        let reflect_shader_module =
            ReflectShaderModule::load_u8_data(&shader_byte_code_u8).map_err(|e| e.to_string())?;
        let shader_module = bytecode_to_shader_module(device, &shader_byte_code_u8)?;

        Ok(Self {
            device: device.clone(),
            entry_point_name: CString::new(entry_point_name).unwrap(),
            shader_module,
            reflect_shader_module,
        })
    }

    pub fn get_shader_module(&self) -> ash::vk::ShaderModule {
        self.shader_module
    }

    pub fn get_shader_stage_create_info(&self) -> ash::vk::PipelineShaderStageCreateInfo {
        let info = ash::vk::PipelineShaderStageCreateInfo::default()
            .stage(ShaderStageFlags::from_raw(
                self.reflect_shader_module.get_shader_stage().bits(),
            ))
            .module(self.get_shader_module())
            .name(&self.entry_point_name);
        info
    }

    // preserved for future use
    // pub fn from_spv(device: &Device, byte_code: &[u8]) -> Result<Self, String> {
    //     // Reflect the shader module
    //     let reflect_shader_module =
    //         ReflectShaderModule::load_u8_data(byte_code).map_err(|e| e.to_string())?;

    //     // Convert bytecode to shader module
    //     let shader_module = bytecode_to_shader_module(device.as_raw(), byte_code)?;

    //     Ok(Self {
    //         shader_module,
    //         reflect_shader_module,
    //         device: device.as_raw().clone(),
    //     })
    // }
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

fn read_code_from_path(full_shader_path: &str) -> Result<String, String> {
    std::fs::read_to_string(full_shader_path).map_err(|e| e.to_string())
}

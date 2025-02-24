use ash::{
    vk::{ShaderModule, ShaderModuleCreateInfo},
    Device,
};
use spirv_reflect::ShaderModule as ReflectShaderModule;

use super::compiler::ShaderCompiler;

pub struct LoadedShader {
    pub shader_module: ShaderModule,
    pub reflect_shader_module: ReflectShaderModule,
}

#[allow(unused)]
fn predict_shader_kind(file_path: &str) -> Result<shaderc::ShaderKind, String> {
    match file_path.split('.').last() {
        Some("vert") => Ok(shaderc::ShaderKind::Vertex),
        Some("frag") => Ok(shaderc::ShaderKind::Fragment),
        Some("comp") => Ok(shaderc::ShaderKind::Compute),
        _ => Err(format!("Unknown shader extension: {}", file_path)),
    }
}

#[allow(unused)]
pub fn load_from_glsl(
    file_path: &str,
    device: Device,
    compiler: &ShaderCompiler,
) -> Result<LoadedShader, String> {
    let code = std::fs::read_to_string(format!("{}{}", env!("PROJECT_ROOT"), file_path))
        .map_err(|e| e.to_string())?;

    let shader_kind = predict_shader_kind(file_path).map_err(|e| e.to_string())?;

    let code = get_code_from_path(file_path)?;

    let shader_byte_code_u8 = compiler
        .compile_to_bytecode(&code, shader_kind, file_path)
        .map_err(|e| e.to_string())?;

    let reflect_shader_module =
        ReflectShaderModule::load_u8_data(&shader_byte_code_u8).map_err(|e| e.to_string())?;

    let shader_module = bytecode_to_shader_module(&device, &shader_byte_code_u8)?;

    Ok(LoadedShader {
        shader_module,
        reflect_shader_module,
    })
}

#[allow(unused)]
pub fn load_from_spv(byte_code: &[u8], device: Device) -> Result<LoadedShader, String> {
    // Reflect the shader module
    let reflect_shader_module =
        ReflectShaderModule::load_u8_data(byte_code).map_err(|e| e.to_string())?;

    // Convert bytecode to shader module
    let shader_module = bytecode_to_shader_module(&device, byte_code)?;

    Ok(LoadedShader {
        shader_module,
        reflect_shader_module,
    })
}

fn bytecode_to_shader_module(
    device: &Device,
    shader_byte_code: &[u8],
) -> Result<ShaderModule, String> {
    let shader_byte_code_u32 = u8_to_u32(shader_byte_code);
    let shader_module_create_info = ShaderModuleCreateInfo::default().code(&shader_byte_code_u32);
    unsafe {
        device
            .create_shader_module(&shader_module_create_info, None)
            .map_err(|e| e.to_string())
    }
}

// TODO: Check correctness!
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

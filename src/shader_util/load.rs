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

pub fn load_from_glsl(
    file_path: &str,
    device: Device,
    shader_kind: shaderc::ShaderKind,
    compiler: &ShaderCompiler,
) -> Result<LoadedShader, String> {
    let code = get_code_from_path(file_path)?;
    let shader_byte_code_u8 = compiler
        .code_to_bytecode(&code, shader_kind, file_path)
        .map_err(|e| e.to_string())?;

    let reflect_shader_module =
        ReflectShaderModule::load_u8_data(&shader_byte_code_u8).map_err(|e| e.to_string())?;

    let shader_module = bytecode_to_shader_module(&device, &shader_byte_code_u8)?;

    Ok(LoadedShader {
        shader_module,
        reflect_shader_module,
    })
}

pub fn load_from_spv(file_path: &str, device: Device) -> Result<LoadedShader, String> {
    let out_dir = env!("OUT_DIR");

    let shader_byte_code_u8 =
        &std::fs::read(format!("{}/{}", out_dir, file_path)).map_err(|e| e.to_string())?;

    let reflect_shader_module =
        ReflectShaderModule::load_u8_data(&shader_byte_code_u8).map_err(|e| e.to_string())?;

    let shader_module = bytecode_to_shader_module(&device, &shader_byte_code_u8)?;

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

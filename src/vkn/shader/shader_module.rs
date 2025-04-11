use super::struct_layout::{StructLayout, StructMember};
use crate::{
    util::{compiler::ShaderCompiler, full_path_from_relative},
    vkn::{DescriptorSetLayout, DescriptorSetLayoutBinding, DescriptorSetLayoutBuilder, Device},
};
use ash::vk::{self, PushConstantRange};
use shaderc::ShaderKind;
use spirv_reflect::{
    types::{ReflectDescriptorSet, ReflectDescriptorType, ReflectTypeFlags},
    ShaderModule as ReflectShaderModule,
};
use std::{collections::HashMap, ffi::CString, fmt::Debug, sync::Arc};

/// Internal struct holding the actual Vulkan `ShaderModule` and reflection data.
struct ShaderModuleInner {
    device: Device,
    entry_point_name: CString,
    shader_module: ash::vk::ShaderModule,
    reflect_shader_module: ReflectShaderModule,

    struct_layouts: HashMap<String, StructLayout>,
}

impl Drop for ShaderModuleInner {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_shader_module(self.shader_module, None);
        }
    }
}

#[derive(Clone)]
pub struct ShaderModule(Arc<ShaderModuleInner>);

impl std::ops::Deref for ShaderModule {
    type Target = ash::vk::ShaderModule;
    fn deref(&self) -> &Self::Target {
        &self.0.shader_module
    }
}

impl Debug for ShaderModule {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let descriptor_bindings = self
            .0
            .reflect_shader_module
            .enumerate_descriptor_bindings(None)
            .unwrap();
        for binding in descriptor_bindings {
            log::debug!("binding: {:#?}", binding);
        }
        Ok(())
    }
}

impl ShaderModule {
    /// Create a new shader module from GLSL code, reflect it, and cache relevant layout metadata.
    ///
    /// * `device` - The device to create the shader module on.
    /// * `compiler` - The shader compiler to use.
    /// * `file_path` - The relative path to the GLSL file, from the project root.
    /// * `entry_point_name` - The name of the entry point function in the shader.
    pub fn from_glsl(
        device: &Device,
        compiler: &ShaderCompiler,
        file_path: &str,
        entry_point_name: &str,
    ) -> Result<Self, String> {
        let full_path = full_path_from_relative(file_path);
        let code = read_code_from_path(&full_path)?;
        let shader_kind = predict_shader_kind(file_path).map_err(|e| e.to_string())?;

        Self::from_glsl_code(
            device,
            &code,
            &full_path,
            entry_point_name,
            compiler,
            shader_kind,
        )
    }

    pub fn get_buffer_layout(&self, name: &str) -> Result<&StructLayout, String> {
        self.0
            .struct_layouts
            .get(name)
            .ok_or_else(|| format!("Buffer layout not found for name: {}", name))
    }

    /// Retrieve the workgroup size (for compute shaders).
    pub fn get_workgroup_size(&self) -> Result<[u32; 3], String> {
        let entry_points = self
            .0
            .reflect_shader_module
            .enumerate_entry_points()
            .unwrap();

        if entry_points.len() != 1 {
            return Err("Multiple entry points found".to_string());
        }

        let entry_point = entry_points.first().unwrap();
        let local_size = entry_point.local_size;
        Ok([local_size.x, local_size.y, local_size.z])
    }

    /// Returns the Vulkan stage flags for this shader.
    pub fn get_stage(&self) -> vk::ShaderStageFlags {
        vk::ShaderStageFlags::from_raw(self.0.reflect_shader_module.get_shader_stage().bits())
    }

    /// Returns the underlying Vulkan `ash::vk::ShaderModule`.
    pub fn get_shader_module(&self) -> ash::vk::ShaderModule {
        self.0.shader_module
    }

    /// Convenience for creating a stage create info (for pipeline creation).
    pub fn get_shader_stage_create_info(&self) -> ash::vk::PipelineShaderStageCreateInfo {
        ash::vk::PipelineShaderStageCreateInfo::default()
            .stage(self.get_stage())
            .module(self.get_shader_module())
            .name(&self.0.entry_point_name)
    }

    fn from_glsl_code(
        device: &Device,
        code: &str,
        full_path_to_shader_file: &str,
        entry_point_name: &str,
        compiler: &ShaderCompiler,
        shader_kind: ShaderKind,
    ) -> Result<Self, String> {
        let reflect_sm = create_reflect_shader_module(
            code,
            shader_kind,
            entry_point_name,
            full_path_to_shader_file,
            compiler,
        )?;

        let sm = create_shader_module(
            device,
            code,
            shader_kind,
            entry_point_name,
            full_path_to_shader_file,
            compiler,
        )?;

        let buffer_layouts = extract_struct_layouts(&reflect_sm).map_err(|e| e.to_string())?;

        Ok(Self(Arc::new(ShaderModuleInner {
            device: device.clone(),
            entry_point_name: CString::new(entry_point_name).unwrap(),
            shader_module: sm,
            reflect_shader_module: reflect_sm,
            struct_layouts: buffer_layouts,
        })))
    }

    pub fn get_push_constant_ranges(&self) -> Option<Vec<PushConstantRange>> {
        let push_constant_blocks = self
            .0
            .reflect_shader_module
            .enumerate_push_constant_blocks(None)
            .ok()?;
        if push_constant_blocks.is_empty() {
            return None;
        }

        let mut ranges = Vec::new();
        for block in push_constant_blocks {
            ranges.push(PushConstantRange {
                stage_flags: self.get_stage(),
                offset: block.offset,
                size: block.size,
            });
        }
        Some(ranges)
    }

    fn get_descriptor_sets(&self) -> Vec<Option<ReflectDescriptorSet>> {
        let descriptor_sets = self
            .0
            .reflect_shader_module
            .enumerate_descriptor_sets(None)
            .expect("Failed to enumerate descriptor sets");

        if descriptor_sets.is_empty() {
            return vec![];
        }

        let max_set_no = descriptor_sets.iter().map(|set| set.set).max().unwrap_or(0);

        let mut sets: Vec<Option<ReflectDescriptorSet>> = vec![None; (max_set_no + 1) as usize];

        for set in descriptor_sets {
            let set_no = set.set;
            sets[set_no as usize] = Some(set);
        }

        sets
    }

    pub fn get_descriptor_set_layouts(&self) -> Option<Vec<DescriptorSetLayout>> {
        let descriptor_sets = self.get_descriptor_sets();

        let mut layouts = Vec::new();
        for descriptor_set in descriptor_sets {
            let mut builder = DescriptorSetLayoutBuilder::new();

            // if the descriptor set is valid, add its bindings to the layout
            if let Some(descriptor_set) = descriptor_set {
                for binding in descriptor_set.bindings {
                    let descriptor_type =
                        reflect_descriptor_type_to_descriptor_type(binding.descriptor_type);
                    let stage_flags = self.get_stage();
                    builder.add_binding(DescriptorSetLayoutBinding {
                        no: binding.binding,
                        descriptor_type,
                        descriptor_count: binding.count,
                        stage_flags,
                    });
                }
            }
            layouts.push(builder.build(&self.0.device).unwrap());
        }

        Some(layouts)
    }
}

/// With zero optimization, no unused bindings will be removed, and the names of the
/// variables will be preserved accurately
fn create_reflect_shader_module(
    code: &str,
    shader_kind: ShaderKind,
    entry_point_name: &str,
    full_path_to_shader_file: &str,
    compiler: &ShaderCompiler,
) -> Result<ReflectShaderModule, String> {
    let shader_byte_code_u8_zero_opti = compiler
        .compile_to_bytecode(
            code,
            shader_kind,
            entry_point_name,
            full_path_to_shader_file,
            shaderc::OptimizationLevel::Zero,
        )
        .map_err(|e| e.to_string())?;
    ReflectShaderModule::load_u8_data(&shader_byte_code_u8_zero_opti).map_err(|e| e.to_string())
}

/// Compile the actual shader module with full optimization.
fn create_shader_module(
    device: &Device,
    code: &str,
    shader_kind: ShaderKind,
    entry_point_name: &str,
    full_path_to_shader_file: &str,
    compiler: &ShaderCompiler,
) -> Result<ash::vk::ShaderModule, String> {
    let shader_byte_code_u8_full_opti = compiler
        .compile_to_bytecode(
            code,
            shader_kind,
            entry_point_name,
            full_path_to_shader_file,
            shaderc::OptimizationLevel::Performance,
        )
        .map_err(|e| e.to_string())?;

    bytecode_to_shader_module(device, &shader_byte_code_u8_full_opti)
}

fn read_code_from_path(full_shader_path: &str) -> Result<String, String> {
    std::fs::read_to_string(full_shader_path).map_err(|e| e.to_string())
}

/// A simple extension-based guess of the shader kind (vert, frag, comp).
fn predict_shader_kind(file_path: &str) -> Result<shaderc::ShaderKind, String> {
    match file_path.split('.').last() {
        Some("vert") => Ok(shaderc::ShaderKind::Vertex),
        Some("frag") => Ok(shaderc::ShaderKind::Fragment),
        Some("comp") => Ok(shaderc::ShaderKind::Compute),
        _ => Err(format!("Unknown shader extension: {}", file_path)),
    }
}

/// Convert our SPIR-V bytecode (u8 Vec) into a Vulkan `ShaderModule`.
fn bytecode_to_shader_module(
    device: &Device,
    shader_byte_code: &[u8],
) -> Result<ash::vk::ShaderModule, String> {
    let shader_byte_code_u32 = u8_to_u32(shader_byte_code);
    let shader_module_create_info =
        vk::ShaderModuleCreateInfo::default().code(&shader_byte_code_u32);
    unsafe {
        device
            .create_shader_module(&shader_module_create_info, None)
            .map_err(|e| e.to_string())
    }
}

/// Converts a byte slice (SPIR-V) into a `Vec<u32>` for Vulkan.
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

fn extract_struct_layouts(
    reflect_module: &ReflectShaderModule,
) -> Result<HashMap<String, StructLayout>, String> {
    let descriptor_bindings = match reflect_module.enumerate_descriptor_bindings(None) {
        Ok(db) => db,
        Err(_) => return Err("Failed to enumerate descriptor bindings".to_string()),
    };

    let mut result = HashMap::new();

    for binding in descriptor_bindings {
        if binding.descriptor_type == ReflectDescriptorType::UniformBuffer
            || binding.descriptor_type == ReflectDescriptorType::StorageBuffer
        {
            let block_info = &binding.block;

            let layout_name = if let Some(ty_description) = binding.type_description.as_ref() {
                ty_description.type_name.clone()
            } else {
                return Err("Failed to get layout name".to_string());
            };

            let members = parse_block_members(&block_info.members);

            let layout = StructLayout {
                type_name: layout_name,
                // the following line leads to incorrect answer when parsing storage buffers
                // (uniform buffers are ok), therefore a customized function is used
                // total_size: block_info.padded_size,
                total_size: get_total_size_from_members(&members),
                members,
                descriptor_type: binding.descriptor_type,
            };

            log::debug!("Extracted buffer layout: {:#?}", layout);
            result.insert(layout.type_name.clone(), layout);
        }
    }

    Ok(result)
}

fn get_total_size_from_members(members: &HashMap<String, StructMember>) -> u32 {
    members.values().map(|m| m.padded_size).sum()
}

/// Given an array of `ReflectBlockVariable`, build a list of our `BufferMember`.
fn parse_block_members(
    members: &[spirv_reflect::types::ReflectBlockVariable],
) -> HashMap<String, StructMember> {
    let mut result = HashMap::new();

    for (i, member) in members.iter().enumerate() {
        let member_name = if member.name.is_empty() {
            format!("member_{}", i)
        } else {
            member.name.clone()
        };

        let type_name = guess_type_name(member);

        result.insert(
            member_name.clone(),
            StructMember {
                offset: member.offset,
                size: member.size,
                padded_size: member.padded_size,
                type_name,
                name: member_name,
            },
        );
    }
    result
}

/// Attempt to guess a GLSL-like type name from SPIR-V reflection metadata.
fn guess_type_name(member: &spirv_reflect::types::ReflectBlockVariable) -> String {
    if let Some(type_desc) = &member.type_description {
        let flags = &type_desc.type_flags;
        let numeric = &type_desc.traits.numeric;

        // Matrices
        if flags.contains(ReflectTypeFlags::MATRIX) {
            let cols = numeric.matrix.column_count;
            let rows = numeric.matrix.row_count;
            return match (rows, cols) {
                (4, 4) => "mat4".to_owned(),
                (3, 3) => "mat3".to_owned(),
                (2, 2) => "mat2".to_owned(),
                _ => format!("mat{}x{}", rows, cols),
            };
        }

        // Vectors
        if flags.contains(ReflectTypeFlags::VECTOR) {
            let comp_count = numeric.vector.component_count;
            // Distinguish float-based vs int-based vs uint-based
            let is_float = flags.contains(ReflectTypeFlags::FLOAT);
            let is_int = flags.contains(ReflectTypeFlags::INT);
            let signedness = numeric.scalar.signedness;

            if is_float {
                return match comp_count {
                    2 => "vec2".to_owned(),
                    3 => "vec3".to_owned(),
                    4 => "vec4".to_owned(),
                    _ => format!("vec{}", comp_count),
                };
            } else if is_int {
                // signedness == 1 => ivec..., else uvec...
                if signedness == 1 {
                    return match comp_count {
                        2 => "ivec2".to_owned(),
                        3 => "ivec3".to_owned(),
                        4 => "ivec4".to_owned(),
                        _ => format!("ivec{}", comp_count),
                    };
                } else {
                    return match comp_count {
                        2 => "uvec2".to_owned(),
                        3 => "uvec3".to_owned(),
                        4 => "uvec4".to_owned(),
                        _ => format!("uvec{}", comp_count),
                    };
                }
            }
        }

        // Scalars
        if flags.contains(ReflectTypeFlags::FLOAT) {
            return "float".to_owned();
        }
        if flags.contains(ReflectTypeFlags::INT) {
            // "bool" in GLSL is 32-bit in SPIR-V, typically stored as int.
            let signed = numeric.scalar.signedness;
            return if signed == 1 {
                "int".to_owned()
            } else {
                "uint".to_owned()
            };
        }
    }
    "unknown".to_owned()
}

fn reflect_descriptor_type_to_descriptor_type(
    reflect_type: ReflectDescriptorType,
) -> vk::DescriptorType {
    match reflect_type {
        ReflectDescriptorType::Sampler => vk::DescriptorType::SAMPLER,
        ReflectDescriptorType::CombinedImageSampler => vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        ReflectDescriptorType::SampledImage => vk::DescriptorType::SAMPLED_IMAGE,
        ReflectDescriptorType::StorageImage => vk::DescriptorType::STORAGE_IMAGE,
        ReflectDescriptorType::UniformTexelBuffer => vk::DescriptorType::UNIFORM_TEXEL_BUFFER,
        ReflectDescriptorType::StorageTexelBuffer => vk::DescriptorType::STORAGE_TEXEL_BUFFER,
        ReflectDescriptorType::UniformBuffer => vk::DescriptorType::UNIFORM_BUFFER,
        ReflectDescriptorType::StorageBuffer => vk::DescriptorType::STORAGE_BUFFER,
        ReflectDescriptorType::UniformBufferDynamic => vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC,
        ReflectDescriptorType::StorageBufferDynamic => vk::DescriptorType::STORAGE_BUFFER_DYNAMIC,
        ReflectDescriptorType::InputAttachment => vk::DescriptorType::INPUT_ATTACHMENT,
        ReflectDescriptorType::AccelerationStructureKHR => {
            vk::DescriptorType::ACCELERATION_STRUCTURE_KHR
        }
        _ => panic!("Unsupported descriptor type in reflection."),
    }
}

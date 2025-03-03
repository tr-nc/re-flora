use super::{
    DescriptorSetLayout, DescriptorSetLayoutBinding, DescriptorSetLayoutBuilder, Device,
    PipelineLayout,
};
use crate::util::compiler::ShaderCompiler;
use ash::vk::{self, PushConstantRange};
use shaderc::ShaderKind;
use spirv_reflect::{types::ReflectDescriptorType, ShaderModule as ReflectShaderModule};
use std::ffi::CString;
use std::fmt::Debug;
use std::sync::Arc;

struct ShaderModuleInner {
    device: Device,
    entry_point_name: CString,
    shader_module: ash::vk::ShaderModule,
    reflect_shader_module: ReflectShaderModule,
}

impl Drop for ShaderModuleInner {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_shader_module(self.shader_module, None);
        }
    }
}

/// Represents the detailed layout of a uniform buffer or push constant block
#[derive(Debug, Clone)]
pub struct BufferLayout {
    pub set: u32,
    pub binding: u32,
    pub name: String,
    pub size: u32,
    pub members: Vec<BufferMember>,
    pub descriptor_type: ReflectDescriptorType,
}

/// Represents a single member within a uniform buffer or push constant block
#[derive(Debug, Clone)]
pub struct BufferMember {
    pub name: String,
    pub offset: u32,
    pub size: u32,
    pub type_name: String,
    pub padded_size: u32,
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
        log::debug!("Shader module reflection:");
        log::debug!(
            "  Entry point: {}",
            self.0.reflect_shader_module.get_entry_point_name()
        );
        log::debug!(
            "  Shader stage: {:?}",
            self.0.reflect_shader_module.get_shader_stage()
        );

        let descriptor_sets = self
            .0
            .reflect_shader_module
            .enumerate_descriptor_sets(None)
            .unwrap();
        log::debug!("ds count: {}", descriptor_sets.len());

        for descriptor_set in descriptor_sets {
            log::debug!("  Descriptor set: {}", descriptor_set.set);
            for binding in descriptor_set.bindings {
                log::debug!("    Binding: {}", binding.binding);
                log::debug!("    Descriptor type: {:?}", binding.descriptor_type);
                log::debug!("    Descriptor count: {}", binding.count);
            }
        }

        Ok(())
    }
}

impl ShaderModule {
    /// Create a new shader module from GLSL code
    ///
    /// * `device` - The device to create the shader module on
    /// * `compiler` - The shader compiler to use
    /// * `file_path` - The path to the GLSL file, from the project root
    /// * `entry_point_name` - The name of the entry point function in the shader
    pub fn from_glsl(
        device: &Device,
        compiler: &ShaderCompiler,
        file_path: &str,
        entry_point_name: &str,
    ) -> Result<Self, String> {
        let full_path = format!("{}{}", env!("PROJECT_ROOT"), file_path);
        let code = read_code_from_path(&full_path)?;
        let shader_kind = predict_shader_kind(file_path)
            .map_err(|e| e.to_string())
            .unwrap();

        Self::from_glsl_code(
            &device,
            &code,
            &Self::get_file_name_from_path(file_path),
            entry_point_name,
            compiler,
            shader_kind,
        )
    }

    /// Debug utility to print all bindings in the shader module
    pub fn print_bindings(&self) {
        let descriptor_bindings = self
            .0
            .reflect_shader_module
            .enumerate_descriptor_bindings(None)
            .unwrap();

        for binding in descriptor_bindings {
            log::debug!("binding: {:?}", binding);
        }
    }

    /// Extracts detailed layout information for uniform buffers (or push constant blocks).
    /// Returns a structured representation of all uniform buffer blocks and their members.
    ///
    /// Note: Because SPIR-V reflection does *not* preserve the original variable names,
    /// each member in the reflected block often comes back with an empty string for its name.
    /// Consequently, we'll auto-generate fallback names like `member_0`, `member_1`, etc.
    /// You can adjust this logic as needed for debugging or readability.
    pub fn extract_buffer_layouts(&self) -> Vec<BufferLayout> {
        let descriptor_bindings = self
            .0
            .reflect_shader_module
            .enumerate_descriptor_bindings(None)
            .unwrap();

        let mut result = Vec::new();

        // Loop over every descriptor binding
        for binding in descriptor_bindings {
            // We're only interested in `UniformBuffer` descriptor types
            if binding.descriptor_type == ReflectDescriptorType::UniformBuffer {
                let block_info = &binding.block;

                // Construct our UniformBufferLayout and parse its members
                let layout = BufferLayout {
                    set: binding.set,
                    binding: binding.binding,
                    name: if binding.name.is_empty() {
                        // If the binding name is empty, manufacture a fallback name
                        format!("ubo_set{}_binding{}", binding.set, binding.binding)
                    } else {
                        binding.name.clone()
                    },
                    size: block_info.padded_size, // total size of the entire struct
                    members: parse_block_members(&block_info.members),
                    descriptor_type: binding.descriptor_type,
                };

                result.push(layout);
            }
        }

        result
    }

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

    fn get_file_name_from_path(file_path: &str) -> String {
        file_path.split('/').last().unwrap().to_string()
    }

    fn from_glsl_code(
        device: &Device,
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

        Ok(Self(Arc::new(ShaderModuleInner {
            device: device.clone(),
            entry_point_name: CString::new(entry_point_name).unwrap(),
            shader_module,
            reflect_shader_module,
        })))
    }

    pub fn get_shader_module(&self) -> ash::vk::ShaderModule {
        self.0.shader_module
    }

    pub fn get_shader_stage_create_info(&self) -> ash::vk::PipelineShaderStageCreateInfo {
        let info = ash::vk::PipelineShaderStageCreateInfo::default()
            .stage(self.get_stage())
            .module(self.get_shader_module())
            .name(&self.0.entry_point_name);
        info
    }

    pub fn get_pipeline_layout(&self, device: &Device) -> PipelineLayout {
        let descriptor_set_layouts = self.get_descriptor_set_layouts();
        let push_constant_ranges = self.get_push_constant_ranges();
        PipelineLayout::new(
            device,
            descriptor_set_layouts.as_deref(),
            push_constant_ranges.as_deref(),
        )
    }

    pub fn get_stage(&self) -> vk::ShaderStageFlags {
        vk::ShaderStageFlags::from_raw(self.0.reflect_shader_module.get_shader_stage().bits())
    }

    fn get_push_constant_ranges(&self) -> Option<Vec<PushConstantRange>> {
        let push_constant_ranges = self
            .0
            .reflect_shader_module
            .enumerate_push_constant_blocks(None)
            .unwrap();
        let mut ranges = Vec::new();

        for range in push_constant_ranges {
            let stage_flags = self.get_stage();
            let offset = range.offset;
            let size = range.size;

            ranges.push(PushConstantRange {
                stage_flags,
                offset,
                size,
            });
        }

        if ranges.is_empty() {
            return None;
        }
        Some(ranges)
    }

    fn get_descriptor_set_layouts(&self) -> Option<Vec<DescriptorSetLayout>> {
        let descriptor_sets = self
            .0
            .reflect_shader_module
            .enumerate_descriptor_sets(None)
            .unwrap();

        let mut layouts = Vec::new();

        for descriptor_set in descriptor_sets {
            // let set_no = descriptor_set.set;
            let mut builder = DescriptorSetLayoutBuilder::new();

            for binding in descriptor_set.bindings {
                let binding_no = binding.binding;
                let descriptor_type = binding.descriptor_type;
                let descriptor_count = binding.count;
                let stage_flags = self.get_stage();

                let b = DescriptorSetLayoutBinding {
                    no: binding_no,
                    descriptor_type: Self::reflect_descriptor_type_to_descriptor_type(
                        descriptor_type,
                    ),
                    descriptor_count: descriptor_count,
                    stage_flags: stage_flags,
                };
                builder.add_binding(b);
            }

            layouts.push(builder.build(&self.0.device).unwrap());
        }

        if layouts.is_empty() {
            return None;
        }
        Some(layouts)
    }

    fn reflect_descriptor_type_to_descriptor_type(
        reflect_type: ReflectDescriptorType,
    ) -> vk::DescriptorType {
        use vk::DescriptorType;
        match reflect_type {
            ReflectDescriptorType::Sampler => DescriptorType::SAMPLER,
            ReflectDescriptorType::CombinedImageSampler => DescriptorType::COMBINED_IMAGE_SAMPLER,
            ReflectDescriptorType::SampledImage => DescriptorType::SAMPLED_IMAGE,
            ReflectDescriptorType::StorageImage => DescriptorType::STORAGE_IMAGE,
            ReflectDescriptorType::UniformTexelBuffer => DescriptorType::UNIFORM_TEXEL_BUFFER,
            ReflectDescriptorType::StorageTexelBuffer => DescriptorType::STORAGE_TEXEL_BUFFER,
            ReflectDescriptorType::UniformBuffer => DescriptorType::UNIFORM_BUFFER,
            ReflectDescriptorType::StorageBuffer => DescriptorType::STORAGE_BUFFER,
            ReflectDescriptorType::UniformBufferDynamic => DescriptorType::UNIFORM_BUFFER_DYNAMIC,
            ReflectDescriptorType::StorageBufferDynamic => DescriptorType::STORAGE_BUFFER_DYNAMIC,
            ReflectDescriptorType::InputAttachment => DescriptorType::INPUT_ATTACHMENT,
            ReflectDescriptorType::AccelerationStructureKHR => {
                DescriptorType::ACCELERATION_STRUCTURE_KHR
            }
            _ => panic!(),
        }
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

//

/// Recursively parse an array of `ReflectBlockVariable` into a vector of `UniformBufferMember`.
fn parse_block_members(
    members: &Vec<spirv_reflect::types::ReflectBlockVariable>,
) -> Vec<BufferMember> {
    let mut result = Vec::new();

    for (i, member) in members.iter().enumerate() {
        // If SPIR-V reflection doesn't preserve the name, we fallback to "member_i"
        let member_name = if member.name.is_empty() {
            format!("member_{}", i)
        } else {
            member.name.clone()
        };

        // We'll guess the member's type name by looking at numeric traits, vector size, etc.
        let type_name = guess_type_name(member);

        // Create the UniformBufferMember for this variable
        let ub_member = BufferMember {
            name: member_name,
            offset: member.offset,
            size: member.size,
            padded_size: member.padded_size,
            type_name,
        };

        result.push(ub_member);

        // If the reflection might contain nested structs, you could recurse further;
        // for now, we stop here since members in typical GLSL uniform blocks are “flat.”
    }

    result
}

/// Use the SPIR-V reflection type flags to guess an appropriate GLSL-like type name.
fn guess_type_name(member: &spirv_reflect::types::ReflectBlockVariable) -> String {
    if let Some(type_desc) = &member.type_description {
        // Check if it’s a matrix
        if type_desc
            .type_flags
            .contains(spirv_reflect::types::ReflectTypeFlags::MATRIX)
        {
            // Handle typical matrix sizes like mat2, mat3, mat4
            let cols = type_desc.traits.numeric.matrix.column_count;
            let rows = type_desc.traits.numeric.matrix.row_count;
            return match (rows, cols) {
                (4, 4) => "mat4".to_string(),
                (3, 3) => "mat3".to_string(),
                (2, 2) => "mat2".to_string(),
                // fallback
                _ => format!("mat{}x{}", rows, cols),
            };
        }

        // Check if it’s a vector
        if type_desc
            .type_flags
            .contains(spirv_reflect::types::ReflectTypeFlags::VECTOR)
        {
            let comp_count = type_desc.traits.numeric.vector.component_count;

            // Distinguish float-based vs int-based vs uint-based vectors
            let is_float = type_desc
                .type_flags
                .contains(spirv_reflect::types::ReflectTypeFlags::FLOAT);
            let is_int = type_desc
                .type_flags
                .contains(spirv_reflect::types::ReflectTypeFlags::INT);

            // We'll read `signedness` to guess if it's int vs. uint
            let numeric = &type_desc.traits.numeric;
            let signedness = numeric.scalar.signedness;

            if is_float {
                return match comp_count {
                    2 => "vec2".to_string(),
                    3 => "vec3".to_string(),
                    4 => "vec4".to_string(),
                    _ => format!("vec{}", comp_count),
                };
            } else if is_int {
                // signedness == 1 => ivec, else uvec
                if signedness == 1 {
                    return match comp_count {
                        2 => "ivec2".to_string(),
                        3 => "ivec3".to_string(),
                        4 => "ivec4".to_string(),
                        _ => format!("ivec{}", comp_count),
                    };
                } else {
                    return match comp_count {
                        2 => "uvec2".to_string(),
                        3 => "uvec3".to_string(),
                        4 => "uvec4".to_string(),
                        _ => format!("uvec{}", comp_count),
                    };
                }
            }
        }

        // If it's a scalar
        if type_desc
            .type_flags
            .contains(spirv_reflect::types::ReflectTypeFlags::FLOAT)
        {
            return "float".to_string();
        }
        if type_desc
            .type_flags
            .contains(spirv_reflect::types::ReflectTypeFlags::INT)
        {
            // “bool” in GLSL is 32-bit in SPIR-V, usually stored as int.
            // Without extra clues, we can’t always know if it’s truly a bool or int/uint,
            // but if you know your own shader code, you can special-case it here.
            // For demonstration, we’ll guess:
            let signed = type_desc.traits.numeric.scalar.signedness;
            return if signed == 1 {
                "int".to_string()
            } else {
                "uint".to_string()
            };
        }
    }

    // Fallback name if we still don’t recognize it
    "unknown".to_string()
}

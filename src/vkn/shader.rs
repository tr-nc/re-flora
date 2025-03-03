use super::{
    DescriptorSetLayout, DescriptorSetLayoutBinding, DescriptorSetLayoutBuilder, Device,
    PipelineLayout,
};
use crate::util::compiler::ShaderCompiler;
use ash::vk::{self, PushConstantRange};
use shaderc::ShaderKind;
use spirv_reflect::{
    types::ReflectDescriptorType, types::ReflectTypeFlags, ShaderModule as ReflectShaderModule,
};
use std::{ffi::CString, fmt::Debug, sync::Arc};

/// Internal struct holding the actual Vulkan `ShaderModule` and reflection data.
struct ShaderModuleInner {
    device: Device,
    entry_point_name: CString,
    shader_module: ash::vk::ShaderModule,
    reflect_shader_module: ReflectShaderModule,

    /// Cached buffer layouts, reflected once. (Uniform buffers, push constants, etc.)
    buffer_layouts: Vec<BufferLayout>,
}

impl Drop for ShaderModuleInner {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_shader_module(self.shader_module, None);
        }
    }
}

/// Represents the layout of a uniform buffer or push constant block.
#[derive(Debug, Clone)]
pub struct BufferLayout {
    pub set: u32,
    pub binding: u32,
    pub total_size: u32,
    pub name: String,
    pub members: Vec<BufferMember>,
    pub descriptor_type: ReflectDescriptorType,
}

/// Represents a single member (field) within a uniform buffer or push constant block.
#[derive(Debug, Clone)]
pub struct BufferMember {
    pub offset: u32,
    pub size: u32,
    pub padded_size: u32,
    pub type_name: String,
    pub name: String,
}

/// Public handle to the `ShaderModule`.
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
        log::debug!("Descriptor set count: {}", descriptor_sets.len());

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

/// # New API for building CPU-side data buffers matching a reflected uniform layout.
///
/// You typically construct a `BufferDataBuilder` by calling:
/// ```ignore
/// let builder = BufferDataBuilder::new(&shader_module, set, binding)?;
/// let data = builder
///     .push_f32(3.0)?
///     .push_u32(42)?
///     .push_mat4(mat4_value)?
///     .build();
/// ```
/// This returns a `Vec<u8>` that you can then upload to your uniform buffer.
pub struct BufferDataBuilder<'a> {
    layout: &'a BufferLayout,
    current_member_index: usize,
    data: Vec<u8>,
}

impl<'a> BufferDataBuilder<'a> {
    /// Creates a new builder for the given set and binding.
    pub fn new(shader_module: &'a ShaderModule, set: u32, binding: u32) -> Result<Self, String> {
        let layout = shader_module
            .0
            .buffer_layouts
            .iter()
            .find(|l| l.set == set && l.binding == binding)
            .ok_or_else(|| format!("No BufferLayout found for set={}, binding={}", set, binding))?;

        // Pre-allocate the data vector to at least the total size
        let data = vec![0u8; layout.total_size as usize];
        Ok(BufferDataBuilder {
            layout,
            current_member_index: 0,
            data,
        })
    }

    /// Push a `f32` into the next member if it matches an expected `float`.
    pub fn push_f32(&mut self, value: f32) -> Result<&mut Self, String> {
        self.check_and_write(&value.to_ne_bytes(), &["float"])?;
        Ok(self)
    }

    /// Push a `u32` into the next member if it matches an expected `uint` or `int` (up to you).
    pub fn push_u32(&mut self, value: u32) -> Result<&mut Self, String> {
        // If you want to allow pushing `u32` to "int", add "int" to the list below.
        self.check_and_write(&value.to_ne_bytes(), &["uint"])?;
        Ok(self)
    }

    /// Push a `i32` into the next member if it matches an expected `int`.
    pub fn push_i32(&mut self, value: i32) -> Result<&mut Self, String> {
        self.check_and_write(&value.to_ne_bytes(), &["int"])?;
        Ok(self)
    }

    /// Push a [f32; 4x4] matrix into the next member if it matches an expected `mat4`.
    /// (Using `glam::Mat4` or any library that can produce a `[f32; 16]` is typical.)
    pub fn push_mat4(&mut self, mat: [f32; 16]) -> Result<&mut Self, String> {
        // Convert to bytes
        let mut bytes = Vec::with_capacity(16 * 4);
        for val in mat {
            bytes.extend_from_slice(&val.to_ne_bytes());
        }
        self.check_and_write(&bytes, &["mat4"])?;
        Ok(self)
    }

    /// Push a [f32; 2] vector if it matches a `vec2`, etc.
    pub fn push_vec2(&mut self, v: [f32; 2]) -> Result<&mut Self, String> {
        // Convert to bytes
        let mut bytes = Vec::with_capacity(2 * 4);
        for val in v {
            bytes.extend_from_slice(&val.to_ne_bytes());
        }
        self.check_and_write(&bytes, &["vec2"])?;
        Ok(self)
    }

    /// Once all pushes are done, finalize the data vector.
    pub fn build(self) -> Vec<u8> {
        self.data
    }

    // ---- Private helpers ----

    /// Checks the next member’s type matches one of the `allowed_types` (e.g. ["float"]) and
    /// writes the given bytes to the corresponding offset.
    fn check_and_write(
        &mut self,
        write_bytes: &[u8],
        allowed_types: &[&str],
    ) -> Result<(), String> {
        if self.current_member_index >= self.layout.members.len() {
            return Err("BufferDataBuilder: no more members left to push.".into());
        }

        let member = &self.layout.members[self.current_member_index];
        if !allowed_types.contains(&member.type_name.as_str()) {
            return Err(format!(
                "Type mismatch for member {} (index {}): expected {:?}, found {:?}",
                member.name, self.current_member_index, allowed_types, member.type_name
            ));
        }

        // Check sizes
        if write_bytes.len() > member.size as usize {
            // Potentially we want to allow partial fill or handle padded sizes differently,
            // but for demonstration, we just fail if the user’s data doesn’t match exactly.
            return Err(format!(
                "write_bytes (len={}) is larger than this member’s size ({})",
                write_bytes.len(),
                member.size
            ));
        }

        // Write into self.data at the correct offset
        let start_offset = member.offset as usize;
        let end_offset = start_offset + write_bytes.len();

        if end_offset > self.data.len() {
            return Err(format!(
                "Offset range [{}, {}) is out of bounds for the data array (len = {}).",
                start_offset,
                end_offset,
                self.data.len()
            ));
        }

        self.data[start_offset..end_offset].copy_from_slice(write_bytes);

        self.current_member_index += 1;
        Ok(())
    }
}

impl ShaderModule {
    /// Create a new shader module from GLSL code, reflect it, and cache relevant layout metadata.
    ///
    /// * `device` - The device to create the shader module on.
    /// * `compiler` - The shader compiler to use.
    /// * `file_path` - The path to the GLSL file, from the project root.
    /// * `entry_point_name` - The name of the entry point function in the shader.
    pub fn from_glsl(
        device: &Device,
        compiler: &ShaderCompiler,
        file_path: &str,
        entry_point_name: &str,
    ) -> Result<Self, String> {
        let full_path = format!("{}{}", env!("PROJECT_ROOT"), file_path);
        let code = read_code_from_path(&full_path)?;
        let shader_kind = predict_shader_kind(file_path).map_err(|e| e.to_string())?;

        Self::from_glsl_code(
            device,
            &code,
            &Self::get_file_name_from_path(file_path),
            entry_point_name,
            compiler,
            shader_kind,
        )
    }

    /// For debugging: print all descriptor bindings in this shader.
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

    /// Retrieve the (cached) buffer layouts for uniform buffers/push-constants.
    /// These were extracted during initialization.
    pub fn get_buffer_layouts(&self) -> &[BufferLayout] {
        &self.0.buffer_layouts
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

    /// Constructs the pipeline layout from the descriptor sets and push constants reflected in this shader.
    pub fn get_pipeline_layout(&self, device: &Device) -> PipelineLayout {
        let descriptor_set_layouts = self.get_descriptor_set_layouts();
        let push_constant_ranges = self.get_push_constant_ranges();
        PipelineLayout::new(
            device,
            descriptor_set_layouts.as_deref(),
            push_constant_ranges.as_deref(),
        )
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

    // --- Private/Helper Methods ---

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
        // Compile to SPIR-V bytecode
        let shader_byte_code_u8 = compiler
            .compile_to_bytecode(code, shader_kind, entry_point_name, file_name)
            .map_err(|e| e.to_string())?;

        // Reflect the SPIR-V
        let reflect_shader_module =
            ReflectShaderModule::load_u8_data(&shader_byte_code_u8).map_err(|e| e.to_string())?;

        // Create the Vulkan ShaderModule
        let shader_module = bytecode_to_shader_module(device, &shader_byte_code_u8)?;

        // Extract (and cache) the buffer layouts
        let buffer_layouts = extract_buffer_layouts(&reflect_shader_module);

        Ok(Self(Arc::new(ShaderModuleInner {
            device: device.clone(),
            entry_point_name: CString::new(entry_point_name).unwrap(),
            shader_module,
            reflect_shader_module,
            buffer_layouts,
        })))
    }

    fn get_push_constant_ranges(&self) -> Option<Vec<PushConstantRange>> {
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

    fn get_descriptor_set_layouts(&self) -> Option<Vec<DescriptorSetLayout>> {
        let descriptor_sets = self
            .0
            .reflect_shader_module
            .enumerate_descriptor_sets(None)
            .ok()?;
        if descriptor_sets.is_empty() {
            return None;
        }

        let mut layouts = Vec::new();
        for descriptor_set in descriptor_sets {
            let mut builder = DescriptorSetLayoutBuilder::new();
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
            layouts.push(builder.build(&self.0.device).unwrap());
        }

        Some(layouts)
    }
}

// ----------------------
// Free Functions / Helpers
// ----------------------

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

/// Extract buffer layouts (uniform blocks, push constant blocks) from reflection.
fn extract_buffer_layouts(reflect_module: &ReflectShaderModule) -> Vec<BufferLayout> {
    let descriptor_bindings = match reflect_module.enumerate_descriptor_bindings(None) {
        Ok(db) => db,
        Err(_) => return Vec::new(),
    };

    let mut result = Vec::new();

    for binding in descriptor_bindings {
        // We only care about UniformBuffer (and possibly StorageBuffer if desired) for example
        if binding.descriptor_type == ReflectDescriptorType::UniformBuffer {
            let block_info = &binding.block;

            // Build our layout
            let layout = BufferLayout {
                set: binding.set,
                binding: binding.binding,
                name: if binding.name.is_empty() {
                    format!("ubo_set{}_binding{}", binding.set, binding.binding)
                } else {
                    binding.name.clone()
                },
                total_size: block_info.padded_size,
                members: parse_block_members(&block_info.members),
                descriptor_type: binding.descriptor_type,
            };

            result.push(layout);
        }
    }

    result
}

/// Given an array of `ReflectBlockVariable`, build a list of our `BufferMember`.
fn parse_block_members(
    members: &[spirv_reflect::types::ReflectBlockVariable],
) -> Vec<BufferMember> {
    let mut result = Vec::new();
    for (i, member) in members.iter().enumerate() {
        let member_name = if member.name.is_empty() {
            format!("member_{}", i)
        } else {
            member.name.clone()
        };

        let type_name = guess_type_name(member);

        result.push(BufferMember {
            offset: member.offset,
            size: member.size,
            padded_size: member.padded_size,
            type_name,
            name: member_name,
        });
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
